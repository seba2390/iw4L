//! Black Ops multiplayer's compass: `hud_hardcore`'s `compass_old` menu,
//! its map turned with the view, the player's arrow at its centre and the
//! enemies its radar shows.

use game_api::hud::{
    CompassActor, CompassFrame, CompassPing, CompassRadar, CompassRadarLine, CompassRadarRules,
    CompassRadarView, CompassRules,
};
use game_api::movement::MovePlayer;
use game_api::{Unknown, unknown};
use movement_t5::state::{e_flags, e_flags2};

/// `compassSize`, `compassMaxRange`, `compassRotation`,
/// `compassPlayerWidth` and `compassPlayerHeight` as registered.
pub(crate) static COMPASS: CompassRules = CompassRules {
    map_owner_draw: 159,
    player_owner_draw: 150,
    size: 1.0,
    max_range: 2500.0,
    rotation: true,
    player_size: [18.75, 18.75],
    frame,
    player_arrow,
    radar: Some(&RADAR),
};

/// The compass's enemies (owner draw 170), teammates (158), the spy
/// plane's sweep line (332) and the counter-spy plane's static (328).
static RADAR: CompassRadarRules = CompassRadarRules {
    enemies_owner_draw: 170,
    friendlies_owner_draw: 158,
    line_owner_draw: 332,
    static_owner_draw: 328,
    update: radar_update,
    pings: radar_pings,
    friendlies: friendly_pings,
    line: radar_line,
};

/// An enemy the client has not seen for this long gets no ping.
const SEEN_MS: i32 = 1500;
/// The perks that hide a player from a ping: Ghost hides it from the spy
/// plane's sweep; nothing hides it from the satellite.
const PERK_GPSJAMMER: u32 = 25;
/// Always on the radar.
const PERK_SHOWONRADAR: u32 = 47;
/// The client's team numbers with no teammates on the compass.
const TEAM_FREE: i32 = 0;
const TEAM_SPECTATOR: i32 = 3;
/// A weapon shot's ping holds full strength this long, then fades over
/// `compassSoundPingFadeTime`.
const FIRE_PING_DELAY_MS: i32 = 200;

/// How far a fire ping has faded: `1` until `at`, then down over
/// `compassSoundPingFadeTime`; `None` once gone or when it never fired.
fn fire_left(view: &CompassRadarView<'_>, at: i32) -> Option<f32> {
    let fade = dvar(view, "compassSoundPingFadeTime", 2.0) * 1000.0;
    if at == 0 || at as f32 + fade <= view.now as f32 {
        return None;
    }
    Some(if at < view.now {
        1.0 - (view.now - at) as f32 / fade
    } else {
        1.0
    })
}

fn dvar(view: &CompassRadarView<'_>, name: &str, default: f32) -> f32 {
    (view.dvar)(name).unwrap_or(default)
}

/// The sweep's line at `progress` (0 to 1 across the map, west to east):
/// its normal and distance. It is as thick as `compassRadarLineThickness`
/// of the compass's range, plus the range's diagonal, so it leaves no
/// enemy on the compass unswept.
fn sweep_line(view: &CompassRadarView<'_>, progress: f32) -> ([f32; 2], f32) {
    let max_range = dvar(view, "compassMaxRange", 2500.0);
    let compass = dvar(view, "compassRadarLineThickness", 0.4) * max_range
        + max_range * std::f32::consts::SQRT_2;
    let map = dvar(view, "cg_hudMapRadarLineThickness", 0.15) * view.frame.size[0];
    let half = compass.max(map) * 0.5;
    let [n0, n1] = view.frame.north;
    let normal = [n1, -n0];
    let [x, y] = view.frame.upper_left;
    let distance = (half * 2.0 + view.frame.size[0]) * progress + y * -n0 + x * n1 - half;
    (normal, distance)
}

fn behind(line: ([f32; 2], f32), p: [f32; 2]) -> bool {
    line.0[0] * p[0] + line.0[1] * p[1] - line.1 < 0.0
}

/// Where a player is, its yaw, its perk bits, and whether it is alive.
fn known(player: &game_api::hud::CompassRadarPlayer) -> ([f32; 2], f32, u64, bool) {
    let mut move_player = player.player;
    let alive = !matches!(
        move_player.move_type,
        game_api::movement::MoveType::Dead | game_api::movement::MoveType::DeadLinked
    );
    let ps = crate::movement::unpack(&mut move_player).0;
    let perks = u64::from(ps.perks) | u64::from(ps.perks_high) << 32;
    ([ps.origin[0], ps.origin[1]], ps.viewangles[1], perks, alive)
}

