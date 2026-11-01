use asset_game::WeaponRegistry;
use asset_world::{ClipCollision, SpawnPoint};
use assets::{
    LoadingScreen, MatchLoadAbort, MatchType10SoundHints, PreparedDestructibleDeath,
    PreparedFpvMeshes, PreparedMatchReady, PreparedWeapons, PreparedXAnims,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use frame::{
    CacWeaponOffer, ClassSelectHandoff, HostClassLoadouts, InstalledMatch, LaunchIdentity,
    LaunchReport, WorldProducts, WorldStamp,
};
use hud_iw4::{
    SCREEN_BLEND_BLURRED, SCREEN_BLEND_FLASHED, SCREEN_BLEND_NONE, ShellshockKick,
    ShellshockLookParms, ShellshockSoundParms, ShellshockTimedSound, ShellshockVision, ShockParams,
};
use net::{AuthorityInputGate, AuthorityLoadHold, AuthorityWorld, ClientSet};

use crate::SessionContentManifest;
use crate::weapon_content;
use render_frontend::adapters::anim::dyn_ent::{DynEntPhysClip, DynEntPhysWorld};
use render_frontend::prepare::scene::camera::SimCamera;
use render_frontend::prepare::scene::world::{WorldScene, world_scene_from_draw};
use render_fx::{PreparedFxCatalog, PreparedFxModels, PreparedImpactFx, PreparedTracers};
use std::sync::Arc;

use crate::loadout::{AuthoritativeClassProjection, ClassRow, project_class};
use crate::startup::{PendingConsoleLines, StartupCommands};

fn log_world_report(report: &[String]) {
    if report.is_empty() {
        diag::info!(World, "world report: no rows");
        return;
    }
    diag::info!(World, "world report: {} rows", report.len());
    for row in report {
        diag::info!(World, "world report: {row}");
    }
}

#[derive(SystemParam)]
pub struct PreparedMatchSource<'w, 's> {
    ready: ResMut<'w, PreparedMatchReady>,
    audio_status: Option<Res<'w, audio::LoadingAudioStatus>>,
    swap: Res<'w, crate::SessionSwapRequest>,
    bridge: Option<Res<'w, net::MasterBridge>>,
    intent: Option<Res<'w, net::MasterLaunchIntent>>,
    loading: Option<ResMut<'w, LoadingScreen>>,
    load: Option<Res<'w, assets::MapLoadProcess>>,
    abort: Option<Res<'w, MatchLoadAbort>>,
    host_classes: Option<Res<'w, HostClassLoadouts>>,
    catalog: Option<Res<'w, asset_game::MenuCatalog>>,
    identity: Option<Res<'w, LaunchIdentity>>,
    install_stage: Local<'s, Option<asset_transport::StageHandle>>,
}

#[derive(SystemParam)]
pub struct MatchInstallConsole<'w> {
    startup: ResMut<'w, StartupCommands>,
    pending: ResMut<'w, PendingConsoleLines>,
}

impl MatchInstallConsole<'_> {
    fn release_startup(&mut self) {
        self.pending.0.append(&mut self.startup.lines);
    }
}

#[derive(SystemParam)]
pub struct MatchInstallAuthority<'w> {
    role: Res<'w, frame::RuntimeRole>,
    input_gate: Res<'w, AuthorityInputGate>,
    cheats: Option<Res<'w, sim::HostCheats>>,
    profile: Option<Res<'w, sim::LocalPlayerProfile>>,
    profile_stats: Option<Res<'w, sim::ProfileStats>>,
    previous: Option<Res<'w, AuthorityWorld>>,
    account: Option<Res<'w, crate::LocalAccount>>,
    local: Option<Res<'w, net::LocalPresentClient>>,
}

#[derive(SystemParam)]
pub struct MatchInstallPresentation<'w> {
    probe: Option<ResMut<'w, LaunchReport>>,
    camera: Res<'w, SimCamera>,
    player_data: Res<'w, sim::LocalPlayerData>,
}

