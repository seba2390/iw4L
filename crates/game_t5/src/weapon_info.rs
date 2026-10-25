//! Black Ops' weapon info, read from its executable: the time its client
//! last showed the weapon info, the action slot arrows and the low ammo
//! warning drawn by that time's fade.

use fastfile_t5::weapon_def::WeaponDefView;
use game_api::hud::{
    LowAmmoWarning, OffhandIcons, WeaponInfoAmmo, WeaponInfoRules, WeaponInfoView,
};
use movement_t5::state::{Buttons, PlayerState, buttons};
use movement_t5::weapon::state as ws;

pub(crate) static WEAPON_INFO: WeaponInfoRules = WeaponInfoRules {
    update,
    arrow,
    bind_label,
    low_ammo,
    slot_hint,
    fade,
    ammo_fade,
    ammo_text,
    name_fade,
    name_right_gap: NAME_RIGHT_GAP,
    offhand,
};

/// The weapon name shows for 1.8 s after the selection changes.
const NAME_SHOW_MS: i32 = 1800;
/// The weapon name ends this far left of its item's right edge.
const NAME_RIGHT_GAP: f32 = 28.0;
/// The rounds beside the clip turn this colour when low.
const LOW_STOCK_RGB: [f32; 3] = [1.0, 0.3, 0.3];

/// `hud_fade_ammodisplay`: how long the weapon info shows, in seconds.
const FADE_AMMO_DISPLAY: f32 = 8.0;
/// `hud_fade_offhand`: how long the offhand icons show, in seconds.
const FADE_OFFHAND: f32 = 8.0;
/// Each next offhand icon is this share of the item's width further right
/// and this much dimmer.
const OFFHAND_STEP: f32 = 0.22;
const OFFHAND_FALLOFF: f32 = 0.8;
/// Weapon flags that hide the offhand icons.
const WEAP_FLAG_HIDE_OFFHAND: u32 = 0x180;
/// Entity flags: the player sees through another crosshair.
const EF_OTHER_CROSSHAIR: u32 = 0x4000;
/// The weapon states on a turret or a vehicle that keep the offhand icons.
const WS_VEHICLE_OFFHAND: std::ops::RangeInclusive<i32> = 0x24..=0x26;
/// Offhand slots a held-back grenade counts for in zombies: lethal and
/// special.
const OFFHAND_SLOTS_THROWBACK: [i32; 2] = [1, 4];
/// The weapon info fades out over its last 700 ms.
const FADE_OUT_MS: i32 = 700;
/// A lit arrow's alpha.
const ARROW_LIT_ALPHA: f32 = 0.8;
/// A lit key label's green and blue factors.
const BIND_LIT_GREEN: f32 = 0.8;
const BIND_LIT_BLUE: f32 = 0.4;
/// The share of a weapon's most rounds beside the clip under which they
/// count as low.
const LOW_STOCK_FRAC: f32 = 0.2;
/// The most rounds the weapon info counts.
const AMMO_COUNT_MAX: i32 = 999;
/// `player_clipSizeMultiplier`'s default.
const CLIP_SIZE_MULTIPLIER: f32 = 1.0;

/// Entity flags: on a turret or a vehicle.
const EF_TURRET_OR_VEHICLE: u32 = 0x300;
/// Entity flags that hide the low ammo warning.
const EF_NO_LOW_AMMO: u32 = 0x4300;
/// Weapon flags that hide the weapon info.
const WEAP_FLAG_HIDE_INFO: u32 = 0x80;
/// Movement flags that keep a reload key from showing the weapon info.
const PMF_NO_RELOAD_INFO: u32 = 4;
/// The last movement type that shows the low ammo warning.
const PM_TYPE_LAST_INFO: i32 = 8;
/// The alternate mode's inventory type.
const INVENTORY_ALT_MODE: i32 = 3;
/// The equipment offhand slot.
const OFFHAND_SLOT_EQUIPMENT: i32 = 3;
/// A guided missile type that never warns of low ammo.
const GUIDED_NO_WARNING: i32 = 6;
/// The weapon class a carried killstreak's action slot may switch to.
const WEAPCLASS_KILLSTREAK_ALT: i32 = 12;
/// Action slot kinds.
const ACTION_SLOT_WEAPON: i32 = 1;
const ACTION_SLOT_ALT_WEAPON: i32 = 2;

