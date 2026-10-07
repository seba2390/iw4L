use crate::frame::FrameWorld;
use crate::identities::MatchPhase;
use crate::script::{Realm, Runtime};
use crate::{ClientLifecycle, Tick};
use bevy_ecs::prelude::World;

const RESPAWN_DELAY_MS: u32 = 2_000;

pub(crate) fn active(world: &World) -> bool {
    world
        .resource::<Runtime>()
        .program
        .as_ref()
        .is_some_and(|p| p.rules() == Realm::T6)
}

fn respawn_due(lifecycle: ClientLifecycle, dead_since: Option<u32>, tick: Tick) -> bool {
    match lifecycle {
        ClientLifecycle::Connecting | ClientLifecycle::ChoosingClass => true,
        ClientLifecycle::Dead => dead_since.is_some_and(|since| {
            tick.0.saturating_sub(since) >= RESPAWN_DELAY_MS / crate::MATCH_TICK_MS
        }),
        _ => false,
    }
}

pub(crate) fn advance(world: &mut World) {
    let tick = world.resource::<crate::step::StepRequest>().tick;
    if world
        .resource::<Runtime>()
        .t6_match_ended
        .is_some_and(|ended| tick.0.saturating_sub(ended.0) >= 100)
    {
        world.resource_mut::<Runtime>().pending_restart = Some(false);
        super::restart::restart_level(world, tick);
        return;
    }
    let mut frame = FrameWorld::from_world(world);
    if frame.bootstrap_ref().kind != gamemode_iw4::GameModeKind::FreeForAll {
        return;
    }
    if frame.phase() == MatchPhase::Warmup {
        for id in frame.client_ids_sorted() {
            let meta = frame.client_meta_mut(id);
            meta.score = 0;
            meta.kills = 0;
            meta.deaths = 0;
        }
        let limit = frame.bootstrap_ref().time_limit_ms;
        let runtime = frame.ecs().resource_mut::<Runtime>().into_inner();
        runtime.t6_match_started = Some(tick);
        runtime.engine.game_end_time = if limit == 0 {
            0
        } else {
            crate::level_time_ms(tick).saturating_add(limit.min(i32::MAX as u32) as i32)
        };
        frame.set_phase(MatchPhase::Playing);
        diag::info!(Sim, "runtime profile=t6 gametype=dm match started");
    }
    if frame.phase() != MatchPhase::Playing {
        return;
    }
    let started = frame
        .ecs()
        .resource::<Runtime>()
        .t6_match_started
        .unwrap_or(tick);
    frame.set_match_elapsed_ms(
        tick.0
            .saturating_sub(started.0)
            .saturating_mul(crate::MATCH_TICK_MS),
    );
    let limit = frame.bootstrap_ref().score_limit;
    let winner = frame
        .clients_scoreboard()
        .into_iter()
        .max_by_key(|(id, meta)| (meta.score, std::cmp::Reverse(id.0)));
    let reason = if limit > 0 && winner.as_ref().is_some_and(|(_, m)| m.score >= limit) {
        Some(crate::MatchEndReason::ScoreLimit)
    } else if frame.bootstrap_ref().time_limit_ms > 0
        && frame.match_elapsed_ms() >= frame.bootstrap_ref().time_limit_ms
    {
        Some(crate::MatchEndReason::TimeLimit)
    } else {
        None
    };
    if let Some(reason) = reason {
        frame.set_game_win_winner(winner.map(|(id, _)| id));
        frame.set_phase(MatchPhase::Intermission);
        frame.push_event(
            tick,
            crate::EventAudience::All,
            crate::SimEvent::MatchEnded { reason },
        );
        for id in frame.client_ids_sorted() {
            frame.client_meta_mut(id).lifecycle = ClientLifecycle::Intermission;
            if let Some(ps) = frame.player_mut(id) {
                ps.pm_type = playerstate_iw4::PM_TYPE_INTERMISSION;
            }
        }
        frame.ecs().resource_mut::<Runtime>().t6_match_ended = Some(tick);
        diag::info!(Sim, "t6 match ended reason={reason:?}");
        return;
    }
    for id in frame.client_ids_sorted() {
        let Some(meta) = frame.client_meta(id) else {
            continue;
        };
        if !respawn_due(meta.lifecycle, meta.dead_since_tick, tick) {
            continue;
        }
        let life = meta.life_sequence;
        let seed = frame
            .root_seed()
            .wrapping_add(u64::from(id.0))
            .wrapping_add(u64::from(tick.0));
        let avoid: Vec<_> = frame
            .client_ids_sorted()
            .into_iter()
            .filter(|other| *other != id)
            .filter(|other| {
                frame
                    .client_meta(*other)
                    .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
            })
            .filter_map(|other| frame.player(other).map(|ps| ps.origin))
            .collect();
        let report = crate::spawn::decide_forced_spawn(
            &frame,
            crate::SpawnPick::Seeded(seed),
            &avoid,
            entity_iw4::TEAM_FREE,
        );
        let Some(spawn) = report.accepted else {
            if tick.0 % 100 == 0 {
                diag::info!(
                    Sim,
                    "t6 spawn refused client={} tried={} rejected={:?}",
                    id.0,
                    report.tried,
                    report.rejected
                );
            }
            continue;
        };
        let candidates: Vec<_> = frame
            .weapon_script_names()
            .iter()
            .enumerate()
            .skip(1)
            .filter_map(|(index, _)| {
                let index = u32::try_from(index).ok()?;
                let setup = frame.weapon_setup(index)?;
                let facts = frame.combat_facts_for(index)?;
                (setup.realm == Realm::T6
                    && setup.attachments.is_empty()
                    && frame.weapon_runnable(index)
                    && !frame.weapon_is_melee_only(index)
                    && frame.equipment_facts_for(index).is_none()
                    && facts.weap_type == weapon_iw4::WEAPTYPE_BULLET)
                    .then_some(index)
            })
            .collect();
        let selected = {
            let runtime = frame.ecs().resource::<Runtime>();
            runtime
                .selected_classes
                .get(&id.0)
                .and_then(|class| runtime.personal_classes.get(&(id.0, *class)).cloned())
        };
        let (weapon, secondary) = if let Some(class) = &selected {
            (
                Some(class.primary),
                (class.secondary != 0).then_some(class.secondary),
            )
        } else {
            let primary = candidates.first().copied();
            let secondary = candidates.iter().copied().find(|weapon| {
                Some(*weapon) != primary
                    && frame
                        .combat_facts_for(*weapon)
                        .is_some_and(|facts| facts.weap_class == weapon_iw4::WEAPCLASS_PISTOL)
            });
            (primary, secondary)
        };
        frame.client_meta_mut(id).client_state_team = entity_iw4::TEAM_FREE;
        frame.client_meta_mut(id).loadout = selected.as_ref().map(|class| crate::LoadoutSpec {
            class_id: class.id,
            revision: class.revision,
            primary: class.primary,
            secondary: class.secondary,
            primary_attachments: class.primary_attachments,
            secondary_attachments: class.secondary_attachments,
            lethal: class.lethal,
            tactical: class.tactical,
            perks: class.perks,
        });
        crate::script_player::spawn(
            &mut frame,
            tick,
            id,
            spawn.traced_origin,
            spawn.raw_angles,
            "playing",
        );
        if let Some(class) = &selected {
            for offhand in [class.lethal, class.tactical]
                .into_iter()
                .filter(|id| *id != 0)
            {
                if let Err(reason) =
                    crate::script_player::give_weapon(&mut frame, id, offhand, false)
                {
                    diag::info!(
                        Sim,
                        "t6 offhand equip refused client={} reason={reason}",
                        id.0
                    );
                    continue;
                }
                if let Some(eq) = frame.equipment_facts_for(offhand)
                    && let Some(ps) = frame.player_mut(id)
                {
                    match eq.offhand_class {
                        1 | 4 | 5 => ps.offhand_primary = eq.offhand_class,
                        2 | 3 => ps.offhand_secondary = eq.offhand_class,
                        _ => {}
                    }
                }
            }
        }
        if let Some(weapon) = weapon {
            if let Some(secondary) = secondary {
                if let Err(reason) =
                    crate::script_player::give_weapon(&mut frame, id, secondary, false)
                {
                    diag::info!(
                        Sim,
                        "t6 secondary equip refused client={} reason={reason}",
                        id.0
                    );
                }
            }
            match crate::script_player::give_weapon(&mut frame, id, weapon, false) {
                Ok(()) => {
                    let _ = crate::script_player::set_spawn_weapon(&mut frame, id, weapon);
                    diag::info!(
                        Sim,
                        "t6 weapon equipped client={} weapon={}",
                        id.0,
                        frame.weapon_script_name(weapon)
                    );
                }
                Err(reason) => diag::info!(
                    Sim,
                    "t6 weapon equip refused client={} reason={reason}",
                    id.0
                ),
            }
        } else {
            diag::info!(
                Sim,
                "t6 weapon equip refused client={} reason=no runnable native bullet weapon",
                id.0
            );
        }
        let runtime = frame.ecs().resource_mut::<Runtime>().into_inner();
        if let Some(slot) = runtime.players.get_mut(&id.0) {
            slot.sessionstate = "playing".into();
            slot.begun = true;
        }
        diag::info!(
            Sim,
            "t6 player spawned authority entity={} source={} origin={:?} angles={:?} previous_life={:?}",
            id.0,
            spawn.source_index,
            spawn.traced_origin,
            spawn.raw_angles,
            life
        );
    }
}