pub fn apply_prepared_match(
    mut commands: Commands,
    source: PreparedMatchSource,
    mut console: MatchInstallConsole,
    authority: MatchInstallAuthority,
    presentation: MatchInstallPresentation,
    mode_selection: Option<Res<sim::HostGameModeSelection>>,
    rules: Option<Res<frame::HostMatchRules>>,
    mut failed: MessageWriter<frame::MapLoadFailed>,
) {
    let PreparedMatchSource {
        mut ready,
        audio_status,
        swap,
        bridge,
        intent,
        mut loading,
        load,
        abort,
        host_classes,
        catalog,
        identity,
        mut install_stage,
    } = source;
    let MatchInstallAuthority {
        role,
        input_gate,
        cheats,
        profile,
        profile_stats,
        previous,
        account,
        local,
    } = authority;
    let allow_debug_actions = cheats.is_some_and(|cheats| cheats.0);
    let MatchInstallPresentation {
        mut probe,
        camera: sim_cam,
        player_data,
    } = presentation;
    let stale_key = !ready.load_key.match_key.is_none()
        && bridge.as_ref().is_none_or(|bridge| {
            let state = bridge.state();
            !state.in_match()
                || state.identity().match_key() != ready.load_key.match_key
                || ready.load_key.incarnation != bridge.incarnation()
        });
    if !swap.accepts_install(ready.request_id)
        || stale_key
        || ready.load_key.local_load_request_id != ready.request_id
    {
        diag::info!(
            World,
            "match install: discarded stale load {:?} ({})",
            ready.load_key,
            ready.zone
        );
        if let Some(stage) = install_stage.take() {
            stage.cancel();
        }
        commands.remove_resource::<PreparedMatchReady>();
        return;
    }
    if ready.load_key.match_key.is_none() && intent.as_ref().is_some_and(|intent| intent.is_join())
    {
        return;
    }
    if abort.is_some_and(|abort| abort.0 == ready.request_id) {
        let dropped = ready.request_id;
        let zone = ready.zone.clone();
        if let Some(stage) = install_stage.take() {
            stage.cancel();
        }
        commands.remove_resource::<PreparedMatchReady>();
        commands.remove_resource::<MatchLoadAbort>();
        diag::info!(
            World,
            "match install: aborted request #{dropped} (`{zone}`) — not occupying the menu"
        );
        return;
    }
    match audio_status.as_deref() {
        Some(audio::LoadingAudioStatus::Ready) => {}
        Some(audio::LoadingAudioStatus::Failed(error)) => {
            failed.write(frame::MapLoadFailed {
                request_id: ready.request_id,
                load_key: ready.load_key,
                zone: ready.zone.clone(),
                error: error.clone(),
            });
            commands.remove_resource::<PreparedMatchReady>();
            return;
        }
        _ => return,
    }
    // The stage opens a frame before the work so the row is already up when
    // the main thread stops answering — the yield is the point, not an
    // accident of where the handle was created.
    let live_load = load
        .as_deref()
        .filter(|process| !process.is_complete())
        .map(|process| &process.progress);
    if live_load.is_some() && install_stage.is_none() {
        if let Some(progress) = live_load {
            *install_stage = Some(progress.begin(asset_transport::StageId::Install, None));
        }
        diag::info!(
            World,
            "match install: yield one frame so the overlay can name the main-thread hitch"
        );
        return;
    }
    let install_stage = install_stage.take();
    let install_started = std::time::Instant::now();
    let request_id = ready.request_id;
    let load_key = ready.load_key;
    let zone = std::mem::take(&mut ready.zone);
    let Some(mut prepared) = ready.prepared.take() else {
        return;
    };
    commands.remove_resource::<PreparedMatchReady>();

    let world_report = std::mem::take(&mut prepared.report);
    log_world_report(&world_report);
    // A zombies match reads its scripts, and their shock files, from the
    // zombie zones' sources.
    let map_shocks: Vec<(String, String)> = prepared
        .zombie_scripts
        .as_ref()
        .unwrap_or(&prepared.scripts)
        .shocks()
        .map(|(name, text)| (name.to_owned(), text.to_owned()))
        .collect();

    let plan = match preflight_match_install(
        prepared,
        load_key,
        &zone,
        mode_selection.as_deref(),
        rules.as_deref(),
        catalog.as_deref(),
        identity.as_deref(),
    ) {
        Ok(plan) => plan,
        Err(refusal) => {
            diag::warn!(World, "match install refused: {}", refusal.error);
            if let Some(probe) = probe.as_deref_mut() {
                if let Some(gap) = refusal.sim_gap {
                    probe.sim_gap = gap;
                }
                probe.world_report = world_report;
            }
            if let Some(stage) = install_stage {
                stage.fail();
            }
            if let Some(loading) = loading.as_deref_mut() {
                loading.fail(refusal.error.clone());
            }
            failed.write(frame::MapLoadFailed {
                request_id,
                load_key,
                zone,
                error: refusal.error,
            });
            console.release_startup();
            return;
        }
    };
    let prepared_install = (|| -> Result<bevy::ecs::world::CommandQueue, InstallRefusal> {
        let mut install = bevy::ecs::world::CommandQueue::default();
        install.push(audio::stage_prepared_match_audio);
        let MatchInstallPlan {
            ui_images,
            scripts,
            actor_anim_sources,
            script_level,
            script_entries,
            client_scripts,
            compiled_client_scripts,
            gametype_settings,
            script_dvars,
            engine_dvars,
            hosted_dvars,
            account_defaults,
            script_sound_aliases,
            script_sound_playback_ms,
            sound_mix_names,
            objective_weapons,
            kind,
            mode,
            gametype,
            scene: loaded_scene,
            weapons,
            killstreaks,
            fpv_meshes,
            bodies,
            soldiers,
            world_weapons,
            projectile_meshes,
            xmodel_walk,
            xanims,
            death,
            player_anim_sources,
            mut prepared_map,
            strings,
            clip,
            pen_table,
            pen_table_loaded,
            lochit_table,
            tracers,
            fx_catalog,
            type10,
            fx_models,
            impact_fx,
            mut authority_models,
            model_spawns,
        } = plan;
        let spawn_count = prepared_map.spawns.len();

        stage_resource(
            &mut install,
            assets::SessionMapIdentity {
                namespace: prepared_map.namespace,
                zone: prepared_map.zone.clone(),
            },
        );
        if mode.binds_account {
            let mut local_player_data = player_data.clone();
            let definitions = script_level
                .schemas
                .get("mp/playerdata.def")
                .ok_or_else(|| {
                    InstallRefusal::new("Local progression: player data schema is missing")
                })?;
            let ranks = weapons
                .registry()
                .rank_progression()
                .map_err(|error| InstallRefusal::new(format!("Local progression: {error}")))?;
            local_player_data
                .install(
                    definitions,
                    ranks,
                    account
                        .as_ref()
                        .and_then(|account| account.snapshot.as_ref()),
                )
                .map_err(|error| InstallRefusal::new(format!("Local progression: {error:?}")))?;
            local_player_data.install_unlock_data(
                account_defaults.clone(),
                script_level
                    .tables
                    .get("mp/allchallengestable.csv")
                    .cloned(),
            );
            for name in ["mp/ranktable.csv", "mp/rankicontable.csv"] {
                if let Some(table) = script_level.tables.get(name) {
                    local_player_data.install_rank_table(name, table.clone());
                }
            }
            diag::info!(
                Sim,
                "local progression prepared before admission: rank={}",
                local_player_data
                    .progression
                    .as_ref()
                    .expect("installed progression")
                    .rank
            );
            let rank_tables = [
                weapons.registry().shared_rank_table(),
                weapons.registry().shared_rank_icons(),
            ]
            .map(|table| table.expect("validated rank presentation").clone());
            install.push(move |world: &mut World| {
                world
                    .resource_mut::<crate::scope_control::InstallAnnouncement>()
                    .0
                    .push(move |world: &mut World| {
                        if let Some(mut catalog) =
                            world.get_resource_mut::<asset_game::MenuCatalog>()
                        {
                            for table in rank_tables {
                                catalog.insert_string_table(table);
                            }
                        }
                        world.insert_resource(local_player_data);
                    });
            });
        }

        let facts = std::mem::take(&mut prepared_map.facts);
        let airstrike_height = facts.airstrike_height;
        stage_resource(&mut install, crate::scope_control::MatchUiImages(ui_images));
        stage_resource(&mut install, mode);
        stage_resource(
            &mut install,
            assets::SessionCompass {
                corners: facts.minimap_corners,
                north_yaw: facts.north_yaw,
                declaration: facts.compass,
            },
        );
        stage_resource(
            &mut install,
            asset_audio::SessionMapScriptSound(facts.script_sound),
        );
        stage_resource(
            &mut install,
            asset_game::SessionTeamSettings(facts.team_settings),
        );
        stage_resource(&mut install, facts.hud_menus);
        stage_resource(&mut install, facts.code_hud_menus);
        stage_resource(&mut install, assets::PreparedLocalizedStrings(strings));
        stage_resource(&mut install, fx_catalog);
        stage_resource(&mut install, type10);
        stage_resource(&mut install, fx_models);
        stage_resource(&mut install, impact_fx);
        stage_resource(&mut install, tracers);

        let mut scene = loaded_scene;
        scene.readiness = frame::WorldReadiness::new(
            WorldStamp::from_install(request_id),
            frame::ReadinessState::Pending,
        );
        let mut sim_cam = *sim_cam;
        let mut input_gate = *input_gate;
        let sim_weapons = weapon_content::compile(
            weapons.registry(),
            &world_weapons.0,
            lochit_table,
            objective_weapons,
            pen_table,
            pen_table_loaded,
            mode.binds_account,
            mode.ammo_numbers,
        )
        .map_err(|error| InstallRefusal::new(format!("Invalid simulation weapons: {error:?}")))?;
        let mut content = sim::SimContentBuilder::for_match(Arc::clone(sim_weapons.content()));
        content
            .set_map(Arc::clone(&prepared_map.content))
            .map_err(|error| InstallRefusal::new(format!("Invalid map content: {error}")))?;
        content.set_script_sound_aliases(script_sound_aliases);
        content.set_script_sound_playback_ms(script_sound_playback_ms);
        content.set_sound_mix_names(sound_mix_names);
        content.set_family(prepared_map.namespace);
        let mut sim = sim::SimWorld::new();
        if role.runs_authority()
            && let Some(previous) = previous.as_ref()
        {
            sim.set_persistent_data(previous.0.persistent_data().for_new_match());
        }
        if *role == frame::RuntimeRole::Listen {
            let profile = previous
                .as_ref()
                .map(|authority| authority.0.local_player_profile())
                .or_else(|| profile.as_deref().copied())
                .unwrap_or(sim::LocalPlayerProfile::default());
            sim.set_local_player_profile(profile);
            let mut stats = previous
                .as_ref()
                .map(|authority| authority.0.profile_stats().clone())
                .or_else(|| profile_stats.as_deref().cloned())
                .unwrap_or_else(sim::ProfileStats::default);
            stats.client = local.as_ref().map(|local| local.0);
            sim.set_profile_stats(stats);
        }
        content.set_player_kit_collisions(
            (0..soldiers.kit_count())
                .map(|kit| player_kit_collision(soldiers.kit(kit).ok().map(Arc::as_ref)))
                .collect::<Result<Vec<_>, _>>()?,
        );
        if let Some(Ok(tree)) = player_anim_sources.compiled() {
            for axis in [false, true] {
                if let Ok(soldier) = soldiers.side(axis)
                    && let Ok(animation) = soldier.animation()
                    && let Ok(definition) = animation.runtime()
                {
                    if axis {
                        content.set_player_axis_anim_tree(Some(Arc::clone(definition)));
                    } else {
                        let names = animation
                            .tree()
                            .nodes()
                            .iter()
                            .map(|node| node.name.clone())
                            .collect();
                        content.set_player_anim_tree(Some(Arc::clone(definition)), names);
                    }
                }
            }
            if let Some(Ok(parsed)) = player_anim_sources.parsed_script() {
                content.set_player_anim_properties(
                    (0..tree.nodes().len())
                        .map(|index| parsed.animation_properties(index as u16))
                        .collect(),
                );
            }
        }

        let anim_namespace = prepared_map.namespace.expect("installed map family");
        content.set_script_model_states(xanims.0.zombie_states(anim_namespace));
        content.set_anim_state_tables(xanims.0.state_tables(anim_namespace).map(
            |(name, table)| {
                let name = name.rsplit('/').next().unwrap_or(name);
                let name = name.strip_suffix(".asd").unwrap_or(name);
                (name.to_owned(), Arc::clone(table))
            },
        ));
        content.set_script_model_clips(
            xanims
                .0
                .names()
                .filter_map(|name| Some((name.to_owned(), xanims.0.clip(anim_namespace, name)?))),
        );
        content.set_script_model_anims(xanims.0.names().filter_map(|name| {
            let parts = &xanims.0.get(anim_namespace, name)?.parts;
            let frequency = if parts.numframes > 0 && parts.framerate > 0.0 {
                parts.framerate / f32::from(parts.numframes)
            } else {
                0.0
            };
            Some((
                name.to_owned(),
                sim::ScriptModelPlayAnim {
                    looping: parts.flags & 1 != 0,
                    frequency,
                },
            ))
        }));
        let trees = actor_anim_sources.iter().filter_map(|(name, atr)| {
            let compiled = asset_anim::compile_animtree(atr.as_bytes())
                .map_err(|error| diag::warn!(Sim, "animation tree {name}: {error:?}"))
                .ok()?;
            let definition = compiled
                .to_runtime_definition(|_, leaf| xanims.0.clip(anim_namespace, leaf))
                .map_err(|error| diag::warn!(Sim, "animation tree {name}: {error:?}"))
                .ok()?;
            let names = compiled
                .nodes()
                .iter()
                .map(|node| node.name.clone())
                .collect();
            diag::info!(
                Sim,
                "animation tree {name}: {} nodes, {} leaves",
                compiled.node_count(),
                compiled.leaf_count()
            );
            Some(Arc::new(sim::script::ActorAnimTree::new(
                name.clone(),
                names,
                definition,
            )))
        });
        content.set_actor_anim_trees(trees.collect::<Vec<_>>());
        content.set_actor_paths(sim::script::ActorPaths::new(
            facts
                .path_nodes
                .iter()
                .map(|node| sim::script::NavNode {
                    kind: match node.kind {
                        asset_world::PathNodeKind::Path => sim::script::NavNodeKind::Path,
                        asset_world::PathNodeKind::NegotiationBegin => {
                            sim::script::NavNodeKind::NegotiationBegin
                        }
                        asset_world::PathNodeKind::NegotiationEnd => {
                            sim::script::NavNodeKind::NegotiationEnd
                        }
                        asset_world::PathNodeKind::Other(_) => sim::script::NavNodeKind::Other,
                    },
                    origin: node.origin,
                    yaw: node.yaw,
                    targetname: node.targetname.clone(),
                    target: node.target.clone(),
                    animscript: node.animscript.clone(),
                    links: node
                        .links
                        .iter()
                        .map(|link| (link.node, link.distance, link.negotiation))
                        .collect(),
                    fields: node
                        .fields
                        .as_ref()
                        .map(|fields| sim::script::NavNodeFields {
                            type_name: fields.type_name.clone(),
                            spawnflags: fields.spawnflags,
                            script_linkname: fields.script_linkname.clone(),
                            script_noteworthy: fields.script_noteworthy.clone(),
                            radius: fields.radius,
                            min_use_dist_sq: fields.min_use_dist_sq,
                        }),
                })
                .collect(),
        ));
        let clips = Arc::clone(&xanims.0);
        content.set_anim_clips(sim::AnimClipLookup::new(move |name| {
            clips.clip(anim_namespace, name)
        }));
        content.set_mantle_xanims(sim::MantleXAnimBind::from_clips(|fast, i| {
            let name = sim::MantleXAnimBind::clip_name(fast, i)?;
            xanims
                .0
                .clip(asset_core::AssetNamespace::Iw4, name)
                .map(|clip| (*clip).clone())
        }));
        content.set_vehicle_turrets(weapons.registry().vehicle_turrets());
        content.set_vehicle_accel(
            weapons
                .registry()
                .vehicle_accel()
                .map(|(name, accel)| (name.to_owned(), accel)),
        );
        content.set_vehicle_compass(
            weapons
                .registry()
                .vehicle_compass()
                .map(|(name, icons, size)| (name.to_owned(), (icons.clone(), size))),
        );
        install_team_voice_prefixes(&mut content, catalog.as_deref(), identity.as_deref(), &zone);
        install_shocks(
            &mut content,
            anim_namespace,
            catalog.as_deref(),
            &map_shocks,
        );
        let mut primary = Vec::new();
        let mut secondary = Vec::new();
        let mut lethal = Vec::new();
        let mut tactical = Vec::new();
        let mut excluded = Vec::new();
        let families = weapons.registry().weapon_families();
        for family in families.offered() {
            let offer = CacWeaponOffer {
                key: family.key.asset_key(),
                item_group: Some(family.item_group.clone()),
                attachments: family
                    .attachments
                    .iter()
                    .map(|choice| choice.name.clone())
                    .collect(),
            };
            match family.slot {
                asset_game::FamilySlot::Primary => primary.push(offer),
                asset_game::FamilySlot::Secondary => secondary.push(offer),
                asset_game::FamilySlot::Lethal => lethal.push(offer),
                asset_game::FamilySlot::Tactical => tactical.push(offer),
                asset_game::FamilySlot::Other => {}
            }
        }
        excluded.extend(families.excluded().iter().cloned());
        stage_resource(&mut install, fpv_meshes);
        stage_resource(&mut install, bodies);
        stage_resource(&mut install, world_weapons);
        stage_resource(&mut install, projectile_meshes);
        stage_resource(&mut install, xmodel_walk);
        if let Some(Ok(parsed)) = player_anim_sources.parsed_script() {
            let tree = player_anim_sources
                .compiled()
                .and_then(|c| c.as_ref().ok())
                .map(|t| t.as_ref());
            let namespace = player_anim_sources
                .family()
                .expect("installed character family");
            let slots = map_anim_items(&parsed.slots, tree, &xanims.0, namespace);
            let events = map_event_items(&parsed.events, tree, &xanims.0, namespace);
            content.set_player_anim_script(Some(sim::PlayerAnimScript::from_tables(slots, events)));
        }
        stage_resource(&mut install, xanims);
        stage_resource(&mut install, death);
        stage_resource(&mut install, player_anim_sources);
        stage_resource(&mut install, soldiers);
        input_gate.local_cmds_enabled = false;

        stage_resource(&mut install, DynEntPhysWorld::default());
        stage_resource(&mut install, DynEntPhysClip(clip.clone()));
        let brush_movers = std::mem::take(&mut authority_models.brush_movers);
        let (sim_gap, lock_reasons) = install_clip_and_player(
            &mut sim,
            content,
            clip,
            authority_models,
            scene.intermission_view.map(|view| sim::AuthoredSpawnPoint {
                classname: gamemode_iw4::playerlogic::MP_GLOBAL_INTERMISSION.to_owned(),
                origin: view.origin,
                angles: view.angles,
                script_linkto: String::new(),
                script_destructable_area: String::new(),
            }),
            airstrike_height,
            &prepared_map.spawns,
            weapons.registry(),
            &sim_weapons,
            &mut sim_cam,
            &mut input_gate,
            host_classes.as_deref(),
            kind,
            mode,
            allow_debug_actions,
            &script_dvars,
            gametype,
        )?;
        let script_facts = script_install_facts(&zone, gametype, &scripts, script_entries.len());
        if *role == frame::RuntimeRole::Listen {
            sim.register_local_presentation_dvars(local.as_ref().map(|local| local.0));
        }
        let mut natives = crate::games::natives(anim_namespace, &mode);
        let (missing, named) = natives.coverage(&scripts);
        diag::info!(
            Sim,
            "gsc: builtin coverage map={zone} gametype={gametype}: {missing} of {named} builtins not implemented yet"
        );
        if mode.report_builtin_gaps {
            let gaps = natives.bind_gaps(&scripts);
            if !gaps.is_empty() {
                diag::info!(
                    Sim,
                    "gsc: {missing} of {named} builtins not implemented yet: {}",
                    gaps.join(" ")
                );
            }
        }
        let client_natives = crate::games::client_natives(anim_namespace, &natives);
        // A client that lists its own keys' types (`radiant/clientkeys.txt`)
        // keeps only those keys.
        let client_level = client_scripts.as_ref().map(|_| sim::script::LevelData {
            entities: script_level.entities.clone(),
            keys: script_level
                .client_keys
                .clone()
                .unwrap_or_else(|| script_level.keys.clone()),
            entity_fields: script_level
                .client_keys
                .as_ref()
                .map(|_| Default::default()),
            effect_lives: script_level.effect_lives.clone(),
            primary_lights: script_level.primary_lights.clone(),
            dyn_ents: script_level.dyn_ents.clone(),
            ..Default::default()
        });
        sim.install_gsc_program(scripts, natives, script_level)
            .map_err(|e| script_refusal(&zone, gametype, "install", &e))?;
        if let (Some((program, entries, callbacks)), Some(mut client_natives), Some(client_level)) =
            (client_scripts, client_natives, client_level)
        {
            let (missing, named) = client_natives.coverage(&program);
            let gaps = client_natives.bind_gaps(&program);
            diag::info!(
                Sim,
                "csc: client builtin coverage map={zone}: {missing} of {named} client builtins not implemented yet: {}",
                gaps.join(" ")
            );
            if let Err(fault) = sim.install_client_scripts(
                program,
                client_natives,
                entries,
                callbacks,
                &client_level,
            ) {
                diag::warn!(Sim, "csc: client scripts not installed: {fault}");
            }
        }
        if mode.waits_for_lobby
            && let Some(bridge) = bridge.as_ref()
            && let net::MasterBridgeState::Hosting { members, .. } = bridge.state()
        {
            sim.set_expected_players(members.len());
        }
        if *role == frame::RuntimeRole::Listen
            && mode.binds_account
            && let (Some(account), Some(local)) = (account.as_ref(), local.as_ref())
        {
            account
                .bind_or_initialize(
                    sim.persistent_data_mut(),
                    local.0,
                    account_defaults.as_ref(),
                )
                .map_err(|error| {
                    InstallRefusal::new(format!("local account binding: {error:?}"))
                })?;
        }
        for (name, value) in &script_dvars {
            sim.set_gsc_dvar(name, value);
        }
        // A match hosted for other players (a lobby or a dedicated server)
        // is the game's hosted session, unless `IW4L_SCRIPT_DVARS` says
        // otherwise.
        let hosted = *role == frame::RuntimeRole::Dedicated
            || bridge.as_ref().is_some_and(|bridge| {
                matches!(bridge.state(), net::MasterBridgeState::Hosting { .. })
            });
        if hosted {
            let overrides = script_dvar_overrides();
            for (name, value) in &hosted_dvars {
                if !overrides
                    .iter()
                    .any(|(set, _)| set.eq_ignore_ascii_case(name))
                {
                    sim.set_gsc_dvar(name, value);
                }
            }
        }
        sim.set_gametype_settings(gametype_settings);
        for name in &engine_dvars {
            sim.publish_gsc_dvar(name);
        }
        for entry in script_entries {
            sim.start_gsc(&entry, sim::script::Value::level(), Vec::new())
                .map_err(|e| script_refusal(&zone, gametype, "entry", &e))?;
        }
        if let Some(client) = compiled_client_scripts {
            let mut natives = sim::script::NativeRegistry::black_ops_2_client();
            let (missing, named) = natives.coverage(&client.program);
            diag::info!(
                Sim,
                "csc: client builtin coverage map={zone} gametype={gametype}: {missing} of {named} client builtins not implemented yet"
            );
            let gaps = natives.bind_gaps(&client.program);
            if !gaps.is_empty() {
                diag::info!(
                    Sim,
                    "csc: {missing} of {named} client builtins not implemented yet: {}",
                    gaps.join(" ")
                );
            }
            sim.install_compiled_client_scripts(client, natives)
                .map_err(|e| script_refusal(&zone, gametype, "client install", &e))?;
        }
        let load_hold = AuthorityLoadHold(true);
        sim.world_objects_mut()
            .set_map_round_epoch(load_key.match_key.match_epoch);
        spawn_script_model_movers(&mut sim, &model_spawns);
        for (id, cmodel, origin, angles) in brush_movers {
            sim.spawn_brush_mover(id, cmodel, origin, angles)
                .expect("no free dynamic entity slot for a brush model");
        }
        // A failed level comes back as its save (`loadgame_continue`): the
        // match resumes at the tick the save was made at.
        let resumed = sim.load_requested_save();
        if resumed.is_some() {
            sim.world_objects_mut()
                .set_map_round_epoch(load_key.match_key.match_epoch);
        }
        stamp_script_mover_numbers(&mut scene, &sim);
        if !model_spawns.is_empty() {
            diag::info!(
                World,
                "script_model spawn: {} ET_SCRIPTMOVER from {}",
                sim.script_mover_count(),
                sim::gentity_spawn_base()
            );
        }

        let class_handoff = ClassSelectHandoff {
            pending: !scene.batches.is_empty(),
            primary,
            secondary,
            lethal,
            tactical,
            excluded,
            lock_reasons,
        };

        let manifest = SessionContentManifest::build(
            &prepared_map,
            weapons.registry(),
            &sim_weapons,
            sim.content_digest(),
        )
        .map_err(|error| InstallRefusal::new(format!("Invalid content manifest: {error:?}")))?;
        stage_resource(&mut install, weapons);
        stage_resource(&mut install, killstreaks);

        let components = sim.content_components();
        diag::info!(
            Sim,
            "session manifest: ruleset=iw4 map={:?} weapons={} digest={:016x} \
         gameplay={:016x} c_map={:016x} c_models={:016x} c_weapons={:016x} c_classes={:016x}",
            manifest.map,
            manifest.weapons.len(),
            manifest.digest,
            sim.content_digest(),
            components.map,
            components.models,
            components.weapons,
            components.classes
        );
        match manifest.match_descriptor(components) {
            Some(descriptor) => {
                diag::info!(
                    Sim,
                    "match descriptor: map={:016x} weapons={:016x} classes={:016x}",
                    descriptor.map,
                    descriptor.weapons,
                    descriptor.classes
                );
                stage_resource(&mut install, descriptor);
            }
            None => return Err(InstallRefusal::new("Required match descriptor is missing")),
        }
        stage_resource(&mut install, manifest);

        diag::info!(
            World,
            "match preparation: {:.1}ms on the main thread (scene handoff, sim boot, catalogs)",
            install_started.elapsed().as_secs_f32() * 1000.0
        );
        if role.runs_client() {
            let client = local.as_ref().map_or(sim::ClientId(0), |local| local.0);
            let mut state = net::ClientPrediction::new(client);
            state.arm_from_content(&sim);
            stage_resource(&mut install, net::ClientPredictionState(state));
        }
        if role.runs_authority() || *role == frame::RuntimeRole::Replay {
            stage_resource(&mut install, AuthorityWorld(sim));
        }
        sim_cam.freeze_fly = true;
        stage_resource(&mut install, WorldProducts::from_walk(scene.products_id));
        stage_resource(&mut install, scene);
        install.push(move |world: &mut World| {
            world
                .resource_mut::<crate::scope_control::InstallAnnouncement>()
                .0
                .push(move |world: &mut World| {
                    world.insert_resource(sim_cam);
                });
        });
        install.push(move |world: &mut World| {
            world
                .resource_mut::<crate::scope_control::InstallAnnouncement>()
                .0
                .push(move |world: &mut World| {
                    world.insert_resource(input_gate);
                });
        });
        install.push(move |world: &mut World| {
            world
                .resource_mut::<crate::scope_control::InstallAnnouncement>()
                .0
                .push(move |world: &mut World| {
                    world.insert_resource(load_hold);
                });
        });
        install.push(move |world: &mut World| {
            world
                .resource_mut::<crate::scope_control::InstallAnnouncement>()
                .0
                .push(move |world: &mut World| {
                    world.insert_resource(class_handoff);
                });
        });
        if let Some(tick) = resumed {
            install.push(move |world: &mut World| {
                world
                    .resource_mut::<crate::scope_control::InstallAnnouncement>()
                    .0
                    .push(move |world: &mut World| {
                        world.insert_resource(net::AuthorityClock {
                            tick: tick.0,
                            time_ms: (tick.0 as i32).wrapping_mul(net::AUTHORITY_MS),
                        });
                    });
            });
        }
        stage_resource(&mut install, crate::LiveWorldIdentity { load_key });
        stage_resource(
            &mut install,
            frame::WorldGeneration::from_install(request_id),
        );
        let installed_zone = zone.clone();
        install.push(move |world: &mut World| {
            world.resource_mut::<crate::scope_control::InstallAnnouncement>().0.push(move |world: &mut World| {
            if let Some(mut probe) = world.get_resource_mut::<LaunchReport>() {
                probe.sim_gap = sim_gap;
                probe.world_report = world_report;
            }
            diag::script_boundary("installed", &script_facts);
            diag::info!(Sim, "match: {} ({}) — world installed ({} dm spawn points); class select waits for admission", kind.display_name(), kind.token(), spawn_count);
            perf::match_installed(&installed_zone, 1);
            frame::scope::insert(world, InstalledMatch {
                request_id,
                load_key,
                zone: installed_zone,
                spawn_count,
            }, frame::MatchScope::Live);
            });
        });

        Ok(install)
    })();
    match prepared_install {
        Ok(install) => {
            commands.queue(move |world: &mut World| {
                let accepts = world
                    .resource::<crate::SessionSwapRequest>()
                    .accepts_install(request_id);
                let same_match = load_key.match_key.is_none()
                    || world
                        .get_resource::<net::MasterBridge>()
                        .is_some_and(|bridge| {
                            bridge.state().in_match()
                                && bridge.state().identity().match_key() == load_key.match_key
                                && bridge.incarnation() == load_key.incarnation
                        });
                if accepts && same_match {
                    world.resource_mut::<crate::ScopeControl>().install(install);
                }
            });
            if let Some(stage) = install_stage {
                stage.done();
            }
        }
        Err(refusal) => {
            diag::warn!(World, "match install refused: {}", refusal.error);
            if let Some(stage) = install_stage {
                stage.fail();
            }
            if let Some(loading) = loading.as_deref_mut() {
                loading.fail(refusal.error.clone());
            }
            failed.write(frame::MapLoadFailed {
                request_id,
                load_key,
                zone,
                error: refusal.error,
            });
        }
    }
    console.release_startup();
}

