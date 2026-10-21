use super::entities::{EntityKind, SPAWNED_PRESENCE_BASE};
use crate::ScriptModelId;
use crate::bullet_collision::{AuthorityDObjState, EntityCollisionCapabilities};
use crate::frame::FrameWorld;
use crate::gentity::ScriptMoverGentity;
use crate::script::{Arc, BTreeMap, RoundScript, Value};
use bevy_ecs::prelude::World;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Shown {
    origin: [f32; 3],
    angles: [f32; 3],
    model: Option<Arc<str>>,
    attachments: Vec<(Arc<str>, Arc<str>)>,
    attach_ignore_collision: u32,
    hidden: bool,
    shown_to: u64,
    solid: bool,
    contents: i32,
    moving: bool,
}

struct Wanted {
    collision_only: bool,
    object: u64,
    presence: ScriptModelId,
    origin: [f32; 3],
    angles: [f32; 3],
    model: Option<Arc<str>>,
    attachments: Vec<(Arc<str>, Arc<str>)>,
    attach_ignore_collision: u32,
    hidden: bool,
    shown_to: u64,
    solid: bool,
    contents: i32,
    part_ops: Vec<(Arc<str>, bool)>,
    anim_op: Option<Option<Arc<str>>>,
}

fn near(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.01)
}

fn near_angles(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter()
        .zip(b)
        .all(|(a, b)| math_iw4::angle_subtract(*a, b).abs() < 0.01)
}

