//! Black Ops multiplayer's compass: `hud_hardcore`'s `compass_old` menu,
//! its map turned with the view and the player's arrow at its centre.

use game_api::hud::{CompassFrame, CompassRules};
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
};

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
    if ps.e_flags2 & e_flags2::MOUNTED != 0 {
        return Err(unknown!(
            "t5.hud.compass_mounted",
            "a player mounted on a vehicle is not drawn on Black Ops' compass (its arrow at the vehicle, turned by the vehicle's yaw)",
            "the compass's vehicle projection and the vehicle's angles (executable)"
        ));
    }
    Ok(ps.e_flags & e_flags::VEHICLE_VIEW == 0)
}
