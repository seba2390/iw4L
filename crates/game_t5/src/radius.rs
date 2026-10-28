//! Black Ops' blasts: damage falling off from the blast's origin to what the
//! blast can see, a body's half eye height or a model's bounds.

use game_api::fire::{AiEvent, AiPointEvent, FireEntity, FireEntityKind, FireWorld, RadiusDamage};
use game_api::{Unknown, unknown};

use crate::fire::{Hit, damage, means_index, sentient};

/// What a blast's sight traces stop at.
const SIGHT_MASK: u32 = 0x0080_2013;
/// Contents whose holder a blast reaches by its bounds alone, whatever its
/// origin's distance.
const BOUNDED_CONTENTS: u32 = 0x405c_0008;
/// A blast's damage flags: radius, no knockback.
const BLAST_FLAGS: i32 = 0x5;
/// How far to either side of a body a blast looks: an actor's, a player's.
const ACTOR_SIDE: f32 = 15.0;
const PLAYER_SIDE: f32 = 8.0;

fn entity_handler_unknown() -> &'static Unknown {
    unknown!(
        "t5.weapons.blast_handler",
        "an entity with its own blast damage rule (a destructible) takes the blast's falloff instead",
        "the entities with Black Ops' own blast damage rule and that rule"
    )
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// The distance squared from the blast to an entity: to its linked bounds,
/// else to its origin.
fn distance_sq(entity: &FireEntity, origin: [f32; 3]) -> f32 {
    let d = match entity.bounds {
        None => sub(entity.origin, origin),
        Some((mins, maxs)) => core::array::from_fn(|i| {
            if mins[i] > origin[i] {
                mins[i] - origin[i]
            } else if origin[i] > maxs[i] {
                origin[i] - maxs[i]
            } else {
                0.0
            }
        }),
    };
    d[0] * d[0] + d[2] * d[2] + d[1] * d[1]
}

/// The bounds of a body as the engine links it: the actor's spawn box, or
/// the player's movement bounds, one unit wider.
pub(crate) fn actor_bounds(origin: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    const MINS: [f32; 3] = [-15.0, -15.0, 0.0];
    const MAXS: [f32; 3] = [15.0, 15.0, 72.0];
    (
        core::array::from_fn(|i| origin[i] + MINS[i] - 1.0),
        core::array::from_fn(|i| origin[i] + MAXS[i] + 1.0),
    )
}

/// An entity with its bounds as Black Ops links it.
fn linked(mut entity: FireEntity) -> FireEntity {
    if entity.kind == FireEntityKind::Actor {
        entity.bounds = Some(actor_bounds(entity.origin));
    }
    entity
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[2] * v[2] + v[1] * v[1] + v[0] * v[0]).sqrt();
    let len = if -len >= 0.0 { 1.0 } else { len };
    let inv = 1.0 / len;
    [v[0] * inv, v[1] * inv, v[2] * inv]
}

/// The game's rotation of a local point by an entity's angles.
fn rotate(angles: [f32; 3], v: [f32; 3]) -> [f32; 3] {
    let (forward, right, up) = movement_t5::math::angle_vectors(angles);
    let left = [-right[0], -right[1], -right[2]];
    [
        forward[0] * v[0] + up[0] * v[2] + left[0] * v[1],
        forward[1] * v[0] + up[1] * v[2] + left[1] * v[1],
        forward[2] * v[0] + up[2] * v[2] + left[2] * v[1],
    ]
}

/// The points a blast must see one of: a body's centre at half its eye
/// height and either side above and below; a model's centre and the corners
/// of its outline across the blast's line.
fn sight_points(entity: &FireEntity, origin: [f32; 3], eye: [f32; 3]) -> [[f32; 3]; 5] {
    let at = entity.origin;
    if sentient(*entity) {
        let side = if entity.kind == FireEntityKind::Player {
            PLAYER_SIDE
        } else {
            ACTOR_SIDE
        };
        let dx = origin[0] - at[0];
        let dy = origin[1] - at[1];
        let len = (dx * dx + dy * dy).sqrt();
        let len = if -len >= 0.0 { 1.0 } else { len };
        let inv = 1.0 / len;
        let nx = inv * dx;
        let nz = inv * 0.0;
        let px = -(inv * dy);
        let half = (eye[2] - at[2]) * 0.5;
        let base = [
            (eye[0] + at[0]) * 0.5,
            (at[1] + eye[1]) * 0.5,
            (at[2] + eye[2]) * 0.5,
        ];
        let point = |side: f32, up: f32| {
            [
                px * side + base[0],
                nx * side + base[1],
                nz * side + base[2] + up,
            ]
        };
        return [
            base,
            point(side, half),
            point(side, -half),
            point(-side, half),
            point(-side, -half),
        ];
    }
    // A model's centre and the extent its bounds reach from it.
    let (local_mins, local_maxs) = entity.model_bounds.unwrap_or(([0.0; 3], [0.0; 3]));
    let local_center: [f32; 3] = core::array::from_fn(|i| (local_mins[i] + local_maxs[i]) * 0.5);
    let r = rotate(entity.angles, local_center);
    let c = [at[0] + r[0], at[1] + r[1], at[2] + r[2]];
    let far = match (entity.script_model, entity.bounds) {
        (false, Some((_, maxs))) => maxs,
        _ => core::array::from_fn(|i| at[i] + local_maxs[i]),
    };
    let v = sub(origin, c);
    let n = normalize(v);
    let (nx, ny, nz) = (n[0], n[1], n[2]);
    let len2 = (nx * nx + (-ny) * (-ny)).sqrt();
    let len2 = if -len2 >= 0.0 { 1.0 } else { len2 };
    let inv2 = 1.0 / len2;
    let (px, py, pz) = (inv2 * -ny, inv2 * nx, inv2 * 0.0);
    let a = pz * ny - nz * py;
    let b = nz * px - pz * nx;
    let cc = py * nx - px * ny;
    let hx = far[0] - c[0];
    let hy = far[1] - c[1];
    let hz = far[2] - c[2];
    let ep = (hx * px).abs() + (hy * py).abs();
    let eu = (b * hy).abs() + (a * hx).abs() + (hz * cc).abs();
    let (a, b, cc) = (a * eu, b * eu, cc * eu);
    let (ex, ey, ez) = (ep * px, ep * py, ep * pz);
    let near = [c[0] + ex, c[1] + ey, ez + c[2]];
    let back = [c[0] - ex, c[1] - ey, c[2] - ez];
    [
        c,
        [near[0] + a, near[1] + b, near[2] + cc],
        [back[0] + a, back[1] + b, back[2] + cc],
        [near[0] - a, near[1] - b, near[2] - cc],
        [back[0] - a, back[1] - b, back[2] - cc],
    ]
}