fn vector(runtime: &mut RoundScript, id: u64, name: &str) -> [f32; 3] {
    match runtime.object_field(id, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

fn model_field(runtime: &mut RoundScript, id: u64) -> Option<Arc<str>> {
    match runtime.object_field(id, "model") {
        Value::String(model) if !model.is_empty() && !model.starts_with('*') => Some(model.into()),
        _ => None,
    }
}

pub(crate) fn initialize_map_models(world: &mut World) {
    let models: BTreeMap<ScriptModelId, Arc<str>> = FrameWorld::from_world(world)
        .entity_collision_capabilities()
        .iter()
        .filter_map(|row| {
            Some((
                row.owner.script_model()?,
                row.dobj.as_ref()?.current_model.as_str().into(),
            ))
        })
        .collect();
    let mut runtime = world.resource_mut::<RoundScript>();
    let objects: Vec<_> = runtime
        .entities
        .iter()
        .filter(|(_, entity)| entity.kind == EntityKind::Map)
        .filter_map(|(object, entity)| Some((*object, models.get(&entity.presence?)?.clone())))
        .collect();
    for (object, model) in objects {
        runtime.set_object_field(object, "model", Value::String(model.into()));
    }
    withhold_client_map_models(world);
}

/// The map models the server leaves to the client draw and collide in the
/// server's world no more: the client presents its own.
fn withhold_client_map_models(world: &mut World) {
    let withheld: std::collections::BTreeSet<ScriptModelId> = world
        .resource::<RoundScript>()
        .withheld_presence
        .iter()
        .copied()
        .collect();
    if withheld.is_empty() {
        return;
    }
    let mut frame = FrameWorld::from_world(world);
    for row in frame.entity_collision_capabilities_mut() {
        if row
            .owner
            .script_model()
            .is_some_and(|id| withheld.contains(&id))
        {
            row.hidden = true;
            row.solid = false;
        }
    }
    let numbers: Vec<i32> = crate::frame::collect_script_movers(frame.ecs())
        .into_iter()
        .filter(|mover| withheld.contains(&mover.id))
        .map(|mover| mover.state.number)
        .collect();
    for number in numbers {
        if let Some(mover) = frame.script_mover_mut_by_number(number) {
            mover.show_only_to(0);
            mover.nonsolid = true;
        }
    }
}

pub(crate) fn spawn_presence(world: &mut World, origin: [f32; 3]) -> Result<ScriptModelId, String> {
    let mut runtime = world.resource_mut::<crate::script::MatchScript>();
    let serial = runtime.next_spawned_presence;
    runtime.next_spawned_presence = serial
        .checked_add(1)
        .filter(|n| {
            SPAWNED_PRESENCE_BASE + *n < crate::script::client::present::CLIENT_PRESENTED_BASE
        })
        .ok_or("spawned script model identifiers exhausted")?;
    let id = ScriptModelId::from_wire(SPAWNED_PRESENCE_BASE + serial);
    let mut frame = FrameWorld::from_world(world);
    frame
        .spawn_script_mover(id, origin, [0.0; 3])
        .map_err(|_| "no free entities".to_owned())?;
    frame.insert_collision_owner(EntityCollisionCapabilities::current_tick(
        crate::AuthorityModelOwner::ScriptModel(id),
        None,
        Vec::new(),
    ));
    Ok(id)
}

/// The observability boundary for script writes: native collision reads go
/// through here, so a query sees every pose, solidity and visibility change a
/// native or field store made earlier in the same tick.
pub(crate) fn settled(world: &mut World) -> FrameWorld<'_> {
    settle_collision(world);
    FrameWorld::from_world(world)
}

/// Collision rows only. Networked mover state is left to `present`: posing a
/// mover mid-tick would break its tick-to-tick velocity.
pub(crate) fn settle_collision(world: &mut World) {
    settle_rows(world, None);
}

/// [`settle_collision`] for one entity's row alone: what a read of that
/// entity's own posed model needs.
fn settle_entity(world: &mut World, object: u64) {
    settle_rows(world, Some(object));
}

fn settle_rows(world: &mut World, only: Option<u64>) {
    let mut runtime = world.resource_mut::<RoundScript>();
    type PlacedRow = (
        u64,
        ScriptModelId,
        bool,
        bool,
        Vec<(Arc<str>, Arc<str>)>,
        u32,
    );
    let placed: Vec<PlacedRow> = match only {
        Some(object) => runtime
            .entities
            .get(&object)
            .and_then(|e| {
                Some((
                    object,
                    e.presence?,
                    e.hidden,
                    e.solid,
                    e.attachments.clone(),
                    e.attach_ignore_collision,
                ))
            })
            .into_iter()
            .collect(),
        None => runtime
            .entities
            .iter()
            .filter_map(|(id, e)| {
                Some((
                    *id,
                    e.presence?,
                    e.hidden,
                    e.solid,
                    e.attachments.clone(),
                    e.attach_ignore_collision,
                ))
            })
            .collect(),
    };
    if placed.is_empty() {
        return;
    }
    let (origin_field, angles_field, model_sym) = (
        runtime.symbol("origin"),
        runtime.symbol("angles"),
        runtime.symbol("model"),
    );
    type SettledRow = (
        [f32; 3],
        [f32; 3],
        bool,
        bool,
        Option<Arc<str>>,
        Vec<(Arc<str>, Arc<str>)>,
        u32,
    );
    let wanted: BTreeMap<ScriptModelId, SettledRow> = placed
        .into_iter()
        .map(
            |(object, presence, hidden, solid, attachments, attach_ignore_collision)| {
                let fields = runtime.objects.get(&object);
                let field = |symbol| match fields.and_then(|fields| fields.get(&symbol)) {
                    Some(Value::Vector(v)) => *v,
                    _ => [0.0; 3],
                };
                let model = match fields.and_then(|fields| fields.get(&model_sym)) {
                    Some(Value::String(m)) if !m.is_empty() && !m.starts_with('*') => {
                        Some(m.clone().into())
                    }
                    _ => None,
                };
                (
                    presence,
                    (
                        field(origin_field),
                        field(angles_field),
                        hidden,
                        solid,
                        model,
                        attachments,
                        attach_ignore_collision,
                    ),
                )
            },
        )
        .collect();
    let turrets: Vec<_> = runtime
        .engine
        .turrets
        .iter()
        .filter(|(object, _)| only.is_none_or(|only| only == **object))
        .filter_map(|(object, turret)| {
            Some((
                *object,
                runtime.entities.get(object)?.presence?,
                turret.clone(),
            ))
        })
        .collect();
    let rotations: BTreeMap<_, _> = turrets
        .into_iter()
        .map(|(object, presence, turret)| {
            let base = vector(&mut runtime, object, "angles");
            (presence, turret.model_rotations(base))
        })
        .collect();
    let game_guns: BTreeMap<ScriptModelId, [f32; 3]> = {
        let runtime = world.resource::<RoundScript>();
        super::natives::t5_turret::turret_guns(world, only)
            .into_iter()
            .filter_map(|(object, gun)| Some((runtime.entities.get(&object)?.presence?, gun)))
            .collect()
    };
    let game_bones = match game_guns.is_empty() {
        true => None,
        false => super::natives::t5_turret::rules(world).map(|rules| rules.bone_controls),
    };
    let mut frame = FrameWorld::from_world(world);
    let mut model_caps: BTreeMap<
        Arc<str>,
        Option<std::sync::Arc<xmodel_runtime::RetainedModelCapability>>,
    > = BTreeMap::new();
    for (_, _, _, _, model, attachments, _) in wanted.values() {
        if let Some(model) = model
            && !model_caps.contains_key(model)
        {
            let cap = frame.model_capability(model).flatten();
            model_caps.insert(model.clone(), cap);
        }
        for (att_model, _) in attachments {
            if !model_caps.contains_key(att_model) {
                let cap = frame.model_capability(att_model).flatten();
                model_caps.insert(att_model.clone(), cap);
            }
        }
    }
    for row in frame.entity_collision_capabilities_mut() {
        let Some((origin, angles, hidden, solid, model, attachments, attach_ignore)) =
            row.owner.script_model().and_then(|id| wanted.get(&id))
        else {
            continue;
        };
        let (origin, angles, hidden, solid, attach_ignore) =
            (*origin, *angles, *hidden, *solid, *attach_ignore);
        row.hidden = hidden;
        row.solid = solid;
        if let Some(model) = model {
            if row.dobj.is_none() {
                let cap = model_caps.get(model).cloned().flatten();
                row.dobj = Some(AuthorityDObjState::at_pose(model, cap, origin, angles));
                row.followed_pose = Some((origin, angles));
            }
            if let Some(dobj) = row.dobj.as_mut() {
                if !dobj.attachments_match(attachments) {
                    let resolved: Vec<_> = attachments
                        .iter()
                        .map(|(m, t)| (&**m, &**t, model_caps.get(m).cloned().flatten()))
                        .collect();
                    dobj.set_attachments(&resolved);
                }
                dobj.set_attach_ignore_collision(attach_ignore);
            }
        }
        if let Some(rotations) = row.owner.script_model().and_then(|id| rotations.get(&id))
            && let Some(dobj) = row.dobj.as_mut()
        {
            dobj.set_tag_rotations(rotations.clone());
        }
        if let Some(bones) = game_bones
            && let Some(&gun) = row.owner.script_model().and_then(|id| game_guns.get(&id))
            && let Some(dobj) = row.dobj.as_mut()
        {
            let rotations = bones(gun, &|tag| dobj.has_tag(tag))
                .into_iter()
                .filter(|(tag, _)| dobj.has_tag(tag))
                .map(|(tag, angles)| xmodel_runtime::TagRotation {
                    tag: tag.into(),
                    angles,
                })
                .collect();
            dobj.set_tag_rotations(rotations);
        }
        let settled = row
            .followed_pose
            .is_some_and(|(at, facing)| near(at, origin) && near_angles(facing, angles));
        if settled {
            continue;
        }
        row.followed_pose = Some((origin, angles));
        if let Some(dobj) = row.dobj.as_mut() {
            dobj.set_world_pose(origin, angles);
            // A trace later in the tick meets the body where it now stands.
            dobj.materialize();
        }
        for brush in &mut row.linked_brushes {
            brush.origin = origin;
            brush.angles = angles;
        }
    }
}

pub(crate) fn sync_presence(world: &mut World) {
    let request = world.resource::<crate::step::StepRequest>();
    if !request.reason.advances_authority_world() {
        return;
    }
    let tick = request.tick;
    let now = crate::level_time_ms(tick);
    let pool = FrameWorld::from_world(world).dynamic_entity_numbers();
    world.resource_mut::<RoundScript>().pool_numbers = pool.into_iter().collect();
    step_t5_ragdolls(world);
    settle_collision(world);
    super::entity_damage::apply_script_blasts(world, tick);
    publish_loop_sounds(world);
    super::objectives::publish(world);
    super::controls::sync_script_locks(world);
    super::triggers::dispatch_triggers(world);
    present(world, now);
    super::actor_anims::present(world);
    super::natives::t6::anim_present(world);
    super::natives::t6::draw_barriers(world);
    super::natives::t6::ragdoll_present(world);
    publish_killcam_cameras(world);
    settle_collision(world);
    resolve_link_tags(world);
}

pub(crate) fn ragdoll_tag_rotations(
    ragdoll: &super::natives::t5::T5Ragdoll,
) -> Vec<xmodel_runtime::TagRotation> {
    let p = ragdoll.pitch_fold;
    let r = ragdoll.roll_fold;
    vec![
        xmodel_runtime::TagRotation {
            tag: "j_mainroot".into(),
            angles: [p * 0.75, 0.0, r * 0.5],
        },
        xmodel_runtime::TagRotation {
            tag: "j_spinelower".into(),
            angles: [p * 0.2, 0.0, r * 0.25],
        },
        xmodel_runtime::TagRotation {
            tag: "j_spineupper".into(),
            angles: [p * 0.2, 0.0, r * 0.25],
        },
        xmodel_runtime::TagRotation {
            tag: "j_neck".into(),
            angles: [-p * 0.3, 0.0, 0.0],
        },
    ]
}

fn step_t5_ragdolls(world: &mut World) {
    const DT: f32 = crate::MATCH_TICK_MS as f32 * 0.001;
    const GRAVITY: f32 = 800.0;
    const MINS: [f32; 3] = [-12.0, -12.0, 0.0];
    const MAXS: [f32; 3] = [12.0, 12.0, 24.0];
    const MASK_SOLID: u32 = 1;
    let active: Vec<(u64, [f32; 3], super::natives::t5::T5Ragdoll)> = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let ids: Vec<u64> = runtime
            .t5
            .ragdolls
            .iter()
            .filter_map(|(&id, r)| (r.launched && !r.grounded).then_some(id))
            .collect();
        ids.into_iter()
            .filter_map(|id| {
                let origin = match runtime.object_field(id, "origin") {
                    crate::script::Value::Vector(v) => v,
                    _ => return None,
                };
                let ragdoll = runtime.t5.ragdolls.get(&id)?.clone();
                Some((id, origin, ragdoll))
            })
            .collect()
    };
    if active.is_empty() {
        return;
    }
    let mut updates = Vec::with_capacity(active.len());
    {
        let frame = FrameWorld::from_world(world);
        for (id, origin, mut ragdoll) in active {
            ragdoll.velocity[2] -= GRAVITY * DT;
            let next = [
                origin[0] + ragdoll.velocity[0] * DT,
                origin[1] + ragdoll.velocity[1] * DT,
                origin[2] + ragdoll.velocity[2] * DT,
            ];
            let sweep = frame.trace_clip(origin, next, MINS, MAXS, MASK_SOLID);
            let mut pos = if sweep.fraction < 1.0 {
                let n = sweep.normal;
                let vn = ragdoll.velocity[0] * n[0]
                    + ragdoll.velocity[1] * n[1]
                    + ragdoll.velocity[2] * n[2];
                if vn < 0.0 {
                    for (vel, norm) in ragdoll.velocity.iter_mut().zip(n.iter()) {
                        *vel -= 1.25 * vn * norm;
                    }
                }
                [
                    origin[0] + (next[0] - origin[0]) * sweep.fraction + n[0] * 0.25,
                    origin[1] + (next[1] - origin[1]) * sweep.fraction + n[1] * 0.25,
                    origin[2] + (next[2] - origin[2]) * sweep.fraction + n[2] * 0.25,
                ]
            } else {
                next
            };
            let ground = frame.trace_clip(
                [pos[0], pos[1], pos[2] + 8.0],
                [pos[0], pos[1], pos[2] - 4.0],
                MINS,
                MAXS,
                MASK_SOLID,
            );
            if ground.fraction < 1.0 && ragdoll.velocity[2] <= 0.0 {
                pos[2] = pos[2] + 8.0 - 12.0 * ground.fraction;
                ragdoll.velocity[0] *= 0.45;
                ragdoll.velocity[1] *= 0.45;
                ragdoll.velocity[2] = 0.0;
                ragdoll.pitch_fold = (ragdoll.pitch_fold + 220.0 * DT).min(85.0);
                if ragdoll.velocity[0].hypot(ragdoll.velocity[1]) < 12.0
                    && ragdoll.pitch_fold >= 80.0
                {
                    ragdoll.velocity = [0.0; 3];
                    ragdoll.grounded = true;
                }
            } else {
                ragdoll.pitch_fold =
                    (ragdoll.pitch_fold + ragdoll.angular_vel[0].abs() * DT).min(85.0);
                ragdoll.roll_fold =
                    (ragdoll.roll_fold + ragdoll.angular_vel[2] * DT * 0.25).clamp(-35.0, 35.0);
            }
            updates.push((id, pos, ragdoll));
        }
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    for (id, pos, ragdoll) in updates {
        runtime.set_object_field(id, "origin", crate::script::Value::Vector(pos));
        runtime.t5.ragdolls.insert(id, ragdoll);
    }
}

fn publish_killcam_cameras(world: &mut World) {
    let runtime = world.resource::<RoundScript>();
    let cameras: Vec<_> = runtime
        .entities
        .iter()
        .filter_map(|(object, entity)| {
            entity.presence?;
            let mode = if runtime.vehicles.contains_key(object) {
                Some(playerstate_iw4::KillCamMode::Mode1Heli)
            } else if runtime.engine.turrets.contains_key(object) {
                Some(playerstate_iw4::KillCamMode::Mode6Turret)
            } else {
                None
            };
            Some((entity.number, mode))
        })
        .collect();
    let mut frame = FrameWorld::from_world(world);
    for (number, mode) in cameras {
        if let Some(mover) = frame.script_mover_mut_by_number(number) {
            mover.killcam_camera = mode;
        }
    }
}

fn present(world: &mut World, now: i32) {
    let retired = std::mem::take(&mut world.resource_mut::<RoundScript>().retired_presence);
    let wanted = collect_wanted(world);
    if retired.is_empty() && wanted.is_empty() {
        return;
    }
    let mut frame = FrameWorld::from_world(world);
    let movers: BTreeMap<ScriptModelId, ScriptMoverGentity> =
        crate::frame::collect_script_movers(frame.ecs())
            .into_iter()
            .map(|mover| (mover.id, mover))
            .collect();
    for (id, spawned) in retired {
        let Some(mover) = movers.get(&id) else {
            continue;
        };
        if spawned {
            frame.remove_script_mover_by_number(mover.state.number);
            frame.remove_collision_owner(id);
        } else if let Some(mover) = frame.script_mover_mut_by_number(mover.state.number) {
            mover.state.e_flags |= entity_iw4::CG_SCRIPT_MOVER_NODRAW;
            mover.nonsolid = true;
        }
    }
    let mut updates = Vec::new();
    for want in wanted {
        let Some(mover) = movers.get(&want.presence) else {
            continue;
        };
        let first = frame
            .ecs()
            .resource::<RoundScript>()
            .shown
            .get(&want.object)
            .cloned();
        let number = first.is_none().then_some(mover.state.number);
        let shown = first.unwrap_or_else(|| Shown {
            origin: mover.state.tr_base,
            angles: mover.state.apos_tr_base,
            model: frame
                .collision_owner_mut(want.presence)
                .and_then(|row| row.dobj.as_ref())
                .map(|dobj| dobj.current_model.as_str().into()),
            attachments: Vec::new(),
            attach_ignore_collision: 0,
            hidden: mover.state.e_flags & entity_iw4::CG_SCRIPT_MOVER_NODRAW != 0,
            shown_to: mover.shown_to,
            solid: !mover.nonsolid,
            contents: 0,
            moving: false,
        });
        let posed = !near(shown.origin, want.origin) || !near_angles(shown.angles, want.angles);
        let aim_target = {
            let runtime = frame.ecs().resource::<RoundScript>();
            runtime
                .entities
                .get(&want.object)
                .is_some_and(|entity| entity.kind == super::entities::EntityKind::Actor)
                && match runtime.peek_object_field(want.object, "health") {
                    Some(crate::script::Value::Int(health)) => health > 0,
                    Some(crate::script::Value::Float(health)) => health > 0.0,
                    _ => false,
                }
        };
        if let Some(mover) = frame.script_mover_mut_by_number(mover.state.number) {
            mover.aim_target = aim_target && !want.hidden;
            if want.hidden || want.collision_only {
                mover.state.e_flags |= entity_iw4::CG_SCRIPT_MOVER_NODRAW;
            } else {
                mover.state.e_flags &= !entity_iw4::CG_SCRIPT_MOVER_NODRAW;
            }
            mover.nonsolid = !want.solid;
            mover.shown_to = want.shown_to;
        }
        if posed || shown.moving {
            frame.set_script_mover_pose(mover.state.number, now, want.origin, want.angles);
        }
        let reshaped = want.model != shown.model
            || want.attachments != shown.attachments
            || want.attach_ignore_collision != shown.attach_ignore_collision
            || want.contents != shown.contents;
        if reshaped || !want.part_ops.is_empty() || want.anim_op.is_some() {
            present_model(&mut frame, &want);
        }
        updates.push((
            want.object,
            number,
            Shown {
                origin: if posed { want.origin } else { shown.origin },
                angles: if posed { want.angles } else { shown.angles },
                model: want.model,
                attachments: want.attachments,
                attach_ignore_collision: want.attach_ignore_collision,
                hidden: want.hidden,
                shown_to: want.shown_to,
                solid: want.solid,
                contents: want.contents,
                moving: posed,
            },
        ));
    }
    let movers = crate::frame::collect_script_movers(frame.ecs());
    crate::presence::follow_movers(frame.entity_collision_capabilities_mut(), &movers, now);
    let mut runtime = world.resource_mut::<RoundScript>();
    for (object, number, shown) in updates {
        let Some(entity) = runtime.entities.get_mut(&object) else {
            continue;
        };
        if let Some(number) = number.filter(|_| !matches!(entity.kind, EntityKind::Missile(_))) {
            entity.number = number;
        }
        runtime.shown.insert(object, shown);
    }
}

fn with_tag_dobj<T>(
    world: &mut World,
    object: u64,
    read: impl FnOnce(&AuthorityDObjState) -> Option<T>,
) -> Option<T> {
    let presence = world
        .resource::<RoundScript>()
        .entities
        .get(&object)?
        .presence;
    settle_entity(world, object);
    let frame = FrameWorld::from_world(world);
    if let Some(presence) = presence
        && let Some(dobj) = frame
            .entity_collision_capabilities()
            .iter()
            .find(|row| row.owner.script_model() == Some(presence))
            .and_then(|row| row.dobj.as_ref())
    {
        return read(dobj);
    }
    let (model, origin, angles, attachments, attach_ignore) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let (attachments, attach_ignore) = runtime.entities.get(&object).map_or_else(
            || (Vec::new(), 0),
            |e| (e.attachments.clone(), e.attach_ignore_collision),
        );
        (
            model_field(&mut runtime, object)?,
            vector(&mut runtime, object, "origin"),
            vector(&mut runtime, object, "angles"),
            attachments,
            attach_ignore,
        )
    };
    let frame = FrameWorld::from_world(world);
    let capability = frame.model_capability(&model).flatten()?;
    let mut dobj = AuthorityDObjState::at_pose(&model, Some(capability), origin, angles);
    if !attachments.is_empty() {
        let resolved: Vec<_> = attachments
            .iter()
            .map(|(m, t)| (&**m, &**t, frame.model_capability(m).flatten()))
            .collect();
        dobj.set_attachments(&resolved);
        dobj.set_attach_ignore_collision(attach_ignore);
    }
    read(&dobj)
}