/// The low ammo warning's colour dvars (`lowAmmoWarningColor1`/`2`,
/// `…NoAmmoColor1`/`2`, `…NoReloadColor1`/`2`) as the dvar keeps them: a
/// byte per channel.
const RELOAD_COLORS: [[u8; 4]; 2] = [[230, 230, 230, 204], [255, 255, 255, 255]];
const NO_AMMO_COLORS: [[u8; 4]; 2] = [[204, 0, 0, 204], [255, 0, 0, 255]];
const NO_RELOAD_COLORS: [[u8; 4]; 2] = [[179, 179, 0, 204], [255, 255, 0, 255]];
/// `lowAmmoWarningPulseFreq`, `…PulseMin`, `…PulseMax`.
const PULSE_FREQ: f32 = 1.7;
const PULSE_MIN: f32 = 0.0;
const PULSE_MAX: f32 = 1.5;

/// The client restarts the weapon info when its weapon selection changes,
/// an action slot key is pressed, the fire, left-hand fire, reload or
/// grenade keys are held, or its weapon is low on rounds; it fades once
/// `hud_fade_ammodisplay` has passed.
fn update(view: &WeaponInfoView<'_>, shown_at: i32) -> i32 {
    let mut player = *view.player;
    let (ps, last) = crate::movement::unpack(&mut player);
    let now = view.now;
    let stamp = |shown_at: &mut i32| {
        if *shown_at < now {
            *shown_at = now;
        }
    };
    let mut shown_at = shown_at;
    if view.selected_at > shown_at {
        shown_at = view.selected_at;
    }
    if view.slot_pressed_at > shown_at {
        shown_at = view.slot_pressed_at;
    }
    if keys_show(view, &ps, last.buttons) {
        stamp(&mut shown_at);
    }
    let weapon = current_weapon(view, &ps);
    if low_stock(view, &ps, weapon) || low_clip(view, weapon) {
        stamp(&mut shown_at);
    }
    // Black Ops lets the weapon info go (its time back to 0) once it has
    // faded; its selection and action slot stamps are single events there.
    // Here they are times the view keeps, so the faded time stays: a zero
    // would take the old selection time again next frame, and the fade
    // reads the same nothing from either.
    shown_at
}

/// The keys held in the newest command that show the weapon info: fire (or
/// a dual-wield weapon's left-hand fire) off a turret, a reload key, or a
/// grenade key.
fn keys_show(view: &WeaponInfoView<'_>, ps: &PlayerState, held: Buttons) -> bool {
    let dual_wield = def(view, ps.weapon).is_some_and(|d| d.dual_wield());
    let mounted = ps.e_flags & EF_TURRET_OR_VEHICLE != 0;
    let fire =
        (held.held(buttons::ATTACK) || (dual_wield && held.held(buttons::THROW))) && !mounted;
    let reload = (held.held(buttons::RELOAD) || held.held(buttons::USE_RELOAD))
        && ps.pm_flags & PMF_NO_RELOAD_INFO == 0
        && !mounted;
    fire || reload || held.held(buttons::FRAG) || held.held(buttons::SMOKE)
}

/// The weapon the weapon info is about: the selected one when the player
/// holds it, else the one in his hands.
fn current_weapon(view: &WeaponInfoView<'_>, ps: &PlayerState) -> u32 {
    if view.selected != 0 && owns(ps, view.selected) {
        view.selected
    } else {
        ps.weapon
    }
}

fn owns(ps: &PlayerState, weapon: u32) -> bool {
    weapon != 0 && ps.held_weapons.iter().any(|held| held.weapon == weapon)
}

fn def<'a>(view: &WeaponInfoView<'a>, weapon: u32) -> Option<WeaponDefView<'a>> {
    let (def, variant) = (view.weapon)(weapon)?;
    WeaponDefView::new(def, variant)
}

fn fade_seconds(view: &WeaponInfoView<'_>) -> f32 {
    (view.dvar)("hud_fade_ammodisplay").unwrap_or(FADE_AMMO_DISPLAY)
}

/// The weapon's clip size, scaled by `player_clipSizeMultiplier` when it
/// holds more than one round.
fn clip_size(def: &WeaponDefView<'_>) -> i32 {
    let size = def.clip_size();
    if size <= 1 {
        return size;
    }
    let scaled = (f64::from(size as f32 * CLIP_SIZE_MULTIPLIER) + 9.313_225_746_154_785e-10)
        .round_ties_even() as i32;
    scaled.max(1)
}

