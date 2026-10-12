use crate::layers::{GameUiFont, UiLayer, game_text_font};
use assets::{PreparedLocalizedStrings, SessionCompass, SessionMapIdentity};
use bevy::prelude::*;
use game_api::ui::{Content, Draw, Font, Input, Player, ScriptUi, Table};
use net::{ClientActionInput, FrameClock, LocalPresentClient, PresentedSnapshot};
use std::collections::{HashMap, HashSet};

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ScriptUiPaint;

#[derive(Component)]
struct ScriptRoot;

#[derive(Resource, Default)]
pub(crate) struct ScriptHud {
    publication: Option<u64>,
    scene: Option<Box<dyn ScriptUi>>,
    root: Option<Entity>,
    slots: HashMap<u32, (Entity, Entity, Draw)>,
    map_images: HashMap<u32, Entity>,
    input: Input,
    pub(crate) scoreboard: bool,
}

pub(crate) fn register(app: &mut App) {
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

type View<'w> = (
    Res<'w, frame::AppScreen>,
    Res<'w, frame::NativeGameMenu>,
    Res<'w, LocalPresentClient>,
    Option<Res<'w, FrameClock>>,
    Res<'w, GameUiFont>,
);

#[allow(clippy::too_many_arguments)]
fn refresh(
    mut commands: Commands,
    publication: Option<Res<asset_material::UiImagePublication>>,
    map: Option<Res<SessionMapIdentity>>,
    mode: Option<Res<game_api::ModeRules>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    view: View<'_>,
    presented: Res<PresentedSnapshot>,
    actions: Option<Res<ClientActionInput>>,
    compass: Option<Res<SessionCompass>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut state: ResMut<ScriptHud>,
    mut art: ResMut<super::t6_art::T6Art>,
    mut images: ResMut<Assets<Image>>,
) {
    let (screen, menu, local, clock, font) = view;
    state.scoreboard = false;
    if *screen != frame::AppScreen::InGame {
        if let Some(root) = state.root.take() {
            commands.entity(root).despawn();
        }
        state.scene = None;
        state.slots.clear();
        state.map_images.clear();
        state.publication = None;
        state.input = Input::default();
        return;
    }
    let visible = *screen == frame::AppScreen::InGame && !menu.0;
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
    let Some(clock) = clock else {
        return;
    };
    let (Some(publication), Some(map), Some(namespace), Some(snapshot), Ok(window)) = (
        publication.as_deref(),
        map.as_deref(),
        map.as_ref().and_then(|map| map.namespace),
        presented.snapshot(),
        windows.single(),
    ) else {
        return;
    };
    if window.width() <= 0.0 || window.height() <= 0.0 {
        return;
    }
    if state.publication != Some(publication.id()) || state.input.map != map.zone {
        if let Some(root) = state.root.take() {
            commands.entity(root).despawn();
        }
        state.slots.clear();
        state.map_images.clear();
        state.publication = Some(publication.id());
        let modules = publication.script_modules(namespace);
        state.scene = match session::games::menus(namespace).script_ui(&modules) {
            Ok(scene) => scene,
            Err(error) => {
                diag::warn!(Ui, "authored UI bootstrap: {error}");
                None
            }
        };
        state.input.map = map.zone.clone();
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
        state.input.localized = match strings.as_ref() {
            Some(strings) => strings
                .0
                .iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect::<Vec<_>>(),
            None => Vec::new(),
        }
        .into();
        state.input.fonts = [
            "normalFont",
            "bigFont",
            "smallFont",
            "extraSmallFont",
            "extraBigFont",
            "italicFont",
            "smallItalicFont",
            "distFont",
        ]
        .into_iter()
        .filter_map(|name| {
            let name = if name == "distFont" {
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
        .into();
    }
    if state.scene.is_none() {
        return;
    }
    let Some(ps) = presented.player(local.0) else {
        return;
    };
    state.input.viewport = [window.width(), window.height()];
    state.input.time = clock.time();
    state.input.local_client = local.0.0;
    state.input.zombies = mode.is_some_and(|mode| mode.hud.zombies);
    state.input.team_game = snapshot.meta.kind.is_team();
    state.input.scoreboard = actions
        .as_ref()
        .is_some_and(|actions| actions.client.kb.scores.active)
        || snapshot.meta.phase == sim::MatchPhase::PostGame;
    state.input.menu = menu.0;
    state.input.remaining = (snapshot.meta.time_limit_ms != 0).then(|| {
        snapshot
            .meta
            .time_limit_ms
            .saturating_sub(snapshot.meta.match_elapsed_ms) as i32
    });
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
    let draws = match state.scene.as_mut().unwrap().update(&input) {
        Ok(draws) => draws,
        Err(error) => {
            diag::warn!(Ui, "authored HUD unavailable: {error}");
            state.scene = None;
            if let Some(root) = state.root.take() {
                commands.entity(root).despawn();
            }
            state.slots.clear();
            state.map_images.clear();
            return;
        }
    };
    state.scoreboard = input.scoreboard;
    let scale = window.height() / 720.0;
    let width = window.width() / scale;
    let root = if let Some(root) = state.root {
        root
    } else {
        let root = commands
            .spawn((
                ScriptRoot,
                super::t6_text::NativeUiRoot(true),
                UiLayer::Hud,
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
        match &draw.content {
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
                commands.entity(entity).insert((
                    node(rectangle),
                    Text::new(value),
                    game_text_font(&font.0, size),
                    TextColor(tint),
                    super::t6_text::NativeFont(font_name.clone()),
                ));
            }
            Content::Image(name) => {
                commands.entity(entity).insert(node(rectangle));
                if name.is_empty() || name == "white" {
                    commands.entity(entity).insert(BackgroundColor(tint));
                } else if let Some(image) = art.image(name, &mut images) {
                    commands.entity(entity).insert(ImageNode {
                        image,
                        color: tint,
                        ..default()
                    });
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
                            project(ps.origin[0] - corners.a[0], ps.origin[1] - corners.a[1])
                                / span;
                        let center = Vec2::new(
                            (rectangle[0] + rectangle[2]) * 0.5,
                            (rectangle[1] + rectangle[3]) * 0.5,
                        );
                        let size = span / *range * (rectangle[3] - rectangle[1]);
                        commands.entity(entity).insert((
                            node([center.x, center.y, center.x, center.y]),
                            UiTransform::from_rotation(Rot2::degrees(
                                ps.viewangles[1] - north.to_degrees(),
                            )),
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
                                - ps.viewangles[1]
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
}