/// Everything the install publishes, assembled while a refusal is still free.
///
/// The plan exists so publication cannot start before the checks are done: the
/// commit half of `apply_prepared_match` has nothing to install until preflight
/// hands one over.
struct MatchInstallPlan {
    ui_images: asset_material::UiImagePublication,
    mode: game_api::ModeRules,
    scripts: sim::script::Program,
    /// `.atr` sources of the animation trees actors may use, by tree name.
    actor_anim_sources: Vec<(String, String)>,
    script_level: sim::script::LevelData,
    script_entries: Vec<String>,
    /// The game's client scripts and their entries, when its client runs
    /// scripts of its own.
    client_scripts: Option<(
        sim::script::Program,
        Vec<String>,
        Option<game_api::ClientCallbacks>,
    )>,
    /// The compiled client scripts the map's local clients run, when its game
    /// has them.
    compiled_client_scripts: Option<game_api::ClientScriptProgram>,
    /// The gametype's settings (`GetGametypeSetting`).
    gametype_settings: Vec<(String, String)>,
    script_dvars: Vec<(String, String)>,
    /// The game's own engine dvars, which its clients read too.
    engine_dvars: Vec<String>,
    /// What a match hosted for other players sets over them.
    hosted_dvars: Vec<(String, String)>,
    account_defaults: Option<sim::PlayerDataDefaults>,
    script_sound_aliases: Option<std::collections::BTreeMap<String, Option<bool>>>,
    script_sound_playback_ms: std::collections::BTreeMap<String, i32>,
    sound_mix_names: Option<asset_core::SoundMixNames>,
    objective_weapons: Vec<(String, u32)>,
    kind: gamemode_iw4::GameModeKind,
    gametype: &'static str,
    scene: WorldScene,
    weapons: PreparedWeapons,
    killstreaks: assets::prepared::PreparedKillstreaks,
    fpv_meshes: PreparedFpvMeshes,
    bodies: assets::PreparedBodies,
    soldiers: asset_game::SoldierPresentations,
    world_weapons: assets::PreparedWorldWeapons,
    projectile_meshes: assets::PreparedProjectileMeshes,
    xmodel_walk: assets::PreparedXModelWalkCensus,
    xanims: PreparedXAnims,
    death: PreparedDestructibleDeath,
    player_anim_sources: asset_anim::PlayerAnimSources,
    prepared_map: assets::PreparedMap,
    strings: asset_game::LocalizeCatalog,
    clip: Option<Arc<ClipCollision>>,
    pen_table: weapon_iw4::PenetrationDepthTable,
    pen_table_loaded: bool,
    lochit_table: Option<[f32; weapon_iw4::HITLOC_COUNT]>,
    tracers: PreparedTracers,
    fx_catalog: PreparedFxCatalog,
    type10: MatchType10SoundHints,
    fx_models: PreparedFxModels,
    impact_fx: PreparedImpactFx,
    authority_models: AuthorityEntityModelInstall,
    model_spawns: Vec<(sim::ScriptModelId, [f32; 3], [f32; 3])>,
}