/// The weapon's clip is at or under its low ammo share.
fn low_clip(view: &WeaponInfoView<'_>, weapon: u32) -> bool {
    if weapon == 0 {
        return false;
    }
    let Some(def) = def(view, weapon) else {
        return false;
    };
    // A weapon with unlimited ammo has no clip.
    if def.clip_only() || def.unlimited_ammo() {
        return false;
    }
    let clip = (view.clip)(weapon);
    if clip < 0 {
        return false;
    }
    let clip = clip.min(AMMO_COUNT_MAX);
    let size = clip_size(&def).min(AMMO_COUNT_MAX);
    size > 0 && clip as f32 <= size as f32 * def.low_ammo_warning_threshold()
}

/// The rounds beside the weapon's clip are at or under a fifth of the most
/// it can carry.
fn low_stock(view: &WeaponInfoView<'_>, ps: &PlayerState, weapon: u32) -> bool {
    if weapon == 0 {
        return false;
    }
    let Some(def) = def(view, weapon) else {
        return false;
    };
    if def.unlimited_ammo() {
        return false;
    }
    let stock = stock(view, &def, weapon).min(AMMO_COUNT_MAX);
    let max = max_stock(view, ps, &def, weapon);
    if max < 0 {
        return false;
    }
    let max = max.min(AMMO_COUNT_MAX);
    stock as f32 <= max as f32 * LOW_STOCK_FRAC
}

/// The rounds the weapon has beside its clip: a clip-only weapon's are its
/// clip. A weapon under a shared cap counts its own (no zombies player
/// holds two weapons of one cap).
fn stock(view: &WeaponInfoView<'_>, def: &WeaponDefView<'_>, weapon: u32) -> i32 {
    if def.clip_only() {
        (view.clip)(weapon)
    } else {
        (view.stock)(weapon)
    }
}

/// The most rounds the player can carry beside the weapon's clip: its
/// shared cap; a clip-only weapon's clip size; else the most of every
/// weapon held that shares its rounds.
fn max_stock(
    view: &WeaponInfoView<'_>,
    ps: &PlayerState,
    def: &WeaponDefView<'_>,
    weapon: u32,
) -> i32 {
    if def.shared_ammo_cap() > 0 {
        return def.shared_ammo_cap();
    }
    if def.clip_only() {
        return clip_size(def);
    }
    let number = (view.ammo_number)(weapon);
    let mut sum = 0;
    for held in ps.held_weapons.iter().map(|h| h.weapon).filter(|&w| w != 0) {
        if (view.ammo_number)(held) != number {
            continue;
        }
        let Some(other) = self::def(view, held) else {
            continue;
        };
        if other.shared_ammo_cap() > 0 {
            return other.shared_ammo_cap();
        }
        sum += if other.ammo_count_clip_relative() {
            other.max_ammo() * clip_size(&other)
        } else {
            other.max_ammo()
        };
    }
    sum
}

/// The weapon info's alpha by the time it was shown: whole until its last
/// 700 ms, then down to nothing; nothing while `cg_drawpaused` is off.
fn fade(view: &WeaponInfoView<'_>, shown_at: i32) -> f32 {
    let seconds = fade_seconds(view);
    let ms = (f64::from(seconds * 1000.0) + 9.313_225_746_154_785e-10).round_ties_even() as i32;
    fade_for(view, shown_at, ms)
}

