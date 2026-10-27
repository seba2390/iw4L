//! Black Ops' `PhysicsLaunch`: the entity is handed to a physics body on
//! the server, which it follows until the body sleeps; then the entity
//! rests and is notified `stationary`. The body is built as Black Ops
//! builds it (the model's own preset, or one made from its bounds and
//! surface; the launch impulse at the point), and moves by the runtime's
//! integrator (`crate::launched`): Black Ops' physics engine is not ported.

use super::super::args::vector;
use super::super::mechanics::{Mechanics, StaticWorld};
use crate::frame::FrameWorld;
use crate::launched::{Body, BodyWorld, Material};
use crate::script::{RoundScript, Value};
use bevy_ecs::prelude::World;
use glam::{EulerRot, Quat, Vec3};

/// The notify a launched entity raises when it comes to rest, or at once
/// when it cannot be given a body.
const STATIONARY: &str = "stationary";
/// `phys_bulletUpBias` and `phys_bulletSpinScale`: the launch's lift and
/// the spin its point off the centre gives.
const UP_BIAS: f32 = 0.5;
const SPIN_SCALE: f32 = 3.0;
/// The share of the spin's arm the runtime's integrator turns by.
const SPIN_SHARE: f32 = 0.02;
/// A default preset: its mass is the box's three sizes times the surface's
/// density, from 1 to 1000; it bounces a quarter, grips fully and takes
/// the whole bullet force and gravity (surface type 8 three tenths).
const DEFAULT_MASS_MIN: f32 = 1.0;
const DEFAULT_MASS_MAX: f32 = 1000.0;
const DEFAULT_BOUNCE: f32 = 0.25;
const DEFAULT_FRICTION: f32 = 1.0;
const SURFACE_LIGHT_FALL: u32 = 8;
const LIGHT_FALL_GRAVITY: f32 = 0.3;
/// The surface a model with no collision surface is taken to have.
const SURFACE_UNKNOWN_MODEL: u32 = 13;
/// Where a surface type sits in surface flags.
const SURFACE_SHIFT: u32 = 20;
const SURFACE_MASK: u32 = 0x3f;

/// What the launch builds the body from.
struct Launchee {
    mins: [f32; 3],
    maxs: [f32; 3],
    preset: xmodel_runtime::PhysPreset,
}

/// `ent PhysicsLaunch([point, force])`, its entity already checked.
pub(crate) fn launch(world: &mut World, object: u64, args: &[Value]) -> Result<Value, String> {
    let (point, force) = if args.len() == 2 {
        (vector(args, 0)?, vector(args, 1)?)
    } else {
        ([0.0; 3], [0.0; 3])
    };
    // A body already launched takes no new push on the server.
    if world.resource::<Mechanics>().is_launched(object) {
        return Ok(Value::Undefined);
    }
    let (classname, model, origin, angles) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let classname = runtime.entities[&object].classname.clone();
        let model = match runtime.object_field(object, "model") {
            Value::String(model) => model.to_string(),
            _ => String::new(),
        };
        let vector = |runtime: &mut RoundScript, name| match runtime.object_field(object, name) {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        let origin = vector(&mut runtime, "origin");
        (classname, model, origin, vector(&mut runtime, "angles"))
    };
    let launchee = if &*classname == "script_brushmodel" {
        match brush_launchee(world, object) {
            Some(launchee) => Some(launchee),
            None => {
                diag::info!(
                    Sim,
                    "WARNING: Brushmodel without brushes '{model}' ({:.1} {:.1} {:.1})",
                    origin[0],
                    origin[1],
                    origin[2]
                );
                return Ok(Value::Undefined);
            }
        }
    } else {
        model_launchee(world, &model)
    };
    let Some(launchee) = launchee else {
        diag::info!(
            Sim,
            "Failed to create physics object for '{model}'.  No physics preset."
        );
        world
            .resource_mut::<Mechanics>()
            .notify_later(object, STATIONARY);
        return Ok(Value::Undefined);
    };
    let preset = launchee.preset;
    let at = Vec3::from_array(origin);
    let rot = Quat::from_euler(
        EulerRot::ZYX,
        angles[1].to_radians(),
        angles[0].to_radians(),
        angles[2].to_radians(),
    );
    // The impulse: the force's way lifted and made whole, as strong as the
    // force times the preset's share, at the point pushed out from the
    // centre of mass by the spin scale.
    let force = Vec3::from_array(force);
    let strength = force.length() * preset.bullet_force_scale;
    let mut dir = force.normalize_or_zero();
    dir.z += UP_BIAS;
    let impulse = dir.normalize_or(Vec3::Z) * strength;
    let center = at + rot * Vec3::from_array(preset.center_of_mass_offset);
    let arm = (Vec3::from_array(point) - center) * SPIN_SCALE;
    let mass = preset.mass.max(f32::MIN_POSITIVE);
    let velocity = impulse / mass;
    let spin = arm.cross(impulse) * SPIN_SHARE / mass.max(0.1);
    let body = {
        let frame = FrameWorld::from_world(world);
        let sweep = StaticWorld(frame);
        Body::wake(
            Some(&sweep as &dyn BodyWorld),
            at,
            rot,
            (launchee.mins, launchee.maxs),
            Material {
                bounce: preset.bounce,
                friction: preset.friction,
                gravity_scale: preset.gravity_scale,
            },
            (velocity, spin),
        )
    };
    world
        .resource_mut::<Mechanics>()
        .launch_body(object, body, STATIONARY);
    Ok(Value::Undefined)
}

