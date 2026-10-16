//! Black Ops 2's client builtins: particle effects, camera shake, vision
//! sets, fog banks, screen overlays, filter passes, shader constants, and
//! primary and mixer lights.

use super::super::{float, int, string, type_name, vector};
use super::game::{LOCAL_CLIENT, client_entity, local_client, server_object};
use crate::script::host::args::arg;
use crate::script::host::client_scripts::with_server;
use crate::script::host::entities::EntityKind;
use crate::script::{MatchScript, Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

/// The client configstring table holds 195 effect slots (1..196).
const EFFECT_SLOTS: i32 = 196;
/// The client's mapped filter material slots (0..=63).
const FILTER_MATERIAL_SLOTS: usize = 64;
/// The client's filter indices (0..8), passes per filter (0..4) and
/// constants per pass (0..16).
const FILTER_SLOTS: i32 = 8;
const FILTER_PASSES: i32 = 4;
const FILTER_CONSTANTS: i32 = 16;
/// Per-entity shader constant slots (0..=6).
const SHADER_CONSTANT_SLOTS: i32 = 6;
/// Maximum mixer light logic parameter index (0..=16) and mixer logic
/// parameter index (0..=4).
const MIXER_LIGHT_PARAM_MAX: i32 = 16;
const MIXER_PARAM_MAX: i32 = 4;

/// The shader constant names `mapshaderconstant` accepts (case aside).
const SHADER_CONSTANTS: [&str; 23] = [
    "_characterCharredAmount",
    "_characterDissolveColor",
    "scriptVector0",
    "scriptVector1",
    "scriptVector2",
    "scriptVector3",
    "scriptVector4",
    "scriptVector5",
    "scriptVector6",
    "scriptVector7",
    "heroLightingR",
    "heroLightingG",
    "heroLightingB",
    "weaponParam0",
    "weaponParam1",
    "weaponParam2",
    "weaponParam3",
    "weaponParam4",
    "weaponParam5",
    "weaponParam6",
    "weaponParam7",
    "weaponParam8",
    "weaponParam9",
];

/// The light behaviors the client engine's mixer steps in native code.
const HARDCODED_LIGHT_BEHAVIORS: [&str; 12] = [
    "firecaster",
    "fire_flicker",
    "electrical_flicker",
    "flicker_then_stop",
    "generic_double_strobe",
    "generic_strobe",
    "light_off",
    "light_solid",
    "television",
    "generic_flickering",
    "generic_pulsing",
    "explode_then_flicker",
];

/// A client entity turned into a dynamic primary light (`makelight`).
#[derive(Clone, Debug)]
struct LightState {
    color_bytes: [u8; 3],
    exponent: u8,
    intensity: f32,
    radius: f32,
    bsp_radius: f32,
    cos_half_fov_outer: f32,
    cos_half_fov_inner: f32,
    bsp_cos_half_fov_outer: f32,
    bsp_cos_half_fov_inner: f32,
    mixer: Option<usize>,
}

impl Default for LightState {
    fn default() -> Self {
        Self {
            color_bytes: [255, 255, 255],
            exponent: 0,
            intensity: 1.0,
            radius: 200.0,
            bsp_radius: f32::MAX,
            cos_half_fov_outer: 0.5,
            cos_half_fov_inner: 0.866_025_4,
            bsp_cos_half_fov_outer: 0.0,
            bsp_cos_half_fov_inner: 0.866_025_4,
            mixer: None,
        }
    }
}

/// Client effects, visionsets, filter materials and lights.
#[derive(Resource)]
struct ClientEffects {
    /// Loaded effect names by 1-based effect id.
    effects: Vec<String>,
    /// Next playback handle for `playfx`, `playfxontag` and `playviewmodelfx`.
    next_handle: i32,
    /// Active effect handles and the client time (ms) they started at.
    playing: BTreeMap<i32, i64>,
    /// The local client's current naked visionset name.
    naked_vision: Option<String>,
    /// The local client's active world fog bank mask.
    world_fog_bank: u16,
    /// The local client's mapped filter material names (`map_material`).
    mapped_materials: [Option<String>; FILTER_MATERIAL_SLOTS],
    /// Entities turned into primary lights (`makelight`), by client object id.
    lights: BTreeMap<u64, LightState>,
    /// Allocated mixer count (`allocatemixerlights`).
    allocated_mixers: usize,
    /// Number of mixers created so far (`makemixerlight`).
    mixer_count: usize,
}

impl Default for ClientEffects {
    fn default() -> Self {
        Self {
            effects: Vec::new(),
            next_handle: 0,
            playing: BTreeMap::new(),
            naked_vision: None,
            world_fog_bank: 0,
            mapped_materials: std::array::from_fn(|_| None),
            lights: BTreeMap::new(),
            allocated_mixers: 0,
            mixer_count: 0,
        }
    }
}

/// A `GfxWorld` world fog volume: its packed word (the bank bits are tested
/// against the active bank mask), script id and bounding planes.
struct FogVolume {
    packed: u32,
    script_id: u16,
    planes: Vec<[f32; 4]>,
}

/// The map's world fog volumes in `GfxWorld` order, as the assets lane
/// captures them.
#[derive(Resource, Default)]
struct FogVolumes(Vec<FogVolume>);

fn ensure_fog_volumes(world: &mut World) -> &FogVolumes {
    if !world.contains_resource::<FogVolumes>() {
        let text = with_server(world, |server| {
            server
                .resource::<RoundScript>()
                .restart
                .as_ref()?
                .raw_files
                .get("gfxworld/fog_volumes.txt")
                .cloned()
        })
        .flatten();
        let volumes = text
            .iter()
            .flat_map(|text| text.lines())
            .filter_map(|line| {
                let mut parts = line.split('\t');
                let packed = parts.next()?.parse().ok()?;
                let script_id = parts.next()?.parse().ok()?;
                let planes = parts
                    .next()?
                    .split(';')
                    .map(|plane| {
                        let mut values = plane.split(' ').filter_map(|v| v.parse().ok());
                        Some([
                            values.next()?,
                            values.next()?,
                            values.next()?,
                            values.next()?,
                        ])
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(FogVolume {
                    packed,
                    script_id,
                    planes,
                })
            })
            .collect();
        world.insert_resource(FogVolumes(volumes));
    }
    world.resource::<FogVolumes>()
}

fn state(world: &mut World) -> bevy_ecs::prelude::Mut<'_, ClientEffects> {
    world.get_resource_or_insert_with(ClientEffects::default)
}

fn now_ms(world: &World) -> i64 {
    world
        .get_resource::<crate::step::StepRequest>()
        .map_or(0, |r| i64::from(r.tick.0) * i64::from(crate::MATCH_TICK_MS))
}

/// A method's receiving client entity object id.
fn receiver_entity(world: &World, receiver: &Value) -> Result<u64, String> {
    match world.resource::<RoundScript>().entity(receiver) {
        Some((id, entity)) if entity.kind != EntityKind::HudElem => Ok(id),
        _ => Err("not an entity".into()),
    }
}

/// An entity parameter read as `Scr_GetEntityRef` reads it.
fn entity_arg(world: &World, args: &[Value], index: usize) -> Result<u64, String> {
    let value = arg(args, index)?;
    match world.resource::<RoundScript>().entity(value) {
        Some((id, entity)) if entity.kind != EntityKind::HudElem => Ok(id),
        _ => Err(format!("type {} is not an entity", type_name(value))),
    }
}

fn entity_origin(world: &mut World, id: u64) -> [f32; 3] {
    match world
        .resource_mut::<RoundScript>()
        .object_field(id, "origin")
    {
        Value::Vector(origin) => origin,
        _ => [0.0; 3],
    }
}

fn entity_angles(world: &mut World, id: u64) -> [f32; 3] {
    match world
        .resource_mut::<RoundScript>()
        .object_field(id, "angles")
    {
        Value::Vector(angles) => angles,
        _ => [0.0; 3],
    }
}

/// The engine's float `Vec3Normalize`: a zero vector keeps its length.
fn normalize(v: [f32; 3]) -> ([f32; 3], f32) {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let scale = 1.0 / if -length >= 0.0 { 1.0 } else { length };
    (v.map(|c| c * scale), length)
}

fn atan2_degrees(y: f32, x: f32, degrees: f32) -> f32 {
    f64::from(y).atan2(f64::from(x)) as f32 * degrees * 0.318_309_87
}

fn signed_pitch(v: [f32; 3]) -> f32 {
    if v[1] == 0.0 && v[0] == 0.0 {
        return if -v[2] >= 0.0 { 90.0 } else { -90.0 };
    }
    atan2_degrees(v[2], (v[0] * v[0] + v[1] * v[1]).sqrt(), -180.0)
}

fn vector_angles(v: [f32; 3]) -> [f32; 3] {
    if v[1] == 0.0 && v[0] == 0.0 {
        return [if -v[2] < 0.0 { 270.0 } else { 90.0 }, 0.0, 0.0];
    }
    let wrap = |a: f32| if a < 0.0 { a + 360.0 } else { a };
    let yaw = wrap(atan2_degrees(v[1], v[0], 180.0));
    let pitch = wrap(atan2_degrees(
        v[2],
        (v[0] * v[0] + v[1] * v[1]).sqrt(),
        -180.0,
    ));
    [pitch, yaw, 0.0]
}

fn sin_cos(degrees: f32) -> (f32, f32) {
    let radians = f64::from(degrees * 0.017_453_292);
    (radians.sin() as f32, radians.cos() as f32)
}

fn axis_angles(axis: [[f32; 3]; 3]) -> [f32; 3] {
    let mut angles = vector_angles(axis[0]);
    let left = axis[1];
    let (sin, cos) = sin_cos(-angles[1]);
    let y = left[0] * sin + left[1] * cos;
    let x = left[0] * cos - left[1] * sin;
    let (sin, cos) = sin_cos(-angles[0]);
    let rolled = [left[2] * sin + x * cos, y, left[2] * cos - x * sin];
    let roll = signed_pitch(rolled);
    angles[2] = if y >= 0.0 {
        -roll
    } else if roll < 0.0 {
        roll + 180.0
    } else {
        roll - 180.0
    };
    angles
}

fn angles_axis(angles: [f32; 3]) -> [[f32; 3]; 3] {
    let (sp, cp) = sin_cos(angles[0]);
    let (sy, cy) = sin_cos(angles[1]);
    let (sr, cr) = sin_cos(angles[2]);
    [
        [cp * cy, cp * sy, -sp],
        [sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, sr * cp],
        [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp],
    ]
}

fn effect_name(world: &World, id: i32) -> Option<String> {
    let index = usize::try_from(id - 1).ok()?;
    world
        .get_resource::<ClientEffects>()
        .and_then(|fx| fx.effects.get(index).cloned())
}

fn fx_param_error(world: &World, id: i32, text: &str) -> String {
    let effect = effect_name(world, id).unwrap_or_else(|| "not successfully loaded".into());
    format!("{text} (effect = {effect})\n")
}

fn orient_angles(
    world: &World,
    id: i32,
    args: &[Value],
    fwd_at: usize,
    up_at: usize,
) -> Result<[f32; 3], String> {
    if args.len() <= fwd_at {
        return Ok([270.0, 0.0, 0.0]);
    }
    let (forward, length) = normalize(vector(args, fwd_at)?);
    if length == 0.0 {
        return Err(fx_param_error(
            world,
            id,
            "playFx called with (0 0 0) forward direction",
        ));
    }
    if args.len() <= up_at {
        return Ok(vector_angles(forward));
    }
    let (up, length) = normalize(vector(args, up_at)?);
    if length == 0.0 {
        return Err(fx_param_error(
            world,
            id,
            "playFx called with (0 0 0) up direction",
        ));
    }
    let along = -(up[0] * forward[0] + up[1] * forward[1] + up[2] * forward[2]);
    let (up, length) = normalize(std::array::from_fn(|i| forward[i] * along + up[i]));
    if length == 0.0 {
        return Err(
            "forward and up vectors are the same direction or exact opposite directions".into(),
        );
    }
    let left = [
        up[1] * forward[2] - up[2] * forward[1],
        up[2] * forward[0] - up[0] * forward[2],
        up[0] * forward[1] - up[1] * forward[0],
    ];
    Ok(axis_angles([forward, left, up]))
}

/// Allocates a client FX handle and marks it playing on this tick.
fn start_fx_handle(world: &mut World) -> i32 {
    let now = now_ms(world);
    let mut fx = state(world);
    fx.next_handle = fx.next_handle.wrapping_add(1).max(1);
    let handle = fx.next_handle;
    fx.playing.insert(handle, now);
    handle
}

/// Emits an effect on the server's presentation world when the effect asset is
/// present.
fn emit_fx_at(world: &World, id: i32, origin: [f32; 3], angles: [f32; 3]) {
    let Some(name) = effect_name(world, id) else {
        return;
    };
    let axis = angles_axis(angles);
    with_server(world, |server| {
        if server
            .resource::<RoundScript>()
            .restart
            .as_ref()
            .is_some_and(|plan| plan.absent_effects.contains(&name))
        {
            return;
        }
        let effect = crate::frame::FrameWorld::from_world(server).effect_name_index(&name);
        crate::script::host::natives::engine::play_fx_at(server, effect, origin, axis[0], axis[2]);
    });
}

/// `loadfx(name)`: registers or looks up the effect's 1-based slot.
fn load_fx(world: &mut World, args: &[Value]) -> Result<Value, String> {
    let name = string(args, 0)?;
    if name
        .get(..3)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("fx/"))
    {
        return Err("effect name should start after the 'fx' folder.".into());
    }
    if name.is_empty() {
        return Err(format!(
            "loadedfx could not find effect {name} - ensure that effect is precached in the server script, and included in appropriate CSVs.\n"
        ));
    }
    let mut fx = state(world);
    if let Some(pos) = fx
        .effects
        .iter()
        .position(|known| known.eq_ignore_ascii_case(&name))
    {
        return Ok(Value::Int((pos + 1) as i32));
    }
    if fx.effects.len() as i32 >= EFFECT_SLOTS - 1 {
        return Err(format!(
            "loadedfx could not find effect {name} - ensure that effect is precached in the server script, and included in appropriate CSVs.\n"
        ));
    }
    fx.effects.push(name.clone());
    let id = fx.effects.len() as i32;
    drop(fx);
    with_server(world, |server| {
        if !server
            .resource::<RoundScript>()
            .restart
            .as_ref()
            .is_some_and(|plan| plan.absent_effects.contains(&name))
        {
            crate::frame::FrameWorld::from_world(server).effect_name_index(&name);
        }
    });
    Ok(Value::Int(id))
}

/// `playfx(localclientnum, id, origin, [forward], [up], [delay], [primlight], [lightorg])`.
fn play_fx(world: &mut World, args: &[Value]) -> Result<Value, String> {
    local_client(args, 0)?;
    let id = int(args, 1)?;
    if !(1..EFFECT_SLOTS).contains(&id) {
        return Err(format!("CScr_PlayFX: invalid effect id {id}"));
    }
    let origin = vector(args, 2)?;
    let angles = if args.len() <= 4 {
        orient_angles(world, id, args, 3, 4)?
    } else {
        let (forward, fwd_len) = normalize(vector(args, 3)?);
        if fwd_len == 0.0 {
            return Err(fx_param_error(
                world,
                id,
                "playFx called with (0 0 0) forward direction",
            ));
        }
        let (up, up_len) = normalize(vector(args, 4)?);
        if up_len == 0.0 {
            return Err(fx_param_error(
                world,
                id,
                "playFx called with (0 0 0) up direction",
            ));
        }
        if args.len() > 5 {
            float(args, 5)?;
        }
        if args.len() > 6 && !matches!(args.get(6), None | Some(Value::Undefined)) {
            float(args, 6)?;
        }
        if args.len() > 7 && !matches!(args.get(7), None | Some(Value::Undefined)) {
            int(args, 7)?;
        }
        let along = -(up[0] * forward[0] + up[1] * forward[1] + up[2] * forward[2]);
        let (up, len) = normalize(std::array::from_fn(|i| forward[i] * along + up[i]));
        if len == 0.0 {
            return Err(
                "forward and up vectors are the same direction or exact opposite directions".into(),
            );
        }
        let left = [
            up[1] * forward[2] - up[2] * forward[1],
            up[2] * forward[0] - up[0] * forward[2],
            up[0] * forward[1] - up[1] * forward[0],
        ];
        axis_angles([forward, left, up])
    };
    emit_fx_at(world, id, origin, angles);
    Ok(Value::Int(start_fx_handle(world)))
}

/// `playfxontag(localclientnum, id, entity, tag)`.
fn play_fx_on_tag(world: &mut World, args: &[Value]) -> Result<Value, String> {
    let client = local_client(args, 0)?;
    let id = int(args, 1)?;
    let ent_id = entity_arg(world, args, 2)?;
    if !world.resource::<RoundScript>().can_receive_call(&ent_id) {
        return Err(format!(
            "CScr_PlayFX: invalid entity for local client {client}, either not in the snapshot for the client or dobj does not exist yet"
        ));
    }
    if !(1..EFFECT_SLOTS).contains(&id) {
        return Err(format!("CScr_PlayFX: invalid effect id {id}"));
    }
    let tag = string(args, 3)?;
    let snapshot_number = client_entity(world, &args[2]).map(|(number, _)| number);
    let fallback_origin = entity_origin(world, ent_id);
    let fallback_angles = entity_angles(world, ent_id);
    if let Some(name) = effect_name(world, id) {
        with_server(world, |server| {
            if server
                .resource::<RoundScript>()
                .restart
                .as_ref()
                .is_some_and(|plan| plan.absent_effects.contains(&name))
            {
                return;
            }
            let (origin, angles) = snapshot_number
                .and_then(|number| server_object(server, number))
                .and_then(|srv_id| {
                    crate::script::host::presence::tag_world(server, srv_id, &tag)
                        .map(|(pos, axis)| (pos, math_iw4::axis_to_angles(axis)))
                })
                .unwrap_or((fallback_origin, fallback_angles));
            let axis = angles_axis(angles);
            let effect = crate::frame::FrameWorld::from_world(server).effect_name_index(&name);
            crate::script::host::natives::engine::play_fx_at(
                server, effect, origin, axis[0], axis[2],
            );
        });
    }
    Ok(Value::Int(start_fx_handle(world)))
}

/// `playviewmodelfx(localclientnum, id, tag)`: plays an effect bolted to the
/// local client's viewmodel.
fn play_viewmodel_fx(world: &mut World, args: &[Value]) -> Result<Value, String> {
    local_client(args, 0)?;
    let _id = int(args, 1)?;
    let tag = string(args, 2)?;
    if tag.is_empty() {
        return Err("PlayViewmodelFX(): unable to find viewmodel tag.\n".into());
    }
    // t6.csc.fx.viewmodel: IW4L has no client viewmodel DObj to bolt
    // first-person weapon effects onto.
    Ok(Value::Int(start_fx_handle(world)))
}

/// `spawnfx(localclientnum, id, origin, delay, [forward], [up])`.
fn spawn_fx(world: &mut World, args: &[Value]) -> Result<Value, String> {
    local_client(args, 0)?;
    let id = int(args, 1)?;
    if !(1..EFFECT_SLOTS).contains(&id) {
        return Err(format!("CScr_PlayFX: invalid effect id {id}"));
    }
    let origin = vector(args, 2)?;
    let _delay = float(args, 3)?;
    let angles = orient_angles(world, id, args, 4, 5)?;
    let entity = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let entity = runtime.create_entity(EntityKind::Spawned, "fx")?;
        runtime.set_object_field(entity, "origin", Value::Vector(origin));
        runtime.set_object_field(entity, "angles", Value::Vector(angles));
        runtime.set_object_field(entity, "type", Value::string("NA"));
        entity
    };
    emit_fx_at(world, id, origin, angles);
    Ok(Value::Object(entity))
}