/// An offhand class's icons. They show while the player lives and nothing
/// hides them, with the time the weapon info was shown (Black Ops shows
/// both together) and their own `hud_fade_offhand`; the weapon is the
/// first held of that class with rounds (in zombies, a lethal or special
/// one while a grenade is held back), else the first held of it; one icon
/// per round in the clips of the class's weapons.
fn offhand(view: &WeaponInfoView<'_>, class: i32, shown_at: i32) -> Option<OffhandIcons> {
    let mut player = *view.player;
    let (ps, _) = crate::movement::unpack(&mut player);
    let on_vehicle =
        ps.e_flags & EF_TURRET_OR_VEHICLE != 0 && !WS_VEHICLE_OFFHAND.contains(&ps.weaponstate);
    if PM_TYPE_LAST_INFO < ps.pm_type
        || ps.weap_flags & WEAP_FLAG_HIDE_OFFHAND != 0
        || ps.e_flags & EF_OTHER_CROSSHAIR != 0
        || (view.dvar)("cg_drawpaused").unwrap_or(1.0) == 0.0
        || shown_at == 0
        || on_vehicle
    {
        return None;
    }
    let held: Vec<(u32, WeaponDefView<'_>)> = ps
        .held_weapons
        .iter()
        .filter(|held| held.weapon != 0)
        .filter_map(|held| Some((held.weapon, def(view, held.weapon)?)))
        .filter(|(_, def)| def.offhand_class() == class)
        .collect();
    let weapon = held
        .iter()
        .find(|(weapon, def)| {
            (0 < ps.throw_back_grenade_time_left
                && OFFHAND_SLOTS_THROWBACK.contains(&def.offhand_slot()))
                || 0 < (view.clip)(*weapon) + (view.stock)(*weapon)
        })
        .or(held.first())
        .map(|(weapon, _)| *weapon)?;
    let seconds = (view.dvar)("hud_fade_offhand").unwrap_or(FADE_OFFHAND);
    let ms = (f64::from(seconds * 1000.0) + 9.313_225_746_154_785e-10).round_ties_even() as i32;
    let alpha = fade_by(view, seconds, shown_at, ms);
    (alpha != 0.0).then(|| OffhandIcons {
        weapon,
        count: held.iter().map(|(weapon, _)| (view.clip)(*weapon)).sum(),
        alpha,
        step: OFFHAND_STEP,
        falloff: OFFHAND_FALLOFF,
    })
}

/// The round counts take the weapon info's fade, and draw nothing while the
/// weapon flags hide the weapon info.
fn ammo_fade(view: &WeaponInfoView<'_>, shown_at: i32) -> f32 {
    let mut player = *view.player;
    let (ps, _) = crate::movement::unpack(&mut player);
    if ps.weap_flags & WEAP_FLAG_HIDE_INFO != 0 {
        return 0.0;
    }
    fade(view, shown_at)
}

/// The weapon name shows for 1.8 s from the last change of selection, by
/// the same fade.
fn name_fade(view: &WeaponInfoView<'_>) -> f32 {
    fade_for(view, view.selected_at, NAME_SHOW_MS)
}

/// The clip printed to the digits of the weapon's clip size (`%01i`,
/// `%02i`, `%03i`), the stock as `/ %i`, red when low.
fn ammo_text(view: &WeaponInfoView<'_>, clip: i32, stock: i32) -> WeaponInfoAmmo {
    let mut player = *view.player;
    let (ps, _) = crate::movement::unpack(&mut player);
    let weapon = current_weapon(view, &ps);
    let size = def(view, weapon).map_or(0, |def| clip_size(&def));
    let digits = if size < 10 {
        1
    } else if size < 100 {
        2
    } else {
        3
    };
    WeaponInfoAmmo {
        clip: format!("{clip:0digits$}"),
        stock: format!("/ {stock}"),
        stock_rgb: low_stock(view, &ps, weapon).then_some(LOW_STOCK_RGB),
    }
}

/// Alpha for something shown at `shown_at` for `ms`: whole until its last
/// 700 ms, then down to nothing; whole while `hud_fade_ammodisplay` is 0,
/// nothing while `cg_drawpaused` is off.
fn fade_for(view: &WeaponInfoView<'_>, shown_at: i32, ms: i32) -> f32 {
    fade_by(view, fade_seconds(view), shown_at, ms)
}

/// [`fade_for`] by a fade time other than `hud_fade_ammodisplay`'s.
fn fade_by(view: &WeaponInfoView<'_>, seconds: f32, shown_at: i32, ms: i32) -> f32 {
    if (view.dvar)("cg_drawpaused").unwrap_or(1.0) == 0.0 {
        return 0.0;
    }
    if seconds == 0.0 {
        return 1.0;
    }
    if shown_at == 0 {
        return 0.0;
    }
    let elapsed = view.now - shown_at;
    if elapsed >= ms {
        return 0.0;
    }
    let left = ms - elapsed;
    if left >= FADE_OUT_MS {
        1.0
    } else {
        left as f32 / FADE_OUT_MS as f32
    }
}

/// An arrow lights (alpha 0.8 over the item's own) for an action slot whose
/// weapon the player holds, unless it is equipment with no rounds left;
/// only the first and fourth slots light for a weapon. The third lights for
/// the alternate mode when the selected weapon is in it or has one, or
/// another weapon held has one.
fn arrow(view: &WeaponInfoView<'_>, slot: usize, item_alpha: f32, shown_at: i32) -> f32 {
    let mut player = *view.player;
    let (ps, _) = crate::movement::unpack(&mut player);
    let alpha = if slot_lit(view, &ps, slot) {
        ARROW_LIT_ALPHA
    } else {
        item_alpha
    };
    alpha * fade(view, shown_at)
}

/// A key label lights by the arrows' rule: its green and blue take 0.8 and
/// 0.4 and its alpha is 0.8; then it takes the weapon info's fade.
fn bind_label(view: &WeaponInfoView<'_>, slot: usize, color: [f32; 4], shown_at: i32) -> [f32; 4] {
    let mut player = *view.player;
    let (ps, _) = crate::movement::unpack(&mut player);
    let mut color = color;
    if slot_lit(view, &ps, slot) {
        color[1] *= BIND_LIT_GREEN;
        color[2] *= BIND_LIT_BLUE;
        color[3] = ARROW_LIT_ALPHA;
    }
    color[3] *= fade(view, shown_at);
    color
}

fn slot_lit(view: &WeaponInfoView<'_>, ps: &PlayerState, slot: usize) -> bool {
    let (kind, weapon) = (view.action_slot)(slot);
    match kind {
        ACTION_SLOT_WEAPON => {
            if !owns(ps, weapon) {
                return false;
            }
            let Some(def) = def(view, weapon) else {
                return false;
            };
            let empty =
                (view.clip)(weapon) == 0 && (view.stock)(weapon) == 0 && !def.unlimited_ammo();
            if def.offhand_slot() == OFFHAND_SLOT_EQUIPMENT && empty {
                return false;
            }
            slot == 0 || slot == 3
        }
        ACTION_SLOT_ALT_WEAPON => {
            let selected = view.selected;
            let alt_mode =
                def(view, selected).is_some_and(|d| d.inventory_type() == INVENTORY_ALT_MODE);
            (alt_mode || has_alternate(view, ps, selected)) && slot == 2
        }
        _ => false,
    }
}

/// The selected weapon has an alternate, or another weapon held (a primary,
/// not a carried killstreak) has one, the player holding two weapons or
/// more.
fn has_alternate(view: &WeaponInfoView<'_>, ps: &PlayerState, selected: u32) -> bool {
    if (view.alternate)(selected) != 0 {
        return true;
    }
    let held: Vec<u32> = ps
        .held_weapons
        .iter()
        .map(|h| h.weapon)
        .filter(|&w| w != 0)
        .collect();
    held.len() >= 2
        && held.iter().any(|&w| {
            w != selected
                && (view.alternate)(w) != 0
                && def(view, w)
                    .is_some_and(|d| d.inventory_type() == 0 && !d.is_carried_killstreak_weapon())
        })
}

/// Pressing an action slot whose weapon is equipment the player holds, had
/// not selected and has no rounds of (and no detonator) raises "no
/// equipment".
fn slot_hint(view: &WeaponInfoView<'_>) -> Option<i32> {
    use movement_t5::state::{pm_flags, pm_type, weap_flags};
    let mut player = *view.player;
    let (ps, _) = crate::movement::unpack(&mut player);
    if matches!(
        ps.weaponstate,
        ws::DROPPING_ALT | ws::RAISING | ws::RAISING_ALT | ws::DEPLOYING
    ) || ps.pm_flags & (pm_flags::RESPAWNED | pm_flags::FROZEN | pm_flags::DIVING) != 0
        || ps.e_flags2 & 0x4000_0000 != 0
        || ps.pm_type >= 9
        || ps.weap_flags & weap_flags::CYCLING_DISABLED != 0
        || matches!(
            ps.pm_type,
            pm_type::LAST_STAND | pm_type::LAST_STAND_REVIVED
        )
        || ps.weap_flags & WEAP_FLAG_HIDE_INFO != 0
        || ps.e_flags2 & 0x1000_0000 != 0
    {
        return None;
    }
    let (kind, weapon) = (view.action_slot)(view.slot_pressed);
    if kind != ACTION_SLOT_WEAPON || weapon == view.slot_pressed_from {
        return None;
    }
    let held_killstreak = def(view, ps.weapon).is_some_and(|d| d.is_carried_killstreak_weapon());
    let slot_def = def(view, weapon)?;
    if held_killstreak && slot_def.weapon_class() == WEAPCLASS_KILLSTREAK_ALT {
        return None;
    }
    if !owns(&ps, weapon) || ps.weapon == weapon {
        return None;
    }
    let empty = (view.clip)(weapon) == 0 && (view.stock)(weapon) == 0 && !slot_def.unlimited_ammo();
    (slot_def.offhand_slot() == OFFHAND_SLOT_EQUIPMENT && empty && !slot_def.has_detonator())
        .then_some(crate::code_hud::HINT_NO_EQUIPMENT)
}

/// Black Ops' low ammo warning: `PLATFORM_RELOAD` when the clip is low and
/// rounds are left beside it, `WEAPON_NO_AMMO` when every clip is empty,
/// else `PLATFORM_LOW_AMMO_NO_RELOAD`; its colour pulses between the
/// warning's two colour dvars and takes the weapon info's fade. Not drawn
/// while reloading, on a turret or vehicle, or past movement type 8.
fn low_ammo(view: &WeaponInfoView<'_>, shown_at: i32) -> Option<LowAmmoWarning> {
    let mut player = *view.player;
    let (ps, _) = crate::movement::unpack(&mut player);
    if ps.pm_type > PM_TYPE_LAST_INFO
        || ps.e_flags & EF_NO_LOW_AMMO != 0
        || ws::is_reloading(ps.weaponstate)
        || ps.weapon == 0
    {
        return None;
    }
    let fade = if ps.weap_flags & WEAP_FLAG_HIDE_INFO != 0 {
        0.0
    } else {
        fade(view, shown_at)
    };
    if fade == 0.0 {
        return None;
    }
    let weapon = current_weapon(view, &ps);
    let def = def(view, weapon)?;
    let left = if def.dual_wield() {
        (view.left_hand)(weapon)
    } else {
        0
    };
    if !low_clip(view, weapon) && !(def.dual_wield() && low_clip(view, left)) {
        return None;
    }
    if def.ammo_counter_clip() == 0 {
        return None;
    }
    let stock = (view.stock)(weapon);
    let (stock_left, clips_empty) = if def.dual_wield() {
        (
            (view.stock)(left) > 0,
            (view.clip)(weapon) == 0 && (view.clip)(left) == 0,
        )
    } else {
        (false, (view.clip)(weapon) == 0)
    };
    let (key, colors) = if stock > 0 || stock_left {
        if def.no_partial_reload() || clip_size(&def) == 1 {
            return None;
        }
        ("PLATFORM_RELOAD", RELOAD_COLORS)
    } else if clips_empty && !def.is_carried_killstreak_weapon() {
        ("WEAPON_NO_AMMO", NO_AMMO_COLORS)
    } else {
        if def.no_partial_reload() || def.guided_missile_type() == GUIDED_NO_WARNING {
            return None;
        }
        if def.dual_wield() && !(low_clip(view, weapon) && low_clip(view, left)) {
            return None;
        }
        ("PLATFORM_LOW_AMMO_NO_RELOAD", NO_RELOAD_COLORS)
    };
    let mut color = pulse(colors, view.now);
    color[3] *= fade;
    Some(LowAmmoWarning {
        key,
        bind: "+reload",
        color,
    })
}

/// The warning's colour between its two colour dvars by a sine pulse of
/// the client time.
fn pulse(colors: [[u8; 4]; 2], now: i32) -> [f32; 4] {
    let half = (PULSE_MAX - PULSE_MIN) * 0.5;
    let angle = now as f32 * PULSE_FREQ * 0.006_283_185_4;
    let k = f64::from(angle).sin() as f32 * half + (PULSE_MIN + half);
    let mut out = [0.0; 4];
    for (channel, value) in out.iter_mut().enumerate() {
        let from = f32::from(colors[0][channel]) * (1.0 / 255.0);
        let to = f32::from(colors[1][channel]) * (1.0 / 255.0);
        *value = (to - from) * k + from;
    }
    out
}