fn has_perk(actor: &CompassActor, bit: u32) -> bool {
    actor.perks & (1 << bit) != 0
}

/// A ping where the enemy is now, when the client saw it lately and no
/// perk hides it.
fn ping(actor: &mut CompassActor, now: i32, hidden_by: Option<u32>) {
    if now < actor.seen_ms {
        actor.seen_ms = 0;
    }
    if actor.seen_ms < now - SEEN_MS || hidden_by.is_some_and(|bit| has_perk(actor, bit)) {
        return;
    }
    actor.ping_pos = actor.pos;
    actor.radar_ms = now;
}

/// The satellite's ping: the enemy's place every frame, and every
/// `compassStaticImageUpdateTime` its still image and a fresh fade.
fn satellite_ping(view: &CompassRadarView<'_>, actor: &mut CompassActor, now: i32) {
    if now < actor.seen_ms {
        actor.seen_ms = 0;
    }
    if actor.seen_ms < now - SEEN_MS {
        return;
    }
    actor.ping_pos = actor.pos;
    if (now - actor.static_ms) as f32 > dvar(view, "compassStaticImageUpdateTime", 5000.0) {
        actor.static_ms = now;
        actor.static_pos = actor.pos;
        actor.satellite_ms = now;
    }
}

fn radar_update(view: &CompassRadarView<'_>, radar: &mut CompassRadar) {
    let now = view.now;
    if radar.last_ms.is_some_and(|last| now < last) {
        *radar = CompassRadar::default();
    }
    radar.last_ms = Some(now);
    let [spyplane, satellite] = view.levels;
    // The players the client sees move; the sweep pings one that crosses
    // its line and the satellite every one.
    let line = sweep_line(view, radar.sweep);
    for player in view
        .players
        .iter()
        .filter(|p| p.client != view.local_client)
    {
        let (origin, yaw, perks, alive) = known(player);
        let Some(actor) = radar.actors.get_mut(player.client).filter(|_| alive) else {
            continue;
        };
        actor.enemy = view.local_team == 0 || player.team != view.local_team;
        actor.perks = perks;
        if actor.enemy {
            let recent = now - SEEN_MS < actor.seen_ms;
            if spyplane != 0 && recent && behind(line, actor.pos) != behind(line, origin) {
                ping(actor, now, Some(PERK_GPSJAMMER));
            }
            if satellite != 0 && recent {
                satellite_ping(view, actor, now);
            }
        }
        actor.pos = origin;
        actor.seen_ms = now;
        actor.yaw = yaw;
    }
    // A shot pings its shooter: a teammate's firing mark, an enemy's place.
    // The executable also gates an enemy's on a bit of a client word it
    // keeps (cleared unless a teammate or a radar is up), not read here.
    for fire in view.fires {
        let Some(team) = view
            .players
            .iter()
            .find(|p| p.client == fire.client)
            .map(|p| p.team)
        else {
            continue;
        };
        let Some(actor) = radar.actors.get_mut(fire.client) else {
            continue;
        };
        if fire.client == view.local_client || team == TEAM_SPECTATOR {
            continue;
        }
        actor.fire_ms = now + FIRE_PING_DELAY_MS;
        actor.enemy = view.local_team == TEAM_FREE || team != view.local_team;
        if actor.enemy {
            actor.ping_pos = fire.pos;
        }
    }
    if spyplane == 0 && satellite == 0 {
        radar.sweep = 0.0;
        return;
    }
    let level = if satellite != 0 { satellite } else { spyplane };
    let period = if level == 2 {
        dvar(view, "compassRadarUpdateFastTime", 2.0)
    } else {
        dvar(view, "compassRadarUpdateTime", 4.0)
    };
    let before = radar.sweep;
    let advanced = before + view.frame_ms as f32 / (period * 1000.0);
    radar.sweep = advanced - advanced.floor();
    // The sweep pings the enemies between its last line and this one.
    if spyplane != 0 && before <= radar.sweep {
        let (from, to) = (sweep_line(view, before), sweep_line(view, radar.sweep));
        for (client, actor) in radar.actors.iter_mut().enumerate() {
            if client == view.local_client || !actor.enemy {
                continue;
            }
            let alive = view
                .players
                .iter()
                .any(|p| p.client == client && known(p).3);
            if alive && behind(from, actor.pos) != behind(to, actor.pos) {
                ping(actor, now, Some(PERK_GPSJAMMER));
            }
        }
    }
}

