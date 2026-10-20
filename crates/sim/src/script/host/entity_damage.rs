use crate::ScriptModelId;
use crate::frame::FrameWorld;
use crate::script::runtime::raise;
use crate::script::{RoundScript, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::World;

#[derive(Clone, Debug)]
pub(crate) struct EntityHit {
    pub target: ScriptModelId,
    pub amount: i32,
    pub attacker: Option<ClientId>,
    /// An attacker that is no player (a turret).
    pub attacker_object: Option<u64>,
    pub means: &'static str,
    pub weapon: u32,
    pub point: [f32; 3],
    pub dir: [f32; 3],
    pub bone: Option<usize>,
    pub flags: i32,
    /// The hit location the game named; else the bone's.
    pub hit_location: Option<&'static str>,
    /// The struck bone's hit location, in its model's own numbering.
    pub part: Option<u8>,
    /// What dealt the damage when it is not the attacker (a grenade, a
    /// turret); `None` is the attacker itself.
    pub inflictor: Option<u64>,
    /// How far behind the match the hit was settled, in milliseconds (the
    /// damage callbacks' time offset).
    pub time_offset: i32,
}

const MEANS: &[&str] = &[
    "MOD_UNKNOWN",
    "MOD_PISTOL_BULLET",
    "MOD_RIFLE_BULLET",
    "MOD_EXPLOSIVE_BULLET",
    "MOD_GRENADE",
    "MOD_GRENADE_SPLASH",
    "MOD_PROJECTILE",
    "MOD_PROJECTILE_SPLASH",
    "MOD_MELEE",
    "MOD_HEAD_SHOT",
    "MOD_CRUSH",
    "MOD_FALLING",
    "MOD_SUICIDE",
    "MOD_TRIGGER_HURT",
    "MOD_EXPLOSIVE",
    "MOD_IMPACT",
    "MOD_BURNED",
    "MOD_GAS",
    "MOD_HIT_BY_OBJECT",
    "MOD_BAYONET",
    "MOD_TELEFRAG",
    "MOD_ARTILLERY",
    "MOD_DOGS",
];

pub(crate) fn means_named(name: &str) -> Result<&'static str, String> {
    MEANS
        .iter()
        .copied()
        .find(|means| means.eq_ignore_ascii_case(name))
        .ok_or_else(|| format!("unknown means of death '{name}'"))
}

