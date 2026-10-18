//! Black Ops' camera from above the players, which Dead Ops Arcade turns on
//! with `player_topDownCamMode`: the client's view in its place, the mouse
//! aim that turns the player toward a cursor around it, and the moves that
//! follow the camera's yaw rather than the view's.

use game_api::top_down::{TopDownCamera, TopDownPose, TopDownRules, TopDownScene, TopDownState};
use game_api::unknown;
use movement_t5::math::{angle_normalize_180, angle_vectors, vectoyaw};

pub(crate) const RULES: TopDownRules = TopDownRules {
    camera,
    view,
    aim,
    move_yaw,
    gaps: &[
        unknown!(
            "t5.top_down.aim_target",
            "the top-down aim holds a pitch of 3 degrees: Black Ops pitches it at the target its aim assist finds near the aimed yaw, and turns the pitch at that target's own rate",
            "the aim assist's target pick and its lock rate (executable)"
        ),
        unknown!(
            "t5.top_down.tween",
            "the top-down camera eases after the player in every frame: Black Ops jumps it instead while a camera tween (startcameratween) runs, and runs that tween",
            "the client's camera tween state from the start-camera-tween event (executable)"
        ),
        unknown!(
            "t5.top_down.body",
            "under the top-down camera the player's legs and torso turn as in the player's own view: Black Ops turns them by the camera's yaw and the move's direction on the screen",
            "the player animation's top-down yaw (executable)"
        ),
    ],
};

/// `player_topDownCamMode`'s ways of following: the player's yaw, the
/// camera's yaw, the players together, a fixed point.
const MODE_PLAYER_YAW: i32 = 1;
const MODE_CAMERA_YAW: i32 = 2;
const MODE_PLAYERS: i32 = 3;
const MODE_FIXED: i32 = 4;

/// The dvars' registered defaults and domains.
const OFFSET_DEFAULT: [f32; 3] = [0.0, 0.0, 500.0];
const OFFSET_DOMAIN: f32 = 3000.0;
const ANGLES_DEFAULT: [f32; 3] = [75.0, 0.0, 0.0];
const ANGLES_DOMAIN: f32 = 180.0;
const CENTER_DEFAULT: [f32; 3] = [-7926.0, -2838.0, -88.0];
const CENTER_DOMAIN: f32 = 20000.0;
const CURSOR_DIST_DEFAULT: f32 = 250.0;
const CURSOR_DIST_MIN: f32 = 1.0;

/// The clients the players' camera frames: the first four.
pub(crate) const CAMERA_PLAYERS: usize = 4;

/// One player alone: the camera stands this much further from the centre
/// than halfway.
const LONE_PLAYER_PULL: f32 = 1.4;
/// The spread across the screen (world y) that starts the pull back and the
/// one that doubles it.
const SPREAD_ACROSS: (f32, f32) = (200.0, 1800.0);
/// The spread up the screen (world x) that starts the pull back and centres
/// the camera across, and the one that does it fully.
const SPREAD_UP: (f32, f32) = (50.0, 600.0);
/// A 4:3 screen pulls back twice as fast for the spread across.
const FOUR_BY_THREE_PULL: f32 = 2.0;
/// How far the camera pulls back at a full spread, alone or not.
const PULL_BACK: f32 = 350.0;
/// What more than one player pulls back at once, and takes from the rest.
const PULL_BACK_PLAYERS: f32 = 200.0;
/// The camera eases after the players at this rate per second, unless it
/// is this far (squared) behind.
const EASE_RATE: f32 = 5.0;
const EASE_LIMIT_SQ: f32 = 1_000_000.0;

/// `ClearBounds`' empty box.
const BOUNDS_EMPTY: f32 = 131_072.0;

/// The aim: a stick (or the cursor) closer to the middle than this aims
/// nowhere; the pitch the aim holds; the rate it turns at (`aim_autoaim_lerp`).
const AIM_DEADZONE: f64 = 0.3;
const AIM_PITCH: f32 = 3.0;
const AIM_LERP: f32 = 40.0;

