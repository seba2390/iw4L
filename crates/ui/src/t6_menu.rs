use crate::ClassLoadoutCatalog;
use crate::classes::{
    setup::{ClassEditRow, ClassSlotState},
    store::{decode_slots, encode_slots, write_class_file},
};
use crate::layers::{GameUiFont, UiLayer, UiLayerVisibility, game_text_font};
use asset_core::AssetNamespace;
use assets::{PreparedWeapons, SessionMapIdentity};
use bevy::prelude::*;
use frame::{AppScreen, GameSettings, NativeGameMenu, UiExecCommand, UiMenuKey, UiMenuRequest};
use net::{ActionRequestIds, ClientActionInbox, LocalPresentClient, ReliableControlEvent};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Pause,
    Classes,
    Edit,
    Settings,
}

#[derive(Resource, Default)]
struct Menu {
    page: Page,
    focus: usize,
    slot: usize,
    profiles: Vec<ClassSlotState>,
    path: Option<std::path::PathBuf>,
    saved: Option<String>,
    preserve_file: bool,
    save_retry: Option<std::time::Instant>,
    pending: Option<u32>,
    host: bool,
    notice: String,
}

#[derive(Component)]
struct Root;

#[derive(Component, Clone, Copy)]
struct Choice {
    order: usize,
    action: Action,
}

#[derive(Clone, Copy)]
enum Action {
    Resume,
    Classes,
    Settings,
    Back,
    Edit(usize),
    Weapon(ClassEditRow),
    Attachment(ClassEditRow),
    Equip,
    Volume,
    Fov,
    Sensitivity,
    Invert,
    Vsync,
    Leave,
    EndMatch,
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<Menu>()
        .add_systems(Update, (drive, paint).chain().in_set(frame::ClientSet::Ui));
}

fn native(slot: &ClassSlotState) -> bool {
    [&slot.primary, &slot.secondary, &slot.lethal, &slot.tactical]
        .into_iter()
        .all(|key| key.is_empty() || key.starts_with("t6:"))
        && !slot.primary.is_empty()
        && [&slot.perk1, &slot.perk2, &slot.perk3, &slot.deathstreak]
            .into_iter()
            .all(|key| key.is_empty() || key == "specialty_null")
}

fn available(slot: &ClassSlotState, registry: &asset_game::WeaponRegistry) -> bool {
    if !native(slot) {
        return false;
    }
    let row = session::ClassRow::from(&frame::HostClassSlot::from(slot));
    let combat = session::combat_table::from_registry(registry, None);
    let equipment = session::combat_table::equipment_from_registry(registry);
    !session::project_class(0, &row, registry, &combat, &equipment)
        .def
        .locked
}