/// A refusal of the whole request: the loading screen shows `error`, the swap
/// completes on it, and nothing of the match has been published.
struct InstallRefusal {
    error: String,
    sim_gap: Option<&'static str>,
}

impl InstallRefusal {
    fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            sim_gap: None,
        }
    }

    fn with_gap(error: impl Into<String>, gap: &'static str) -> Self {
        Self {
            error: error.into(),
            sim_gap: Some(gap),
        }
    }
}

fn script_install_facts(
    zone: &str,
    gametype: &str,
    program: &sim::script::Program,
    entries: usize,
) -> String {
    let fingerprint: String = program
        .fingerprint()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!(
        " map={zone} gametype={gametype} fingerprint={fingerprint} modules={} functions={} natives={} entries={entries}",
        program.modules().len(),
        program.function_count(),
        program.native_count(),
    )
}

fn config_sets(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.split("//").next()?.trim();
            let rest = line
                .strip_prefix("set ")
                .or_else(|| line.strip_prefix("seta "))?
                .trim_start();
            let (name, value) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let value = value.trim().trim_matches('"');
            (!name.is_empty()).then(|| (name.to_owned(), value.to_owned()))
        })
        .collect()
}

/// A config's `set` lines, with those of the configs its `exec` lines run
/// where they run them; a config that is missing is reported.
fn config_sets_with_execs(
    name: &str,
    text: &dyn Fn(&str) -> Option<String>,
    depth: usize,
) -> Vec<(String, String)> {
    let Some(source) = text(name) else {
        diag::warn!(
            Sim,
            "gsc: front end config {name} is in no zone the match read"
        );
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in source.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        match line.strip_prefix("exec ") {
            Some(file) if depth < 8 => {
                out.extend(config_sets_with_execs(
                    file.trim().trim_matches('"'),
                    text,
                    depth + 1,
                ));
            }
            _ => out.extend(config_sets(line)),
        }
    }
    out
}

fn script_dvar_overrides() -> Vec<(String, String)> {
    std::env::var("IW4L_SCRIPT_DVARS")
        .map(|text| config_sets(&text.replace(';', "\n")))
        .unwrap_or_else(|_| Vec::new())
}

fn script_refusal(
    zone: &str,
    gametype: &str,
    stage: &str,
    fault: &sim::script::Fault,
) -> InstallRefusal {
    let text = fault.to_string();
    let head = text.lines().next().unwrap_or("").replace('"', "'");
    diag::script_boundary(
        "refused",
        &format!(" map={zone} gametype={gametype} stage={stage} fault=\"{head}\""),
    );
    InstallRefusal::new(format!("GSC {stage}: {text}"))
}

fn unknown_refusal(zone: &str, gametype: &str, gap: &game_api::Unknown) -> InstallRefusal {
    diag::script_boundary(
        "refused",
        &format!(
            " map={zone} gametype={gametype} stage=program unknown={}",
            gap.id
        ),
    );
    InstallRefusal::new(format!(
        "GSC program: {} is unknown: {} (needs {})",
        gap.id, gap.what, gap.needs
    ))
}