fn tag_lookup(world: &mut World, object: u64, tag: &str) -> Option<Option<[f32; 3]>> {
    with_tag_dobj(world, object, |dobj| {
        Some(dobj.tag_world_pose(tag).map(|(at, _)| {
            dobj.world_from_model
                .inverse()
                .transform_point3(glam::Vec3::from(at))
                .to_array()
        }))
    })
}

pub(crate) fn tag_world(
    world: &mut World,
    object: u64,
    tag: &str,
) -> Option<([f32; 3], [[f32; 3]; 3])> {
    let matrix = with_tag_dobj(world, object, |dobj| dobj.tag_world_matrix(tag))?;
    let axis = |v: glam::Vec4| v.truncate().normalize_or_zero().to_array();
    Some((
        matrix.w_axis.truncate().to_array(),
        [
            axis(matrix.x_axis),
            axis(matrix.y_axis),
            axis(matrix.z_axis),
        ],
    ))
}

/// A tag's world position and axis.
pub(crate) type TagPose = ([f32; 3], [[f32; 3]; 3]);

/// A tag's pose on an entity's posed model: none when the entity has no
/// posed model, `Some(None)` when the model lacks the tag.
pub(crate) fn tag_pose(world: &mut World, object: u64, tag: &str) -> Option<Option<TagPose>> {
    tag_pose_controlled(world, object, tag, &[])
}

