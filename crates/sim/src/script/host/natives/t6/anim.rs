//! Black Ops 2's actor animation: the animation state definition (`.asd`)
//! an actor's aitype names, the state and substate scripts pick from it,
//! the notetracks its clips send under the state's notify, scripted
//! animations, and the animation and orientation modes.

use super::super::super::arrays::new_array;
use super::super::super::entities::EntityKind;
use super::{int, string, vector};
use crate::frame::FrameWorld;
use crate::script::runtime::{notify_now, raise, run_now};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;
use std::sync::Arc;
use xmodel_runtime::{AnimClip, AnimStateTable};

/// The modes `animmode` takes.
const ANIM_MODES: [&str; 11] = [
    "none",
    "normal",
    "pos deltas",
    "angle deltas",
    "gravity",
    "noclip",
    "nogravity",
    "zonly_physics",
    "nophysics",
    "point relative",
    "slide",
];
/// The modes `orientmode` takes.
const ORIENT_MODES: [&str; 9] = [
    "face angle",
    "face current",
    "face direction",
    "face enemy",
    "face enemy or motion",
    "face goal",
    "face motion",
    "face point",
    "face default",
];
const SCRIPTED: &str = "maps/mp/animscripts/zm_scripted";

/// A clip an actor plays from its state definition.
#[derive(Clone, Debug)]
struct Playing {
    state: String,
    substate: usize,
    clip: Arc<AnimClip>,
    /// The notify its notetracks go out under, when the state names one.
    notify: Option<String>,
    /// How far the clip has played, 0 to 1, and how far its notetracks went.
    time: f32,
    sent: f32,
    done: bool,
    /// A scripted animation plays from this origin and angles.
    anchor: Option<([f32; 3], [f32; 3])>,
}

/// An actor's animation state beside its entity.
#[derive(Clone, Debug)]
pub(super) struct ActorAnim {
    table: Option<(String, Arc<AnimStateTable>)>,
    playing: Option<Playing>,
    pub(super) mode: &'static str,
    pub(super) orient: &'static str,
    /// The yaw `face angle`, `face direction` or `face point` turns to.
    pub(super) orient_yaw: Option<f32>,
    /// How far the clip's root moved this frame, in the actor's frame, when
    /// it is not a scripted clip.
    pub(super) step: [f32; 3],
    /// The aim weights `setaimanimweights` sets; no aim clip blends by them
    /// yet (`t6.actors.anim_playback`).
    aim_weights: [f32; 2],
}

impl Default for ActorAnim {
    fn default() -> Self {
        Self {
            table: None,
            playing: None,
            mode: "normal",
            orient: "face default",
            orient_yaw: None,
            step: [0.0; 3],
            aim_weights: [0.0; 2],
        }
    }
}

#[derive(Resource, Default)]
pub(super) struct ActorAnims(pub(super) BTreeMap<u64, ActorAnim>);

fn actor(world: &World, receiver: &Value) -> Result<u64, String> {
    match world.resource::<RoundScript>().entity(receiver) {
        Some((id, entity)) if entity.kind == EntityKind::Actor => Ok(id),
        _ => Err("not an actor".into()),
    }
}

fn anim_mut(world: &mut World, id: u64) -> &mut ActorAnim {
    world
        .get_resource_or_insert_with(ActorAnims::default)
        .into_inner()
        .0
        .entry(id)
        .or_default()
}

/// The state definition the actor's aitype named (`animstatedef`).
fn table(world: &mut World, id: u64) -> Result<(String, Arc<AnimStateTable>), String> {
    if let Some(table) = anim_mut(world, id).table.clone() {
        return Ok(table);
    }
    let name = match world
        .resource_mut::<RoundScript>()
        .object_field(id, "animstatedef")
    {
        Value::String(name) => name.to_string(),
        _ => return Err("actor has no AnimStateDef".into()),
    };
    let name = name
        .strip_suffix(".asd")
        .unwrap_or(&name)
        .to_ascii_lowercase();
    let table = FrameWorld::from_world(world)
        .anim_state_table(&name)
        .ok_or_else(|| format!("AnimStateDef '{name}' is in no zone"))?;
    anim_mut(world, id).table = Some((name.clone(), Arc::clone(&table)));
    Ok((name, table))
}

/// The engine's random step: the next of `seed` and an index below `count`.
fn random_below(seed: &mut u32, count: usize) -> usize {
    *seed = seed.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
    (((*seed >> 17) as i64 * count as i64) >> 15) as usize
}