fn preflight_match_install(
    mut prepared: assets::PreparedMatch,
    load_key: frame::LocalLoadKey,
    zone: &str,
    mode_selection: Option<&sim::HostGameModeSelection>,
    rules: Option<&frame::HostMatchRules>,
    catalog: Option<&asset_game::MenuCatalog>,
    identity: Option<&LaunchIdentity>,
) -> Result<MatchInstallPlan, InstallRefusal> {
    let killstreaks = assets::prepared::PreparedKillstreaks(
        prepared
            .scripts
            .tables()
            .get("mp/killstreaktable.csv")
            .map_or_else(Vec::new, |table| {
                let mut names: Vec<String> = (0..table.rows)
                    .filter_map(|row| {
                        let cells = table
                            .cells
                            .get(row * table.columns..(row + 1) * table.columns)?;
                        cells.get(4)?.parse::<i32>().ok()?;
                        let weapon = cells.get(12)?;
                        if weapon.is_empty()
                            || prepared.weapons.resolve_index(weapon).ok().flatten()? == 0
                        {
                            return None;
                        }
                        let name = cells.get(1)?;
                        (!name.is_empty()
                            && name != "none"
                            && sim::menu_response_field(name).is_some())
                        .then(|| name.clone())
                    })
                    .collect();
                names.sort();
                names.dedup();
                names
            }),
    );
    let weapons = PreparedWeapons::for_match(prepared.weapons, load_key);
    let fpv_meshes = PreparedFpvMeshes(prepared.fpv_meshes);
    let bodies = assets::PreparedBodies(prepared.bodies);
    let soldiers = prepared.soldiers;
    let world_weapons = assets::PreparedWorldWeapons(Arc::new(prepared.world_weapons));
    let projectile_meshes = assets::PreparedProjectileMeshes(prepared.projectile_meshes);
    let xmodel_walk = std::mem::take(&mut prepared.xmodel_walk);
    let xanims = PreparedXAnims(prepared.xanims);
    let death = PreparedDestructibleDeath(std::mem::take(&mut prepared.destructible_death));
    log_destructible_death_assets(&death);
    let player_anim_sources = prepared.player_anim_sources;
    if !soldiers.owned_by(&bodies.0, &fpv_meshes.0, &xanims.0, &player_anim_sources) {
        return Err(InstallRefusal::new(
            "soldier presentation owners differ from match catalogs",
        ));
    }
    let mut prepared_map = prepared.prepared_map;
    if prepared_map.namespace == Some(asset_core::AssetNamespace::Iw4)
        && let Some(catalog) = catalog
        && let Some(table) = catalog.string_table(gamemode_iw4::FACTION_TABLE)
    {
        let arena = catalog
            .rawfile_text("mp/basemaps.arena")
            .map(str::to_owned)
            .or_else(|| identity.and_then(|id| asset_game::read_basemaps_arena(&id.games_root)));
        prepared_map.facts.team_settings = asset_game::team_settings_for_zone(
            table,
            asset_core::AssetNamespace::Iw4,
            arena.as_deref(),
            zone,
        );
    }
    let kind = match match_kind(mode_selection) {
        Ok(kind) => kind,
        Err(gap) => {
            diag::info!(Sim, "match install refused: {gap}");
            return Err(InstallRefusal::with_gap(gap, gap));
        }
    };
    let family = prepared_map
        .namespace
        .ok_or_else(|| InstallRefusal::new(format!("`{zone}` belongs to no game")))?;
    let has_module = |module: &str| prepared.scripts.module(module).is_some();
    let mode = match crate::games::modes(family).mode_for_match(zone, kind.token(), &has_module) {
        game_api::Rule::Known(mode) => mode,
        game_api::Rule::Unknown(gap) => return Err(unknown_refusal(zone, kind.token(), gap)),
    };
    let mut objective_weapons = Vec::new();
    let mut absent_effects = std::collections::BTreeSet::new();
    if let Some(
        namespace @ (asset_core::AssetNamespace::T5
        | asset_core::AssetNamespace::Iw5
        | asset_core::AssetNamespace::T6),
    ) = prepared_map.namespace
        && mode.binds_objectives
        && !matches!(
            kind,
            gamemode_iw4::GameModeKind::FreeForAll | gamemode_iw4::GameModeKind::TeamDeathmatch
        )
    {
        let catalog = catalog.ok_or_else(|| {
            InstallRefusal::new("IW4 menu catalog missing for objective bindings".to_owned())
        })?;
        let table = catalog
            .string_table(gamemode_iw4::FACTION_TABLE)
            .ok_or_else(|| {
                InstallRefusal::new("IW4 faction table missing for objective bindings".to_owned())
            })?;
        let (allies, axis) = iw4_charsets(catalog, identity, zone);
        let iw4 =
            asset_game::ObjectiveVisuals::from_faction_table(table, Some(&allies), Some(&axis));
        let realm = &prepared_map.facts.objective_visuals;
        let bindings = objective_bindings(&iw4, realm, kind, namespace)?;
        absent_effects.extend(bindings.absent_effects);
        let scene = &mut prepared.world.map_xmodel_scene_assets;
        let mut served = Vec::new();
        let mut missing = Vec::new();
        let native_model = |asset: Option<&asset_world::MapXModelSceneAsset>| {
            matches!(
                (prepared_map.namespace, asset),
                (
                    Some(asset_core::AssetNamespace::Iw5),
                    Some(asset_world::MapXModelSceneAsset::Iw5(_))
                ) | (
                    Some(asset_core::AssetNamespace::T5),
                    Some(asset_world::MapXModelSceneAsset::T5(_))
                ) | (
                    Some(asset_core::AssetNamespace::T6),
                    Some(asset_world::MapXModelSceneAsset::T6(_))
                )
            )
        };
        for (from, to) in bindings.models {
            let native = native_model(scene.get_name(to));
            if !native || !scene.alias(from, to) {
                missing.push(to);
            } else {
                served.push(format!("{from}->{to}"));
            }
        }
        for (from, to) in bindings.optional_models {
            if native_model(scene.get_name(to)) && scene.alias(from, to) {
                served.push(format!("{from}->{to}"));
            }
        }
        for (from, to) in bindings.effects {
            match prepared.fx.alias_for_map(from, to) {
                true => served.push(format!("{from}->{to}")),
                false => missing.push(to),
            }
        }
        for (from, to) in bindings.weapons {
            let weapon = objective_weapon_binding(weapons.registry(), namespace, to);
            match weapon {
                Some(id) => {
                    objective_weapons.push((from.to_owned(), id));
                    served.push(format!("{from}->{}:{to}#{id}", namespace.as_str()));
                }
                None => missing.push(to),
            }
        }
        diag::info!(
            World,
            "objective visuals from the map's game: served={served:?} missing={missing:?}"
        );
        if !missing.is_empty() {
            return Err(InstallRefusal::new(format!(
                "objective assets missing from the map's game: {}",
                missing.join(", ")
            )));
        }
    }
    let strings = std::mem::take(&mut prepared.strings);
    struct Sources(assets::ScriptSources);
    impl sim::script::SourceResolver for Sources {
        fn read(&self, module: &str) -> Result<String, String> {
            self.0
                .read(module)
                .map(|bytes| sim::script::decode_source(&bytes))
        }
        fn read_bytes(&self, module: &str) -> Result<Vec<u8>, String> {
            self.0.read(module)
        }
        fn read_module(&self, module: &str) -> Result<Vec<u8>, String> {
            self.0
                .module(module)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| format!("missing compiled module {module}"))
        }
        fn origin(&self, module: &str) -> sim::script::SourceOrigin {
            match self.0.origin(module) {
                Some(assets::ScriptSourceOrigin::Packaged) => sim::script::SourceOrigin::Packaged,
                Some(assets::ScriptSourceOrigin::BuiltIn) => sim::script::SourceOrigin::BuiltIn,
                None => sim::script::SourceOrigin::External,
            }
        }
    }
    let zombies = match (mode.zombie_zone_scripts, prepared.zombie_scripts.take()) {
        (true, Some(scripts)) => Some(scripts),
        (true, None) => {
            return Err(InstallRefusal::new(format!(
                "the zombies mode needs a T5 zombie map; `{zone}` is not one"
            )));
        }
        _ => None,
    };
    let is_zombies = zombies.is_some();
    let sources = Sources(zombies.unwrap_or_else(|| std::mem::take(&mut prepared.scripts)));
    let gametype = kind
        .script_tokens()
        .iter()
        .copied()
        .find(|token| {
            sources
                .0
                .read(&format!("maps/mp/gametypes/{token}"))
                .is_ok()
        })
        .unwrap_or(kind.token());
    let account_defaults =
        sources
            .0
            .config("mp/stats_init.cfg")
            .map(|config| sim::PlayerDataDefaults {
                config: config.to_owned(),
                class_names: std::array::from_fn(|index| {
                    let key = format!("CLASS_SLOT{}", index + 1);
                    strings
                        .raw_text(&key)
                        .filter(|bytes| bytes.first().is_some_and(|byte| *byte != 0))
                        .map_or_else(|| key.into_bytes(), |bytes| bytes.to_vec())
                }),
            });
    let keys = match sources.0.config("radiant/keys.txt") {
        Some(text) => sim::script::parse_radiant_keys(text).unwrap_or_else(|error| {
            diag::warn!(Sim, "gsc: radiant/keys.txt: {error}");
            Default::default()
        }),
        None => {
            diag::warn!(
                Sim,
                "gsc: radiant/keys.txt is not in the zones; map keys stay strings"
            );
            Default::default()
        }
    };
    let client_keys = sources.0.config("radiant/clientkeys.txt").and_then(|text| {
        match sim::script::parse_radiant_keys(text) {
            Ok(keys) => Some(keys),
            Err(error) => {
                diag::warn!(Sim, "csc: radiant/clientkeys.txt: {error}");
                None
            }
        }
    });
    let effect_lives = prepared
        .fx
        .effects()
        .map(|effect| (effect.name.to_ascii_lowercase(), effect.life_bound_msec()))
        .collect();
    let primary_lights = prepared.world.draw.as_ref().map_or_else(Vec::new, |draw| {
        draw.primary_lights
            .iter()
            .map(|light| sim::script::MapPrimaryLight {
                exponent: light.exponent,
                color: light.color,
                direction: light.direction,
                origin: light.origin,
                radius: light.radius,
                cos_half_fov_outer: light.cos_outer,
                cos_half_fov_inner: light.cos_inner,
                limits: light.t6_limits,
            })
            .collect()
    });
    let dyn_ents = [
        &prepared.world.dyn_ents.models,
        &prepared.world.dyn_ents.brushes,
    ]
    .into_iter()
    .zip(0u8..)
    .flat_map(|(defs, pool)| {
        defs.iter().map(move |def| sim::script::MapDynEnt {
            pool,
            index: def.index,
            origin: def.origin,
        })
    })
    .collect();
    let mut script_level = sim::script::LevelData {
        absent_effects,
        effect_lives,
        primary_lights,
        dyn_ents,
        client_keys,
        player_data_defaults: account_defaults.clone(),
        schemas: sources.0.schemas().clone(),
        entities: sim::script::parse_entity_string(sources.0.entities().unwrap_or("")),
        keys,
        entity_fields: None,
        layouts: sources.0.layouts().clone(),
        barriers: sources.0.barriers().clone(),
        weapons: sources.0.weapons().clone(),
        raw_files: sources
            .0
            .configs()
            .map(|(name, text)| (name.to_ascii_lowercase(), Arc::from(text)))
            .collect(),
        tables: sources
            .0
            .tables()
            .iter()
            .map(|(name, table)| {
                let table = sim::script::StringTable {
                    columns: table.columns,
                    rows: table.rows,
                    cells: table.cells.clone(),
                };
                (name.clone(), table)
            })
            .collect(),
    };
    if mode.binds_account {
        let shared_ranks = weapons.registry().shared_rank_table().map_err(|error| {
            InstallRefusal::new(format!("Invalid shared rank progression: {error}"))
        })?;
        script_level.tables.insert(
            "mp/ranktable.csv".into(),
            sim::script::StringTable {
                columns: shared_ranks.columns,
                rows: shared_ranks.rows,
                cells: shared_ranks.cells.clone(),
            },
        );
        let shared_icons = weapons
            .registry()
            .shared_rank_icons()
            .map_err(|error| InstallRefusal::new(format!("Invalid shared rank icons: {error}")))?;
        script_level.tables.insert(
            "mp/rankicontable.csv".into(),
            sim::script::StringTable {
                columns: shared_icons.columns,
                rows: shared_icons.rows,
                cells: shared_icons.cells.clone(),
            },
        );
    }
    let game_scripts = crate::games::scripts(family);
    let config = |name: &str| sources.0.config(name);
    let table = |name: &str| {
        sources
            .0
            .tables()
            .get(&name.to_ascii_lowercase())
            .map(|table| (table.columns, table.cells.as_slice()))
    };
    let mut traversals: Vec<String> = prepared_map
        .facts
        .path_nodes
        .iter()
        .filter(|node| matches!(node.kind, asset_world::PathNodeKind::NegotiationBegin))
        .filter(|node| !node.animscript.is_empty())
        .map(|node| node.animscript.to_ascii_lowercase())
        .collect();
    traversals.sort_unstable();
    traversals.dedup();
    let start = asset_transport::zombies_start().filter(|start| start.gametype == gametype);
    let request = game_api::ScriptRequest {
        map: zone,
        gametype,
        location: start.as_ref().and_then(|start| start.location.as_deref()),
        entities: sources.0.entities().unwrap_or(""),
        traversals: &traversals,
        config: &config,
        table: &table,
    };
    let program = match game_scripts.program(&request, &sources) {
        game_api::Rule::Known(program) => program,
        game_api::Rule::Unknown(gap) => return Err(unknown_refusal(zone, gametype, gap)),
    };
    let compiled_client_scripts = match game_scripts.compiled_client_program(&request, &sources) {
        Some(game_api::Rule::Known(client)) => Some(client),
        Some(game_api::Rule::Unknown(gap)) => {
            diag::warn!(
                Sim,
                "csc: the map's client scripts do not run: {} ({})",
                gap.what,
                gap.id
            );
            None
        }
        None => None,
    };
    diag::info!(
        Sim,
        "gsc: startup namespace={:?} roots={:?} entries={:?}",
        prepared_map.namespace,
        program.roots,
        program.entries
    );
    let roots: Vec<&str> = program.roots.iter().map(String::as_str).collect();
    let scripts = match program.built {
        Some(built) => built,
        None => sim::script::Program::load(&sources, &roots, &program.catalog)
            .map_err(|e| script_refusal(zone, gametype, "compile", &e))?,
    };
    let client_scripts = game_scripts
        .client_program(&request, &sources)
        .and_then(|client| {
            let roots: Vec<&str> = client.roots.iter().map(String::as_str).collect();
            match sim::script::Program::load(&sources, &roots, &client.catalog) {
                Ok(compiled) => {
                    diag::info!(
                        Sim,
                        "csc: compiled map={zone}: {} modules, {} functions, {} client builtins named",
                        compiled.modules.len(),
                        compiled.function_count(),
                        compiled.natives.len()
                    );
                    Some((compiled, client.entries, game_scripts.client_callbacks(gametype)))
                }
                Err(error) => {
                    diag::warn!(Sim, "csc: client scripts do not compile: {error}");
                    None
                }
            }
        });
    let mut script_dvars = match game_scripts.match_config() {
        None => Vec::new(),
        Some(name) => {
            let config = sources
                .0
                .config(name)
                .or_else(|| {
                    catalog
                        .filter(|catalog| catalog.namespace == Some(family))
                        .and_then(|catalog| catalog.rawfile_text(name))
                })
                .map(str::to_owned);
            match config {
                Some(text) => config_sets(&text),
                None => {
                    diag::warn!(
                        Sim,
                        "gsc: {name} is in neither the map's zones nor its game's menu catalog"
                    );
                    Vec::new()
                }
            }
        }
    };
    let config_text = |name: &str| {
        sources
            .0
            .config(name)
            .or_else(|| {
                catalog
                    .filter(|catalog| catalog.namespace == Some(family))
                    .and_then(|catalog| catalog.rawfile_text(name))
            })
            .map(str::to_owned)
    };
    for name in game_scripts.front_end_configs(gametype) {
        for (name, value) in config_sets_with_execs(name, &config_text, 0) {
            match script_dvars
                .iter_mut()
                .find(|(set, _)| set.eq_ignore_ascii_case(&name))
            {
                Some((_, set)) => *set = value,
                None => script_dvars.push((name, value)),
            }
        }
    }
    for (name, value) in game_scripts.config_defaults() {
        if !script_dvars
            .iter()
            .any(|(set, _)| set.eq_ignore_ascii_case(name))
        {
            script_dvars.push(((*name).to_owned(), (*value).to_owned()));
        }
    }
    script_dvars.push(("mapname".into(), zone.to_owned()));
    script_dvars.push(("g_gametype".into(), gametype.to_owned()));
    script_dvars.extend(
        game_scripts
            .engine_dvars(gametype)
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned())),
    );
    for (name, value) in &program.dvars {
        match script_dvars
            .iter_mut()
            .find(|(set, _)| set.eq_ignore_ascii_case(name))
        {
            Some((_, set)) => set.clone_from(value),
            None => script_dvars.push((name.clone(), value.clone())),
        }
    }
    let engine_dvars = game_scripts
        .engine_dvars(gametype)
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect();
    let hosted_dvars = game_scripts
        .hosted_dvars(gametype)
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();
    for (name, value) in rules.map_or(&[][..], |rules| &rules.0) {
        match script_dvars
            .iter_mut()
            .find(|(set, _)| set.eq_ignore_ascii_case(name))
        {
            Some((_, set)) => set.clone_from(value),
            None => script_dvars.push((name.clone(), value.clone())),
        }
    }
    if let Some(rules) = rules {
        diag::info!(
            Sim,
            "gsc: {} host rule dvars over {}",
            rules.0.len(),
            game_scripts
                .match_config()
                .unwrap_or("the engine's defaults")
        );
    }
    script_dvars.extend(script_dvar_overrides());
    let script_entries = program.entries;
    let mut gametype_settings = program.settings;
    // A short test match's limit, over the gametype's own (`docs/ENV.md`).
    if let Ok(limit) = std::env::var("IW4L_SCORE_LIMIT") {
        for (name, value) in &mut gametype_settings {
            if name == "scorelimit" {
                value.clone_from(&limit);
            }
        }
    }
    let key_type = |ty| match ty {
        game_api::KeyType::Int => sim::script::KeyType::Int,
        game_api::KeyType::Float => sim::script::KeyType::Float,
        game_api::KeyType::String => sim::script::KeyType::String,
        game_api::KeyType::Vector => sim::script::KeyType::Vector,
    };
    if !program.key_types.is_empty() {
        script_level.keys = program
            .key_types
            .into_iter()
            .map(|(key, ty)| (key, key_type(ty)))
            .collect();
    }
    if !program.entity_fields.is_empty() {
        script_level.entity_fields = Some(
            program
                .entity_fields
                .into_iter()
                .map(|(key, ty)| (key, key_type(ty)))
                .collect(),
        );
    }
    let actor_anim_sources: Vec<(String, String)> = if is_zombies {
        sources
            .0
            .configs()
            .filter_map(|(name, text)| {
                let tree = name.strip_prefix("animtrees/")?.strip_suffix(".atr")?;
                Some((tree.to_owned(), text.to_owned()))
            })
            .collect()
    } else {
        Vec::new()
    };
    let authority_models = authority_entity_model_install(&prepared.world);
    let model_spawns = script_model_spawns(&prepared.world.script_model_instances);
    let fx_catalog = PreparedFxCatalog(std::mem::take(&mut prepared.fx));
    let type10 = MatchType10SoundHints(
        fx_catalog
            .0
            .unique_type10_sound_hints()
            .into_iter()
            .map(str::to_owned)
            .collect(),
    );
    let fx_models = PreparedFxModels(std::mem::take(&mut prepared.world.fx_models));
    let impact_fx = PreparedImpactFx(std::mem::take(&mut prepared.world.impact_fx));
    let tracers = PreparedTracers(std::mem::take(&mut prepared.tracers));
    let scene = match world_scene_from_draw(
        prepared.world,
        prepared.materials,
        &authority_models.installed_owners,
    ) {
        Ok(scene) => scene,
        Err(error) => {
            let gap = format!("match install refused: {error}");
            diag::info!(World, "{gap}");
            return Err(InstallRefusal::new(gap));
        }
    };
    if scene.batches.is_empty() {
        diag::error!(
            World,
            "map load failed: no drawable world; see world report in the game log"
        );
        return Err(InstallRefusal::new(
            "No drawable world. See world report in the game log.",
        ));
    }
    Ok(MatchInstallPlan {
        ui_images: prepared.ui_images,
        mode,
        script_sound_aliases: prepared.script_sound_aliases,
        script_sound_playback_ms: prepared.script_sound_playback_ms,
        sound_mix_names: prepared.sound_mix_names,
        scripts,
        actor_anim_sources,
        script_level,
        script_entries,
        client_scripts,
        compiled_client_scripts,
        gametype_settings,
        script_dvars,
        engine_dvars,
        hosted_dvars,
        account_defaults,
        objective_weapons,
        kind,
        gametype,
        scene,
        weapons,
        killstreaks,
        fpv_meshes,
        bodies,
        soldiers,
        world_weapons,
        projectile_meshes,
        xmodel_walk,
        xanims,
        death,
        player_anim_sources,
        prepared_map,
        strings,
        clip: prepared.clip,
        pen_table: prepared.pen_table,
        pen_table_loaded: prepared.pen_table_loaded,
        lochit_table: prepared.lochit_table,
        tracers,
        fx_catalog,
        type10,
        fx_models,
        impact_fx,
        authority_models,
        model_spawns,
    })
}