/// [`tag_pose`] with control angles (degrees) set on the named bones the
/// model has.
pub(crate) fn tag_pose_controlled(
    world: &mut World,
    object: u64,
    tag: &str,
    controls: &[(&str, [f32; 3])],
) -> Option<Option<TagPose>> {
    let presence = world
        .resource::<RoundScript>()
        .entities
        .get(&object)?
        .presence?;
    settle_entity(world, object);
    let frame = FrameWorld::from_world(world);
    let dobj = frame
        .entity_collision_capabilities()
        .iter()
        .find(|row| row.owner.script_model() == Some(presence))?
        .dobj
        .as_ref()?;
    let axis = |v: glam::Vec4| v.truncate().normalize_or_zero().to_array();
    let matrix = if controls.is_empty() {
        dobj.tag_world_matrix(tag)
    } else {
        dobj.tag_world_matrix_controlled(tag, controls)
    };
    Some(matrix.map(|matrix| {
        (
            matrix.w_axis.truncate().to_array(),
            [
                axis(matrix.x_axis),
                axis(matrix.y_axis),
                axis(matrix.z_axis),
            ],
        )
    }))
}

/// Which of the named tags an entity's posed model has; none when it has
/// no posed model.
pub(crate) fn tags_present<const N: usize>(
    world: &mut World,
    object: u64,
    tags: [&str; N],
) -> Option<[bool; N]> {
    let presence = world
        .resource::<RoundScript>()
        .entities
        .get(&object)?
        .presence?;
    settle_entity(world, object);
    let frame = FrameWorld::from_world(world);
    let dobj = frame
        .entity_collision_capabilities()
        .iter()
        .find(|row| row.owner.script_model() == Some(presence))?
        .dobj
        .as_ref()?;
    Some(tags.map(|tag| dobj.has_tag(tag)))
}

