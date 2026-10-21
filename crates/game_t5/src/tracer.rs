//! The beams Black Ops' client draws along its bullets: every bullet of a
//! weapon with a tracer frequency draws one from the shooter's flash point,
//! a tracer on every frequency-th bullet of that shooter and a thin bullet
//! streak on the others.

use fastfile_t5::weapon_def::WeaponDefView;
use game_api::fire::{BeamShooter, BulletBeam, BulletBeamRules, FireEntityKind, FireTeam};

pub(crate) const RULES: BulletBeamRules = BulletBeamRules { beam, start_back };

/// The client's media for the three beams (the session loads them as
/// `asset_game::T5_CLIENT_BEAM_MATERIALS`).
const RED_TRACER: &str = "gfx_red_tracer";
const GREEN_TRACER: &str = "gfx_green_tracer";
const BULLET: &str = "gfx_bullet";

/// `weaponType` bullet.
const WEAPTYPE_BULLET: i32 = 0;

// The client's tracer dvars, at their defaults.
/// `cg_tracerSpeed`
const TRACER_SPEED: f32 = 7500.0;
/// `cg_tracerwidth`, for a weapon whose tracer width is 0.
const TRACER_WIDTH: f32 = 3.0;
/// `cg_tracerlength`, for a weapon whose tracer length is 0.
const TRACER_LENGTH: f32 = 100.0;
/// `cg_bulletwidth` and `cg_bulletlength`: the streak.
const BULLET_WIDTH: f32 = 2.0;
const BULLET_LENGTH: f32 = 80.0;
/// `cg_tracerScrewDist` and `cg_tracerScrewRadius`.
const SCREW_DIST: f32 = 100.0;
const SCREW_RADIUS: f32 = 0.15;

/// The beam kinds: an axis shooter's tracer, anyone else's, and the streak.
enum Kind {
    Red,
    Green,
    Bullet,
}

/// Each of the shooter's bullets counts (a byte, per entity); a weapon with
/// no tracer frequency, or one that is no bullet weapon, draws nothing.
/// Every frequency-th bullet is a tracer: green for a player or actor on
/// axis, red for anyone else. The other bullets draw the streak.
fn beam(def: &[u8], variant: &[u8], shooter: BeamShooter, shots: &mut u8) -> Option<BulletBeam> {
    let def = WeaponDefView::new(def, variant)?;
    *shots = shots.wrapping_add(1);
    let frequency = def.tracer_frequency();
    if def.weapon_type() != WEAPTYPE_BULLET || frequency == 0 {
        return None;
    }
    let kind = if i32::from(*shots) % frequency != 0 {
        Kind::Bullet
    } else if matches!(shooter.kind, FireEntityKind::Player | FireEntityKind::Actor)
        && shooter.team == Some(FireTeam::Axis)
    {
        Kind::Green
    } else {
        Kind::Red
    };
    let (material, width, length) = match kind {
        Kind::Bullet => (BULLET, BULLET_WIDTH, BULLET_LENGTH),
        Kind::Red | Kind::Green => {
            let width = def.tracer_width();
            let length = def.tracer_length();
            (
                if matches!(kind, Kind::Green) {
                    GREEN_TRACER
                } else {
                    RED_TRACER
                },
                if width == 0.0 { TRACER_WIDTH } else { width },
                if length == 0.0 { TRACER_LENGTH } else { length },
            )
        }
    };
    Some(BulletBeam {
        material,
        speed: TRACER_SPEED,
        length,
        width,
        screw_dist: SCREW_DIST,
        screw_radius: SCREW_RADIUS,
    })
}

/// A beam starts up to half a client frame back: `rand()` modulo the
/// frame's length, halved (toward zero).
fn start_back(frametime_ms: i32, rand: &mut dyn FnMut() -> i32) -> i32 {
    if frametime_ms == 0 {
        return 0;
    }
    rand().wrapping_rem(frametime_ms) / 2
}