pub fn install_script_model_id(content: asset_world::ScriptModelId) -> sim::ScriptModelId {
    sim::ScriptModelId::from_authored_source_ordinal(content.source_ordinal())
}

struct ObjectiveBindings<'a> {
    models: Vec<(&'a str, &'a str)>,
    optional_models: Vec<(&'a str, &'a str)>,
    effects: Vec<(&'a str, &'a str)>,
    weapons: Vec<(&'a str, &'a str)>,
    absent_effects: Vec<String>,
}

fn objective_bindings<'a>(
    host: &'a asset_game::ObjectiveVisuals,
    native: &'a asset_game::ObjectiveVisuals,
    kind: gamemode_iw4::GameModeKind,
    family: asset_core::FamilyId,
) -> Result<ObjectiveBindings<'a>, InstallRefusal> {
    use gamemode_iw4::GameModeKind;
    let pair = |from: &'a Option<String>, to: &'a Option<String>| {
        from.as_deref()
            .filter(|name| !name.is_empty())
            .zip(to.as_deref().filter(|name| !name.is_empty()))
            .ok_or_else(|| {
                InstallRefusal::new("required objective binding missing for the map's game")
            })
    };
    let mut bindings = ObjectiveBindings {
        models: Vec::new(),
        optional_models: host
            .crate_model
            .iter()
            .chain(&host.crate_overlay)
            .zip(native.crate_model.iter().chain(&native.crate_overlay))
            .filter_map(|(from, to)| Some((from.as_deref()?, to.as_deref()?)))
            .collect(),
        effects: Vec::new(),
        weapons: Vec::new(),
        absent_effects: Vec::new(),
    };
    match kind {
        GameModeKind::SearchAndDestroy | GameModeKind::Sabotage | GameModeKind::Demolition => {
            bindings.models.push(pair(&host.bomb, &native.bomb)?);
            bindings
                .effects
                .push(pair(&host.bomb_explosion_fx, &native.bomb_explosion_fx)?);
            bindings
                .weapons
                .push(pair(&host.plant_weapon, &native.plant_weapon)?);
            bindings
                .weapons
                .push(pair(&host.defuse_weapon, &native.defuse_weapon)?);
        }
        GameModeKind::Domination | GameModeKind::CaptureTheFlag => {
            if kind == GameModeKind::Domination {
                bindings
                    .models
                    .push(pair(&host.neutral_flag, &native.neutral_flag)?);
            }
            for index in 0..2 {
                bindings
                    .models
                    .push(pair(&host.flag[index], &native.flag[index])?);
                if kind == GameModeKind::CaptureTheFlag {
                    bindings
                        .models
                        .push(pair(&host.flag_carry[index], &native.flag_carry[index])?);
                }
                if family == asset_core::FamilyId::T6 && native.flag_base_fx[index].is_none() {
                    let from = host.flag_base_fx[index]
                        .as_ref()
                        .ok_or_else(|| InstallRefusal::new("host flag effect binding missing"))?;
                    bindings.absent_effects.push(from.clone());
                } else {
                    bindings.effects.push(pair(
                        &host.flag_base_fx[index],
                        &native.flag_base_fx[index],
                    )?);
                }
            }
        }
        _ => {}
    }
    Ok(bindings)
}

fn objective_weapon_binding(
    registry: &WeaponRegistry,
    family: asset_core::FamilyId,
    script_name: &str,
) -> Option<u32> {
    registry
        .published_weapons()
        .find(|weapon| {
            let id = weapon.wire_id();
            registry.identity_namespace_of(id) == Some(family)
                && registry.script_name_of(id) == script_name
                && [asset_model::ALLIES_KIT, asset_model::AXIS_KIT]
                    .into_iter()
                    .all(|kit| registry.fpv_assemblies_of(id, kit).is_some())
        })
        .map(|weapon| weapon.wire_id())
}

fn player_kit_collision(
    soldier: Option<&asset_game::SoldierPresentation>,
) -> Result<sim::PlayerKitCollision, InstallRefusal> {
    let Some(soldier) = soldier else {
        return Ok(sim::PlayerKitCollision::default());
    };
    let head = soldier.head().map_err(|reason| {
        InstallRefusal::new(format!("soldier head collision refused: {reason}"))
    })?;
    Ok(sim::PlayerKitCollision {
        body_key: soldier.kit().body.clone(),
        body: soldier.body().skel.retained_capability().map(Arc::new),
        head_key: soldier.kit().head.clone().unwrap_or_else(String::new),
        head: head
            .and_then(|head| head.entry().skel.retained_capability())
            .map(Arc::new),
    })
}

fn match_kind(
    selection: Option<&sim::HostGameModeSelection>,
) -> Result<gamemode_iw4::GameModeKind, &'static str> {
    if let Some(selection) = selection {
        return Ok(selection.kind());
    }
    match std::env::var("IW4L_GAMETYPE") {
        Ok(s) if !s.trim().is_empty() => gamemode_iw4::GameModeKind::parse_ascii_ignore_case(&s)
            .ok_or("IW4L_GAMETYPE names no known gametype (gsc.mode.out-of-scope)"),
        _ => Ok(gamemode_iw4::GameModeKind::FreeForAll),
    }
}

fn script_model_spawns(
    instances: &[asset_world::ScriptModelSceneInstance],
) -> Vec<(sim::ScriptModelId, [f32; 3], [f32; 3])> {
    instances
        .iter()
        .map(|instance| {
            let (yaw, pitch, roll) = instance.transform.rotation.to_euler(EulerRot::ZYX);
            (
                install_script_model_id(instance.id),
                instance.transform.translation.to_array(),
                [pitch.to_degrees(), yaw.to_degrees(), roll.to_degrees()],
            )
        })
        .collect()
}

fn spawn_script_model_movers(
    sim: &mut sim::SimWorld,
    models: &[(sim::ScriptModelId, [f32; 3], [f32; 3])],
) {
    for (id, origin, angles) in models {
        sim.spawn_script_mover(*id, *origin, *angles)
            .expect("no free dynamic entity slot for a script model");
    }
}

fn stamp_script_mover_numbers(scene: &mut WorldScene, sim: &sim::SimWorld) {
    for instance in &mut scene.script_model_instances {
        let id = install_script_model_id(instance.id);
        instance.gentity_number = sim.gentity_number(id).and_then(|n| u16::try_from(n).ok());
    }
}

fn map_anim_items(
    slots: &[(u8, u8, Vec<asset_anim::ParsedAnimItem>)],
    tree: Option<&asset_anim::CompiledAnimTreeDefinition>,
    catalog: &asset_anim::XAnimCatalog,
    namespace: asset_core::AssetNamespace,
) -> Vec<(u8, u8, Vec<sim::AnimScriptItem>)> {
    slots
        .iter()
        .map(|(state, movetype, items)| {
            (
                *state,
                *movetype,
                items
                    .iter()
                    .map(|item| map_script_item(item, tree, catalog, namespace))
                    .collect(),
            )
        })
        .collect()
}

fn map_event_items(
    events: &[(u8, Vec<asset_anim::ParsedAnimItem>)],
    tree: Option<&asset_anim::CompiledAnimTreeDefinition>,
    catalog: &asset_anim::XAnimCatalog,
    namespace: asset_core::AssetNamespace,
) -> Vec<(u8, Vec<sim::AnimScriptItem>)> {
    events
        .iter()
        .map(|(event, items)| {
            (
                *event,
                items
                    .iter()
                    .map(|item| map_script_item(item, tree, catalog, namespace))
                    .collect(),
            )
        })
        .collect()
}