pub(crate) fn tag_offset(world: &mut World, object: u64, tag: &str) -> Option<[f32; 3]> {
    tag_lookup(world, object, tag).flatten()
}

fn resolve_link_tags(world: &mut World) {
    let pending: Vec<(u64, u64, Arc<str>)> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter_map(|(id, e)| {
            let link = e.linked_to.as_ref().filter(|l| l.tag_offset.is_none())?;
            Some((*id, link.parent, link.tag.clone()?))
        })
        .collect();
    for (id, parent, tag) in pending {
        let Some(offset) = tag_lookup(world, parent, &tag) else {
            continue;
        };
        if let Some(link) = world
            .resource_mut::<RoundScript>()
            .entities
            .get_mut(&id)
            .and_then(|e| e.linked_to.as_mut())
        {
            link.tag_offset = Some(offset.unwrap_or([0.0; 3]));
        }
    }
}

fn collect_wanted(world: &mut World) -> Vec<Wanted> {
    let mut runtime = world.resource_mut::<RoundScript>();
    let ids: Vec<(u64, ScriptModelId, bool, u64, bool)> = runtime
        .entities
        .iter()
        .filter_map(|(id, e)| e.presence.map(|p| (*id, p, e.hidden, e.shown_to, e.solid)))
        .collect();
    let mut wanted = Vec::with_capacity(ids.len());
    for (object, presence, hidden, shown_to, solid) in ids {
        let origin = vector(&mut runtime, object, "origin");
        let angles = vector(&mut runtime, object, "angles");
        let model = model_field(&mut runtime, object);
        let entity = runtime.entities.get_mut(&object).unwrap();
        let part_ops = std::mem::take(&mut entity.part_ops);
        let anim_op = entity.anim_op.take();
        let attachments = entity.attachments.clone();
        let attach_ignore_collision = entity.attach_ignore_collision;
        let collision_only = matches!(entity.kind, EntityKind::Missile(_));
        let contents = entity.contents;
        let unchanged = part_ops.is_empty()
            && anim_op.is_none()
            && runtime.shown.get(&object).is_some_and(|shown| {
                !shown.moving
                    && shown.hidden == hidden
                    && shown.shown_to == shown_to
                    && shown.solid == solid
                    && shown.contents == contents
                    && shown.model == model
                    && shown.attachments == attachments
                    && shown.attach_ignore_collision == attach_ignore_collision
                    && near(shown.origin, origin)
                    && near_angles(shown.angles, angles)
            });
        if unchanged {
            continue;
        }
        wanted.push(Wanted {
            collision_only,
            object,
            presence,
            origin,
            angles,
            model,
            attachments,
            attach_ignore_collision,
            hidden,
            shown_to,
            solid,
            contents,
            part_ops,
            anim_op,
        });
    }
    wanted
}

