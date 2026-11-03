//! Black Ops multiplayer's grenade throwback. Each frame the server's use
//! search can put a live grenade near a player under their cursor hint, with
//! what is left of its fuse; the frag key then takes the grenade off the
//! ground into the player's hand, on that fuse, and movement's offhand path
//! throws it. Throwing it needs no grenade of the player's own.

use fastfile_t5::weapon_def::WeaponDefView;
use game_api::fire::{FireWorld, GameMissile, NotifyArg};
use game_api::{Unknown, unknown};
use movement_t5::PlayerState;
use movement_t5::perks::mp::{self as perks, PIN_BACK};
use movement_t5::state::{ENTITYNUM_NONE, pm_flags, pm_type};
use movement_t5::weapon::state as ws;

use crate::missile::{LinkTag, State};

/// `player_throwbackInnerRadius`: how near, flat, a grenade comes under the
/// hint; `perk_throwbackInnerRadius` with Flak Jacket Pro.
const INNER_RADIUS: f32 = 90.0;
const PERK_INNER_RADIUS: f32 = 120.0;
/// `player_throwbackOuterRadius`: how far from the eye it stays under it.
const OUTER_RADIUS: f32 = 160.0;
/// `bg_maxGrenadeIndicatorSpeed`: a faster grenade is still flying.
const MAX_SPEED: f32 = 100.0;
/// `perk_grenadeTossBackTimer`: the least fuse Flak Jacket Pro leaves.
const PERK_TOSS_BACK_MS: i32 = 2500;
/// How far above and below the eye the search looks.
const SEARCH_HEIGHT: f32 = 96.0;
/// A candidate's score: its distance, plus how far from the view it is at
/// this much for straight behind, less what puts a missile before others.
const FACING_SCORE: f32 = 256.0;
const MISSILE_SCORE: f32 = 512.0;
/// The sight line from the eye: solid and glass.
const SIGHT_MASK: u32 = 0x11;
/// Water up to the eyes: no throwback.
const UNDERWATER: i32 = 3;
/// The cursor hint's owner when the grenade's owner has gone.
const OWNER_GONE: i32 = 0x3fe;
/// The offhand classes whose switch event takes a grenade: frag and sticky.
const THROWN_BACK_CLASSES: [i32; 2] = [1, 4];
const DEPLOY_STATES: std::ops::RangeInclusive<i32> = 0x24..=0x26;

pub(crate) fn throwback_unknown() -> &'static Unknown {
    unknown!(
        "t5.mp.throwback",
        "the use search ranks grenades before every use trigger and its sight line stops at the world; its turret, vehicle and two unidentified gates are not tested, and the cursor hint's text is not drawn",
        "the grenade's place in Black Ops' full use search and the cursor hint's text"
    )
}