fn map_script_item(
    item: &asset_anim::ParsedAnimItem,
    tree: Option<&asset_anim::CompiledAnimTreeDefinition>,
    catalog: &asset_anim::XAnimCatalog,
    namespace: asset_core::AssetNamespace,
) -> sim::AnimScriptItem {
    let commands: Vec<_> = item
        .commands
        .iter()
        .filter(|cmd| {
            tree.and_then(|t| t.node(cmd.anim_index))
                .is_none_or(|node| {
                    catalog
                        .get(namespace, &node.name)
                        .is_some_and(|c| !c.parts.names.is_empty())
                })
        })
        .map(|cmd| sim::AnimScriptCommand {
            body_part: cmd.body_part,
            anim_index: cmd.anim_index,
            duration_ms: command_duration_ms(tree, catalog, cmd, namespace),
        })
        .collect();
    sim::AnimScriptItem {
        skip: item.skip || commands.is_empty(),
        conditions: item
            .conditions
            .iter()
            .map(|c| sim::AnimScriptCondition {
                index: c.index,
                bitflags: c.bitflags,
                bits: c.bits,
                value: c.value,
            })
            .collect(),
        commands,
    }
}

fn command_duration_ms(
    tree: Option<&asset_anim::CompiledAnimTreeDefinition>,
    catalog: &asset_anim::XAnimCatalog,
    cmd: &asset_anim::ParsedAnimCommand,
    namespace: asset_core::AssetNamespace,
) -> i32 {
    if let Some(ms) = cmd.duration_ms {
        return if ms <= 0 { 500 } else { ms };
    }
    let Some(node) = tree.and_then(|t| t.node(cmd.anim_index)) else {
        return 500;
    };
    let Some(captured) = catalog.get(namespace, &node.name) else {
        return 500;
    };
    if captured.parts.framerate > 0.0 {
        let ms = (captured.parts.numframes as f32 / captured.parts.framerate * 1000.0) as i32;
        if ms <= 0 { 500 } else { ms }
    } else {
        500
    }
}

fn log_destructible_death_assets(death: &PreparedDestructibleDeath) {
    for row in &death.0 {
        diag::info!(
            World,
            "destructible death clip {}: {}",
            row.clip_hint,
            row.clip.edge_kind()
        );
        diag::info!(
            World,
            "destructible death husk {}: {}",
            row.husk_hint,
            row.husk.edge_kind()
        );
    }
}

struct AuthorityEntityModelInstall {
    capabilities: Vec<sim::EntityCollisionCapabilities>,
    brush_movers: Vec<(sim::ScriptModelId, u32, [f32; 3], [f32; 3])>,
    models: std::collections::BTreeMap<
        String,
        Option<std::sync::Arc<xmodel_runtime::RetainedModelCapability>>,
    >,
    installed_owners: Vec<(asset_world::ScriptModelId, sim::AuthorityModelOwner)>,
    ambiguous_brush_links: usize,
    standalone_brush_links: usize,
}

fn retained(
    asset: Option<&asset_world::MapXModelSceneAsset>,
) -> Option<std::sync::Arc<xmodel_runtime::RetainedModelCapability>> {
    retained_with_preset(asset, None)
}

/// The model's capability with its physics preset, for the server's
/// launched bodies.
fn retained_with_preset(
    asset: Option<&asset_world::MapXModelSceneAsset>,
    preset: Option<&asset_world::OwnedPhysPreset>,
) -> Option<std::sync::Arc<xmodel_runtime::RetainedModelCapability>> {
    match asset {
        Some(
            asset_world::MapXModelSceneAsset::Iw4(model)
            | asset_world::MapXModelSceneAsset::Iw5(model)
            | asset_world::MapXModelSceneAsset::T6(model)
            | asset_world::MapXModelSceneAsset::T5(model),
        ) => model.retained_capability().map(|mut capability| {
            capability.phys_preset = preset.map(|preset| xmodel_runtime::PhysPreset {
                mass: preset.mass,
                bounce: preset.bounce,
                friction: preset.friction,
                bullet_force_scale: preset.bullet_force_scale,
                gravity_scale: preset.gravity_scale,
                center_of_mass_offset: preset.center_of_mass_offset,
            });
            std::sync::Arc::new(capability)
        }),
        Some(asset_world::MapXModelSceneAsset::Unavailable { .. }) | None => None,
    }
}

fn authority_entity_model_install(world: &assets::PreparedWorld) -> AuthorityEntityModelInstall {
    let mut installed_owners = Vec::new();
    let mut ambiguous_brush_links = 0;
    let mut capabilities: Vec<sim::EntityCollisionCapabilities> = world
        .script_model_instances
        .iter()
        .map(|instance| {
            let sim_id = install_script_model_id(instance.id);
            let owner = sim::AuthorityModelOwner::ScriptModel(sim_id);
            installed_owners.push((instance.id, owner));
            if matches!(
                instance.metadata.brush_link,
                asset_world::ScriptBrushModelLink::Ambiguous { .. }
            ) {
                ambiguous_brush_links += 1;
            }
            let mut dobj = sim::AuthorityDObjState::new_dirty(
                instance.current_model.0.clone(),
                retained(world.map_xmodel_scene_assets.get(&instance.current_model)),
                instance.transform.to_matrix(),
            );
            dobj.semantic_state.hide_part_bits = instance.dobj_state.hide_part_bits;
            dobj.pose_request.hide_part_bits = instance.dobj_state.hide_part_bits;
            if let Some(definition) = &instance.metadata.t5_destructible {
                sim::t5_destructible::install(&mut dobj, definition.clone());
            }
            sim::EntityCollisionCapabilities::current_tick(owner, Some(dobj), Vec::new())
        })
        .collect();
    let mut brush_movers = Vec::new();
    for brush in &world.script_brush_models {
        let sim_id = install_script_model_id(asset_world::ScriptModelId::from_source_ordinal(
            brush.source_ordinal,
        ));
        capabilities.push(sim::EntityCollisionCapabilities::current_tick(
            sim::AuthorityModelOwner::ScriptModel(sim_id),
            None,
            vec![sim::LinkedBrushCollisionBrush {
                cmodel_handle: brush.cmodel_handle,
                origin: brush.origin,
                angles: brush.angles,
            }],
        ));
        brush_movers.push((sim_id, brush.cmodel_handle, brush.origin, brush.angles));
    }
    let standalone_brush_links = brush_movers.len();
    let models = world
        .map_xmodel_scene_assets
        .iter()
        .map(|(key, asset)| {
            let preset = world.map_xmodel_scene_assets.phys_preset(key);
            (key.0.clone(), retained_with_preset(Some(asset), preset))
        })
        .collect();
    AuthorityEntityModelInstall {
        capabilities,
        brush_movers,
        models,
        installed_owners,
        ambiguous_brush_links,
        standalone_brush_links,
    }
}

/// What the two runtime owners of the map collision are given: one immutable
/// backing they share, and the static models only the sim traces against.
fn install_clip_and_player(
    sim: &mut sim::SimWorld,
    mut content: sim::SimContentBuilder,
    clip: Option<Arc<ClipCollision>>,
    authority_models: AuthorityEntityModelInstall,
    intermission_view: Option<sim::AuthoredSpawnPoint>,
    airstrike_height: Option<f32>,
    spawns_in: &[SpawnPoint],
    weapons: &WeaponRegistry,
    sim_weapons: &weapon_content::PreparedSimWeapons,
    sim_cam: &mut SimCamera,
    input_gate: &mut AuthorityInputGate,
    host_classes: Option<&HostClassLoadouts>,
    kind: gamemode_iw4::GameModeKind,
    mode: game_api::ModeRules,
    allow_debug_actions: bool,
    rules: &[(String, String)],
    gametype: &str,
) -> Result<(&'static str, Vec<Option<String>>), InstallRefusal> {
    if !sim_weapons.owned_by(weapons) || !content.uses_weapons(sim_weapons.content()) {
        return Err(InstallRefusal::new(
            "Simulation weapon registry owner mismatch",
        ));
    }
    let clip = clip.ok_or_else(|| InstallRefusal::new("Required collision geometry is missing"))?;
    let static_models = &clip.static_models;
    let count = clip.brushes.len();
    let node_count = clip.nodes.len();
    let leaf_count = clip.leaves.len();
    let vert_count = clip.mesh.verts.len();
    let tri_count = clip.mesh.tri_count();
    let smodel_count = static_models.len();
    let brushes: Vec<sim::SimBrush> = clip
        .brushes
        .iter()
        .map(|brush| sim::SimBrush {
            planes: brush.planes.clone(),
            contents: brush.contents,
            plane_surface_flags: brush.plane_surface_flags.clone(),
            glass_encoded: brush.glass_encoded,
        })
        .collect();
    if count == 0 && tri_count == 0 {
        return Err(InstallRefusal::new("Required collision geometry is empty"));
    }
    let bsp = sim::SimClipBsp {
        nodes: clip
            .nodes
            .iter()
            .map(|n| clipmap_iw4::ClipNode {
                plane: n.plane,
                children: n.children,
            })
            .collect(),
        leaves: clip
            .leaves
            .iter()
            .map(|l| clipmap_iw4::ClipLeaf {
                first_brush: l.first_brush,
                num_brushes: l.num_brushes,
                first_coll_aabb_index: l.first_coll_aabb_index,
                coll_aabb_count: l.coll_aabb_count,
            })
            .collect(),
        leafbrushes: clip.leafbrushes.clone(),
    };
    let mesh = sim::SimClipMesh {
        tables: Arc::clone(&clip.mesh),
        static_models: static_models
            .iter()
            .map(|sm| sim::SimStaticModel {
                index: sm.index,
                name: sm.name.clone(),
                model: sm.model.clone(),
            })
            .collect(),
        ..Default::default()
    };
    let cmodel_count = clip.cmodels.len();
    let cmodels = sim::SimClipCmodels {
        models: clip
            .cmodels
            .iter()
            .map(|c| clipmap_iw4::ClipCmodel {
                mins: c.mins,
                maxs: c.maxs,
                radius: c.radius,
                first_brush: c.first_brush,
                num_brushes: c.num_brushes,
            })
            .collect(),
        triggers: clip
            .trigger_models
            .iter()
            .map(|hulls| {
                hulls
                    .iter()
                    .map(|h| sim::SimTriggerHull {
                        mid: h.mid,
                        half: h.half,
                        slabs: h.slabs.clone(),
                    })
                    .collect()
            })
            .collect(),
    };
    content.set_clip_map(brushes, bsp, mesh, cmodels);
    content.set_clip_hulls(clip.hulls.clone());
    sim.install_content(content.finish());
    sim.install_model_library(authority_models.models);
    let owner_count = authority_models.capabilities.len();
    let linked_brushes = authority_models
        .capabilities
        .iter()
        .map(|capabilities| capabilities.linked_brushes.len())
        .sum::<usize>();
    sim.install_entity_collision_capabilities(authority_models.capabilities);
    sim::phase_materialize_entity_dobjs(sim);
    let collision_bones = sim
        .entity_collision_capabilities()
        .iter()
        .filter_map(|capabilities| capabilities.dobj.as_ref())
        .filter_map(|state| state.current_collision.as_ref())
        .map(|geometry| geometry.bones.len())
        .sum::<usize>();
    diag::info!(
        Sim,
        "spawn: {} script collision owners, {} nonzero XBoneInfo OBBs, {} linked brush cmodels, {} ambiguous brush links, {} standalone brush cmodels",
        owner_count,
        collision_bones,
        linked_brushes,
        authority_models.ambiguous_brush_links,
        authority_models.standalone_brush_links,
    );

    diag::info!(
        Sim,
        "spawn: {} clip brushes, {} BSP nodes / {} leaves, {} verts / {} tris, {} cmodels, {} smodels",
        count,
        node_count,
        leaf_count,
        vert_count,
        tri_count,
        cmodel_count,
        smodel_count
    );

    let spawns: Vec<sim::AuthoredSpawnPoint> = spawns_in
        .iter()
        .map(|p| sim::AuthoredSpawnPoint {
            classname: p.classname.clone(),
            origin: p.origin,
            angles: p.angles,
            script_linkto: p.script_linkto.clone(),
            script_destructable_area: p.script_destructable_area.clone(),
        })
        .collect();
    let rows = bootstrap_class_rows(host_classes);
    let projected: Vec<AuthoritativeClassProjection> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| project_class(index as u32, row, weapons, sim_weapons))
        .collect();
    let lock_reasons: Vec<Option<String>> = projected
        .iter()
        .map(|row| row.lock_reason.clone())
        .collect();
    let classes: Vec<sim::ClassDef> = projected.into_iter().map(|row| row.def).collect();
    let bot_classes: Vec<sim::ClassDef> = frame::showcase_classes()
        .iter()
        .enumerate()
        .map(|(index, preset)| {
            let row = ClassRow::from(&preset.into());
            project_class(index as u32, &row, weapons, sim_weapons).def
        })
        .filter(|class| !class.locked)
        .collect();
    let locked_n = lock_reasons.iter().filter(|r| r.is_some()).count();
    let has_intermission_view = intermission_view.is_some();
    let unlimited = match mode.unlimited {
        game_api::Rule::Known(unlimited) => unlimited,
        game_api::Rule::Unknown(gap) => {
            diag::info!(Sim, "game gap {}: {}", gap.id, gap.what);
            true
        }
    };
    let config_rule = |suffix: &str| {
        mode.limits_from_config
            .then(|| {
                rules
                    .iter()
                    .rev()
                    .find(|(key, _)| key == &format!("scr_{gametype}_{suffix}"))
            })
            .flatten()
            .map(|(_, value)| value.as_str())
    };
    let config_score = config_rule("scorelimit")
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|value| *value >= 0);
    let config_time = config_rule("timelimit")
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && (0.0..=10000.0).contains(value))
        .map(|minutes| (minutes * 60000.0).round() as u32);
    if let Err(err) = sim.bootstrap(sim::MatchBootstrap {
        spawns,
        classes,
        bot_classes,
        seed: 0,
        kind,
        mode: Some(mode),
        score_limit: if unlimited {
            0
        } else {
            std::env::var("IW4L_SCORE_LIMIT")
                .ok()
                .and_then(|s| s.parse().ok())
                .or(config_score)
                .or(mode.default_score_limit)
                .unwrap_or(match kind {
                    gamemode_iw4::GameModeKind::Domination => gamemode_iw4::dom::SCORE_LIMIT,
                    gamemode_iw4::GameModeKind::Demolition => 0,
                    _ => sim::FFA.score_limit,
                })
        },
        time_limit_ms: if unlimited {
            0
        } else {
            config_time.unwrap_or(match kind {
                gamemode_iw4::GameModeKind::Domination => gamemode_iw4::dom::TIME_LIMIT_MS,
                gamemode_iw4::GameModeKind::Demolition => gamemode_iw4::dd::TIME_LIMIT_MS,
                _ => sim::FFA.time_limit_ms,
            })
        },
        allow_debug_actions,
        intermission_view,
        airstrike_height,
    }) {
        diag::info!(Sim, "bootstrap: {err}");
        return Err(InstallRefusal::new(format!(
            "MatchBootstrap refused: {err}"
        )));
    }

    sim_cam.enabled = false;
    input_gate.local_cmds_enabled = false;
    sim_cam.freeze_fly = has_intermission_view;
    diag::info!(
        Sim,
        "spawn: match bootstrapped with {} clip brushes and {} dm spawns ({} / {} classes Locked by coverage)",
        count,
        spawns_in.len(),
        locked_n,
        rows.len()
    );
    let gap = if has_intermission_view {
        "player awaits SelectClass; mp_global_intermission holds the camera until Equip ack"
    } else {
        "player awaits SelectClass; Equip ack enables sim camera (no authored intermission)"
    };
    Ok((gap, lock_reasons))
}