fn present_model(frame: &mut FrameWorld, want: &Wanted) {
    let capability = want
        .model
        .as_deref()
        .and_then(|model| frame.model_capability(model))
        .flatten()
        .map(|capability| {
            if want.contents == 0 {
                return capability;
            }
            let mut capability = (*capability).clone();
            capability.contents = Some(want.contents as u32);
            Arc::new(capability)
        });
    let anim = want
        .anim_op
        .as_ref()
        .and_then(|op| op.as_deref().and_then(|clip| frame.script_model_anim(clip)));
    let attachments: Vec<_> = want
        .attachments
        .iter()
        .map(|(model, tag)| (&**model, &**tag, frame.model_capability(model).flatten()))
        .collect();
    let Some(row) = frame.collision_owner_mut(want.presence) else {
        return;
    };
    match (&want.model, row.dobj.as_mut()) {
        (None, _) => row.dobj = None,
        (Some(model), Some(dobj)) => {
            if dobj.current_model != **model
                || dobj.capability.as_ref().and_then(|cap| cap.contents)
                    != capability.as_ref().and_then(|cap| cap.contents)
            {
                dobj.replace_model(model, capability);
            }
        }
        (Some(model), None) => {
            row.dobj = Some(AuthorityDObjState::at_pose(
                model,
                capability,
                want.origin,
                want.angles,
            ));
            row.followed_pose = Some((want.origin, want.angles));
        }
    }
    let Some(dobj) = row.dobj.as_mut() else {
        return;
    };
    dobj.set_attachments(&attachments);
    dobj.set_attach_ignore_collision(want.attach_ignore_collision);
    for (tag, hidden) in &want.part_ops {
        dobj.set_tag_hidden(tag, *hidden);
    }
    match &want.anim_op {
        Some(Some(clip)) => {
            let anim = anim.unwrap_or(crate::ScriptModelPlayAnim {
                looping: false,
                frequency: 0.0,
            });
            dobj.begin_script_model_play_anim(clip, anim.looping, anim.frequency);
        }
        Some(None) => dobj.clear_script_model_play_anim(),
        None => {}
    }
}