pub(crate) fn damage(world: &mut World, tick: Tick, hit: &crate::script_player::Hit) {
    let mut frame = FrameWorld::from_world(world);
    if hit.amount <= 0 || frame.phase() != MatchPhase::Playing {
        return;
    }
    if crate::script_player::finish_damage(&mut frame, hit.victim, hit.amount, Some(hit.dir))
        == crate::script_player::Finish::Killed
    {
        crate::script_player::kill(&mut frame, tick, hit.victim, hit.attacker, hit.commit);
        frame.client_meta_mut(hit.victim).deaths += 1;
        if let Some(attacker) = hit.attacker.filter(|id| *id != hit.victim)
            && frame.client_meta(attacker).is_some()
        {
            let meta = frame.client_meta_mut(attacker);
            meta.kills += 1;
            meta.score += 1;
        }
        crate::script_player::obituary(
            &mut frame,
            tick,
            hit.victim,
            hit.attacker,
            hit.weapon,
            hit.means,
        );
        for id in [Some(hit.victim), hit.attacker].into_iter().flatten() {
            if let Some(meta) = frame.client_meta(id) {
                let (score, kills, deaths) = (meta.score, meta.kills, meta.deaths);
                frame.push_event(
                    tick,
                    crate::EventAudience::All,
                    crate::SimEvent::ScoreChanged {
                        client: id,
                        score,
                        kills,
                        deaths,
                    },
                );
            }
        }
        if let Some(slot) = frame
            .ecs()
            .resource_mut::<Runtime>()
            .players
            .get_mut(&hit.victim.0)
        {
            slot.sessionstate = "dead".into();
        }
        diag::info!(
            Sim,
            "t6 player killed victim={} attacker={:?}",
            hit.victim.0,
            hit.attacker
        );
    }
}