fn dvar(world: &dyn FireWorld, name: &str, default: f32) -> f32 {
    world
        .dvar(name)
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

fn eye(ps: &PlayerState) -> [f32; 3] {
    [
        ps.origin[0],
        ps.origin[1],
        ps.origin[2] + ps.view_height_current,
    ]
}

fn forward(angles: [f32; 3]) -> [f32; 3] {
    let (sp, cp) = angles[0].to_radians().sin_cos();
    let (sy, cy) = angles[1].to_radians().sin_cos();
    [cp * cy, cp * sy, -sp]
}

fn throwback_weapon(missile: &GameMissile, world: &dyn FireWorld) -> bool {
    world.weapon(missile.weapon).is_some_and(|weapon| {
        WeaponDefView::new(&weapon.def, &weapon.variant).is_some_and(|def| def.throwback())
    })
}

/// The search's test of one grenade.
fn candidate(ps: &PlayerState, missile: &GameMissile, world: &mut dyn FireWorld) -> bool {
    let state = State::read(&missile.game);
    if ps.water_level >= UNDERWATER
        || !matches!(state.link, LinkTag::None)
        || !throwback_weapon(missile, world)
    {
        return false;
    }
    if missile.owner != ps.client_num {
        let team = |world: &mut dyn FireWorld, entity| world.entity(entity).and_then(|e| e.team);
        let (own, theirs) = (team(world, ps.client_num), team(world, missile.owner));
        if own.is_some() && own == theirs && dvar(world, "scr_team_fftype", 0.0) == 0.0 {
            return false;
        }
    }
    if ps.cursor_hint_ent_index != missile.entity {
        let inner = if perks::has(ps, PIN_BACK) {
            dvar(world, "perk_throwbackInnerRadius", PERK_INNER_RADIUS)
        } else {
            dvar(world, "player_throwbackInnerRadius", INNER_RADIUS)
        };
        let dx = ps.origin[0] - missile.origin[0];
        let dy = ps.origin[1] - missile.origin[1];
        if inner * inner < dx * dx + dy * dy {
            return false;
        }
    }
    let speed = dvar(world, "bg_maxGrenadeIndicatorSpeed", MAX_SPEED);
    let [dx, dy, dz] = missile.position.delta;
    dx * dx + dy * dy + dz * dz <= speed * speed
}

/// The nearest grenade to the eye and the view the eye can see.
fn search(ps: &PlayerState, missiles: &[GameMissile], world: &mut dyn FireWorld) -> Option<i32> {
    let eye = eye(ps);
    let view = forward(ps.viewangles);
    let outer = dvar(world, "player_throwbackOuterRadius", OUTER_RADIUS);
    let mut found: Vec<(f32, &GameMissile)> = Vec::new();
    for missile in missiles {
        let to: [f32; 3] = std::array::from_fn(|i| missile.origin[i] - eye[i]);
        let distance = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt();
        if to[2].abs() > SEARCH_HEIGHT || distance > outer || !candidate(ps, missile, world) {
            continue;
        }
        let facing = if distance > 0.0 {
            (view[0] * to[0] + view[1] * to[1] + view[2] * to[2]) / distance
        } else {
            0.0
        };
        let score = (1.0 - (facing + 1.0) * 0.5) * FACING_SCORE - MISSILE_SCORE + distance;
        found.push((score, missile));
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().find_map(|(_, missile)| {
        world
            .sight_clear(
                eye,
                missile.origin,
                [ps.client_num, missile.entity],
                SIGHT_MASK,
            )
            .then_some(missile.entity)
    })
}

/// The frame's cursor hint on a grenade: the player keeps it until another
/// grenade comes first or it is gone; what is left of its fuse holds only
/// while the search finds it, or once it is in the player's hand.
pub(crate) fn use_search(client: i32, world: &mut dyn FireWorld) {
    let missiles = world.game_missiles();
    let Some(mut player) = world.player(client) else {
        return;
    };
    let (mut ps, mut last) = crate::movement::unpack(&mut player);
    let was = (ps.cursor_hint_ent_index, ps.throw_back_grenade_time_left);
    if ps.throw_back_grenade_owner == ENTITYNUM_NONE {
        ps.throw_back_grenade_time_left = 0;
    }
    let hinted = ps.cursor_hint_ent_index;
    if hinted != ENTITYNUM_NONE && hinted != 0 && !missiles.iter().any(|m| m.entity == hinted) {
        ps.cursor_hint_ent_index = ENTITYNUM_NONE;
    }
    if DEPLOY_STATES.contains(&ps.weaponstate) || ps.pm_type == pm_type::LAST_STAND {
        ps.cursor_hint_ent_index = ENTITYNUM_NONE;
    } else if !missiles.is_empty()
        && ps.pm_flags & (pm_flags::MANTLE | pm_flags::LADDER | pm_flags::SPRINTING) == 0
        && !(ws::OFFHAND_INIT..=ws::OFFHAND).contains(&ps.weaponstate)
        && world.entity(client).is_some_and(|entity| entity.health > 0)
        && let Some(entity) = search(&ps, &missiles, world)
    {
        ps.cursor_hint_ent_index = entity;
        if let Some(missile) = missiles.iter().find(|m| m.entity == entity) {
            ps.throw_back_grenade_time_left =
                State::read(&missile.game).explode_at - world.level_time();
        }
    }
    if was != (ps.cursor_hint_ent_index, ps.throw_back_grenade_time_left) {
        crate::movement::pack(&mut player, &mut ps, &mut last);
        world.store_player(client, &player);
    }
}

/// The offhand switch event of a player with a grenade under the hint: the
/// grenade leaves the ground for the hand, on what is left of its fuse, at
/// least Flak Jacket Pro's `perk_grenadeTossBackTimer`.
pub(crate) fn pick_up(client: i32, weapon: u32, world: &mut dyn FireWorld) -> bool {
    let class = world
        .weapon(weapon)
        .and_then(|w| WeaponDefView::new(&w.def, &w.variant).map(|def| def.offhand_class()));
    if !class.is_some_and(|class| THROWN_BACK_CLASSES.contains(&class)) {
        return false;
    }
    let Some(mut player) = world.player(client) else {
        return false;
    };
    let (mut ps, mut last) = crate::movement::unpack(&mut player);
    let Some(missile) = world
        .game_missiles()
        .into_iter()
        .find(|m| m.entity == ps.cursor_hint_ent_index)
    else {
        return false;
    };
    if !throwback_weapon(&missile, world) || ps.throw_back_grenade_time_left == 0 {
        return false;
    }
    ps.throw_back_grenade_owner = if world.player(missile.owner).is_some() {
        missile.owner
    } else {
        OWNER_GONE
    };
    let left = ps.throw_back_grenade_time_left;
    ps.grenade_time_left = if perks::has(&ps, PIN_BACK) {
        left.max(dvar(world, "perk_grenadeTossBackTimer", PERK_TOSS_BACK_MS as f32) as i32)
    } else {
        left
    };
    ps.offhand_index = missile.weapon;
    ps.cursor_hint_ent_index = ENTITYNUM_NONE;
    crate::movement::pack(&mut player, &mut ps, &mut last);
    world.store_player(client, &player);
    world.remove_game_missile(missile.entity);
    true
}

/// A grenade left a player's hand: its first owner is told to their scripts.
pub(crate) fn thrown(client: i32, grenade: Option<i32>, world: &mut dyn FireWorld) {
    let (Some(grenade), Some(mut player)) = (grenade, world.player(client)) else {
        return;
    };
    let (ps, _) = crate::movement::unpack(&mut player);
    let owner = ps.throw_back_grenade_owner;
    if owner != ENTITYNUM_NONE && owner != OWNER_GONE {
        world.notify(
            client,
            "grenade_throwback",
            &[NotifyArg::Entity(owner), NotifyArg::Entity(grenade)],
        );
    }
}