fn dvar_vec3(dvar: &dyn Fn(&str) -> Option<String>, name: &str) -> Option<[f32; 3]> {
    let text = dvar(name)?;
    let mut out = [0.0; 3];
    let mut parts = text
        .split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ','))
        .filter(|part| !part.is_empty());
    for slot in &mut out {
        *slot = parts.next()?.parse().ok()?;
    }
    Some(out)
}

fn camera(dvar: &dyn Fn(&str) -> Option<String>) -> Option<TopDownCamera> {
    let mode = dvar("player_topdowncammode")?
        .trim()
        .parse::<f32>()
        .ok()?
        .clamp(0.0, MODE_FIXED as f32) as i32;
    if mode == 0 {
        return None;
    }
    let within = |v: [f32; 3], domain: f32| v.map(|c| c.clamp(-domain, domain));
    Some(TopDownCamera {
        mode,
        offset: within(
            dvar_vec3(dvar, "player_topdowncamoffset").unwrap_or(OFFSET_DEFAULT),
            OFFSET_DOMAIN,
        ),
        angles: within(
            dvar_vec3(dvar, "player_topdowncamangles").unwrap_or(ANGLES_DEFAULT),
            ANGLES_DOMAIN,
        ),
        center: within(
            dvar_vec3(dvar, "player_topdowncamcenterpos").unwrap_or(CENTER_DEFAULT),
            CENTER_DOMAIN,
        ),
        cursor_dist: dvar("player_topdowncursordist")
            .and_then(|text| text.trim().parse::<f32>().ok())
            .map_or(CURSOR_DIST_DEFAULT, |dist| dist.max(CURSOR_DIST_MIN)),
    })
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scaled(v: [f32; 3], s: f32) -> [f32; 3] {
    v.map(|c| c * s)
}

/// The view: the camera's angles, and where it stands for its mode.
fn view(camera: &TopDownCamera, scene: &TopDownScene<'_>, state: &mut TopDownState) -> TopDownPose {
    let angles = if camera.mode == MODE_PLAYER_YAW {
        [camera.angles[0], scene.viewangles[1], 0.0]
    } else {
        camera.angles
    };
    let origin = match camera.mode {
        MODE_PLAYERS => players_view(camera, scene, state, angles),
        MODE_FIXED => add(camera.center, camera.offset),
        _ => add(scene.origin, camera.offset),
    };
    state.last_origin = Some(origin);
    TopDownPose { origin, angles }
}

/// The players' camera: over the middle of the players and the centre,
/// pulled back as they spread, easing after them.
fn players_view(
    camera: &TopDownCamera,
    scene: &TopDownScene<'_>,
    state: &TopDownState,
    angles: [f32; 3],
) -> [f32; 3] {
    let (forward, right, up) = angle_vectors(angles);
    let left = right.map(|c| -c);
    let mut mins = [BOUNDS_EMPTY; 3];
    let mut maxs = [-BOUNDS_EMPTY; 3];
    let players = scene.players.iter().take(CAMERA_PLAYERS);
    let count = players.len();
    for &player in players {
        add_to_bounds(&mut mins, &mut maxs, player);
    }
    let spread = [maxs[0] - mins[0], maxs[1] - mins[1]];
    add_to_bounds(&mut mins, &mut maxs, camera.center);
    let mut middle: [f32; 3] = std::array::from_fn(|axis| (mins[axis] + maxs[axis]) * 0.5);
    if count < 2 {
        middle = std::array::from_fn(|axis| {
            (middle[axis] - camera.center[axis]) * LONE_PLAYER_PULL + camera.center[axis]
        });
    }
    let [left_by, up_by, back_by] = camera.offset;
    let mut origin = add(
        add(
            add(middle, scaled(forward, -back_by)),
            scaled(left, left_by),
        ),
        scaled(up, up_by),
    );
    let mut across = 0.0;
    if spread[1].abs() > SPREAD_ACROSS.0 {
        across = (spread[1].abs() - SPREAD_ACROSS.0) / (SPREAD_ACROSS.1 - SPREAD_ACROSS.0);
        if scene.four_by_three {
            across *= FOUR_BY_THREE_PULL;
        }
    }
    let mut pull = 0.0;
    if spread[0].abs() > SPREAD_UP.0 {
        pull = (spread[0].abs() - SPREAD_UP.0) / (SPREAD_UP.1 - SPREAD_UP.0);
        let centring = if -pull >= 0.0 { 0.0 } else { pull.min(1.0) };
        origin[1] = (camera.center[1] - origin[1]) * centring + origin[1];
    }
    if across >= pull {
        pull = across;
    }
    let mut back = PULL_BACK;
    if count > 1 {
        back -= PULL_BACK_PLAYERS;
        let together = if pull > 1.0 {
            pull * PULL_BACK_PLAYERS
        } else {
            PULL_BACK_PLAYERS
        };
        origin = add(origin, scaled(forward, -together));
    }
    origin = add(origin, scaled(forward, -(back * pull)));
    if let Some(last) = state.last_origin {
        let behind = [
            origin[0] - last[0],
            origin[1] - last[1],
            origin[2] - last[2],
        ];
        if behind[0] * behind[0] + behind[1] * behind[1] + behind[2] * behind[2] < EASE_LIMIT_SQ {
            let ease = scene.frametime_ms as f32 * 0.001 * EASE_RATE;
            origin = add(last, scaled(behind, ease));
        }
    }
    origin
}

fn add_to_bounds(mins: &mut [f32; 3], maxs: &mut [f32; 3], point: [f32; 3]) {
    for axis in 0..3 {
        mins[axis] = mins[axis].min(point[axis]);
        maxs[axis] = maxs[axis].max(point[axis]);
    }
}

/// `from` turned toward `to` at `rate` per second; within reach it lands.
fn turn_toward(to: f32, from: f32, rate: f32, frametime: f32) -> f32 {
    let mut to = to;
    while to - from > 180.0 {
        to -= 360.0;
    }
    while to - from < -180.0 {
        to += 360.0;
    }
    let step = (to - from) * rate * frametime;
    if (to - from).abs() > 0.001 && step.abs() <= (to - from).abs() {
        to = step + from;
    }
    angle_normalize_180(to)
}

/// The mouse moves the cursor round the player, and the player turns
/// toward it: the cursor keeps its distance, so the mouse steers its
/// direction.
fn aim(
    camera: &TopDownCamera,
    mouse: [f32; 2],
    frametime: f32,
    viewangles: [f32; 3],
    state: &mut TopDownState,
) -> Option<[f32; 2]> {
    if mouse == [0.0; 2] {
        return None;
    }
    let mut toward = [mouse[0] + state.cursor[0], mouse[1] + state.cursor[1]];
    let mut length = (toward[1] * toward[1] + toward[0] * toward[0]).sqrt();
    if -length >= 0.0 {
        length = 1.0;
    }
    toward = toward.map(|c| c / length);
    state.cursor = toward.map(|c| c * camera.cursor_dist);
    let [right, down] = toward;
    let reach = (down * down + right * right).sqrt().min(1.0);
    let yaw = if f64::from(reach) <= AIM_DEADZONE {
        viewangles[1]
    } else {
        angle_normalize_180(vectoyaw([down, right, 0.0]) + camera.angles[1] + 180.0)
    };
    let [pitch_from, yaw_from] = state.aim.unwrap_or([viewangles[0], viewangles[1]]);
    let pitch = turn_toward(AIM_PITCH, pitch_from, AIM_LERP, frametime);
    let yaw = turn_toward(yaw, yaw_from, AIM_LERP, frametime);
    state.aim = Some([pitch, yaw]);
    Some([
        angle_normalize_180(pitch - viewangles[0]),
        angle_normalize_180(yaw - viewangles[1]),
    ])
}

/// From the camera's own yaw up, the moves follow the camera.
fn move_yaw(camera: &TopDownCamera) -> Option<f32> {
    (camera.mode >= MODE_CAMERA_YAW).then_some(camera.angles[1])
}