const UNPRESENTED_LOOP_OWNER: u32 = 0x2000_0000;

fn publish_loop_sounds(world: &mut World) {
    let mut runtime = world.resource_mut::<RoundScript>();
    let speaking: Vec<(u64, Option<ScriptModelId>, Arc<str>, i32)> = runtime
        .entities
        .iter()
        .filter_map(|(object, e)| Some((*object, e.presence, e.loop_sound.clone()?, e.number)))
        .collect();
    let placed: Vec<(ScriptModelId, Arc<str>, [f32; 3], Option<u32>)> = speaking
        .into_iter()
        .map(|(object, presence, alias, number)| {
            let owner = presence.unwrap_or_else(|| {
                ScriptModelId::from_wire(UNPRESENTED_LOOP_OWNER | (object as u32 & 0x0fff_ffff))
            });
            (
                owner,
                alias,
                vector(&mut runtime, object, "origin"),
                u32::try_from(number).ok(),
            )
        })
        .collect();
    let client_loops = crate::script::client::loop_rows(world);
    if crate::sound_trace::enabled() {
        let traced: Vec<_> = placed
            .iter()
            .map(|(owner, alias, origin, _)| (*owner, &**alias, *origin))
            .chain(
                client_loops
                    .iter()
                    .map(|(owner, alias, origin)| (*owner, alias.as_str(), *origin)),
            )
            .collect();
        crate::sound_trace::loops(world, &traced);
    }
    let mut frame = FrameWorld::from_world(world);
    let rows = placed
        .into_iter()
        .chain(
            client_loops
                .into_iter()
                .map(|(owner, alias, origin)| (owner, Arc::from(alias), origin, None)),
        )
        .map(
            |(owner, alias, origin, snd_ent)| crate::world_objects::DestructibleLoopSound {
                snd_ent,
                owner,
                alias_index: frame.sound_alias_index(&alias),
                origin,
            },
        )
        .collect();
    frame.world_objects_mut().set_destructible_loop_sounds(rows);
}

impl RoundScript {
    // GSC can link solid visual models to a missile (for example its grenade indicator).
    // Those models remain hittable, but must not obstruct their own parent missile.
    pub(crate) fn missile_collision_models(&self, id: crate::ProjectileId) -> Vec<ScriptModelId> {
        let Some(&missile) = self.missiles.get(&id) else {
            return Vec::new();
        };
        self.entities
            .iter()
            .filter_map(|(&object, entity)| {
                let presence = entity.presence?;
                let mut parent = object;
                for _ in 0..self.entities.len() {
                    if parent == missile {
                        return Some(presence);
                    }
                    parent = self.entities.get(&parent)?.linked_to.as_ref()?.parent;
                }
                None
            })
            .collect()
    }

    pub(crate) fn presence_of(&self, value: &Value) -> Option<ScriptModelId> {
        self.entity(value).and_then(|(_, e)| e.presence)
    }

    pub(crate) fn presented_by(&self, id: ScriptModelId) -> Option<u64> {
        self.entities
            .iter()
            .find(|(_, e)| e.presence == Some(id) && e.kind != EntityKind::HudElem)
            .map(|(object, _)| *object)
    }
}