/// Rounding used by the client executable's float-to-int conversions.
fn round_int(value: f32) -> i32 {
    (f64::from(value) + 9.313_225_746_154_785e-10).round_ties_even() as i32
}

/// Packs a normalized color channel into `0..=255` as `Byte4PackRgba` does.
fn pack_color_byte(channel: f32) -> u8 {
    round_int(channel * 255.0).clamp(0, 255) as u8
}

/// Checks that `receiver` is a client entity turned into a light by
/// `makelight`, with the handler's error text when it is not.
fn light_entity(world: &World, receiver: &Value, with_period: bool) -> Result<u64, String> {
    let id = receiver_entity(world, receiver)?;
    let is_light = world
        .get_resource::<ClientEffects>()
        .is_some_and(|fx| fx.lights.contains_key(&id));
    if !is_light {
        return Err(if with_period {
            "Function can only be called on a 'light' entity.".into()
        } else {
            "Function can only be called on a 'light' entity".into()
        });
    }
    Ok(id)
}

/// Checks that `receiver` is a light entity for a mixer builtin, formatting
/// its origin when `makelight` was not called on it.
fn mixer_light_entity(world: &mut World, receiver: &Value) -> Result<u64, String> {
    let id = receiver_entity(world, receiver)?;
    let is_light = world
        .get_resource::<ClientEffects>()
        .is_some_and(|fx| fx.lights.contains_key(&id));
    if !is_light {
        let at = entity_origin(world, id);
        return Err(format!(
            "MakeMixerLight can only be called on ents that have been turned into lights using the 'makelight' command.  Failed light is at ({:.2}, {:.2}, {:.2})\n",
            f64::from(at[0]),
            f64::from(at[1]),
            f64::from(at[2]),
        ));
    }
    Ok(id)
}