/// How much of a ping's fade is left: `1` until `at`, then down to `0`
/// over `fade` seconds; `None` when it never pinged.
fn fade_left(now: i32, at: i32, fade: f32) -> Option<f32> {
    if at == 0 && now > at {
        return None;
    }
    Some(if at < now {
        1.0 - (now - at) as f32 / (fade * 1000.0)
    } else {
        1.0
    })
}

/// The enemy jamming leaves this much of the radar's icons; an enemy
/// counter-spy plane none.
fn jam_alpha(view: &CompassRadarView<'_>) -> f32 {
    if view.global_scrambler {
        0.0
    } else {
        (1.0 - view.enemy_scrambler).max(0.0)
    }
}

fn radar_pings(view: &CompassRadarView<'_>, radar: &CompassRadar) -> Vec<CompassPing> {
    let scale = jam_alpha(view);
    let mut pings = Vec::new();
    if scale == 0.0 {
        return pings;
    }
    let now = view.now;
    let radar_fade = dvar(view, "compassRadarPingFadeTime", 4.0);
    let satellite_fade = dvar(view, "compassSatellitePingFadeTime", 10.0);
    let static_fade = dvar(view, "compassSatelliteStaticImageFadeTime", 0.0);
    let turn_from = if dvar(view, "compassRotation", 1.0) != 0.0 {
        view.view_yaw
    } else {
        view.north_yaw
    };
    for (client, actor) in radar.actors.iter().enumerate() {
        let connected = view.players.iter().any(|p| p.client == client);
        if client == view.local_client || !actor.enemy || !connected {
            continue;
        }
        let live = view.show_enemies || has_perk(actor, PERK_SHOWONRADAR);
        if live && (actor.seen_ms > now || actor.seen_ms < now - SEEN_MS) {
            continue;
        }
        let shown = |at: i32, fade: f32| at != 0 && (now as f32) < fade * 1000.0 + at as f32;
        let radar_shown = live || shown(actor.radar_ms, radar_fade);
        let satellite_shown = shown(actor.satellite_ms, satellite_fade);
        let static_shown = shown(actor.static_ms, static_fade);
        let firing = fire_left(view, actor.fire_ms);
        if !(radar_shown || satellite_shown || static_shown || firing.is_some()) {
            continue;
        }
        let pos = if live { actor.pos } else { actor.ping_pos };
        let radar_alpha = if live && actor.radar_ms < now {
            Some(1.0)
        } else {
            fade_left(now, actor.radar_ms, radar_fade)
        };
        if radar_shown && let Some(alpha) = radar_alpha.filter(|a| *a >= 0.0) {
            pings.push(CompassPing {
                pos,
                material: "compassping_enemy",
                alpha: alpha * scale,
                turn: None,
                clamps: true,
            });
        }
        if satellite_shown
            && let Some(alpha) =
                fade_left(now, actor.satellite_ms, satellite_fade).filter(|a| *a >= 0.0)
        {
            pings.push(CompassPing {
                pos,
                material: "compassping_enemydirectional",
                alpha: alpha * scale,
                turn: Some(turn_from - actor.yaw),
                clamps: true,
            });
        }
        if let Some(alpha) = firing.filter(|a| *a >= 0.0) {
            pings.push(CompassPing {
                pos,
                material: "compassping_enemyfiring",
                alpha: alpha * scale,
                turn: None,
                clamps: true,
            });
        }
        if static_fade > 0.0
            && let Some(alpha) = fade_left(now, actor.static_ms, static_fade).filter(|a| *a >= 0.0)
        {
            pings.push(CompassPing {
                pos: actor.static_pos,
                material: "compassping_enemysatellite",
                alpha,
                turn: None,
                clamps: false,
            });
        }
    }
    pings
}