/// The seed a substate is picked with: the actor's number and the time.
fn seed(world: &mut World, id: u64) -> u32 {
    let number = world.resource::<RoundScript>().entities[&id].number as u32;
    let now = super::super::super::players::now_ms(world) as u32;
    number.wrapping_add(now).wrapping_mul(number)
}

enum Substate {
    Any,
    Index(i32),
    Alias(String),
}

/// A substate of `state`, as the engine picks it, with its error text.
fn pick(
    table: &(String, Arc<AnimStateTable>),
    state: &str,
    substate: Substate,
    seed: &mut u32,
    builtin: &str,
) -> Result<usize, String> {
    let (name, table) = table;
    let entries = table
        .states
        .get(state)
        .ok_or_else(|| format!("{builtin}: state '{state}' not found in AnimStateDef '{name}'"))?;
    match substate {
        Substate::Alias(alias) => {
            let matching: Vec<usize> = entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.alias.as_deref() == Some(alias.as_str()))
                .map(|(index, _)| index)
                .collect();
            if matching.is_empty() {
                return Err(format!(
                    "{builtin}: subStateAlias '{alias}' not found in state '{state}' in AnimStateDef '{name}'"
                ));
            }
            Ok(matching[random_below(seed, matching.len())])
        }
        Substate::Index(index) => usize::try_from(index)
            .ok()
            .filter(|&index| index < entries.len())
            .ok_or_else(|| {
                format!(
                    "{builtin}: subStateIndex '{index}' not found in state '{state}' in AnimStateDef '{name}'"
                )
            }),
        Substate::Any => Ok(random_below(seed, entries.len())),
    }
}

fn state_name(args: &[Value], index: usize) -> Result<String, String> {
    Ok(string(args, index)?.to_ascii_lowercase())
}

/// The substate argument at `index`: none, an int, or a string alias.
fn substate_arg(args: &[Value], index: usize, builtin: &str) -> Result<Substate, String> {
    Ok(match args.get(index) {
        None | Some(Value::Undefined) => Substate::Any,
        Some(Value::Int(n)) => Substate::Index(*n),
        Some(Value::String(alias)) => Substate::Alias(alias.to_string().to_ascii_lowercase()),
        Some(_) => {
            return Err(format!(
                "{builtin}: substate must be either int, string, or unspecified (to select at random)"
            ));
        }
    })
}

fn clip_of(
    world: &mut World,
    table: &AnimStateTable,
    state: &str,
    substate: usize,
) -> Result<Arc<AnimClip>, String> {
    let name = table
        .states
        .get(state)
        .and_then(|entries| entries.get(substate))
        .map(|entry| entry.clip.clone())
        .ok_or("no such substate")?;
    FrameWorld::from_world(world)
        .anim_clip_named(&name)
        .ok_or_else(|| format!("animation '{name}' is not loaded in the simulation"))
}

/// The state and substate playing, or the named one (`state`, substate).
fn chosen(
    world: &mut World,
    id: u64,
    args: &[Value],
    builtin: &str,
) -> Result<(String, usize), String> {
    if args.is_empty() {
        let playing = anim_mut(world, id).playing.as_ref();
        return Ok(playing.map_or((String::new(), 0), |p| (p.state.clone(), p.substate)));
    }
    if args.len() != 2 {
        return Err(format!("{builtin}: improper param count, must be 0 or 2"));
    }
    let state = state_name(args, 0)?;
    let Value::Int(substate) = args[1] else {
        return Err(format!("{builtin}: substate must be int"));
    };
    let table = table(world, id)?;
    let substate = pick(&table, &state, Substate::Index(substate), &mut 0, builtin)?;
    Ok((state, substate))
}

/// Starts a clip of the actor's state; the client is told to play it.
fn play(
    world: &mut World,
    id: u64,
    state: &str,
    substate: usize,
    anchor: Option<([f32; 3], [f32; 3])>,
) -> Result<(), String> {
    let (_, table) = table(world, id)?;
    let clip = clip_of(world, &table, state, substate)?;
    let notify = table.properties.get(state).and_then(|properties| {
        properties
            .iter()
            .position(|word| word == "notify")
            .and_then(|at| properties.get(at + 1))
            .cloned()
    });
    let name: Arc<str> = clip.name.as_str().into();
    diag::debug!(
        Sim,
        "actor {id}: plays {state}/{substate} {name} for {:.2}s, notify {notify:?}, notes {:?}",
        clip.duration(),
        clip.notifies
            .iter()
            .map(|n| (n.name.as_str(), n.time))
            .collect::<Vec<_>>()
    );
    anim_mut(world, id).playing = Some(Playing {
        state: state.to_owned(),
        substate,
        clip,
        notify,
        time: 0.0,
        sent: -1.0,
        done: false,
        anchor,
    });
    if let Some(entity) = world.resource_mut::<RoundScript>().entities.get_mut(&id) {
        entity.anim_op = Some(Some(name));
    }
    if anchor.is_some() {
        place(world, id);
    }
    Ok(())
}