fn load(
    menu: &mut Menu,
    identity: &frame::LaunchIdentity,
    catalog: &ClassLoadoutCatalog,
    registry: &asset_game::WeaponRegistry,
) {
    if menu.path.is_some()
        || !catalog
            .primary
            .iter()
            .any(|offer| offer.key.starts_with("t6:"))
    {
        return;
    }
    let path = std::env::var_os("IW4L_PROFILE_PATH")
        .map(std::path::PathBuf::from)
        .map(|path| path.with_extension("t6-classes.txt"))
        .unwrap_or_else(|| identity.artifacts.join("profile/t6-classes.txt"));
    match std::fs::metadata(&path) {
        Ok(metadata) if metadata.len() <= 65536 => {
            match std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| decode_slots(&text))
            {
                Some(slots) if slots.len() <= 5 && slots.iter().all(native) => {
                    menu.profiles = slots
                }
                _ => {
                    menu.preserve_file = true;
                    menu.notice =
                        "The saved BO2 classes could not be read; the file is preserved.".into();
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => {
            menu.preserve_file = true;
            menu.notice = "The saved BO2 classes could not be read; the file is preserved.".into();
        }
    }
    if menu.profiles.is_empty() {
        let mut base = ClassSlotState::from_host_slot(&frame::HostClassSlot {
            name: "Custom 1".into(),
            primary: String::new(),
            secondary: String::new(),
            primary_attachments: vec![],
            secondary_attachments: vec![],
            lethal: String::new(),
            tactical: String::new(),
            perks: Default::default(),
            deathstreak: String::new(),
            camos: Default::default(),
        });
        let primary: Vec<_> = catalog
            .primary
            .iter()
            .filter(|offer| offer.key.starts_with("t6:"))
            .filter(|offer| {
                base.primary = offer.key.clone();
                available(&base, registry)
            })
            .map(|offer| offer.key.clone())
            .collect();
        for index in 0..5 {
            let Some(key) = primary.get(index % primary.len().max(1)) else {
                break;
            };
            base.primary = key.clone();
            base.name = format!("Custom {}", index + 1);
            menu.profiles.push(base.clone());
        }
    }
    menu.saved = Some(encode_slots(&menu.profiles));
    menu.path = Some(path);
}

fn rows(page: Page, count: usize, host: bool) -> usize {
    match page {
        Page::Pause => 4 + usize::from(host),
        Page::Classes => count + 1,
        Page::Edit => 8,
        Page::Settings => 6,
    }
}

#[allow(clippy::too_many_arguments)]
fn drive(
    screen: Res<AppScreen>,
    role: Res<frame::RuntimeRole>,
    map: Option<Res<SessionMapIdentity>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    catalog: Res<ClassLoadoutCatalog>,
    weapons: Option<Res<PreparedWeapons>>,
    input: (
        Res<ButtonInput<KeyCode>>,
        Res<ButtonInput<MouseButton>>,
        Res<frame::HudInputView>,
        Res<frame::UiBindingCapture>,
    ),
    mut requests: MessageReader<UiMenuRequest>,
    mut reliable: MessageReader<ReliableControlEvent>,
    mut menu: ResMut<Menu>,
    mut open: ResMut<NativeGameMenu>,
    mut settings: ResMut<GameSettings>,
    choices: Query<(&Interaction, &Choice)>,
    authority: (
        ResMut<ClientActionInbox>,
        ResMut<ActionRequestIds>,
        Res<LocalPresentClient>,
    ),
    mut exec: MessageWriter<UiExecCommand>,
) {
    let (keys, mouse, hud, bindings) = input;
    let (mut actions, mut ids, local) = authority;
    menu.host = matches!(
        *role,
        frame::RuntimeRole::Listen | frame::RuntimeRole::Dedicated
    );
    let messages: Vec<_> = requests.read().cloned().collect();
    let active = *screen == AppScreen::InGame
        && map.is_some_and(|map| map.namespace == Some(AssetNamespace::T6));
    if !active {
        open.0 = false;
        if menu.pending.is_some() {
            menu.pending = None;
        }
        reliable.clear();
        return;
    }
    if let (Some(identity), Some(weapons)) = (identity.as_deref(), weapons.as_deref()) {
        load(&mut menu, identity, &catalog, weapons.registry());
    }
    for event in reliable.read() {
        match &event.0 {
            sim::SimEvent::ClassAccepted { request_id, .. }
                if menu.pending == Some(*request_id) =>
            {
                menu.pending = None;
                menu.notice = "Loadout accepted. It applies on your next respawn.".into();
                open.0 = false;
            }
            sim::SimEvent::ClassRejected {
                request_id, reason, ..
            } if menu.pending == Some(*request_id) => {
                menu.pending = None;
                menu.notice = format!("Loadout refused: {}", reason.as_str());
            }
            _ => {}
        }
    }
    if hud.console_open || bindings.command.is_some() {
        return;
    }
    let mut navigation: Vec<_> = messages
        .iter()
        .filter_map(|message| match message {
            UiMenuRequest::Key(key) => Some(*key),
            UiMenuRequest::Toggle => Some(UiMenuKey::Escape),
            _ => None,
        })
        .collect();
    if messages
        .iter()
        .any(|message| matches!(message, UiMenuRequest::Open(name) if name == "t6/pause"))
    {
        open.0 = true;
        menu.page = Page::Pause;
        menu.focus = 0;
    }
    for (code, key) in [
        (KeyCode::Escape, UiMenuKey::Escape),
        (KeyCode::Enter, UiMenuKey::Enter),
        (KeyCode::ArrowUp, UiMenuKey::Up),
        (KeyCode::ArrowDown, UiMenuKey::Down),
    ] {
        if keys.just_pressed(code) {
            navigation.push(key);
        }
    }
    let mut selected = Vec::new();
    for key in navigation {
        match key {
            UiMenuKey::Escape => {
                if !open.0 {
                    open.0 = true;
                    menu.page = Page::Pause;
                    menu.focus = 0;
                } else if menu.page == Page::Pause {
                    open.0 = false;
                } else {
                    selected.push(Action::Back);
                }
            }
            UiMenuKey::Up if open.0 => {
                menu.focus = (menu.focus + rows(menu.page, menu.profiles.len(), menu.host) - 1)
                    % rows(menu.page, menu.profiles.len(), menu.host)
            }
            UiMenuKey::Down if open.0 => {
                menu.focus = (menu.focus + 1) % rows(menu.page, menu.profiles.len(), menu.host)
            }
            UiMenuKey::Enter if open.0 => {
                if let Some((_, choice)) = choices
                    .iter()
                    .find(|(_, choice)| choice.order == menu.focus)
                {
                    selected.push(choice.action);
                }
            }
            _ => {}
        }
    }
    if open.0 && mouse.just_pressed(MouseButton::Left) {
        if let Some((_, choice)) = choices
            .iter()
            .find(|(interaction, _)| **interaction == Interaction::Pressed)
        {
            selected.push(choice.action);
        }
    }
    for action in selected {
        match action {
            Action::Resume => open.0 = false,
            Action::Classes => {
                menu.page = Page::Classes;
                menu.focus = 0;
            }
            Action::Settings => {
                menu.page = Page::Settings;
                menu.focus = 0;
            }
            Action::Back => {
                menu.page = if menu.page == Page::Edit {
                    Page::Classes
                } else {
                    Page::Pause
                };
                menu.focus = 0;
            }
            Action::Edit(slot) => {
                menu.slot = slot;
                menu.page = Page::Edit;
                menu.focus = 0;
            }
            Action::Weapon(row) | Action::Attachment(row) => {
                let Some(registry) = weapons.as_ref().map(|weapons| weapons.registry()) else {
                    continue;
                };
                let index = menu.slot;
                let Some(slot) = menu.profiles.get(index).cloned() else {
                    continue;
                };
                let attachment = matches!(action, Action::Attachment(_));
                let candidates: Vec<String> = if attachment {
                    std::iter::once(String::new())
                        .chain(
                            catalog
                                .attachments(row, slot.row_value(row))
                                .iter()
                                .cloned(),
                        )
                        .collect()
                } else {
                    let offers = match row {
                        ClassEditRow::Primary => &catalog.primary,
                        ClassEditRow::Secondary => &catalog.secondary,
                        ClassEditRow::Lethal => &catalog.lethal,
                        _ => &catalog.tactical,
                    };
                    std::iter::once(String::new())
                        .filter(|_| row != ClassEditRow::Primary)
                        .chain(
                            offers
                                .iter()
                                .filter(|offer| offer.key.starts_with("t6:"))
                                .map(|offer| offer.key.clone()),
                        )
                        .collect()
                };
                let current = if attachment {
                    match row {
                        ClassEditRow::Primary => slot.primary_attachments.first(),
                        _ => slot.secondary_attachments.first(),
                    }
                    .map_or("", String::as_str)
                } else {
                    slot.row_value(row)
                };
                let first = candidates
                    .iter()
                    .position(|key| key == current)
                    .unwrap_or(candidates.len().saturating_sub(1));
                let mut replacement = None;
                for offset in 1..=candidates.len() {
                    let mut changed = slot.clone();
                    let key = candidates[(first + offset) % candidates.len()].clone();
                    if attachment {
                        let list = if row == ClassEditRow::Primary {
                            &mut changed.primary_attachments
                        } else {
                            &mut changed.secondary_attachments
                        };
                        *list = if key.is_empty() { vec![] } else { vec![key] };
                    } else {
                        changed.set_row(row, key);
                    }
                    if available(&changed, registry) {
                        replacement = Some(changed);
                        break;
                    }
                }
                if let Some(slot) = replacement {
                    menu.profiles[index] = slot;
                    if !menu.preserve_file {
                        menu.notice.clear();
                    }
                } else {
                    menu.notice = "No prepared native option is available for this slot.".into();
                }
            }
            Action::Equip => {
                if menu.pending.is_some() {
                    continue;
                }
                let Some(slot) = menu.profiles.get(menu.slot) else {
                    continue;
                };
                let Some(registry) = weapons.as_ref().map(|weapons| weapons.registry()) else {
                    continue;
                };
                let row = session::ClassRow::from(&frame::HostClassSlot::from(slot));
                match session::loadout::resolve_personal_class(&row, registry) {
                    Ok(loadout) => {
                        let request_id = ids.allocate();
                        match actions.push(
                            local.0,
                            sim::ClientAction::SelectClass {
                                request_id,
                                class_id: sim::ClassId(menu.slot as u32),
                                revision: 1,
                                loadout,
                            },
                        ) {
                            Ok(()) => {
                                menu.pending = Some(request_id);
                                menu.notice = "Waiting for the host...".into();
                            }
                            Err(error) => menu.notice = error.to_string(),
                        }
                    }
                    Err(error) => menu.notice = error,
                }
            }
            Action::Volume => {
                settings.master_volume = if settings.master_volume >= 0.99 {
                    0.0
                } else {
                    (settings.master_volume + 0.1).min(1.0)
                };
                settings.touch();
            }
            Action::Fov => {
                settings.fov = if settings.fov >= 110.0 {
                    65.0
                } else {
                    settings.fov + 5.0
                };
                settings.touch();
            }
            Action::Sensitivity => {
                settings.sensitivity = if settings.sensitivity >= 10.0 {
                    1.0
                } else {
                    settings.sensitivity + 0.5
                };
                settings.touch();
            }
            Action::Invert => {
                settings.invert_mouse = !settings.invert_mouse;
                settings.touch();
            }
            Action::Vsync => {
                settings.vsync = !settings.vsync;
                settings.touch();
            }
            Action::Leave => {
                exec.write(UiExecCommand {
                    text: "disconnect".into(),
                });
                open.0 = false;
            }
            Action::EndMatch => {
                if menu.host {
                    exec.write(UiExecCommand {
                        text: "end_match".into(),
                    });
                    open.0 = false;
                }
            }
        }
    }
    if !menu.preserve_file
        && !menu.profiles.is_empty()
        && menu
            .save_retry
            .is_none_or(|time| std::time::Instant::now() >= time)
    {
        let contents = encode_slots(&menu.profiles);
        if menu.saved.as_deref() != Some(contents.as_str()) {
            if let Some(path) = &menu.path {
                match write_class_file(path, &contents) {
                    Ok(()) => {
                        menu.saved = Some(contents);
                        menu.save_retry = None;
                    }
                    Err(error) => {
                        menu.notice = format!("Cannot save loadout: {error}");
                        menu.save_retry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
                    }
                }
            }
        }
    }
}

fn fallback_label(key: &str) -> String {
    if key.is_empty() {
        "None".into()
    } else {
        key.split_once(':')
            .map_or(key, |(_, name)| name)
            .rsplit('/')
            .next()
            .unwrap_or(key)
            .trim_end_matches("_mp")
            .replace('_', " ")
    }
}

fn paint(
    mut commands: Commands,
    menu: Res<Menu>,
    open: Res<NativeGameMenu>,
    settings: Res<GameSettings>,
    catalog: Res<ClassLoadoutCatalog>,
    weapons: Option<Res<PreparedWeapons>>,
    strings: Option<Res<assets::PreparedLocalizedStrings>>,
    font: Res<GameUiFont>,
    roots: Query<Entity, With<Root>>,
    mut previous: Local<String>,
) {
    let signature = format!(
        "{} {:?} {} {} {:?} {} {:?} {}",
        open.0, menu.page, menu.focus, menu.slot, menu.profiles, menu.notice, *settings, menu.host
    );
    if *previous == signature {
        return;
    }
    *previous = signature;
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    if !open.0 {
        return;
    }
    let label = |key: &str| {
        weapons
            .as_ref()
            .and_then(|weapons| {
                weapons
                    .registry()
                    .resolve_index(key)
                    .ok()
                    .flatten()
                    .and_then(|id| weapons.registry().display_name_key_of(id))
            })
            .and_then(|name| {
                strings
                    .as_ref()?
                    .0
                    .text_in(AssetNamespace::T6, name.trim_start_matches('@'))
            })
            .or_else(|| {
                catalog.previews.get(key).and_then(|preview| {
                    strings
                        .as_ref()?
                        .0
                        .text_in(AssetNamespace::T6, preview.name_key.trim_start_matches('@'))
                })
            })
            .map_or_else(|| fallback_label(key), str::to_owned)
    };
    let mut buttons = Vec::new();
    let title = match menu.page {
        Page::Pause => {
            buttons.extend([
                ("RESUME GAME".into(), Action::Resume),
                ("CREATE A CLASS".into(), Action::Classes),
                ("SETTINGS".into(), Action::Settings),
            ]);
            if menu.host {
                buttons.push(("END MATCH / RETURN TO LOBBY".into(), Action::EndMatch));
            }
            buttons.push(("LEAVE MATCH".into(), Action::Leave));
            "BLACK OPS II"
        }
        Page::Classes => {
            for (index, slot) in menu.profiles.iter().enumerate() {
                buttons.push((
                    format!("{} / {}", slot.name, label(&slot.primary)),
                    Action::Edit(index),
                ));
            }
            buttons.push(("BACK".into(), Action::Back));
            "CREATE A CLASS"
        }
        Page::Edit => {
            if let Some(slot) = menu.profiles.get(menu.slot) {
                for row in [
                    ClassEditRow::Primary,
                    ClassEditRow::Secondary,
                    ClassEditRow::Lethal,
                    ClassEditRow::Tactical,
                ] {
                    buttons.push((
                        format!("{} / {}", row.label(), label(slot.row_value(row))),
                        Action::Weapon(row),
                    ));
                }
                buttons.push((
                    format!(
                        "PRIMARY ATTACHMENT / {}",
                        label(slot.primary_attachments.first().map_or("", String::as_str))
                    ),
                    Action::Attachment(ClassEditRow::Primary),
                ));
                buttons.push((
                    format!(
                        "SECONDARY ATTACHMENT / {}",
                        label(
                            slot.secondary_attachments
                                .first()
                                .map_or("", String::as_str)
                        )
                    ),
                    Action::Attachment(ClassEditRow::Secondary),
                ));
            }
            buttons.push(("USE ON NEXT RESPAWN".into(), Action::Equip));
            buttons.push(("BACK".into(), Action::Back));
            "EDIT LOADOUT"
        }
        Page::Settings => {
            buttons.extend([
                (
                    format!(
                        "MASTER VOLUME / {}%",
                        (settings.master_volume * 100.0).round()
                    ),
                    Action::Volume,
                ),
                (format!("FIELD OF VIEW / {}", settings.fov), Action::Fov),
                (
                    format!("SENSITIVITY / {:.1}", settings.sensitivity),
                    Action::Sensitivity,
                ),
                (
                    format!(
                        "INVERT MOUSE / {}",
                        if settings.invert_mouse { "ON" } else { "OFF" }
                    ),
                    Action::Invert,
                ),
                (
                    format!("VSYNC / {}", if settings.vsync { "ON" } else { "OFF" }),
                    Action::Vsync,
                ),
                ("BACK".into(), Action::Back),
            ]);
            "SETTINGS"
        }
    };
    let accent = Color::srgb(1.0, 0.65, 0.3);
    commands
        .spawn((
            Root,
            UiLayer::Overlay,
            UiLayerVisibility,
            GlobalZIndex(100),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                padding: UiRect::all(Val::Px(48.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.025, 0.03, 0.94)),
        ))
        .with_children(|root| {
            root.spawn(Node {
                width: Val::Percent(75.0),
                max_width: Val::Px(850.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(7.0),
                ..default()
            })
            .with_children(|panel| {
                panel.spawn((
                    Text::new(title),
                    game_text_font(&font.0, 32.0),
                    TextColor(accent),
                ));
                for (order, (label, action)) in buttons.into_iter().enumerate() {
                    panel
                        .spawn((
                            Button,
                            Choice { order, action },
                            Node {
                                min_height: Val::Px(38.0),
                                padding: UiRect::all(Val::Px(10.0)),
                                border: UiRect::left(Val::Px(3.0)),
                                ..default()
                            },
                            BorderColor::all(if order == menu.focus {
                                accent
                            } else {
                                Color::NONE
                            }),
                            BackgroundColor(Color::srgba(0.08, 0.09, 0.1, 0.9)),
                        ))
                        .with_children(|row| {
                            row.spawn((
                                Text::new(label),
                                game_text_font(&font.0, 18.0),
                                TextColor(Color::srgb(0.94, 0.94, 0.92)),
                            ));
                        });
                }
                panel.spawn((
                    Text::new(&menu.notice),
                    game_text_font(&font.0, 16.0),
                    TextColor(accent),
                ));
            });
        });
}
