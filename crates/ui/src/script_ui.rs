use crate::layers::{GameUiFont, UiLayer, UiLayerVisibility, game_text_font};
use assets::{PreparedLocalizedStrings, PreparedWeapons, SessionCompass, SessionMapIdentity};
use bevy::prelude::*;
use game_api::ui::{
    Content, Draw, Font, Input, OffhandSlot, Player, ScriptUi, Table, UiEvent, UiRequest,
};
use net::{ClientActionInput, FrameClock, LocalPresentClient, PresentedSnapshot};
use std::collections::{HashMap, HashSet};

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ScriptUiPaint;

#[derive(Component)]
struct ScriptRoot;

#[derive(Resource, Default)]
pub(crate) struct ScriptHud {
    publication: Option<u64>,
    front_end: bool,
    scene: Option<Box<dyn ScriptUi>>,
    root: Option<Entity>,
    slots: HashMap<u32, (Entity, Entity, Draw)>,
    map_images: HashMap<u32, Entity>,
    globes: HashMap<u32, Handle<super::t6_globe::GlobeMaterial>>,
    input: Input,
    client_field_serial: u32,
    lui_notify_serial: u32,
    last_cursor: Option<Vec2>,
    logged_texts: Vec<String>,
    pub(crate) active: bool,
    pub(crate) scoreboard: bool,
}

pub(crate) fn register(app: &mut App) {
    super::t6_globe::register(app);
    app.init_resource::<ScriptHud>().add_systems(
        Update,
        refresh
            .in_set(frame::ClientSet::Ui)
            .in_set(ScriptUiPaint)
            .after(super::t6_text::NativeUiPaint),
    );
}

fn node(rectangle: [f32; 4]) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(rectangle[0]),
        top: Val::Px(rectangle[1]),
        width: Val::Px((rectangle[2] - rectangle[0]).max(0.0)),
        height: Val::Px((rectangle[3] - rectangle[1]).max(0.0)),
        ..default()
    }
}

fn load_fonts(namespace: asset_core::AssetNamespace) -> std::sync::Arc<[Font]> {
    [
        "normalFont",
        "bigFont",
        "smallFont",
        "boldFont",
        "consoleFont",
        "extraSmallFont",
        "extraBigFont",
        "italicFont",
        "smallItalicFont",
        "distFont",
        "smallDevFont",
        "bigDevFont",
    ]
    .into_iter()
    .filter_map(|name| {
        let name = if matches!(name, "distFont" | "smallDevFont" | "bigDevFont") {
            format!("fonts/{name}")
        } else {
            format!("fonts/720/{name}")
        };
        let font = asset_material::ui_font::ui_font(namespace, &name)?;
        Some(Font {
            name,
            height: font.pixel_height as f32,
            advances: font
                .glyphs
                .iter()
                .map(|(code, glyph)| (*code, glyph.dx as f32))
                .collect(),
        })
    })
    .collect::<Vec<_>>()
    .into()
}