/// A scripted clip holds the actor at its anchor plus the clip's root motion.
fn place(world: &mut World, id: u64) {
    let Some(playing) = anim_mut(world, id).playing.clone() else {
        return;
    };
    let Some((origin, angles)) = playing.anchor else {
        return;
    };
    let delta = playing.clip.abs_delta_trans(playing.time);
    let (sin, cos) = angles[1].to_radians().sin_cos();
    let at = [
        origin[0] + delta[0] * cos - delta[1] * sin,
        origin[1] + delta[0] * sin + delta[1] * cos,
        origin[2] + delta[2],
    ];
    let yaw = angles[1] + playing.clip.abs_delta_yaw(playing.time);
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.set_object_field(id, "origin", Value::Vector(at));
    runtime.set_object_field(id, "angles", Value::Vector([angles[0], yaw, angles[2]]));
}

/// Plays every actor's clip one server frame on: its notetracks go out
/// under its state's notify, a looping clip starts over.
fn advance(world: &mut World) {
    let Some(anims) = world.get_resource::<ActorAnims>() else {
        return;
    };
    let ids: Vec<u64> = anims.0.keys().copied().collect();
    let step = crate::MATCH_TICK_MS as f32 / 1000.0;
    for id in ids {
        if !world.resource::<RoundScript>().entities.contains_key(&id) {
            world.resource_mut::<ActorAnims>().0.remove(&id);
            continue;
        }
        anim_mut(world, id).step = [0.0; 3];
        let Some(mut playing) = anim_mut(world, id).playing.clone() else {
            continue;
        };
        if playing.done {
            continue;
        }
        let from = playing.time;
        let mut cycles = 0.0;
        let length = playing.clip.duration();
        let mut time = if length > 0.0 {
            playing.time + step / length
        } else {
            1.0
        };
        let mut notes = Vec::new();
        loop {
            let reach = time.min(1.0);
            notes.extend(
                playing
                    .clip
                    .notifies
                    .iter()
                    .filter(|note| note.time > playing.sent && note.time <= reach)
                    .map(|note| note.name.clone()),
            );
            playing.sent = reach;
            if time < 1.0 {
                break;
            }
            if !playing.clip.looping {
                time = 1.0;
                playing.done = true;
                break;
            }
            time -= 1.0;
            cycles += 1.0;
            playing.sent = -1.0;
        }
        playing.time = time;
        let notify = playing.notify.clone();
        if playing.anchor.is_none() {
            let (start, end, full) = (
                playing.clip.abs_delta_trans(from),
                playing.clip.abs_delta_trans(time),
                playing.clip.abs_delta_trans(1.0),
            );
            anim_mut(world, id).step =
                std::array::from_fn(|i| end[i] - start[i] + cycles * full[i]);
        }
        anim_mut(world, id).playing = Some(playing);
        place(world, id);
        if let Some(notify) = notify {
            for note in notes {
                raise(
                    world,
                    Value::Object(id),
                    &notify,
                    vec![Value::string(&note)],
                );
            }
        }
    }
}

const SCRIPTED_BUILTIN: &str = "ScrCmd_animscriptedInternal";

/// The arguments both scripted animation builtins take, checked as the
/// engine checks them: origin, angles, state, substate and mode.
struct ScriptedArgs {
    origin: [f32; 3],
    angles: [f32; 3],
    state: String,
    substate: Substate,
    mode: Option<String>,
}