/// Whether a blast reaches an entity: within its radius (unless the
/// entity's contents reach by bounds) and with sight of one of its points
/// inside the blast's cone.
fn can_damage(
    entity: &FireEntity,
    target: i32,
    blast: &RadiusDamage,
    eye: [f32; 3],
    world: &mut dyn FireWorld,
) -> bool {
    let d = sub(blast.origin, entity.origin);
    if entity.contents & BOUNDED_CONTENTS == 0
        && blast.radius * blast.radius <= d[0] * d[0] + d[2] * d[2] + d[1] * d[1]
    {
        return false;
    }
    for point in sight_points(entity, blast.origin, eye) {
        if let Some((cos, dir)) = blast.cone {
            let v = normalize(sub(point, blast.origin));
            if cos > dir[2] * v[2] + dir[1] * v[1] + dir[0] * v[0] {
                continue;
            }
        }
        if world.sight_clear(blast.origin, point, [target, blast.inflictor], SIGHT_MASK) {
            return true;
        }
    }
    false
}

/// A blast: every entity whose bounds lie within its radius and that it can
/// see takes its damage, from the inner damage at the origin to the outer
/// at the edge. True when it struck an enemy of the attacker.
pub(crate) fn radius_damage(
    blast: &RadiusDamage,
    world: &mut dyn FireWorld,
) -> Vec<&'static Unknown> {
    let mut gaps = Vec::new();
    world.ai_event(AiEvent::Point {
        originator: blast.attacker,
        kind: AiPointEvent::Explosion,
        teams: Some(0xe),
        origin: blast.origin,
    });
    let radius = blast.radius.max(1.0);
    let reach = radius * 1.41421354;
    let o = blast.origin;
    let mins = [o[0] - reach, o[1] - reach, o[2] - reach];
    let maxs = [o[0] + reach, o[1] + reach, o[2] + reach];
    let blast = RadiusDamage { radius, ..*blast };
    let skip_players = blast.script && world.players_ignore_radius_damage();
    for target in world.entities_in_box(mins, maxs) {
        if target == blast.ignore {
            continue;
        }
        let Some(entity) = world.entity(target).map(linked) else {
            continue;
        };
        if !entity.takes_damage || entity.kind == FireEntityKind::Player && skip_players {
            continue;
        }
        if distance_sq(&entity, blast.origin) >= radius * radius {
            continue;
        }
        let dist = distance_sq(&entity, blast.origin).sqrt();
        let amount = (1.0 - dist / radius) * (blast.inner - blast.outer) + blast.outer;
        let eye = match entity.kind {
            FireEntityKind::Player => {
                crate::fire::player_eye(world, target).unwrap_or(entity.origin)
            }
            FireEntityKind::Actor => entity.eye.unwrap_or(entity.origin),
            FireEntityKind::Other => entity.origin,
        };
        if !can_damage(&entity, target, &blast, eye, world) {
            continue;
        }
        if entity.blast_handler {
            gaps.push(entity_handler_unknown());
        }
        let at = entity.origin;
        let dir = [at[0] - o[0], at[1] - o[1], (at[2] - o[2]) + 24.0];
        damage(
            blast.attacker,
            blast.inflictor,
            entity,
            Hit {
                target,
                dir,
                point: o,
                amount: amount as i32,
                flags: BLAST_FLAGS,
                means: means_index(blast.means),
                weapon: blast.weapon,
                location: 0,
                bone: None,
            },
            world,
        );
    }
    world.blast_effects(&blast);
    gaps
}