fn collect_events(
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    window: &Window,
    messages: &[frame::UiMenuRequest],
    in_game_zombies: bool,
    menu_active: bool,
    last_cursor: &mut Option<Vec2>,
) -> Vec<UiEvent> {
    let mut events = Vec::new();
    if in_game_zombies
        && (keys.just_pressed(KeyCode::Escape)
            || messages.iter().any(|m| {
                matches!(m, frame::UiMenuRequest::Toggle)
                    || matches!(m, frame::UiMenuRequest::Open(n) if n == "t6/pause")
                    || (menu_active && *m == frame::UiMenuRequest::Key(frame::UiMenuKey::Escape))
            }))
    {
        events.push(UiEvent::InGameMenu {
            menu: "class".into(),
        });
    }
    if !menu_active {
        return events;
    }
    for (code, req, button) in [
        (KeyCode::Enter, Some(frame::UiMenuKey::Enter), "primary"),
        (KeyCode::ArrowUp, Some(frame::UiMenuKey::Up), "up"),
        (KeyCode::ArrowDown, Some(frame::UiMenuKey::Down), "down"),
        (KeyCode::ArrowLeft, Some(frame::UiMenuKey::Left), "left"),
        (KeyCode::ArrowRight, Some(frame::UiMenuKey::Right), "right"),
        (
            KeyCode::Backspace,
            Some(frame::UiMenuKey::Backspace),
            "secondary",
        ),
    ] {
        if keys.just_pressed(code)
            || req.is_some_and(|rk| messages.contains(&frame::UiMenuRequest::Key(rk)))
        {
            events.push(UiEvent::Button {
                button,
                qualifier: None,
                down: true,
            });
            events.push(UiEvent::Button {
                button,
                qualifier: None,
                down: false,
            });
        }
    }
    if !in_game_zombies
        && (keys.just_pressed(KeyCode::Escape)
            || messages.contains(&frame::UiMenuRequest::Key(frame::UiMenuKey::Escape)))
    {
        events.push(UiEvent::Button {
            button: "secondary",
            qualifier: None,
            down: true,
        });
        events.push(UiEvent::Button {
            button: "secondary",
            qualifier: None,
            down: false,
        });
    }
    if let Some(pos) = window.cursor_position() {
        let scale = (window.height() / 720.0).max(0.001);
        let (x, y) = (pos.x / scale, pos.y / scale);
        if *last_cursor != Some(Vec2::new(x, y)) {
            *last_cursor = Some(Vec2::new(x, y));
            events.push(UiEvent::MouseMove { x, y });
        }
        for (btn, name) in [(MouseButton::Left, "left"), (MouseButton::Right, "right")] {
            if mouse.just_pressed(btn) {
                events.push(UiEvent::Mouse {
                    button: name,
                    down: true,
                    x,
                    y,
                });
            }
            if mouse.just_released(btn) {
                events.push(UiEvent::Mouse {
                    button: name,
                    down: false,
                    x,
                    y,
                });
            }
        }
    }
    events
}

type View<'w> = (
    Res<'w, frame::AppScreen>,
    ResMut<'w, frame::NativeGameMenu>,
    Res<'w, LocalPresentClient>,
    Option<Res<'w, FrameClock>>,
    Res<'w, GameUiFont>,
    Option<Res<'w, PreparedWeapons>>,
    Res<'w, Time>,
    Res<'w, frame::GameSettings>,
    Res<'w, super::launcher::T6ZombiesFrontEnd>,
);

type UiIo<'w, 's> = (
    Res<'w, ButtonInput<KeyCode>>,
    Res<'w, ButtonInput<MouseButton>>,
    Option<Res<'w, frame::HudInputView>>,
    MessageReader<'w, 's, frame::UiMenuRequest>,
    MessageWriter<'w, frame::UiExecCommand>,
    MessageWriter<'w, frame::UiPlaySound>,
    MessageWriter<'w, frame::UiPlayMusic>,
    MessageWriter<'w, frame::UiStopMusic>,
);