fn scripted_args(world: &mut World, id: u64, args: &[Value]) -> Result<ScriptedArgs, String> {
    let origin = vector(args, 0)?;
    let angles = vector(args, 1)?;
    let state = state_name(args, 2)?;
    let (name, table) = table(world, id)?;
    if !table.states.contains_key(&state) {
        return Err(format!(
            "{SCRIPTED_BUILTIN}: state '{state}' not found in AnimStateDef '{name}'"
        ));
    }
    let substate = substate_arg(args, 3, SCRIPTED_BUILTIN)?;
    let mode = match args.get(4) {
        None | Some(Value::Undefined) => None,
        Some(_) => {
            let mode = mode_arg(args, 4)?;
            if mode != "normal" && mode != "deathplant" {
                return Err(format!(
                    "Illegal mode {mode} for animScripted. Valid modes are normal and deathplant"
                ));
            }
            Some(mode)
        }
    };
    Ok(ScriptedArgs {
        origin,
        angles,
        state,
        substate,
        mode,
    })
}

/// `animscripted`: the engine leaves the actor's animscript for the scripted
/// one, whose init is given the arguments and whose main plays the clip.
fn animscripted(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = actor(world, receiver)?;
    let scripted = scripted_args(world, id, args)?;
    let alive = matches!(
        world.resource_mut::<RoundScript>().object_field(id, "health"),
        Value::Int(n) if n > 0
    );
    if !alive {
        let mut runtime = world.resource_mut::<RoundScript>();
        let number = runtime.entities[&id].number;
        let classname = runtime.entities[&id].classname.clone();
        let text = |runtime: &mut RoundScript, field| match runtime.object_field(id, field) {
            Value::String(text) => text.to_string(),
            _ => "<undefined>".to_owned(),
        };
        let team = text(&mut runtime, "team");
        let targetname = text(&mut runtime, "targetname");
        let origin = match runtime.object_field(id, "origin") {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        return Err(format!(
            "tried to play a scripted animation on a dead AI; entity {number} team {team} origin {} {} {} targetname {targetname} classname {classname}\n",
            origin[0], origin[1], origin[2]
        ));
    }
    let substate = match scripted.substate {
        Substate::Any => Value::Undefined,
        Substate::Index(n) => Value::Int(n),
        Substate::Alias(alias) => Value::string(&alias),
    };
    let mode = scripted
        .mode
        .map_or(Value::Undefined, |mode| Value::string(&mode));
    let now = super::super::super::players::now_ms(world);
    notify_now(world, receiver.clone(), "killanimscript", Vec::new(), now);
    let init = format!("{SCRIPTED}::init");
    run_now(
        world,
        &init,
        receiver.clone(),
        vec![
            Value::Vector(scripted.origin),
            Value::Vector(scripted.angles),
            Value::string(&scripted.state),
            substate,
            mode,
        ],
        now,
    )
    .map_err(|fault| format!("{init}: {fault:?}"))?;
    let main = format!("{SCRIPTED}::main");
    run_now(world, &main, receiver.clone(), Vec::new(), now)
        .map_err(|fault| format!("{main}: {fault:?}"))?;
    Ok(Value::Undefined)
}

/// `startscriptedanim`: plays the clip from the origin and angles given.
fn start_scripted(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = actor(world, receiver)?;
    let scripted = scripted_args(world, id, args)?;
    let table = table(world, id)?;
    let mut seed = seed(world, id);
    let substate = pick(
        &table,
        &scripted.state,
        scripted.substate,
        &mut seed,
        SCRIPTED_BUILTIN,
    )?;
    play(
        world,
        id,
        &scripted.state,
        substate,
        Some((scripted.origin, scripted.angles)),
    )?;
    Ok(Value::Undefined)
}

fn mode_arg(args: &[Value], index: usize) -> Result<String, String> {
    Ok(string(args, index)?.to_ascii_lowercase())
}

fn anim_clip(world: &mut World, args: &[Value], index: usize) -> Result<Arc<AnimClip>, String> {
    let name = match args.get(index) {
        Some(Value::Animation { name, .. }) => name.clone(),
        _ => return Err("parameter is not an animation".into()),
    };
    FrameWorld::from_world(world)
        .anim_clip_named(&name)
        .ok_or_else(|| format!("animation '{name}' is not loaded in the simulation"))
}

/// Where a clip aligned to an origin and angles starts: its root delta at
/// its first frame, in that frame.
fn start_pose(world: &mut World, args: &[Value]) -> Result<([f32; 3], [f32; 3]), String> {
    let origin = vector(args, 0)?;
    let angles = vector(args, 1)?;
    let clip = anim_clip(world, args, 2)?;
    let axis = math_iw4::angles_to_axis(angles);
    let trans = clip.abs_delta_trans(0.0);
    let start = std::array::from_fn(|i| {
        origin[i] + trans[0] * axis[0][i] + trans[1] * axis[1][i] + trans[2] * axis[2][i]
    });
    let (sin, cos) = clip.abs_delta_yaw(0.0).to_radians().sin_cos();
    let rotation = [[cos, sin, 0.0], [-sin, cos, 0.0], [0.0, 0.0, 1.0]];
    let angles = math_iw4::axis_to_angles(math_iw4::matrix_multiply(rotation, axis));
    Ok((start, angles))
}

/// The scripted clip's anchor, while one plays.
pub(super) fn scripted(world: &mut World, id: u64) -> bool {
    anim_mut(world, id)
        .playing
        .as_ref()
        .is_some_and(|playing| playing.anchor.is_some() && !playing.done)
}

pub(super) fn get(world: &mut World, id: u64) -> ActorAnim {
    anim_mut(world, id).clone()
}

pub(super) fn reset_orient(world: &mut World, id: u64) {
    let anim = anim_mut(world, id);
    anim.orient = "face default";
    anim.orient_yaw = None;
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    registry.on_tick(advance);
    registry.register(Function, "getstartorigin", |world, _, args| {
        Ok(Value::Vector(start_pose(world, args)?.0))
    });
    registry.register(Function, "getstartangles", |world, _, args| {
        Ok(Value::Vector(start_pose(world, args)?.1))
    });
    registry.register(Method, "setanimstatefromasd", |world, receiver, args| {
        const BUILTIN: &str = "ActorCmd_SetAnimStateFromASD";
        let id = actor(world, receiver)?;
        let state = state_name(args, 0)?;
        let table = table(world, id)?;
        if !table.1.states.contains_key(&state) {
            return Err(format!(
                "{BUILTIN}: state '{state}' not found in AnimStateDef '{}'",
                table.0
            ));
        }
        let substate = substate_arg(args, 1, BUILTIN)?;
        let mut seed = seed(world, id);
        let substate = pick(&table, &state, substate, &mut seed, BUILTIN)?;
        play(world, id, &state, substate, None)?;
        Ok(Value::Int(substate as i32))
    });
    registry.register(Method, "getanimstatefromasd", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let state = anim_mut(world, id)
            .playing
            .as_ref()
            .map_or(String::new(), |p| p.state.clone());
        Ok(Value::string(&state))
    });
    registry.register(Method, "hasanimstatefromasd", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let state = state_name(args, 0)?;
        let (_, table) = table(world, id)?;
        Ok(Value::Int(table.states.contains_key(&state).into()))
    });
    registry.register(Method, "getanimsubstatefromasd", |world, receiver, args| {
        const BUILTIN: &str = "ActorCmd_GetAnimSubStateFromASD";
        let id = actor(world, receiver)?;
        if args.is_empty() {
            let substate = anim_mut(world, id)
                .playing
                .as_ref()
                .map_or(0, |p| p.substate);
            return Ok(Value::Int(substate as i32));
        }
        let state = state_name(args, 0)?;
        let substate = match args.get(1) {
            None => Substate::Any,
            Some(Value::String(alias)) => Substate::Alias(alias.to_string().to_ascii_lowercase()),
            Some(_) => return Err(format!("{BUILTIN}: substate must be string")),
        };
        let table = table(world, id)?;
        let mut seed = seed(world, id);
        Ok(Value::Int(
            pick(&table, &state, substate, &mut seed, BUILTIN)? as i32,
        ))
    });
    registry.register(Method, "getanimsubstatecountfromasd", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let state = state_name(args, 0)?;
        let (name, table) = table(world, id)?;
        let count = table.states.get(&state).map(Vec::len).ok_or_else(|| {
            format!(
                "ActorCmd_GetAnimSubStateCountFromASD: state '{state}' not found in AnimStateDef '{name}'"
            )
        })?;
        Ok(Value::Int(count as i32))
    });
    registry.register(Method, "getanimfromasd", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let (state, substate) = chosen(world, id, args, "ActorCmd_GetAnimFromASD")?;
        let (name, table) = table(world, id)?;
        let clip = table
            .states
            .get(&state)
            .and_then(|entries| entries.get(substate))
            .map(|entry| entry.clip.clone())
            .ok_or("no animation is playing")?;
        Ok(Value::Animation {
            tree: name.into(),
            name: clip.into(),
        })
    });
    registry.register(Method, "getanimlengthfromasd", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let (state, substate) = chosen(world, id, args, "ActorCmd_GetAnimLengthFromASD")?;
        let (_, table) = table(world, id)?;
        Ok(Value::Float(
            clip_of(world, &table, &state, substate)?.duration(),
        ))
    });
    registry.register(
        Method,
        "getanimhasnotetrackfromasd",
        |world, receiver, args| {
            const BUILTIN: &str = "ActorCmd_GetAnimHasNotetrackFromASD";
            let id = actor(world, receiver)?;
            let (state, substate, note) = match args.len() {
                1 => {
                    let (state, substate) = chosen(world, id, &[], BUILTIN)?;
                    (state, substate, string(args, 0)?)
                }
                3 => {
                    let (state, substate) = chosen(world, id, &args[..2], BUILTIN)?;
                    (state, substate, string(args, 2)?)
                }
                _ => return Err(format!("{BUILTIN}: improper param count, must be 1 or 3")),
            };
            let (_, table) = table(world, id)?;
            let clip = clip_of(world, &table, &state, substate)?;
            Ok(Value::Int(
                clip.notifies
                    .iter()
                    .any(|n| n.name.eq_ignore_ascii_case(&note))
                    .into(),
            ))
        },
    );
    registry.register(Method, "animscripted", animscripted);
    registry.register(Method, "startscriptedanim", start_scripted);
    registry.register(Method, "stopanimscripted", |world, receiver, _| {
        let id = actor(world, receiver)?;
        if let Some(playing) = anim_mut(world, id).playing.as_mut() {
            playing.anchor = None;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "animmode", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let name = mode_arg(args, 0)?;
        if args.len() > 1 {
            int(args, 1)?;
        }
        let mode = ANIM_MODES
            .iter()
            .find(|mode| **mode == name)
            .ok_or("illegal call to animmode()\n")?;
        anim_mut(world, id).mode = mode;
        Ok(Value::Undefined)
    });
    registry.register(Method, "orientmode", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let name = mode_arg(args, 0)?;
        let Some(mode) = ORIENT_MODES.iter().find(|mode| **mode == name) else {
            return Err("orientMode must be 'face angle', 'face current', 'face direction', 'face enemy', 'face enemy or motion', 'face goal', 'face motion', 'face point', or 'face default'\n'face direction' and 'face point' take a second argument that is a vector giving the way to face\n'face angle' takes a second argument that is a yaw angle\n".into());
        };
        let origin = match world.resource_mut::<RoundScript>().object_field(id, "origin") {
            Value::Vector(v) => v,
            _ => [0.0; 3],
        };
        let yaw = match *mode {
            "face angle" => Some(super::float(args, 1)?),
            "face current" => match world.resource_mut::<RoundScript>().object_field(id, "angles") {
                Value::Vector(v) => Some(v[1]),
                _ => None,
            },
            "face direction" => {
                let direction = vector(args, 1)?;
                if direction[0] == 0.0 && direction[1] == 0.0 {
                    return Err("cannot face (0, 0, *)".into());
                }
                Some(direction[1].atan2(direction[0]).to_degrees())
            }
            "face point" => {
                let point = vector(args, 1)?;
                let (x, y) = (point[0] - origin[0], point[1] - origin[1]);
                (x * x + y * y >= 1.0).then(|| y.atan2(x).to_degrees())
            }
            _ => None,
        };
        if args.len() > 2 {
            int(args, 2)?;
        }
        let anim = anim_mut(world, id);
        anim.orient = mode;
        anim.orient_yaw = yaw;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setaimanimweights", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let weights = [super::float(args, 0)?, super::float(args, 1)?];
        anim_mut(world, id).aim_weights = weights;
        Ok(Value::Undefined)
    });
    registry.register(Function, "getanimlength", |world, _, args| {
        Ok(Value::Float(anim_clip(world, args, 0)?.duration()))
    });
    registry.register(Function, "animhasnotetrack", |world, _, args| {
        let clip = anim_clip(world, args, 0)?;
        let note = string(args, 1)?;
        Ok(Value::Int(
            clip.notifies
                .iter()
                .any(|n| n.name.eq_ignore_ascii_case(&note))
                .into(),
        ))
    });
    registry.register(Function, "getnotetracktimes", |world, _, args| {
        let clip = anim_clip(world, args, 0)?;
        let note = string(args, 1)?;
        let times: Vec<Value> = clip
            .notifies
            .iter()
            .filter(|n| n.name.eq_ignore_ascii_case(&note))
            .map(|n| Value::Float(n.time))
            .collect();
        new_array(world, times)
    });
}