/// A brush model's body: its brush model's box and a default preset by
/// the surface of its first brush; none when it has no brushes.
fn brush_launchee(world: &mut World, object: u64) -> Option<Launchee> {
    let presence = world.resource::<RoundScript>().entities[&object].presence?;
    let frame = FrameWorld::from_world(world);
    let handle = frame
        .entity_collision_capabilities()
        .iter()
        .find(|row| row.owner.script_model() == Some(presence))?
        .linked_brushes
        .first()?
        .cmodel_handle;
    let model = frame.clip_cmodels().models.get(handle as usize)?;
    if model.num_brushes == 0 {
        return None;
    }
    let brush = *frame
        .clip_bsp()
        .leafbrushes
        .get(model.first_brush as usize)?;
    let surface = frame
        .clip_brushes()
        .get(usize::from(brush))
        .and_then(|brush| brush.plane_surface_flags.first())
        .map_or(0, |flags| flags >> SURFACE_SHIFT & SURFACE_MASK);
    Some(Launchee {
        mins: model.mins,
        maxs: model.maxs,
        preset: default_preset(sub(model.maxs, model.mins), surface),
    })
}

/// A model's body: its bounds, and its own preset or a default one by the
/// surface of its first collision surface.
fn model_launchee(world: &mut World, model: &str) -> Option<Launchee> {
    let capability = FrameWorld::from_world(world)
        .model_capability(model)
        .flatten()?;
    let (mid, half) = capability.bounds.unwrap_or(([0.0; 3], [0.0; 3]));
    let mins = std::array::from_fn(|i| mid[i] - half[i]);
    let maxs = std::array::from_fn(|i| mid[i] + half[i]);
    let preset = capability.phys_preset.unwrap_or_else(|| {
        let surface = capability
            .coll_surfs
            .first()
            .map_or(SURFACE_UNKNOWN_MODEL, |surf| {
                surf.surf_flags >> SURFACE_SHIFT & SURFACE_MASK
            });
        default_preset(sub(maxs, mins), surface)
    });
    Some(Launchee { mins, maxs, preset })
}

/// Black Ops' preset for a body that has none, from its size and surface.
fn default_preset(size: [f32; 3], surface: u32) -> xmodel_runtime::PhysPreset {
    let mass = ((size[2] + size[1] + size[0]) * density(surface))
        .clamp(DEFAULT_MASS_MIN, DEFAULT_MASS_MAX);
    xmodel_runtime::PhysPreset {
        mass,
        bounce: DEFAULT_BOUNCE,
        friction: DEFAULT_FRICTION,
        bullet_force_scale: 1.0,
        gravity_scale: if surface == SURFACE_LIGHT_FALL {
            LIGHT_FALL_GRAVITY
        } else {
            1.0
        },
        center_of_mass_offset: [0.0; 3],
    }
}

/// A surface type's density: the light materials weigh less than 1.
fn density(surface: u32) -> f32 {
    match surface {
        0x10 => 0.3,
        0x15 => 0.5,
        0x18 => 0.25,
        0x19 => 0.6,
        0x1a => 0.2,
        0x1b => 0.15,
        _ => 1.0,
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