#[allow(clippy::too_many_arguments)]
fn refresh(
    mut commands: Commands,
    publication: Option<Res<asset_material::UiImagePublication>>,
    map: Option<Res<SessionMapIdentity>>,
    mode: Option<Res<game_api::ModeRules>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    view: View<'_>,
    mut io: UiIo<'_, '_>,
    presented: Res<PresentedSnapshot>,
    actions: Option<Res<ClientActionInput>>,
    compass: Option<Res<SessionCompass>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut state: ResMut<ScriptHud>,
    mut art: ResMut<super::t6_art::T6Art>,
    mut images: ResMut<Assets<Image>>,
    mut globes: ResMut<Assets<super::t6_globe::GlobeMaterial>>,
    code_hud: Res<super::t6_code_hud::CodeHudDraws>,
) {
    let (screen, mut menu, local, clock, font, weapons, time, settings, t6_front_end) = view;
    let (
        keys,
        mouse,
        hud_input,
        ref mut menu_in,
        ref mut exec,
        ref mut sound,
        ref mut start_music,
        ref mut stop_music,
    ) = io;
    let messages: Vec<_> = menu_in.read().cloned().collect();
    let console_open = hud_input.as_ref().is_some_and(|h| h.console_open);
    state.scoreboard = false;
    let front_end_mode = *screen == frame::AppScreen::MainMenu && t6_front_end.0.is_some();
    let in_game_mode = *screen == frame::AppScreen::InGame;
    if !in_game_mode && menu.0 {
        menu.0 = false;
    }
    if !front_end_mode && !in_game_mode {
        if let Some(root) = state.root.take() {
            commands.entity(root).despawn();
        }
        state.scene = None;
        state.active = false;
        state.front_end = false;
        state.slots.clear();
        state.map_images.clear();
        state.globes.clear();
        state.publication = None;
        state.client_field_serial = 0;
        state.lui_notify_serial = 0;
        state.logged_texts.clear();
        state.input = Input::default();
        return;
    }
    let publication = if front_end_mode {
        art.publication()
            .cloned()
            .or_else(|| publication.as_deref().cloned())
    } else {
        publication.as_deref().cloned()
    };
    let (Some(publication), Ok(window)) = (publication.as_ref(), windows.single()) else {
        return;
    };
    if window.width() <= 0.0 || window.height() <= 0.0 {
        return;
    }
    let namespace = if let Some(ns) = t6_front_end.0.filter(|_| front_end_mode) {
        ns
    } else if let Some(ns) = map.as_ref().and_then(|m| m.namespace) {
        ns
    } else {
        return;
    };
    let mut ps_origin = [0.0f32; 3];
    let mut ps_yaw = 0.0f32;
    let draws = if front_end_mode {
        let first_frame = state.publication != Some(publication.id()) || !state.front_end;
        if first_frame {
            if let Some(root) = state.root.take() {
                commands.entity(root).despawn();
            }
            state.slots.clear();
            state.map_images.clear();
            state.globes.clear();
            state.logged_texts.clear();
            state.publication = Some(publication.id());
            state.front_end = true;
            state.input = Input::default();
            let modules = publication.script_modules(namespace);
            state.scene = match session::games::menus(namespace).front_end_ui(&modules) {
                Ok(scene) => scene,
                Err(error) => {
                    diag::warn!(Ui, "authored front-end bootstrap: {error}");
                    None
                }
            };
            state.active = state.scene.is_some();
            state.input.zombies = true;
            state.input.safe_area = [1.0, 1.0];
            state.input.tables = publication
                .script_tables(namespace)
                .into_iter()
                .map(|table| Table {
                    name: table.name,
                    columns: table.columns,
                    cells: table.cells,
                })
                .collect::<Vec<_>>()
                .into();
            state.input.localized = publication.script_localized(namespace);
            state.input.layouts = publication.script_layouts(namespace);
            state.input.installed_maps = publication.script_installed_maps(namespace);
            state.input.fonts = load_fonts(namespace);
        }
        if state.scene.is_none() {
            state.active = false;
            return;
        }
        state.input.viewport = [window.width(), window.height()];
        if first_frame && state.input.zombies {
            let captured = !asset_transport::t6_content::captured_patch_files("_zm").is_empty()
                || asset_transport::t6_content::captured_script_modules("_zm")
                    .iter()
                    .any(|(name, bytes)| {
                        name == "maps/mp/gametypes_zm/_globallogic.gsc" && !bytes.is_empty()
                    });
            if let Some(mode) = session::games::modes(namespace).zombies() {
                state.input.unavailable_maps = mode
                    .maps
                    .iter()
                    .filter_map(|(map, _)| {
                        let reason = if !state
                            .input
                            .installed_maps
                            .iter()
                            .any(|installed| installed == map)
                        {
                            "Map not installed."
                        } else if !captured
                            && mode
                                .capture
                                .as_ref()
                                .is_some_and(|capture| !capture.exempt.contains(map))
                        {
                            "Zombies patch capture required."
                        } else {
                            return None;
                        };
                        Some(((*map).to_owned(), reason.to_owned()))
                    })
                    .collect::<Vec<_>>()
                    .into();
            }
        }
        let dt = ((time.delta_secs() * 1000.0).round() as i32).clamp(1, 100);
        state.input.time = state.input.time.wrapping_add(dt);
        state.input.players = vec![Player {
            id: 0,
            name: settings.player_name.clone(),
            team: 1,
            rank: 1,
            ..Default::default()
        }];
        state.input.events = if console_open || first_frame {
            Vec::new()
        } else {
            collect_events(
                &keys,
                &mouse,
                window,
                &messages,
                false,
                true,
                &mut state.last_cursor,
            )
        };
        let input = state.input.clone();
        let scene = state.scene.as_mut().unwrap();
        let lui_draws = match scene.update(&input) {
            Ok(draws) => draws,
            Err(error) => {
                diag::warn!(Ui, "authored front end unavailable: {error}");
                state.scene = None;
                state.active = false;
                return;
            }
        };
        let requests = scene.take_requests();
        let mut picked_mode = None;
        for req in &requests {
            match req {
                UiRequest::Exec(text) if text == "startMultiplayer" => {
                    exec.write(frame::UiExecCommand {
                        text: "openmenu launcher/multiplayer".into(),
                    });
                }
                UiRequest::Exec(text) if text == "startSingleplayer" => {
                    exec.write(frame::UiExecCommand {
                        text: "openmenu launcher/campaign".into(),
                    });
                }
                UiRequest::ZombiesStart { gametype, location } => {
                    asset_transport::select_zombies_start(Some(asset_transport::ZombiesStart {
                        gametype: gametype.clone(),
                        location: (!location.is_empty()).then(|| location.clone()),
                    }));
                    picked_mode = sim::HostGameModeSelection::from_token(gametype);
                }
                UiRequest::Exec(text) => {
                    if text.starts_with("map ")
                        && let Some(zombies) = session::games::modes(namespace).zombies()
                        && let Some(mode) = picked_mode
                            .take()
                            .or_else(|| sim::HostGameModeSelection::from_token(zombies.gametype))
                    {
                        commands.insert_resource(mode);
                    }
                    exec.write(frame::UiExecCommand { text: text.clone() });
                }
                UiRequest::Sound(alias) => {
                    sound.write(frame::UiPlaySound {
                        alias: alias.clone(),
                    });
                }
                UiRequest::MenuMusic(Some(alias)) => {
                    start_music.write(frame::UiPlayMusic {
                        alias: alias.clone(),
                    });
                }
                UiRequest::MenuMusic(None) => {
                    stop_music.write(frame::UiStopMusic);
                }
                UiRequest::Rumble { .. } => {}
            }
        }
        let texts: Vec<String> = lui_draws
            .iter()
            .filter_map(|d| match &d.content {
                Content::Text { value, .. } if d.color[3] > 0.0 && !value.is_empty() => {
                    Some(value.clone())
                }
                _ => None,
            })
            .collect();
        if texts != state.logged_texts {
            diag::info!(Ui, "t6 front end: texts={texts:?}");
            state.logged_texts = texts;
        }
        lui_draws
    } else {
        let zombies = mode.as_ref().is_some_and(|mode| mode.hud.zombies);
        let visible = !menu.0 || zombies;
        if let Some(root) = state.root {
            commands.entity(root).insert(if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
        if !visible {
            return;
        }
        let (Some(clock), Some(map), Some(snapshot)) =
            (clock.as_deref(), map.as_deref(), presented.snapshot())
        else {
            return;
        };
        if state.publication != Some(publication.id())
            || state.input.map != map.zone
            || state.front_end
        {
            if let Some(root) = state.root.take() {
                commands.entity(root).despawn();
            }
            state.slots.clear();
            state.map_images.clear();
            state.globes.clear();
            state.client_field_serial = 0;
            state.lui_notify_serial = 0;
            state.logged_texts.clear();
            state.publication = Some(publication.id());
            state.front_end = false;
            let modules = publication.script_modules(namespace);
            state.scene = match session::games::menus(namespace).script_ui(&modules) {
                Ok(scene) => scene,
                Err(error) => {
                    diag::warn!(Ui, "authored UI bootstrap: {error}");
                    None
                }
            };
            state.active = state.scene.is_some();
            state.input.map = map.zone.clone();
            if let Some(start) = asset_transport::zombies_start()
                && map.zone.starts_with("zm_")
            {
                state.input.gametype = start.gametype;
                state.input.location = start.location.unwrap_or_else(String::new);
            }
            state.input.tables = publication
                .script_tables(namespace)
                .into_iter()
                .map(|table| Table {
                    name: table.name,
                    columns: table.columns,
                    cells: table.cells,
                })
                .collect::<Vec<_>>()
                .into();
            let mut loc_map: std::collections::BTreeMap<String, String> = publication
                .script_localized(namespace)
                .iter()
                .cloned()
                .collect();
            if let Some(strings) = strings.as_ref() {
                for (k, v) in strings.0.iter() {
                    loc_map.insert(k.to_owned(), v.to_owned());
                }
            }
            state.input.localized = loc_map.into_iter().collect::<Vec<_>>().into();
            state.input.layouts = publication.script_layouts(namespace);
            state.input.installed_maps = publication.script_installed_maps(namespace);
            state.input.fonts = load_fonts(namespace);
        }
        if state.scene.is_none() {
            state.active = false;
            return;
        }
        let Some(ps) = presented.player(local.0) else {
            return;
        };
        ps_origin = ps.origin;
        ps_yaw = ps.viewangles[1];
        state.input.viewport = [window.width(), window.height()];
        state.input.time = clock.time();
        state.input.local_client = local.0.0;
        state.input.zombies = zombies;
        state.input.team_game = snapshot.meta.kind.is_team();
        state.input.scoreboard = actions
            .as_ref()
            .is_some_and(|actions| actions.client.kb.scores.active)
            || snapshot.meta.phase == sim::MatchPhase::PostGame;
        let menu_active = state.scene.as_ref().is_some_and(|s| s.menu_open());
        state.input.menu = menu.0 || menu_active;
        state.input.events = if console_open {
            Vec::new()
        } else {
            collect_events(
                &keys,
                &mouse,
                window,
                &messages,
                zombies,
                menu_active,
                &mut state.last_cursor,
            )
        };
        state.input.rounds_played = snapshot.meta.rounds_played;
        state.input.remaining = (snapshot.meta.time_limit_ms != 0).then(|| {
            snapshot
                .meta
                .time_limit_ms
                .saturating_sub(snapshot.meta.match_elapsed_ms) as i32
        });
        state.input.client_fields.clear();
        state.input.lui_notifies.clear();
        if let Some(meta) = snapshot.meta.for_client(local.0) {
            let weapon = ps.weapon;
            let bound_weapons = weapons
                .as_ref()
                .and_then(|w| w.for_snapshot(presented.weapon_epoch()).ok());
            let registry = bound_weapons.as_ref().map(|w| w.registry());
            let raw_weapon_name = registry
                .as_ref()
                .map_or_else(String::new, |r| r.name_of(weapon).to_owned());
            state.input.weapon_name = raw_weapon_name
                .strip_prefix("t6:weapon/")
                .unwrap_or(&raw_weapon_name)
                .to_owned();
            state.input.weapon_display_name = registry.as_ref().map_or_else(String::new, |r| {
                r.display_name_key_of(weapon)
                    .and_then(|key| strings.as_ref()?.0.text_in(namespace, key))
                    .unwrap_or_else(|| r.name_of(weapon))
                    .to_owned()
            });
            let hud_facts = bound_weapons
                .as_ref()
                .and_then(|w| w.row(weapon).and_then(|row| row.hud_facts()));
            state.input.weapon_inventory_type = if hud_facts.is_some_and(|f| f.is_alternate()) {
                3
            } else {
                0
            };
            let (clip, stock) = meta
                .ammo_by_weapon
                .iter()
                .find(|(index, _, _)| *index == weapon)
                .map_or((meta.ammo_clip, meta.ammo_stock), |(_, clip, stock)| {
                    (*clip, *stock)
                });
            state.input.ammo_clip = clip;
            state.input.ammo_stock = stock;
            state.input.low_clip = hud_facts.is_some_and(|f| {
                f.clip_size > 0
                    && (clip as f32) <= (f.clip_size as f32 * f.low_ammo_warning_threshold).ceil()
            });
            let mut offhand = [OffhandSlot::default(), OffhandSlot::default()];
            if let (Some(bound), Some(registry)) = (bound_weapons.as_ref(), registry.as_ref()) {
                for &slot in &ps.weapons {
                    if slot <= 0 {
                        continue;
                    }
                    let index = slot as u32;
                    let Some(facts) = bound.row(index).and_then(|w| w.hud_facts()) else {
                        continue;
                    };
                    let bucket = match facts.offhand_class {
                        1 | 4 | 5 => Some((0, "hud_us_grenade")),
                        2 | 3 => Some((1, "hud_cymbal_monkey_bo2")),
                        _ => None,
                    };
                    let Some((bucket, fallback)) = bucket else {
                        continue;
                    };
                    let ammo = meta
                        .ammo_by_weapon
                        .iter()
                        .find(|(w, _, _)| *w == index)
                        .map_or(0, |(_, c, s)| c + s);
                    let material = registry
                        .hud_icon_image_of(index)
                        .unwrap_or(fallback)
                        .to_owned();
                    offhand[bucket] = OffhandSlot { ammo, material };
                }
            }
            state.input.offhand = offhand;
            let prev_cf = state.client_field_serial;
            state.input.client_fields = meta
                .lui_client_fields
                .iter()
                .filter(|(serial, _)| *serial > prev_cf)
                .map(|(_, ev)| ev.clone())
                .collect();
            if let Some((last, _)) = meta.lui_client_fields.last() {
                state.client_field_serial = *last;
            }
            let prev_notify = state.lui_notify_serial;
            state.input.lui_notifies = meta
                .lui_notifies
                .iter()
                .filter(|(serial, _)| *serial > prev_notify)
                .map(|(_, ev)| ev.clone())
                .collect();
            if let Some((last, _)) = meta.lui_notifies.last() {
                state.lui_notify_serial = *last;
            }
        }
        state.input.players = snapshot
            .meta
            .clients
            .iter()
            .map(|(id, meta)| Player {
                id: id.0,
                name: String::from_utf8_lossy(
                    &meta.name[..meta
                        .name
                        .iter()
                        .position(|byte| *byte == 0)
                        .unwrap_or(meta.name.len())],
                )
                .into_owned(),
                team: if snapshot.meta.kind.is_team() {
                    meta.client_state_team
                } else {
                    0
                },
                score: meta.score,
                kills: meta.kills,
                deaths: meta.deaths,
                rank: meta.rank,
                prestige: meta.prestige,
                zombie_stats: meta.zombie_stats,
                ping: None,
            })
            .collect();
        state
            .input
            .players
            .sort_by_key(|player| (player.team, std::cmp::Reverse(player.score), player.id));
        let input = state.input.clone();
        let scene = state.scene.as_mut().unwrap();
        let lui_draws = match scene.update(&input) {
            Ok(draws) => draws,
            Err(error) => {
                diag::warn!(Ui, "authored HUD unavailable: {error}");
                state.scene = None;
                state.active = false;
                if let Some(root) = state.root.take() {
                    commands.entity(root).despawn();
                }
                state.slots.clear();
                state.map_images.clear();
                state.globes.clear();
                return;
            }
        };
        let is_open = scene.menu_open();
        let requests = scene.take_requests();
        if zombies {
            menu.0 = is_open;
            if is_open {
                let texts: Vec<String> = lui_draws
                    .iter()
                    .filter_map(|d| match &d.content {
                        Content::Text { value, .. } if d.color[3] > 0.0 && !value.is_empty() => {
                            Some(value.clone())
                        }
                        _ => None,
                    })
                    .collect();
                if texts != state.logged_texts {
                    diag::info!(Ui, "t6 pause menu: texts={texts:?}");
                    state.logged_texts = texts;
                }
            } else {
                state.logged_texts.clear();
            }
        }
        for req in requests {
            match req {
                UiRequest::Exec(text) => {
                    exec.write(frame::UiExecCommand { text });
                }
                UiRequest::Sound(alias) => {
                    sound.write(frame::UiPlaySound { alias });
                }
                UiRequest::MenuMusic(Some(alias)) => {
                    start_music.write(frame::UiPlayMusic { alias });
                }
                UiRequest::MenuMusic(None) => {
                    stop_music.write(frame::UiStopMusic);
                }
                UiRequest::Rumble { .. } | UiRequest::ZombiesStart { .. } => {}
            }
        }
        state.scoreboard = input.scoreboard && !is_open;
        let mut draws =
            Vec::with_capacity(code_hud.back.len() + lui_draws.len() + code_hud.front.len());
        draws.extend(code_hud.back.iter().cloned());
        draws.extend(lui_draws);
        draws.extend(code_hud.front.iter().cloned());
        draws
    };
    state.active = true;
    let scale = window.height() / 720.0;
    let width = window.width() / scale;
    let root = if let Some(root) = state.root {
        root
    } else {
        let layer = if front_end_mode {
            UiLayer::Shell
        } else {
            UiLayer::Hud
        };
        let root = commands
            .spawn((
                ScriptRoot,
                super::t6_text::NativeUiRoot(true),
                layer,
                UiLayerVisibility,
                GlobalZIndex(if front_end_mode { 85 } else { 0 }),
                node([0.0, 0.0, width, 720.0]),
                UiTransform::default(),
            ))
            .id();
        state.root = Some(root);
        root
    };
    commands.entity(root).insert((
        node([0.0, 0.0, width, 720.0]),
        UiTransform {
            scale: Vec2::splat(scale),
            translation: Val2::px(width * (scale - 1.0) * 0.5, 720.0 * (scale - 1.0) * 0.5),
            ..default()
        },
    ));
    let mut seen = HashSet::new();
    for (order, draw) in draws.into_iter().enumerate() {
        seen.insert(draw.id);
        let dynamic = matches!(draw.content, Content::Compass { .. });
        if !dynamic
            && state
                .slots
                .get(&draw.id)
                .is_some_and(|(_, _, previous)| *previous == draw)
        {
            continue;
        }
        let (wrapper, entity) = state
            .slots
            .get(&draw.id)
            .map(|(wrapper, entity, _)| (*wrapper, *entity))
            .unwrap_or_else(|| {
                let wrapper = commands.spawn(ChildOf(root)).id();
                let entity = commands.spawn(ChildOf(wrapper)).id();
                (wrapper, entity)
            });
        let map_layer = matches!(draw.content, Content::Compass { layer: 0, .. });
        let mut bounds = draw.clip.unwrap_or([0.0, 0.0, width, 720.0]);
        if map_layer {
            bounds = [
                bounds[0].max(draw.rectangle[0]),
                bounds[1].max(draw.rectangle[1]),
                bounds[2].min(draw.rectangle[2]),
                bounds[3].min(draw.rectangle[3]),
            ];
        }
        let mut wrapper_node = node(bounds);
        if draw.clip.is_some() || map_layer {
            wrapper_node.overflow = Overflow::clip();
        }
        commands
            .entity(wrapper)
            .insert((wrapper_node, ZIndex(order as i32)));
        let mut rectangle = draw.rectangle;
        rectangle[0] -= bounds[0];
        rectangle[2] -= bounds[0];
        rectangle[1] -= bounds[1];
        rectangle[3] -= bounds[1];
        let tint = Color::srgba(draw.color[0], draw.color[1], draw.color[2], draw.color[3]);
        commands.entity(entity).insert((
            ZIndex(order as i32),
            UiTransform::from_rotation(Rot2::degrees(draw.rotation)),
            Visibility::Inherited,
        ));
        if !matches!(draw.content, Content::Globe { .. }) {
            commands
                .entity(entity)
                .remove::<MaterialNode<super::t6_globe::GlobeMaterial>>();
        }
        match &draw.content {
            Content::Globe { material, rotation } => {
                commands
                    .entity(entity)
                    .remove::<(Text, ImageNode, BackgroundColor)>()
                    .insert(node(rectangle));
                if let Some(map) = art.image(material, &mut images) {
                    let value = super::t6_globe::GlobeMaterial {
                        color: Vec4::from_array(draw.color),
                        rotation: Vec4::from_array(*rotation),
                        map,
                    };
                    let handle = state
                        .globes
                        .entry(draw.id)
                        .or_insert_with(|| globes.add(value.clone()))
                        .clone();
                    if let Some(mut material) = globes.get_mut(&handle) {
                        *material = value;
                    }
                    commands.entity(entity).insert(MaterialNode(handle));
                }
            }
            Content::Text {
                value,
                font: font_name,
            } => {
                let size = (rectangle[3] - rectangle[1]).max(1.0);
                let native = asset_material::ui_font::ui_font(namespace, font_name);
                let text_width = native.as_ref().map_or(0.0, |font| {
                    value
                        .chars()
                        .filter_map(|letter| font.glyphs.get(&(letter as u16)))
                        .map(|glyph| glyph.dx as f32)
                        .sum::<f32>()
                        * size
                        / font.pixel_height.max(1) as f32
                });
                if draw.alignment == 1 {
                    rectangle[0] += ((rectangle[2] - rectangle[0]) - text_width) * 0.5;
                } else if draw.alignment == 2 {
                    rectangle[0] = rectangle[2] - text_width;
                }
                rectangle[2] = rectangle[0] + text_width.max(1.0);
                commands
                    .entity(entity)
                    .remove::<(ImageNode, BackgroundColor)>()
                    .insert((
                        node(rectangle),
                        Text::new(value),
                        game_text_font(&font.0, size),
                        TextColor(tint),
                        super::t6_text::NativeFont(font_name.clone()),
                    ));
                if draw.text_shadow {
                    commands.entity(entity).remove::<super::t6_text::NoShadow>();
                } else {
                    commands.entity(entity).insert(super::t6_text::NoShadow);
                }
            }
            Content::Image(name) => {
                commands
                    .entity(entity)
                    .remove::<Text>()
                    .insert(node(rectangle));
                if name.is_empty() || name == "white" {
                    commands
                        .entity(entity)
                        .remove::<ImageNode>()
                        .insert(BackgroundColor(tint));
                } else if let Some(image) = art.image(name, &mut images) {
                    commands
                        .entity(entity)
                        .remove::<BackgroundColor>()
                        .insert(ImageNode {
                            image,
                            color: tint,
                            ..default()
                        });
                } else if name == "black" {
                    commands
                        .entity(entity)
                        .remove::<ImageNode>()
                        .insert(BackgroundColor(Color::srgba(0.0, 0.0, 0.0, draw.color[3])));
                }
            }
            Content::Compass {
                layer,
                material,
                range,
                full,
            } => {
                commands.entity(entity).insert(node(rectangle));
                if *layer == 0
                    && let (Some(corners), Some(image)) = (
                        compass.as_ref().and_then(|compass| compass.corners),
                        art.image(material, &mut images),
                    )
                {
                    let north = compass
                        .as_ref()
                        .and_then(|compass| compass.north_yaw)
                        .unwrap_or(0.0)
                        .to_radians();
                    let (sin, cos) = north.sin_cos();
                    let project = |x: f32, y: f32| Vec2::new(sin * x - cos * y, -cos * x - sin * y);
                    let span = project(corners.b[0] - corners.a[0], corners.b[1] - corners.a[1]);
                    if !span.is_finite()
                        || span.x <= 0.0
                        || span.y <= 0.0
                        || !range.is_finite()
                        || *range <= 0.0
                    {
                        commands.entity(wrapper).despawn();
                        state.slots.remove(&draw.id);
                        state.map_images.remove(&draw.id);
                        continue;
                    }
                    if *full {
                        commands.entity(entity).insert(ImageNode {
                            image,
                            color: tint,
                            ..default()
                        });
                    } else {
                        let player =
                            project(ps_origin[0] - corners.a[0], ps_origin[1] - corners.a[1])
                                / span;
                        let center = Vec2::new(
                            (rectangle[0] + rectangle[2]) * 0.5,
                            (rectangle[1] + rectangle[3]) * 0.5,
                        );
                        let size = span / *range * (rectangle[3] - rectangle[1]);
                        commands.entity(entity).insert((
                            node([center.x, center.y, center.x, center.y]),
                            UiTransform::from_rotation(Rot2::degrees(ps_yaw - north.to_degrees())),
                        ));
                        let map_image = *state
                            .map_images
                            .entry(draw.id)
                            .or_insert_with(|| commands.spawn(ChildOf(entity)).id());
                        commands.entity(map_image).insert((
                            node([
                                -player.x * size.x,
                                -player.y * size.y,
                                (1.0 - player.x) * size.x,
                                (1.0 - player.y) * size.y,
                            ]),
                            ImageNode {
                                image,
                                color: tint,
                                ..default()
                            },
                        ));
                    }
                } else if *layer == 1
                    && let Some(image) = art.image("compassping_player", &mut images)
                {
                    let center = Vec2::new(
                        (rectangle[0] + rectangle[2]) * 0.5,
                        (rectangle[1] + rectangle[3]) * 0.5,
                    );
                    commands.entity(entity).insert((
                        node([
                            center.x - 8.0,
                            center.y - 8.0,
                            center.x + 8.0,
                            center.y + 8.0,
                        ]),
                        ImageNode {
                            image,
                            color: tint,
                            ..default()
                        },
                        UiTransform::from_rotation(Rot2::degrees(if *full {
                            compass
                                .as_ref()
                                .and_then(|compass| compass.north_yaw)
                                .unwrap_or(0.0)
                                - ps_yaw
                        } else {
                            0.0
                        })),
                    ));
                }
            }
            Content::PlayerEmblem(_) | Content::Voice(_) | Content::GameTimer => {}
        }
        state.slots.insert(draw.id, (wrapper, entity, draw));
    }
    state.slots.retain(|id, (wrapper, _, _)| {
        if seen.contains(id) {
            true
        } else {
            commands.entity(*wrapper).despawn();
            false
        }
    });
    state.map_images.retain(|id, _| seen.contains(id));
    state.globes.retain(|id, _| seen.contains(id));
}