/// The teammates the client saw lately, in a team game: each an arrow
/// turned to its heading (against the view when the compass turns with
/// it, else against north), faded by the enemy's jamming and gone under
/// an enemy counter-spy plane, kept at the compass's edge when off it with
/// `compassClampIcons`.
fn friendly_pings(view: &CompassRadarView<'_>, radar: &CompassRadar) -> Vec<CompassPing> {
    let scale = jam_alpha(view);
    if scale == 0.0 || matches!(view.local_team, TEAM_FREE | TEAM_SPECTATOR) {
        return Vec::new();
    }
    let now = view.now;
    let turn_from = if dvar(view, "compassRotation", 1.0) != 0.0 {
        view.view_yaw
    } else {
        view.north_yaw
    };
    radar
        .actors
        .iter()
        .enumerate()
        .filter(|&(client, actor)| {
            client != view.local_client
                && !actor.enemy
                && actor.seen_ms <= now
                && now - SEEN_MS <= actor.seen_ms
                && view.players.iter().any(|p| p.client == client)
        })
        .flat_map(|(_, actor)| {
            let turn = Some(turn_from - actor.yaw);
            let firing = fire_left(view, actor.fire_ms);
            // At full strength the firing mark stands in for the arrow.
            let arrow = (firing != Some(1.0)).then_some(CompassPing {
                pos: actor.pos,
                material: "compassping_friendly_mp",
                alpha: scale,
                turn,
                clamps: true,
            });
            let mark = firing.map(|alpha| CompassPing {
                pos: actor.pos,
                material: "compassping_friendlyfiring_mp",
                alpha: alpha.min(scale),
                turn,
                clamps: true,
            });
            arrow.into_iter().chain(mark)
        })
        .collect()
}

/// The spy plane's line where the sweep is, across the compass's map.
fn radar_line(view: &CompassRadarView<'_>, radar: &CompassRadar) -> Option<CompassRadarLine> {
    if view.levels[0] == 0 || jam_alpha(view) == 0.0 {
        return None;
    }
    let (normal, distance) = sweep_line(view, radar.sweep);
    let thickness = 1.0 / dvar(view, "compassRadarLineThickness", 0.4);
    let max_range = dvar(view, "compassMaxRange", 2500.0);
    let across = normal[0] * view.origin[0] + normal[1] * view.origin[1] - distance;
    let turn = if dvar(view, "compassRotation", 1.0) != 0.0 {
        view.north_yaw - view.view_yaw
    } else {
        0.0
    };
    Some(CompassRadarLine {
        center_s: across / max_range * thickness + 0.5,
        radius_st: thickness * 0.5,
        turn,
        material: "compass_radarline",
    })
}

/// `_compass.gsc`'s `setupMiniMap` (no required aspect ratio: Black Ops
/// registers no `scr_requiredMapAspectRatio`) and then `setminimap`'s
/// frame, which refuses corners not south-east of the north-west one.
fn frame(a: [f32; 2], b: [f32; 2], north_yaw: f32) -> Option<CompassFrame> {
    let (sin, cos) = north_yaw.to_radians().sin_cos();
    let north = [cos, sin];
    let west = [-north[1], north[0]];
    let dot = |u: [f32; 2], v: [f32; 2]| u[0] * v[0] + u[1] * v[1];
    let diff = [b[0] - a[0], b[1] - a[1]];
    let along_north = dot(diff, north);
    let side = [north[0] * along_north, north[1] * along_north];
    let (northwest, southeast) = match (dot(diff, west) > 0.0, along_north > 0.0) {
        (true, true) => (b, a),
        (true, false) => (
            [b[0] - side[0], b[1] - side[1]],
            [a[0] + side[0], a[1] + side[1]],
        ),
        (false, true) => (
            [a[0] + side[0], a[1] + side[1]],
            [b[0] - side[0], b[1] - side[1]],
        ),
        (false, false) => (a, b),
    };
    let dx = southeast[0] - northwest[0];
    let dy = southeast[1] - northwest[1];
    let width = sin * dx - cos * dy;
    let height = -(cos * dx) - dy * sin;
    (width >= 0.0 && height >= 0.0 && width * height > 0.0).then_some(CompassFrame {
        upper_left: northwest,
        north,
        size: [width, height],
    })
}

/// The arrow hides while the player looks through a vehicle; a player
/// mounted on one is drawn at the vehicle's place, which the runtime does
/// not draw.
fn player_arrow(player: &MovePlayer) -> Result<bool, &'static Unknown> {
    let mut player = *player;
    let ps = crate::movement::unpack(&mut player).0;
    if ps.e_flags2 & e_flags2::MOUNTED != 0 && ps.e_flags & e_flags::VEHICLE_VIEW != 0 {
        return Err(unknown!(
            "t5.hud.compass_mounted",
            "a player mounted on a vehicle is not drawn on Black Ops' compass (its arrow at the vehicle, turned by the vehicle's yaw)",
            "the compass's vehicle projection and the vehicle's angles (executable)"
        ));
    }
    Ok(ps.e_flags & e_flags::VEHICLE_VIEW == 0)
}