/// Checks that `receiver` is a light entity that has had `makemixerlight`
/// called on it.
fn mixer_entity(world: &mut World, receiver: &Value) -> Result<u64, String> {
    let id = mixer_light_entity(world, receiver)?;
    let has_mixer = world
        .get_resource::<ClientEffects>()
        .and_then(|fx| fx.lights.get(&id))
        .is_some_and(|light| light.mixer.is_some());
    if !has_mixer {
        return Err(
            "Attempt to call light mixer function on a light that doesn't have a mixer associated with it."
                .into(),
        );
    }
    Ok(id)
}

/// Reads the mixer side argument (`0` / `"left"` vs `1` / `"right"`).
fn mixer_side(args: &[Value], index: usize) -> Result<usize, String> {
    match arg(args, index)? {
        Value::Int(n) => Ok(usize::from(*n >= 1)),
        Value::String(s) => Ok(usize::from(&**s != "left")),
        Value::LocalizedString(s) => Ok(usize::from(&**s != "left")),
        _ => Ok(0),
    }
}

/// Validates a mixer logic parameter value (int, float, string, or undefined).
fn check_mixer_param_value(args: &[Value], index: usize, error: &str) -> Result<(), String> {
    match arg(args, index)? {
        Value::Undefined
        | Value::Int(_)
        | Value::Float(_)
        | Value::String(_)
        | Value::LocalizedString(_) => Ok(()),
        _ => Err(error.into()),
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.register(Function, "loadfx", |world, _, args| load_fx(world, args));
    registry.register(Function, "playfx", |world, _, args| play_fx(world, args));
    registry.register(Function, "playfxontag", |world, _, args| {
        play_fx_on_tag(world, args)
    });
    registry.register(Function, "playviewmodelfx", |world, _, args| {
        play_viewmodel_fx(world, args)
    });
    registry.register(Function, "spawnfx", |world, _, args| spawn_fx(world, args));
    // stopfx(localclientnum, handle): stops a playing client effect.
    registry.register(Function, "stopfx", |world, _, args| {
        local_client(args, 0)?;
        let handle = int(args, 1)?;
        state(world).playing.remove(&handle);
        Ok(Value::Undefined)
    });
    // deletefx(localclientnum, handle, [immediate]): deletes a playing client
    // effect.
    registry.register(Function, "deletefx", |world, _, args| {
        local_client(args, 0)?;
        let handle = int(args, 1)?;
        if args.len() >= 3 {
            int(args, 2)?;
        }
        state(world).playing.remove(&handle);
        Ok(Value::Undefined)
    });
    // isfxplaying(localclientnum, handle): whether the effect is still active.
    registry.register(Function, "isfxplaying", |world, _, args| {
        local_client(args, 0)?;
        let handle = int(args, 1)?;
        let now = now_ms(world);
        // t6.csc.fx.lifetime: IW4L's effect events do not report particle
        // lifetimes back to the client VM; an effect answers as playing on the
        // tick it started and finished once time advances or stopfx/deletefx
        // stops it.
        let mut fx = state(world);
        let active = match fx.playing.get(&handle).copied() {
            Some(started) if now <= started => true,
            Some(_) => {
                fx.playing.remove(&handle);
                false
            }
            None => false,
        };
        Ok(Value::Int(active.into()))
    });

    // bullettracer(start, end): spawns a client bullet tracer beam.
    registry.register(Function, "bullettracer", |_, _, args| {
        vector(args, 0)?;
        vector(args, 1)?;
        // t6.csc.fx.bullet_tracer: IW4L has no client-spawned standalone
        // bullet tracer beam presentation.
        Ok(Value::Undefined)
    });

    // ent earthquake(scale, duration, source, radius): shakes the local
    // camera.
    registry.register(Method, "earthquake", |world, _, args| {
        let scale = float(args, 0)?;
        let duration = float(args, 1)?;
        let duration_ms = round_int(duration * 1000.0);
        let origin = vector(args, 2)?;
        let radius = float(args, 3)?;
        with_server(world, |server| {
            let now = crate::level_time_ms(server.resource::<crate::step::StepRequest>().tick);
            let mut quake = crate::ScriptEarthquake {
                id: 0,
                scale,
                duration_ms,
                origin,
                radius,
                start_ms: now,
            };
            if !quake.valid() {
                return;
            }
            let mut runtime = server.resource_mut::<RoundScript>();
            let engine = &mut runtime.engine;
            engine.earthquakes.retain(|q| q.active(now));
            if engine.earthquakes.len() < crate::MAX_SCRIPT_EARTHQUAKES {
                quake.id = engine.next_earthquake;
                engine.next_earthquake = engine.next_earthquake.wrapping_add(1);
                engine.earthquakes.push(quake);
            }
        });
        Ok(Value::Undefined)
    });

    // visionsetnaked(localclientnum, visionset, [transition_seconds]): sets
    // the local client's naked visionset; argument counts other than 2 or 3
    // do nothing.
    registry.register(Function, "visionsetnaked", |world, _, args| {
        let duration_ms = match args.len() {
            2 => 1000,
            3 => round_int(float(args, 2)? * 1000.0),
            _ => return Ok(Value::Undefined),
        };
        local_client(args, 0)?;
        let name = string(args, 1)?;
        if name.is_empty() {
            return Ok(Value::Undefined);
        }
        state(world).naked_vision = Some(name.clone());
        with_server(world, |server| {
            let set_ms = crate::level_time_ms(server.resource::<crate::step::StepRequest>().tick);
            let mut frame = crate::frame::FrameWorld::from_world(server);
            let client_id = crate::ClientId(LOCAL_CLIENT as u32);
            if frame.client_meta(client_id).is_some() {
                frame.client_meta_mut(client_id).view_effects.naked_vision =
                    Some(crate::VisionChange {
                        name,
                        duration_ms: duration_ms.max(0),
                        set_ms,
                        lerp: None,
                    });
            }
        });
        Ok(Value::Undefined)
    });

    // visionsetnakedlerp(localclientnum, from, to, fraction): shows the
    // naked vision blended from `to` (fraction 0) toward `from` (fraction 1),
    // the fraction clamped to 0..1, at once.
    registry.register(Function, "visionsetnakedlerp", |world, _, args| {
        local_client(args, 0)?;
        let from = string(args, 1)?;
        let to = string(args, 2)?;
        let fraction = float(args, 3)?;
        let fraction = if -fraction >= 0.0 {
            0.0
        } else if fraction - 1.0 >= 0.0 {
            1.0
        } else {
            fraction
        };
        if from.is_empty() || to.is_empty() {
            return Ok(Value::Undefined);
        }
        with_server(world, |server| {
            let set_ms = crate::level_time_ms(server.resource::<crate::step::StepRequest>().tick);
            let mut frame = crate::frame::FrameWorld::from_world(server);
            let client_id = crate::ClientId(LOCAL_CLIENT as u32);
            if frame.client_meta(client_id).is_some() {
                frame.client_meta_mut(client_id).view_effects.naked_vision =
                    Some(crate::VisionChange {
                        name: to,
                        duration_ms: 0,
                        set_ms,
                        lerp: Some(crate::VisionLerp { from, fraction }),
                    });
            }
        });
        Ok(Value::Undefined)
    });

    // getvisionsetnaked(localclientnum): the local client's current naked
    // visionset name.
    registry.register(Function, "getvisionsetnaked", |world, _, args| {
        int(args, 0)?;
        if let Some(name) = world
            .get_resource::<ClientEffects>()
            .and_then(|fx| fx.naked_vision.clone())
        {
            return Ok(Value::string(&name));
        }
        let from_server = with_server(world, |server| {
            let frame = crate::frame::FrameWorld::from_world(server);
            let client_id = crate::ClientId(LOCAL_CLIENT as u32);
            frame
                .client_meta(client_id)
                .and_then(|meta| {
                    meta.view_effects
                        .naked_vision
                        .as_ref()
                        .map(|v| v.name.clone())
                })
                .or_else(|| {
                    server
                        .resource::<RoundScript>()
                        .engine
                        .naked_vision
                        .as_ref()
                        .map(|v| v.name.clone())
                })
        })
        .flatten();
        let name = from_server.unwrap_or_else(|| {
            world
                .resource::<MatchScript>()
                .dvars
                .get("mapname")
                .cloned()
                .unwrap_or_else(String::new)
        });
        Ok(Value::string(&name))
    });

    // ent setinfraredvisionset(visionset): sets the local client's infrared
    // visionset.
    registry.register(Method, "setinfraredvisionset", |world, receiver, args| {
        receiver_entity(world, receiver)?;
        let name = string(args, 0)?;
        if !name.is_empty() {
            with_server(world, |server| {
                let set_ms =
                    crate::level_time_ms(server.resource::<crate::step::StepRequest>().tick);
                let mut frame = crate::frame::FrameWorld::from_world(server);
                let client_id = crate::ClientId(LOCAL_CLIENT as u32);
                if frame.client_meta(client_id).is_some() {
                    frame.client_meta_mut(client_id).view_effects.thermal_vision =
                        Some(crate::VisionChange {
                            name,
                            duration_ms: 0,
                            set_ms,
                            lerp: None,
                        });
                }
            });
        }
        Ok(Value::Undefined)
    });

    // setworldfogactivebank(localclientnum, bank): sets the active world fog
    // bank mask.
    registry.register(Function, "setworldfogactivebank", |world, _, args| {
        local_client(args, 0)?;
        let bank = (int(args, 1)? & 65535) as u16;
        // t6.csc.fog.world_bank: `getworldfogscriptid` tests the mask; the
        // renderer draws no world fog volumes by bank.
        state(world).world_fog_bank = bank;
        Ok(Value::Undefined)
    });

    // getworldfogscriptid(localclientnum): the script id of the world fog
    // volume containing the camera, or -1 when in none.
    registry.register(Function, "getworldfogscriptid", |world, _, args| {
        local_client(args, 0)?;
        let camera = with_server(world, |server| {
            crate::script_player::eye(
                &crate::frame::FrameWorld::from_world(server),
                crate::ClientId(LOCAL_CLIENT as u32),
            )
        })
        .flatten()
        .unwrap_or([0.0; 3]);
        let bank = u32::from(state(world).world_fog_bank);
        let id = ensure_fog_volumes(world)
            .0
            .iter()
            .find(|volume| {
                volume.packed & bank != 0
                    && volume.planes.iter().all(|plane| {
                        plane[0] * camera[0]
                            + plane[1] * camera[1]
                            + plane[2] * camera[2]
                            + plane[3]
                            <= 0.0
                    })
            })
            .map_or(-1, |volume| i32::from(volume.script_id));
        Ok(Value::Int(id))
    });

    // setlutscriptindex(localclientnum, index): sets the color grading LUT
    // script index.
    registry.register(Function, "setlutscriptindex", |_, _, args| {
        local_client(args, 0)?;
        int(args, 1)?;
        // t6.csc.vision.lut: IW4L's post-processing has no color LUT bank
        // switcher.
        Ok(Value::Undefined)
    });

    // setblurbylocalclientnum(localclientnum, blur, time): blurs the local
    // client's view over `time` seconds.
    registry.register(Function, "setblurbylocalclientnum", |world, _, args| {
        local_client(args, 0)?;
        let blur = float(args, 1)?;
        let seconds = float(args, 2)?;
        if !(seconds >= 0.0) {
            return Err("Time must be positive".into());
        }
        let duration_ms = (seconds * 1000.0) as i32;
        if !(blur >= 0.0) {
            return Err("Blur value must be greater than 0".into());
        }
        with_server(world, |server| {
            let now = crate::level_time_ms(server.resource::<crate::step::StepRequest>().tick);
            let mut frame = crate::frame::FrameWorld::from_world(server);
            let client_id = crate::ClientId(LOCAL_CLIENT as u32);
            if frame.client_meta(client_id).is_some() {
                let view = &mut frame.client_meta_mut(client_id).view_effects;
                view.blur = Some(crate::ScriptBlur {
                    from: view.blur.map_or(0.0, |b| b.sample(now)),
                    to: blur,
                    set_ms: now,
                    duration_ms,
                });
            }
        });
        Ok(Value::Undefined)
    });

    // setburn(localclientnum, seconds): starts the first-person burn overlay.
    registry.register(Function, "setburn", |_, _, args| {
        local_client(args, 0)?;
        let seconds = float(args, 1)?;
        if !(seconds >= 0.0) {
            return Err("Time must be positive".into());
        }
        // t6.csc.vision.burn: IW4L has no first-person screen burn overlay.
        Ok(Value::Undefined)
    });

    // setelectrified(localclientnum, seconds): starts the first-person
    // electrified overlay.
    registry.register(Function, "setelectrified", |_, _, args| {
        local_client(args, 0)?;
        let seconds = float(args, 1)?;
        if !(seconds >= 0.0) {
            return Err("Time must be positive".into());
        }
        // t6.csc.vision.electrified: IW4L has no first-person screen
        // electrified overlay.
        Ok(Value::Undefined)
    });

    // ent map_material(index, name): registers a filter material in slot
    // 0..=63.
    registry.register(Method, "map_material", |world, receiver, args| {
        receiver_entity(world, receiver)?;
        let index = int(args, 0)?;
        let name = string(args, 1)?;
        if !(0..FILTER_MATERIAL_SLOTS as i32).contains(&index) {
            return Err(
                "USAGE: ent map_material( <material index 0-63 >, <material name>)\n".into(),
            );
        }
        // t6.csc.filter.pass: IW4L's post-processing does not run client
        // script filter passes.
        state(world).mapped_materials[index as usize] = Some(name);
        Ok(Value::Undefined)
    });

    // ent set_filter_pass_material(filter, pass, material): assigns a mapped
    // material to a filter pass.
    registry.register(
        Method,
        "set_filter_pass_material",
        |world, receiver, args| {
            receiver_entity(world, receiver)?;
            let filter = int(args, 0)?;
            let pass = int(args, 1)?;
            let material = int(args, 2)?;
            if !(0..FILTER_SLOTS).contains(&filter)
                || !(0..FILTER_PASSES).contains(&pass)
                || !(0..FILTER_MATERIAL_SLOTS as i32).contains(&material)
            {
                return Err(
                    "USAGE: ent set_filter_pass_material( <filterindex 0-3>, <passindex 0-15>, <materialindex 0-63>, <targetindex 0-4>, <sampler0index 0-4>, <sampler0index 0-4> )\n"
                        .into(),
                );
            }
            if state(world).mapped_materials[material as usize].is_none() {
                return Err(
                    "USAGE: ent set_filter_pass_material( <filterindex 0-3>, <passindex 0-15>, <materialindex 0-63>, <targetindex 0-4>, <sampler0index 0-4>, <sampler0index 0-4> ) - no material at that index\n"
                        .into(),
                );
            }
            Ok(Value::Undefined)
        },
    );

    // ent set_filter_pass_enabled(filter, pass, enabled): enables or disables
    // a filter pass.
    registry.register(
        Method,
        "set_filter_pass_enabled",
        |world, receiver, args| {
            receiver_entity(world, receiver)?;
            let filter = int(args, 0)?;
            let pass = int(args, 1)?;
            let _enabled = int(args, 2)?;
            if !(0..FILTER_SLOTS).contains(&filter) || !(0..FILTER_PASSES).contains(&pass) {
                return Err(
                    "USAGE: ent set_filter_pass_enabled( <filterindex 0-3>, <passindex 0-16>, <enabled true/false> )\n"
                        .into(),
                );
            }
            Ok(Value::Undefined)
        },
    );

    // ent set_filter_pass_constant(filter, pass, constant, value): sets a
    // filter pass constant.
    registry.register(
        Method,
        "set_filter_pass_constant",
        |world, receiver, args| {
            receiver_entity(world, receiver)?;
            let filter = int(args, 0)?;
            let pass = int(args, 1)?;
            let constant = int(args, 2)?;
            let _value = float(args, 3)?;
            if !(0..FILTER_SLOTS).contains(&filter)
                || !(0..FILTER_PASSES).contains(&pass)
                || !(0..FILTER_CONSTANTS).contains(&constant)
            {
                return Err(
                    "USAGE: ent set_filter_pass_constant( <filterindex 0-3>, <passindex 0-16>, <constantindex 0-16>, <value> )\n"
                        .into(),
                );
            }
            Ok(Value::Undefined)
        },
    );

    // ent mapshaderconstant(localclientnum, index, name, [x], [y], [z], [w]):
    // binds a shader constant slot on the entity to a code constant name;
    // answers 1 on success, 0 on an out-of-range slot or unknown name.
    registry.register(Method, "mapshaderconstant", |world, receiver, args| {
        receiver_entity(world, receiver)?;
        local_client(args, 0)?;
        let index = int(args, 1)?;
        let name = string(args, 2)?;
        for at in 3..args.len().min(7) {
            float(args, at)?;
        }
        // t6.csc.shader.constants: IW4L's renderer does not read per-entity
        // script shader constant vectors.
        let valid = (0..=SHADER_CONSTANT_SLOTS).contains(&index)
            && SHADER_CONSTANTS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(&name));
        Ok(Value::Int(valid.into()))
    });

    // ent setshaderconstant(localclientnum, index, x, y, z, w): updates a
    // mapped shader constant vector on the entity.
    registry.register(Method, "setshaderconstant", |world, receiver, args| {
        receiver_entity(world, receiver)?;
        local_client(args, 0)?;
        int(args, 1)?;
        float(args, 2)?;
        float(args, 3)?;
        float(args, 4)?;
        float(args, 5)?;
        Ok(Value::Undefined)
    });

    // ent setmaterialoverride(surface) / ent clearmaterialoverride(): sets or
    // clears the entity's footstep surface type override.
    registry.register(Method, "setmaterialoverride", |_, _, args| {
        string(args, 0)?;
        // t6.csc.sound.surface_override: IW4L's client game plays no footstep
        // sounds for an entity surface material override.
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearmaterialoverride", |_, _, _| {
        Ok(Value::Undefined)
    });

    // ent setalphafadeforname(alpha): sets the overhead name fade alpha for a
    // player entity (`0..18`).
    registry.register(Method, "setalphafadeforname", |world, receiver, args| {
        let (_, number, _) = super::entity::receiver_entity(world, receiver)?;
        let alpha = float(args, 0)?;
        if (0..18).contains(&number) {
            world
                .get_resource_or_insert_with(super::game::ClientGame::default)
                .name_fade_alpha
                .insert(number, alpha);
        }
        // t6.csc.ui.name_fade: overhead player names are not yet painted with
        // the per-player fade alpha.
        Ok(Value::Undefined)
    });

    // animateui(localclientnum, menu, item, state, duration): animates a legacy
    // UI menu item to a state when `menu` is in `uiContext` or `cgDC`.
    registry.register(Function, "animateui", |_, _, args| {
        int(args, 0)?;
        string(args, 1)?;
        string(args, 2)?;
        string(args, 3)?;
        int(args, 4)?;
        // t6.csc.ui.animate: no Zombies HUD menu in `patch_zm` or the map
        // zones carries `itemDef_s::animInfo` (or the MP overlay menus
        // `_explode.csc`, `_sticky_grenade.csc` and `_players.csc` name);
        // front-end legacy menu item state animations are not run.
        Ok(Value::Undefined)
    });

    // loadgump(name, slot): streams a gump sub-zone into a slot when `name`
    // is non-empty.
    registry.register(Function, "loadgump", |_, _, args| {
        let name = string(args, 0)?;
        if !name.is_empty() {
            int(args, 1)?;
        }
        // t6.csc.gump.stream: IW4L loads the whole map at start rather than
        // streaming gump sub-zones.
        Ok(Value::Undefined)
    });

    // sethidegumpalpha(localclientnum, [color]): sets the gump transition
    // fade color and alpha.
    registry.register(Function, "sethidegumpalpha", |_, _, args| {
        local_client(args, 0)?;
        if args.len() > 1 {
            vector(args, 1)?;
        }
        // t6.csc.gump.hide_alpha: IW4L has no gump streaming transition fade
        // overlay.
        Ok(Value::Undefined)
    });

    // ent makelight(primary_light_index): turns the client entity into a
    // dynamic primary light initialized from the map's primary light.
    registry.register(Method, "makelight", |world, receiver, args| {
        let id = receiver_entity(world, receiver)?;
        int(args, 0)?;
        world
            .resource_mut::<RoundScript>()
            .set_object_field(id, "type", Value::string("NA"));
        // t6.csc.light.primary: IW4L's sim world does not carry ComWorld
        // primary light records, so makelight initializes default light
        // parameters.
        // t6.csc.light.render: IW4L's renderer does not draw client script
        // dynamic light overrides.
        state(world).lights.insert(id, LightState::default());
        Ok(Value::Undefined)
    });

    // ent makelightslave(primary_light_index): links a split-screen slave
    // light entity to a primary light's mixer.
    registry.register(Method, "makelightslave", |world, receiver, args| {
        receiver_entity(world, receiver)?;
        int(args, 0)?;
        Ok(Value::Undefined)
    });

    // ent getlightcolor(): the light entity's unpacked RGB color vector.
    registry.register(Method, "getlightcolor", |world, receiver, _| {
        let id = light_entity(world, receiver, true)?;
        let bytes = state(world).lights[&id].color_bytes;
        let scale = 0.003_921_569;
        Ok(Value::Vector([
            f32::from(bytes[0]) * scale,
            f32::from(bytes[1]) * scale,
            f32::from(bytes[2]) * scale,
        ]))
    });

    // ent setlightcolor(rgb): packs and sets the light entity's RGB color.
    registry.register(Method, "setlightcolor", |world, receiver, args| {
        let id = light_entity(world, receiver, false)?;
        let rgb = vector(args, 0)?;
        let packed = rgb.map(pack_color_byte);
        if let Some(light) = state(world).lights.get_mut(&id) {
            light.color_bytes = packed;
        }
        Ok(Value::Undefined)
    });

    // ent getlightintensity(): the light entity's intensity.
    registry.register(Method, "getlightintensity", |world, receiver, _| {
        let id = light_entity(world, receiver, false)?;
        Ok(Value::Float(state(world).lights[&id].intensity))
    });

    // ent setlightintensity(intensity): sets the light entity's intensity.
    registry.register(Method, "setlightintensity", |world, receiver, args| {
        let id = light_entity(world, receiver, false)?;
        let intensity = float(args, 0)?;
        if !(intensity >= -0.001_000_000_05) {
            return Err("intensity must be >= 0".into());
        }
        let clamped = if intensity >= 0.0 { intensity } else { 0.0 };
        if let Some(light) = state(world).lights.get_mut(&id) {
            light.intensity = clamped;
        }
        Ok(Value::Undefined)
    });

    // ent getlightradius(): the light entity's radius.
    registry.register(Method, "getlightradius", |world, receiver, _| {
        let id = light_entity(world, receiver, false)?;
        Ok(Value::Float(state(world).lights[&id].radius))
    });

    // ent setlightradius(radius): sets the light entity's radius within the
    // compiled BSP radius.
    registry.register(Method, "setlightradius", |world, receiver, args| {
        let id = light_entity(world, receiver, false)?;
        let radius = float(args, 0)?;
        if !(radius >= -0.001_000_000_05) {
            return Err("radius must be >= 0".into());
        }
        let bsp_radius = state(world).lights[&id].bsp_radius;
        if !(radius <= bsp_radius + 0.001_000_000_05) {
            return Err(format!(
                "radius must be less than the bsp radius ({bsp_radius})"
            ));
        }
        let clamped = if -radius >= 0.0 {
            0.0
        } else {
            radius.min(bsp_radius)
        };
        if let Some(light) = state(world).lights.get_mut(&id) {
            light.radius = clamped;
        }
        Ok(Value::Undefined)
    });

    // ent getlightfovinner() / ent getlightfovouter(): the light entity's
    // inner or outer cone angle in degrees.
    registry.register(Method, "getlightfovinner", |world, receiver, _| {
        let id = light_entity(world, receiver, false)?;
        let cos = state(world).lights[&id].cos_half_fov_inner;
        Ok(Value::Float(cos.acos() * 2.0 * 57.295_776))
    });
    registry.register(Method, "getlightfovouter", |world, receiver, _| {
        let id = light_entity(world, receiver, false)?;
        let cos = state(world).lights[&id].cos_half_fov_outer;
        Ok(Value::Float(cos.acos() * 2.0 * 57.295_776))
    });

    // ent setlightfovrange(outer, [inner]): sets the light entity's outer and
    // inner cone angles in degrees.
    registry.register(Method, "setlightfovrange", |world, receiver, args| {
        let id = light_entity(world, receiver, false)?;
        let outer = float(args, 0)?;
        if !(0.999_000_01..136.001_01).contains(&outer) {
            return Err("outer fov must be in the range of 1 to 136".into());
        }
        let cos_outer = (outer * 0.017_453_292 * 0.5).cos();
        let (bsp_cos_outer, bsp_cos_inner) = {
            let light = &state(world).lights[&id];
            (light.bsp_cos_half_fov_outer, light.bsp_cos_half_fov_inner)
        };
        if cos_outer < bsp_cos_outer - 0.001_000_000_05 {
            return Err("outer fov cannot be larger than the fov when the map was compiled".into());
        }
        let cos_outer = cos_outer.clamp(bsp_cos_outer, 1.0);
        let cos_inner = if args.len() == 2 {
            let inner = float(args, 1)?;
            if !(inner >= -0.001_000_000_05 && inner < outer + 0.001_000_000_05) {
                return Err("inner fov must be in the range of 0 to outer fov".into());
            }
            (inner * 0.017_453_292 * 0.5)
                .cos()
                .clamp(cos_outer + 0.001_000_000_05, 1.0)
        } else {
            bsp_cos_inner.max(cos_outer + 0.001_000_000_05)
        };
        if let Some(light) = state(world).lights.get_mut(&id) {
            light.cos_half_fov_outer = cos_outer;
            light.cos_half_fov_inner = cos_inner;
        }
        Ok(Value::Undefined)
    });

    // ent getlightexponent(): the light entity's falloff exponent.
    registry.register(Method, "getlightexponent", |world, receiver, _| {
        let id = light_entity(world, receiver, false)?;
        Ok(Value::Int(i32::from(state(world).lights[&id].exponent)))
    });

    // ent setlightexponent(exponent): sets the light entity's falloff
    // exponent in 0..=100.
    registry.register(Method, "setlightexponent", |world, receiver, args| {
        let id = light_entity(world, receiver, false)?;
        let exponent = int(args, 0)?;
        if !(0..=100).contains(&exponent) {
            return Err("exponent must be in the range from 0 to 100".into());
        }
        if let Some(light) = state(world).lights.get_mut(&id) {
            light.exponent = exponent as u8;
        }
        Ok(Value::Undefined)
    });

    // allocatemixerlights(count): allocates the client's light mixer table.
    registry.register(Function, "allocatemixerlights", |world, _, args| {
        let count = int(args, 0)?;
        // t6.csc.light.mixer: IW4L's renderer does not evaluate or draw
        // client light mixers.
        let mut fx = state(world);
        fx.allocated_mixers = count.max(0) as usize;
        fx.mixer_count = 0;
        Ok(Value::Undefined)
    });

    // ismixerlightbehaviorhardcoded(name): whether `name` is one of the
    // engine's 12 native light behaviors.
    registry.register(Function, "ismixerlightbehaviorhardcoded", |_, _, args| {
        let name = string(args, 0)?;
        Ok(Value::Int(
            HARDCODED_LIGHT_BEHAVIORS.contains(&name.as_str()).into(),
        ))
    });

    // ent makemixerlight(event): attaches a new mixer to the light entity.
    registry.register(Method, "makemixerlight", |world, receiver, args| {
        let id = mixer_light_entity(world, receiver)?;
        string(args, 0)?;
        let mut fx = state(world);
        fx.mixer_count += 1;
        let mixer = fx.mixer_count;
        if let Some(light) = fx.lights.get_mut(&id) {
            light.mixer = Some(mixer);
        }
        Ok(Value::Undefined)
    });

    // ent activatemixer(): activates the light entity's mixer.
    registry.register(Method, "activatemixer", |world, receiver, _| {
        mixer_entity(world, receiver)?;
        Ok(Value::Undefined)
    });

    // ent setmixerbehavior(name): sets the mixer's behavior.
    registry.register(Method, "setmixerbehavior", |world, receiver, args| {
        mixer_entity(world, receiver)?;
        string(args, 0)?;
        Ok(Value::Undefined)
    });

    // ent setmixerexploderid(id): associates a client exploder id with the
    // mixer.
    registry.register(Method, "setmixerexploderid", |world, receiver, args| {
        mixer_entity(world, receiver)?;
        int(args, 0)?;
        Ok(Value::Undefined)
    });

    // ent setmixerl2rrampspeed(speed) / ent setmixerr2lrampspeed(speed): sets
    // the mixer's left-to-right or right-to-left ramp speed.
    for name in ["setmixerl2rrampspeed", "setmixerr2lrampspeed"] {
        registry.register(Method, name, |world, receiver, args| {
            mixer_entity(world, receiver)?;
            float(args, 0)?;
            Ok(Value::Undefined)
        });
    }

    // ent setmixerlightrotatetime(seconds): sets the mixer's yaw rotation
    // period.
    registry.register(
        Method,
        "setmixerlightrotatetime",
        |world, receiver, args| {
            mixer_entity(world, receiver)?;
            float(args, 0)?;
            Ok(Value::Undefined)
        },
    );

    // ent setmixerlightbehavior(side, name): sets a mixer side's hardcoded
    // light behavior.
    registry.register(Method, "setmixerlightbehavior", |world, receiver, args| {
        mixer_entity(world, receiver)?;
        mixer_side(args, 0)?;
        let behavior = string(args, 1)?;
        if !HARDCODED_LIGHT_BEHAVIORS.contains(&behavior.as_str()) {
            return Err(format!("Light behavior '{behavior}' unknown."));
        }
        Ok(Value::Undefined)
    });

    // ent setmixerlightcolor(side, rgb): sets a mixer side's color vector.
    registry.register(Method, "setmixerlightcolor", |world, receiver, args| {
        mixer_entity(world, receiver)?;
        mixer_side(args, 0)?;
        vector(args, 1)?;
        Ok(Value::Undefined)
    });

    // ent setmixerlightexponent(side, exponent): sets a mixer side's
    // exponent, clamped to 0..=100.
    registry.register(Method, "setmixerlightexponent", |world, receiver, args| {
        mixer_entity(world, receiver)?;
        mixer_side(args, 0)?;
        int(args, 1)?;
        Ok(Value::Undefined)
    });

    // ent setmixerlightfovrange(side, outer, [inner]): sets a mixer side's
    // outer and inner cone angles.
    registry.register(Method, "setmixerlightfovrange", |world, receiver, args| {
        mixer_entity(world, receiver)?;
        mixer_side(args, 0)?;
        let outer = float(args, 1)?;
        if args.len() == 3 {
            let inner = float(args, 2)?;
            if !(inner >= -0.001_000_000_05 && inner < outer + 0.001_000_000_05) {
                return Err("inner fov must be in the range of 0 to outer fov".into());
            }
        }
        Ok(Value::Undefined)
    });

    // ent setmixerlightintensity(side, intensity) /
    // ent setmixerlightradius(side, radius).
    for name in ["setmixerlightintensity", "setmixerlightradius"] {
        registry.register(Method, name, |world, receiver, args| {
            mixer_entity(world, receiver)?;
            mixer_side(args, 0)?;
            float(args, 1)?;
            Ok(Value::Undefined)
        });
    }

    // ent setmixerlightparam(side, index, value): sets a parameter (0..=16)
    // on a mixer side's light behavior.
    registry.register(Method, "setmixerlightparam", |world, receiver, args| {
        mixer_entity(world, receiver)?;
        mixer_side(args, 0)?;
        let index = int(args, 1)?;
        if !(0..=MIXER_LIGHT_PARAM_MAX).contains(&index) {
            return Err(format!(
                "Please ensure light logic param number specified is in range 0 - {MIXER_LIGHT_PARAM_MAX}\n"
            ));
        }
        check_mixer_param_value(
            args,
            2,
            "Light logic parameter specified is of an invalid type.",
        )?;
        Ok(Value::Undefined)
    });

    // ent setmixerparam(index, value): sets a parameter (0..=4) on the
    // mixer's behavior.
    registry.register(Method, "setmixerparam", |world, receiver, args| {
        mixer_entity(world, receiver)?;
        let index = int(args, 0)?;
        if !(0..=MIXER_PARAM_MAX).contains(&index) {
            return Err(format!(
                "Please ensure light logic param number specified is in range 0 - {MIXER_PARAM_MAX}\n"
            ));
        }
        check_mixer_param_value(
            args,
            1,
            "Mixer logic parameter specified is of an invalid type.",
        )?;
        Ok(Value::Undefined)
    });

    // ent setmixerlightmodels(model_ent, on_model, off_model): associates on
    // and off models with the mixer.
    registry.register(Method, "setmixerlightmodels", |world, receiver, args| {
        mixer_entity(world, receiver)?;
        entity_arg(world, args, 0)?;
        string(args, 1)?;
        string(args, 2)?;
        // t6.csc.light.mixer_models: IW4L's server only tracks GSC
        // precachemodel names, not Radiant map models in CS_MODELS.
        Ok(Value::Undefined)
    });

    // ent setmixerspinmodels(model_ent, on_model, off_model, [fx_id, tag]):
    // associates spinning on and off models (and optional effect) with the
    // mixer.
    registry.register(Method, "setmixerspinmodels", |world, receiver, args| {
        mixer_entity(world, receiver)?;
        entity_arg(world, args, 0)?;
        string(args, 1)?;
        string(args, 2)?;
        if !matches!(args.get(3), None | Some(Value::Undefined)) {
            int(args, 3)?;
            string(args, 4)?;
        }
        Ok(Value::Undefined)
    });
}