pub(crate) fn bootstrap_class_rows(host: Option<&HostClassLoadouts>) -> Vec<ClassRow> {
    let fallback = HostClassLoadouts::default();
    let host = host.filter(|h| !h.slots.is_empty()).unwrap_or(&fallback);
    host.slots.iter().map(ClassRow::from).collect()
}

fn install_shocks(
    world: &mut sim::SimContentBuilder,
    family: asset_core::FamilyId,
    catalog: Option<&asset_game::MenuCatalog>,
    map_shocks: &[(String, String)],
) {
    let format = match crate::games::vision(family).shellshock() {
        game_api::Rule::Known(format) => format,
        game_api::Rule::Unknown(gap) => {
            diag::info!(Zone, "game gap {}: {}", gap.id, gap.what);
            world.set_shocks(std::collections::BTreeMap::new());
            return;
        }
    };
    let common = catalog
        .filter(|catalog| catalog.namespace == Some(family))
        .into_iter()
        .flat_map(|catalog| {
            catalog.rawfiles.iter().filter_map(|(path, text)| {
                let lower = path.to_ascii_lowercase();
                let name = lower
                    .strip_prefix("shock/")?
                    .strip_suffix(".shock")?
                    .to_owned();
                Some((name, text.as_str()))
            })
        });
    let map = map_shocks
        .iter()
        .map(|(name, text)| (name.clone(), text.as_str()));
    let mut shocks = std::collections::BTreeMap::new();
    let mut gaps = std::collections::BTreeMap::new();
    for (name, text) in common.chain(map) {
        let parsed = match format {
            game_api::ShockFormat::ModernWarfare2 => ShockParams::parse(text),
            game_api::ShockFormat::Game { parse, .. } => parse(text).map(|file| {
                gaps.insert(name.clone(), file.unrun.clone());
                game_shock(&file)
            }),
        };
        match parsed {
            Ok(params) => {
                shocks.insert(name, params);
            }
            Err(error) => diag::warn!(Zone, "shock/{name}.shock: {error}"),
        }
    }
    diag::info!(Zone, "shellshocks: {:?}", shocks.keys().collect::<Vec<_>>());
    world.set_shocks(shocks);
    if let game_api::ShockFormat::Game { fallback, .. } = format {
        world.set_game_shocks(fallback, gaps);
    }
}

/// A game's own shock file in the shared screen's terms. Its sound is the
/// game's own timed sound, not the shared shellshock loop.
fn game_shock(file: &game_api::ShockFile) -> ShockParams {
    ShockParams {
        screen_type: match file.screen {
            game_api::ShockScreen::Blurred => SCREEN_BLEND_BLURRED,
            game_api::ShockScreen::Flashed => SCREEN_BLEND_FLASHED,
            game_api::ShockScreen::None => SCREEN_BLEND_NONE,
        },
        blur_blend_ms: file.blur_blend_ms,
        blur_fade_ms: file.blur_fade_ms,
        white_fade_ms: file.white_fade_ms,
        shot_fade_ms: file.shot_fade_ms,
        look: ShellshockLookParms {
            affect: file.look.affect,
            fade_ms: file.look.fade_ms,
            mouse_sensitivity: file.look.mouse_sensitivity,
            max_pitch_speed: file.look.max_pitch_speed,
            max_yaw_speed: file.look.max_yaw_speed,
            reciprocal_fade: file.look.reciprocal_fade,
        },
        sound: ShellshockSoundParms {
            affect: false,
            channel_volumes: None,
            loop_alias: String::new(),
            end_alias: String::new(),
            abort_alias: String::new(),
        },
        movement: false,
        movement_scale: file.movement_scale,
        animation: file.animation,
        kick: file.kick.map(|kick| ShellshockKick {
            fade_ms: kick.fade_ms,
            rate: kick.rate,
            radius: kick.radius,
            ordered_sums: kick.ordered_sums,
        }),
        timed_sound: file.sound.as_ref().map(|sound| ShellshockTimedSound {
            loop_alias: sound.loop_alias.clone(),
            end_alias: sound.end_alias.clone(),
            fade_in_ms: sound.fade_in_ms,
            fade_out_ms: sound.fade_out_ms,
            mod_end_delay_ms: sound.mod_end_delay_ms,
            room: sound.room.clone(),
            dry: sound.dry,
            wet: sound.wet,
            snapshot: sound.snapshot.clone(),
        }),
        vision: file.vision.as_ref().map(|vision| ShellshockVision {
            name: vision.name.clone(),
            in_ms: vision.in_ms,
            out_ms: vision.out_ms,
        }),
    }
}

fn charsets(arena_text: Option<&str>, zone: &str) -> (String, String) {
    let row = arena_text.and_then(|text| asset_game::arena_charsets(text, zone));
    let allies = row
        .as_ref()
        .and_then(|r| r.allieschar.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| gamemode_iw4::DEFAULT_ALLIES_CHARSET.to_owned());
    let axis = row
        .as_ref()
        .and_then(|r| r.axischar.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| gamemode_iw4::DEFAULT_AXIS_CHARSET.to_owned());
    (allies, axis)
}

fn iw4_charsets(
    catalog: &asset_game::MenuCatalog,
    identity: Option<&LaunchIdentity>,
    zone: &str,
) -> (String, String) {
    let arena_text = catalog
        .rawfile_text("mp/basemaps.arena")
        .map(str::to_owned)
        .or_else(|| identity.and_then(|id| asset_game::read_basemaps_arena(&id.games_root)));
    charsets(arena_text.as_deref(), zone)
}

fn install_team_voice_prefixes(
    world: &mut sim::SimContentBuilder,
    catalog: Option<&asset_game::MenuCatalog>,
    identity: Option<&LaunchIdentity>,
    zone: &str,
) {
    let Some(catalog) = catalog else {
        return;
    };
    let Some(table) = catalog.string_table(gamemode_iw4::FACTION_TABLE) else {
        return;
    };
    let arena_text = catalog
        .rawfile_text("mp/basemaps.arena")
        .map(str::to_owned)
        .or_else(|| identity.and_then(|id| asset_game::read_basemaps_arena(&id.games_root)));
    if let Some(entry) = arena_text
        .as_deref()
        .and_then(|text| asset_game::arena_entry(text, zone))
    {
        world.set_map_custom(entry);
    }
    let (allies_cs, axis_cs) = charsets(arena_text.as_deref(), zone);
    let allies = table.lookup_col(&allies_cs, gamemode_iw4::FACTION_VOICE_PREFIX_COL);
    let axis = table.lookup_col(&axis_cs, gamemode_iw4::FACTION_VOICE_PREFIX_COL);
    world.set_team_voice_prefixes(
        (!allies.is_empty()).then(|| allies.to_owned()),
        (!axis.is_empty()).then(|| axis.to_owned()),
    );
}

pub fn register_match_apply_systems(app: &mut App) {
    app.init_resource::<StartupCommands>()
        .init_resource::<PendingConsoleLines>()
        .init_resource::<HostClassLoadouts>()
        .add_systems(
            Update,
            apply_prepared_match
                .after(frame::SessionSwapApplied)
                .run_if(resource_exists::<PreparedMatchReady>)
                .in_set(ClientSet::Load),
        );
}

fn stage_resource<T: Resource>(queue: &mut bevy::ecs::world::CommandQueue, resource: T) {
    queue.push(move |world: &mut World| {
        frame::scope::stage(world, resource);
    });
}