#[derive(Clone, Debug)]
pub(crate) struct ScriptBlast {
    pub origin: [f32; 3],
    pub radius: f32,
    pub max: f32,
    pub min: f32,
    pub attacker: Option<ClientId>,
    pub inflictor: Option<ScriptModelId>,
    pub means: &'static str,
    pub weapon: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct ScriptHit {
    pub piece: Option<i32>,
    pub target: HitTarget,
    pub amount: i32,
    pub origin: [f32; 3],
    pub attacker: Option<ClientId>,
    /// The attacking script entity when it is not a player (an actor).
    pub attacker_object: Option<u64>,
    pub inflictor: Option<ScriptModelId>,
    pub means: &'static str,
    pub weapon: u32,
    pub flags: i32,
    pub hitloc: u8,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum HitTarget {
    Player(ClientId),
    Entity(ScriptModelId),
}

pub(crate) fn apply_script_blasts(world: &mut World, tick: crate::Tick) {
    let (blasts, hits) = match world.get_resource_mut::<RoundScript>() {
        Some(mut runtime) => (
            std::mem::take(&mut runtime.blasts),
            std::mem::take(&mut runtime.hits),
        ),
        None => return,
    };
    for hit in hits {
        match hit.target {
            HitTarget::Player(_) => {
                crate::damage::apply_script_hit(&mut FrameWorld::from_world(world), tick, &hit);
            }
            HitTarget::Entity(target) => {
                if let Some(piece) = hit.piece {
                    crate::t5_destructible::apply_piece_hit(
                        &mut FrameWorld::from_world(world),
                        tick,
                        crate::AuthorityModelOwner::ScriptModel(target),
                        piece,
                        hit.amount,
                        hit.attacker,
                    );
                    continue;
                }
                let mut runtime = world.resource_mut::<RoundScript>();
                let inflictor = hit.inflictor.and_then(|id| runtime.presented_by(id));
                let at = match runtime
                    .presented_by(target)
                    .map(|o| runtime.object_field(o, "origin"))
                {
                    Some(Value::Vector(at)) => at,
                    _ => hit.origin,
                };
                let hit_location = crate::script_player::hit_location_name(hit.hitloc);
                damage_entity(
                    world,
                    &EntityHit {
                        target,
                        amount: hit.amount,
                        attacker: hit.attacker,
                        attacker_object: hit.attacker_object,
                        means: hit.means,
                        weapon: hit.weapon,
                        point: hit.origin,
                        dir: std::array::from_fn(|i| at[i] - hit.origin[i]),
                        bone: None,
                        flags: hit.flags,
                        hit_location: Some(hit_location),
                        part: Some(hit.hitloc),
                        inflictor,
                        time_offset: 0,
                    },
                );
            }
        }
    }
    for blast in blasts {
        let mut frame = FrameWorld::from_world(world);
        if crate::game_fire::script_blast(&mut frame, tick, &blast) {
            continue;
        }
        crate::damage::apply_script_blast(&mut frame, tick, &blast);
        let inflictor = blast
            .inflictor
            .and_then(|id| world.get_resource::<RoundScript>()?.presented_by(id));
        for (target, mid, dist) in radius_targets(world, blast.origin, blast.radius) {
            if Some(target) == blast.inflictor {
                continue;
            }
            let amount =
                gamemode_iw4::radius_damage_amount(blast.max, blast.min, blast.radius, dist, 1.0);
            if amount <= 0 {
                continue;
            }
            damage_entity(
                world,
                &EntityHit {
                    target,
                    amount,
                    attacker: blast.attacker,
                    attacker_object: None,
                    means: blast.means,
                    weapon: blast.weapon,
                    point: mid,
                    dir: std::array::from_fn(|i| mid[i] - blast.origin[i]),
                    bone: None,
                    flags: 1,
                    hit_location: Some("none"),
                    part: None,
                    inflictor,
                    time_offset: 0,
                },
            );
        }
    }
}

pub(crate) fn radius_targets(
    world: &mut World,
    origin: [f32; 3],
    radius: f32,
) -> Vec<(ScriptModelId, [f32; 3], f32)> {
    let Some(mut runtime) = world.get_resource_mut::<RoundScript>() else {
        return Vec::new();
    };
    let candidates: Vec<(u64, ScriptModelId)> = runtime
        .entities
        .iter()
        .filter(|(_, entity)| entity.accepts_damage(crate::script_player::IDFLAGS_RADIUS))
        .filter_map(|(object, entity)| Some((*object, entity.presence?)))
        .collect();
    let placed: Vec<(ScriptModelId, [f32; 3])> = candidates
        .into_iter()
        .map(
            |(object, id)| match runtime.object_field(object, "origin") {
                Value::Vector(at) => (id, at),
                _ => (id, [0.0; 3]),
            },
        )
        .collect();
    let frame = FrameWorld::from_world(world);
    let mut targets = Vec::new();
    for (id, at) in placed {
        let (mid, half) = frame
            .entity_collision_capabilities()
            .iter()
            .find(|row| row.owner.script_model() == Some(id))
            .and_then(|row| row.linked_brushes.first())
            .and_then(|brush| {
                let model = frame
                    .clip_cmodels()
                    .models
                    .get(brush.cmodel_handle as usize)?;
                Some((
                    std::array::from_fn(|i| {
                        brush.origin[i] + (model.mins[i] + model.maxs[i]) * 0.5
                    }),
                    std::array::from_fn(|i| (model.maxs[i] - model.mins[i]) * 0.5),
                ))
            })
            .unwrap_or((at, [0.0; 3]));
        let dist = gamemode_iw4::radius_damage_distance_to_aabb(origin, mid, half);
        if dist < radius {
            targets.push((id, mid, dist));
        }
    }
    targets.sort_by(|a, b| a.2.total_cmp(&b.2));
    targets
}

pub(crate) fn damage_entity(world: &mut World, hit: &EntityHit) -> bool {
    let Some(object) = world
        .get_resource::<RoundScript>()
        .and_then(|runtime| runtime.presented_by(hit.target))
        .filter(|object| {
            world.resource::<RoundScript>().entities[object].accepts_damage(hit.flags)
        })
    else {
        return false;
    };
    let hooks: Vec<_> = world
        .get_resource::<crate::script::NativeRegistry>()
        .map(crate::script::NativeRegistry::entity_hit_hooks)
        .into_iter()
        .flatten()
        .collect();
    for hook in hooks {
        if hook(world, object, hit) {
            return true;
        }
    }
    let frame = FrameWorld::from_world(world);
    let (part, model_index) = hit
        .bone
        .and_then(|bone| {
            let dobj = frame
                .entity_collision_capabilities()
                .iter()
                .find(|row| row.owner.script_model() == Some(hit.target))
                .and_then(|row| row.dobj.as_ref())?;
            let name = dobj.bone_name(bone).unwrap_or("").to_owned();
            Some((name, dobj.bone_model_index(bone)))
        })
        .unwrap_or_else(|| (String::new(), 0));
    let weapon = crate::script_player::weapon_name(&frame, hit.weapon);
    let attacker = match hit.attacker {
        Some(a) => super::players::player_object(world, a.0),
        None => {
            super::players::damage_entity(world, hit.attacker_object.map(Value::Object).as_ref())
        }
    };
    if world.resource::<RoundScript>().entities[&object].kind
        == super::entities::EntityKind::Vehicle
    {
        super::vehicles::damage(world, object, hit, attacker, &weapon, &part);
        return true;
    }
    if world.resource::<RoundScript>().entities[&object].kind == super::entities::EntityKind::Actor
    {
        super::actors::damage(world, object, hit, attacker, &weapon, model_index);
        return true;
    }
    let amount = super::t6_zombies::entity_damage_amount(world, object, hit);
    let (model_name, tag_name) = if model_index > 0 {
        super::actors::attached_model_tag(world, object, model_index)
    } else {
        let mut runtime = world.resource_mut::<RoundScript>();
        let model = match runtime.object_field(object, "model") {
            Value::String(model) => Value::String(model),
            _ => Value::string(""),
        };
        (model, Value::string(""))
    };
    let mut runtime = world.resource_mut::<RoundScript>();
    let before = match runtime.object_field(object, "health") {
        Value::Int(health) => health,
        Value::Float(health) => health as i32,
        _ => 0,
    };
    let after = before.saturating_sub(amount);
    runtime.set_object_field(object, "health", Value::Int(after));
    super::t6_zombies::entity_damage(world, object, hit, before, after, part.contains("head"));
    let receiver = Value::Object(object);
    raise(
        world,
        receiver.clone(),
        "damage",
        vec![
            Value::Int(amount),
            attacker.clone(),
            Value::Vector(hit.dir),
            Value::Vector(hit.point),
            Value::string(hit.means),
            model_name,
            tag_name,
            Value::String(part.as_str().into()),
            Value::Int(hit.flags),
            Value::String(weapon.into()),
        ],
    );
    if before > 0 && after <= 0 {
        raise(world, receiver, "death", vec![attacker]);
    }
    true
}

pub(crate) fn destructible_callback(
    world: &mut World,
    owner: crate::AuthorityModelOwner,
    entry: &str,
    args: Vec<Value>,
) {
    let Some(object) = owner
        .script_model()
        .and_then(|id| world.get_resource::<RoundScript>()?.presented_by(id))
    else {
        return;
    };
    let name = format!("iw4l_maps/destructibles::{entry}");
    let now = super::players::now_ms(world);
    if let Err(fault) =
        crate::script::runtime::run_now(world, &name, Value::Object(object), args, now)
    {
        diag::warn!(Sim, "destructible callback unavailable: {fault:?}");
    }
}

pub(crate) fn destructible_attacker(world: &World, attacker: Option<crate::ClientId>) -> Value {
    attacker.map_or(Value::Undefined, |client| {
        super::players::player_object(world, client.0)
    })
}

pub(crate) fn destructible_effect(
    world: &mut World,
    name: String,
    origin: [f32; 3],
    direction: [f32; 3],
    up: [f32; 3],
) -> Option<u64> {
    let now = super::players::now_ms(world);
    let mut runtime = world.get_resource_mut::<RoundScript>()?;
    let Ok(id) = runtime.create_entity(super::entities::EntityKind::Spawned, "script_model") else {
        return None;
    };
    let forward = glam::Vec3::from_array(direction)
        .try_normalize()
        .unwrap_or(glam::Vec3::Z);
    runtime.engine.effects.insert(
        id,
        super::entities::PersistentFx {
            name,
            origin,
            forward: forward.to_array(),
            up,
            start_ms: Some(now as i32),
            repeat_ms: 0,
            cull_distance: 0.0,
        },
    );
    Some(id)
}

pub(crate) fn stop_destructible_effect(world: &mut World, id: u64) {
    if let Some(mut runtime) = world.get_resource_mut::<RoundScript>() {
        runtime.delete_entity(id);
    }
}

pub(crate) fn set_destructible_model(
    world: &mut World,
    owner: crate::AuthorityModelOwner,
    model: Option<&str>,
    sound: Option<&str>,
) {
    let Some(mut runtime) = world.get_resource_mut::<RoundScript>() else {
        return;
    };
    let Some(object) = owner.script_model().and_then(|id| runtime.presented_by(id)) else {
        return;
    };
    if let Some(model) = model {
        runtime.set_object_field(object, "model", Value::string(model));
    }
    runtime.entities.get_mut(&object).unwrap().loop_sound = sound.map(std::sync::Arc::from);
}

pub(crate) fn destructible_debris(
    world: &mut World,
    dobj: crate::AuthorityDObjState,
    offset: [f32; 3],
    half: [f32; 3],
    velocity: [f32; 3],
) {
    let origin = dobj.world_from_model.w_axis.truncate().to_array();
    let rotation = dobj.world_from_model;
    let forward = rotation.x_axis.truncate();
    let angles = [
        -forward.z.atan2(forward.truncate().length()).to_degrees(),
        forward.y.atan2(forward.x).to_degrees(),
        0.0,
    ];
    let Ok(object) = world
        .resource_mut::<RoundScript>()
        .create_entity(super::entities::EntityKind::Spawned, "script_model")
    else {
        return;
    };
    let presence = match super::presence::spawn_presence(world, origin) {
        Ok(presence) => presence,
        Err(_) => {
            let number = world
                .resource::<RoundScript>()
                .entities
                .get(&object)
                .map(|entity| entity.number);
            if let Some(number) = number {
                crate::frame::FrameWorld::from_world(world).despawn_dynamic_entity(number);
            }
            world.resource_mut::<RoundScript>().delete_entity(object);
            return;
        }
    };
    let model = dobj.current_model.clone();
    let mut frame = FrameWorld::from_world(world);
    if let Some(row) = frame.collision_owner_mut(presence) {
        row.dobj = Some(dobj);
        row.solid = false;
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.set_object_field(object, "origin", Value::Vector(origin));
    runtime.set_object_field(object, "angles", Value::Vector(angles));
    runtime.set_object_field(object, "model", Value::string(&model));
    let entity = runtime.entities.get_mut(&object).unwrap();
    entity.presence = Some(presence);
    entity.solid = false;
    let now = super::players::now_ms(world);
    world
        .resource_mut::<RoundScript>()
        .lingering
        .push((now + 30000, object));
    world
        .resource_mut::<super::mechanics::Mechanics>()
        .launch_piece(object, velocity, offset, half);
}

/// What a game's shot reads of a script entity it reaches.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShotTarget {
    pub actor: bool,
    pub takes_damage: bool,
    pub health: i32,
    pub team: Option<&'static str>,
    pub ai_type: Option<&'static str>,
    pub ignore_locational_damage: bool,
    pub allow_death: bool,
    /// An actor whose pending AI states (scripted animation, negotiation)
    /// hold off a lethal hit.
    pub delays_death: bool,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub script_model: bool,
    /// An actor's `tag_eye`.
    pub eye: Option<[f32; 3]>,
    /// A brush model's bounds: in the world, and in its own space.
    pub brush: Option<(([f32; 3], [f32; 3]), ([f32; 3], [f32; 3]))>,
    pub contents: u32,
    pub destructible: bool,
}

pub(crate) fn shot_target(world: &mut World, target: ScriptModelId) -> Option<ShotTarget> {
    let (object, actor, base_takes_damage) = {
        let runtime = world.get_resource::<RoundScript>()?;
        let object = runtime.presented_by(target)?;
        let entity = &runtime.entities[&object];
        (
            object,
            entity.kind == super::entities::EntityKind::Actor,
            entity.accepts_damage(0),
        )
    };
    let in_use = !actor || super::natives::t6::actor_in_use(world, object);
    let mut runtime = world.get_resource_mut::<RoundScript>()?;
    let takes_damage = base_takes_damage && in_use;
    let int = |value: Value| match value {
        Value::Int(n) => Some(n),
        Value::Float(f) => Some(f as i32),
        _ => None,
    };
    let named = |value: Value, names: &[&'static str]| match value {
        Value::String(text) => names
            .iter()
            .copied()
            .find(|name| name.as_bytes() == text.as_bytes()),
        _ => None,
    };
    let health = int(runtime.object_field(object, "health")).unwrap_or(0);
    let team = named(
        runtime.object_field(object, "team"),
        &["axis", "allies", "neutral"],
    );
    let ai_type = named(
        runtime.object_field(object, "type"),
        &["human", "zombie", "zombie_dog", "dog"],
    )
    .filter(|_| actor);
    let ignore_locational_damage =
        int(runtime.object_field(object, "ignorelocationaldamage")).is_some_and(|n| n != 0);
    // Black Ops spawns its actors with allowdeath off.
    let allow_death = int(runtime.object_field(object, "allowdeath")).is_some_and(|n| n != 0);
    let delays_death = actor && super::actor_brain::delays_death(&runtime, object);
    let vector = |value: Value| match value {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    let origin = vector(runtime.object_field(object, "origin"));
    let angles = vector(runtime.object_field(object, "angles"));
    let script_model = matches!(
        runtime.object_field(object, "classname"),
        Value::String(name) if name.as_bytes() == b"script_model"
    );
    let eye = if actor {
        super::presence::tag_world(world, object, "tag_eye").map(|(at, _)| at)
    } else {
        None
    };
    let frame = FrameWorld::from_world(world);
    let row = frame
        .entity_collision_capabilities()
        .iter()
        .find(|row| row.owner.script_model() == Some(target));
    let destructible = row
        .and_then(|row| row.dobj.as_ref())
        .is_some_and(|dobj| dobj.t5_destructible.is_some());
    let (brush, contents) = row
        .and_then(|row| row.linked_brushes.first())
        .and_then(|brush| {
            let model = frame
                .clip_cmodels()
                .models
                .get(brush.cmodel_handle as usize)?;
            let at = brush.origin;
            Some((
                Some((
                    (
                        core::array::from_fn(|i| at[i] + model.mins[i]),
                        core::array::from_fn(|i| at[i] + model.maxs[i]),
                    ),
                    (model.mins, model.maxs),
                )),
                // A brush model is solid to what its brushes are.
                frame
                    .clip_bsp()
                    .leafbrushes
                    .iter()
                    .skip(model.first_brush as usize)
                    .take(usize::from(model.num_brushes))
                    .filter_map(|&id| frame.clip_brushes().get(id as usize))
                    .fold(0, |contents, b| contents | b.contents),
            ))
        })
        .unwrap_or((None, 0));
    Some(ShotTarget {
        actor,
        takes_damage,
        health,
        team,
        ai_type,
        ignore_locational_damage,
        allow_death,
        delays_death,
        origin,
        angles,
        script_model,
        eye,
        brush,
        contents,
        destructible,
    })
}

/// A value a game's notify passes to scripts.
#[derive(Clone, Debug)]
pub(crate) enum NotifyValue {
    String(String),
    /// A script object; undefined when the entity has none.
    Entity(Option<u64>),
    Vector([f32; 3]),
    Float(f32),
    Int(i32),
}

/// Notifies `name` on a script object.
pub(crate) fn notify_entity(world: &mut World, object: u64, name: &str, args: &[NotifyValue]) {
    let args = args
        .iter()
        .map(|arg| match arg {
            NotifyValue::String(text) => Value::string(text),
            NotifyValue::Entity(Some(object)) => Value::Object(*object),
            NotifyValue::Entity(None) => Value::Undefined,
            NotifyValue::Vector(v) => Value::Vector(*v),
            NotifyValue::Float(f) => Value::Float(*f),
            NotifyValue::Int(n) => Value::Int(*n),
        })
        .collect();
    raise(world, Value::Object(object), name, args);
}

/// A player's script object.
pub(crate) fn player_object_id(world: &World, client: u32) -> Option<u64> {
    match super::players::player_object(world, client) {
        Value::Object(object) => Some(object),
        _ => None,
    }
}

/// The script object an entity number stands for.
pub(crate) fn object_by_number(world: &World, number: i32) -> Option<u64> {
    world
        .get_resource::<RoundScript>()?
        .entities
        .iter()
        .find(|(_, entity)| entity.number == number)
        .map(|(object, _)| *object)
}

/// Every script entity the match presents, with its origin.
pub(crate) fn presented_origins(world: &mut World) -> Vec<(ScriptModelId, [f32; 3])> {
    let Some(mut runtime) = world.get_resource_mut::<RoundScript>() else {
        return Vec::new();
    };
    let presented: Vec<(u64, ScriptModelId)> = runtime
        .entities
        .iter()
        .filter_map(|(object, entity)| Some((*object, entity.presence?)))
        .collect();
    presented
        .into_iter()
        .map(
            |(object, id)| match runtime.object_field(object, "origin") {
                Value::Vector(at) => (id, at),
                _ => (id, [0.0; 3]),
            },
        )
        .collect()
}
