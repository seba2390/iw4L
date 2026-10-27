use frame::ScopeApp;
use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;

use crate::occupancy::dyn_ent::xmodel_phys_hull;
use asset_world::ClipCollision;
use net::FrameClock;
use render_scene::DynEntModelEntity;
use render_scene::WorldDynEntInstance;
use sim::launched::{Body, BodyWorld, Material, Sweep};

const MSEC_STEP: i32 = sim::launched::STEP_MS;

const MAX_ITERS: i32 = 2;

/// The clip map, swept by the shared integrator.
struct ClipWorld<'c>(&'c ClipCollision);

impl BodyWorld for ClipWorld<'_> {
    fn sweep(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
    ) -> Sweep {
        let hit = self.0.sweep_box(start, end, mins, maxs, mask);
        Sweep {
            fraction: hit.fraction,
            endpos: hit.endpos,
            normal: hit.normal,
            startsolid: hit.startsolid,
            allsolid: hit.allsolid,
        }
    }
}

#[derive(Resource, Clone, Default)]
pub struct DynEntPhysClip(pub Option<Arc<ClipCollision>>);

#[derive(Resource, Default)]
pub struct DynEntPhysWorld {
    last_time: Option<i32>,
    bodies: HashMap<Entity, Body>,
}

impl DynEntPhysWorld {
    pub fn is_awake(&self, entity: Entity) -> bool {
        self.bodies.contains_key(&entity)
    }

    pub fn destroy_body(&mut self, entity: Entity) {
        self.bodies.remove(&entity);
    }
}

#[derive(Clone, Copy, Debug, Message)]
pub struct DynEntPhysImpulse {
    pub entity: Entity,

    pub impulse: Vec3,
    pub ang_impulse: Vec3,
}

pub(crate) fn register_dyn_ent_phys(app: &mut App) {
    app.staged::<DynEntPhysClip>(frame::MatchScope::Live)
        .staged::<DynEntPhysWorld>(frame::MatchScope::Live)
        .scoped_message::<DynEntPhysImpulse>(frame::MatchScope::Live);
}

pub fn step_phys_world0(
    mut world: ResMut<DynEntPhysWorld>,
    clip: Res<DynEntPhysClip>,
    clock: Option<Res<FrameClock>>,
    catalog: Option<Res<asset_world::MapXModelSceneCatalog>>,
    mut impulses: MessageReader<DynEntPhysImpulse>,
    mut instances: Query<
        (Entity, &mut WorldDynEntInstance, &mut Transform),
        With<DynEntModelEntity>,
    >,
) {
    let catalog = catalog.as_deref();
    for impulse in impulses.read() {
        let Ok((entity, inst, transform)) = instances.get(impulse.entity) else {
            continue;
        };
        if inst.dead {
            continue;
        }
        let Some(props) = asset_world::dyn_ent_props(inst.ty) else {
            continue;
        };
        if !props.use_physics {
            continue;
        }
        let Some(preset) = inst.phys_preset.as_ref() else {
            continue;
        };
        if preset.mass <= 0.0 {
            continue;
        }
        let (mins, maxs) = xmodel_phys_hull(catalog, &inst.current_model);
        let add_vel = impulse.impulse / preset.mass;
        let add_ang_vel = impulse.ang_impulse / preset.mass.max(0.1);
        let phys_clip = clip.0.as_deref().map(ClipWorld);
        world
            .bodies
            .entry(entity)
            .and_modify(|body| {
                body.vel += add_vel;
                body.ang_vel += add_ang_vel;
                body.asleep_ms = 0;
            })
            .or_insert_with(|| {
                Body::wake(
                    phys_clip.as_ref().map(|clip| clip as &dyn BodyWorld),
                    transform.translation,
                    transform.rotation,
                    (mins, maxs),
                    Material {
                        bounce: preset.bounce,
                        friction: preset.friction,
                        gravity_scale: preset.gravity_scale,
                    },
                    (add_vel, add_ang_vel),
                )
            });
    }

    let now = clock
        .as_deref()
        .filter(|clock| clock.started())
        .map(FrameClock::time);
    let mut advanced_ms = 0;
    if let Some(now) = now {
        match world.last_time {
            None => world.last_time = Some(now),
            Some(last) if now <= last => {}
            Some(last) => {
                let clip = clip.0.as_deref().map(ClipWorld);
                let clip = clip.as_ref().map(|clip| clip as &dyn BodyWorld);
                let mut remain = now.saturating_sub(last);
                let mut left = MAX_ITERS;
                let mut advanced = 0i32;
                while remain > 0 && left > 0 {
                    let step = if MSEC_STEP < remain / left.max(1) {
                        remain / left
                    } else {
                        MSEC_STEP
                    }
                    .clamp(1, remain);
                    let dt = step as f32 / 1000.0;
                    for body in world.bodies.values_mut() {
                        body.step(clip, dt);
                    }
                    remain -= step;
                    left -= 1;
                    advanced += step;
                }
                world.last_time = Some(last + advanced);
                advanced_ms = advanced;
            }
        }
    }

    let mut sleep = Vec::new();
    for (entity, body) in world.bodies.iter_mut() {
        if body.rest(advanced_ms) {
            sleep.push(*entity);
        }
    }

    for (entity, mut inst, mut transform) in &mut instances {
        let Some(body) = world.bodies.get(&entity) else {
            continue;
        };
        transform.translation = body.origin;
        transform.rotation = body.rot;
        inst.lighting_origin = body.origin.to_array();
        inst.transform.translation = body.origin;
        inst.transform.rotation = body.rot;
    }

    for entity in sleep {
        world.bodies.remove(&entity);
    }
}
