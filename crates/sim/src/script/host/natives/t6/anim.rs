//! Black Ops 2's actor animation: the animation state definition (`.asd`)
//! an actor's aitype names, the state and substate scripts pick from it,
//! the notetracks its clips send under the state's notify, scripted
//! animations, and the animation and orientation modes.

use super::super::super::arrays::new_array;
use super::super::super::entities::EntityKind;
use super::super::engine::trace;
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
/// The engine's base blend duration on an `.asd` state change (`ASD_SetState`).
const ASD_BLEND_SECS: f32 = 0.2;
/// `deathplant`'s ground probe reach and mask (`MASK_PLAYERSOLID & ~CONTENTS_BODY`).
const DEATHPLANT_REACH: f32 = 18.0;
const DEATHPLANT_MASK: u32 = 0x0081_0011;
const DEATHPLANT_GRAVITY: f32 = 800.0;

#[derive(Clone, Copy, Debug)]
struct ScriptedAnchor {
    origin: [f32; 3],
    angles: [f32; 3],
    deathplant: bool,
    fall_vel: f32,
}

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
    weight: f32,
    goal_weight: f32,
    goal_time: f32,
    /// A scripted animation plays from this origin and angles.
    anchor: Option<ScriptedAnchor>,
}

/// An actor's animation state beside its entity.
#[derive(Clone, Debug)]
pub(super) struct ActorAnim {
    table: Option<(String, Arc<AnimStateTable>)>,
    playing: Option<Playing>,
    blending: Vec<Playing>,
    pub(super) mode: &'static str,
    pub(super) orient: &'static str,
    /// The yaw `face angle`, `face direction` or `face point` turns to.
    pub(super) orient_yaw: Option<f32>,
    /// How far the clip's root moved this frame, in the actor's frame, when
    /// it is not a scripted clip.
    pub(super) step: [f32; 3],
    pub(super) step_yaw: f32,
    /// The aim weights `setaimanimweights` sets (used only by the legacy
    /// non-Zombies animstate table).
    aim_weights: [f32; 2],
}

impl Default for ActorAnim {
    fn default() -> Self {
        Self {
            table: None,
            playing: None,
            blending: Vec::new(),
            mode: "normal",
            orient: "face default",
            orient_yaw: None,
            step: [0.0; 3],
            step_yaw: 0.0,
            aim_weights: [0.0; 2],
        }
    }
}

#[derive(Resource, Default)]
pub(super) struct ActorAnims(pub(super) BTreeMap<u64, ActorAnim>);

#[derive(Resource, Default)]
struct PrecachedAsds {
    by_aitype: BTreeMap<String, i32>,
    by_index: BTreeMap<i32, String>,
}

pub(super) fn register_aitype_index(world: &mut World, index: i32, aitype: &str) {
    world
        .get_resource_or_insert_with(PrecachedAsds::default)
        .by_aitype
        .insert(aitype.to_ascii_lowercase(), index);
}

/// A client-side notetrack crossed by an actor's animation clip.
#[derive(Clone, Debug)]
pub(crate) enum ActorClientNote {
    /// `sndnt#<alias>` played on the actor.
    Sound { number: i32, alias: String },
    /// `rmbnt#<rumble>` played on the actor.
    Rumble { number: i32, rumble: String },
    /// `footstep_*` or `dogstep_*` queued into `CG_ProcessFootsteps`.
    Footstep {
        number: i32,
        aitype: Option<String>,
        note: String,
        bone: &'static str,
    },
}

#[derive(Resource, Default)]
pub(crate) struct ActorClientNotes(pub(crate) Vec<ActorClientNote>);

const FOOTSTEP_BONES: [(&str, &str); 26] = [
    ("footstep_left_large", "j_ball_le"),
    ("footstep_left_small", "j_ball_le"),
    ("footstep_right_large", "j_ball_ri"),
    ("footstep_right_small", "j_ball_ri"),
    ("footstep_left", "j_ball_le"),
    ("footstep_right", "j_ball_ri"),
    ("footstep_front_left", "jnt_f_l_ankle"),
    ("footstep_front_right", "jnt_f_r_ankle"),
    ("footstep_rear_left", "jnt_r_l_ankle"),
    ("footstep_rear_right", "jnt_r_r_ankle"),
    ("footstep_front_left_small", "jnt_f_l_ankle"),
    ("footstep_front_right_small", "jnt_f_r_ankle"),
    ("footstep_rear_left_small", "jnt_r_l_ankle"),
    ("footstep_rear_right_small", "jnt_r_r_ankle"),
    ("footstep_front_left_shuffle", "jnt_f_l_ankle"),
    ("footstep_front_right_shuffle", "jnt_f_r_ankle"),
    ("footstep_rear_left_shuffle", "jnt_r_l_ankle"),
    ("footstep_rear_right_shuffle", "jnt_r_r_ankle"),
    ("footstep_front_left_scrape", "jnt_f_l_knee_upper_dmg"),
    ("footstep_front_right_scrape", "jnt_f_r_knee_upper_dmg"),
    ("footstep_rear_left_scrape", "jnt_r_l_knee_upper_dmg"),
    ("footstep_rear_right_scrape", "jnt_r_r_knee_upper_dmg"),
    ("dogstep_lf", "j_ball_le"),
    ("dogstep_rf", "j_ball_ri"),
    ("dogstep_lr", "j_palm_le"),
    ("dogstep_rr", "j_palm_ri"),
];

fn record_client_notes(world: &mut World, id: u64, notes: &[String]) {
    if notes.is_empty()
        || !world.contains_resource::<crate::script::host::client_scripts::ClientScripts>()
    {
        return;
    }
    let (number, aitype) = {
        let runtime = world.resource::<RoundScript>();
        let Some(entity) = runtime.entities.get(&id) else {
            return;
        };
        (
            entity.number,
            entity.classname.strip_prefix("actor_").map(str::to_owned),
        )
    };
    if number < 0 {
        return;
    }
    let mut out = world.get_resource_or_insert_with(ActorClientNotes::default);
    for note in notes {
        let lower = note.to_ascii_lowercase();
        if let Some(alias) = lower.strip_prefix("sndnt#") {
            if !alias.is_empty() {
                out.0.push(ActorClientNote::Sound {
                    number,
                    alias: alias.to_owned(),
                });
            }
        } else if let Some(rumble) = lower.strip_prefix("rmbnt#") {
            if !rumble.is_empty() {
                out.0.push(ActorClientNote::Rumble {
                    number,
                    rumble: rumble.to_owned(),
                });
            }
        } else if lower.starts_with("footstep_") || lower.starts_with("dogstep_") {
            let bone = FOOTSTEP_BONES
                .iter()
                .find(|(k, _)| *k == lower)
                .map_or("", |(_, b)| *b);
            out.0.push(ActorClientNote::Footstep {
                number,
                aitype: aitype.clone(),
                note: lower,
                bone,
            });
        }
    }
}

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

/// The state definition the actor's aitype named (`animstatedef` or
/// `precacheanimstatedef`).
fn table(world: &mut World, id: u64) -> Result<(String, Arc<AnimStateTable>), String> {
    if let Some(table) = anim_mut(world, id).table.clone() {
        return Ok(table);
    }
    let name = match world
        .resource_mut::<RoundScript>()
        .object_field(id, "animstatedef")
    {
        Value::String(name) if !name.is_empty() => name.to_string(),
        _ => {
            let aitype = world
                .resource::<RoundScript>()
                .entities
                .get(&id)
                .and_then(|e| e.classname.strip_prefix("actor_"))
                .map(str::to_ascii_lowercase);
            aitype
                .and_then(|aitype| {
                    let precached = world.get_resource::<PrecachedAsds>()?;
                    let idx = precached.by_aitype.get(&aitype)?;
                    precached.by_index.get(idx).cloned()
                })
                .ok_or_else(|| String::from("actor has no AnimStateDef"))?
        }
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
        // -1 picks at random too, as the engine's substate choice reads it.
        None | Some(Value::Undefined) | Some(Value::Int(-1)) => Substate::Any,
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

/// Starts a clip of the actor's state (`ASD_SetState`): blends out any
/// previously playing clip over `0.2 / rate` seconds and starts the new clip
/// (continuing from its current time when the same clip is already playing and
/// the state is not marked `restart`).
fn play(
    world: &mut World,
    id: u64,
    state: &str,
    substate: usize,
    anchor: Option<ScriptedAnchor>,
) -> Result<(), String> {
    let (_, table) = table(world, id)?;
    let clip = clip_of(world, &table, state, substate)?;
    let properties = table.properties.get(state);
    let notify = properties.and_then(|properties| {
        properties
            .iter()
            .position(|word| word == "notify")
            .and_then(|at| properties.get(at + 1))
            .cloned()
    });
    let restart = anchor.is_some()
        || properties.is_some_and(|properties| properties.iter().any(|word| word == "restart"));
    let rate = world
        .get_resource::<AnimRates>()
        .and_then(|rates| rates.0.get(&id).copied())
        .unwrap_or(1.0);
    let blend_time = if rate > 0.0 {
        ASD_BLEND_SECS / rate
    } else {
        ASD_BLEND_SECS
    };
    diag::debug!(
        Sim,
        "actor {id}: plays {state}/{substate} {} for {:.2}s, notify {notify:?}, restart {restart}",
        clip.name,
        clip.duration(),
    );
    {
        let anim = anim_mut(world, id);
        if !restart
            && let Some(current) = anim.playing.as_mut()
            && current.clip.name == clip.name
            && !current.done
        {
            current.state = state.to_owned();
            current.substate = substate;
            current.notify = notify;
            current.goal_weight = 1.0;
            current.goal_time = blend_time;
            current.anchor = anchor;
            for b in &mut anim.blending {
                b.goal_weight = 0.0;
                b.goal_time = blend_time;
            }
        } else {
            if let Some(mut prev) = anim.playing.take()
                && prev.weight > 0.0
            {
                prev.goal_weight = 0.0;
                prev.goal_time = blend_time;
                prev.anchor = None;
                anim.blending.retain(|b| b.clip.name != clip.name);
                if prev.clip.name != clip.name {
                    anim.blending.push(prev);
                }
            }
            for b in &mut anim.blending {
                b.goal_weight = 0.0;
                b.goal_time = blend_time;
            }
            let initial_weight = if anim.blending.is_empty() { 1.0 } else { 0.0 };
            anim.playing = Some(Playing {
                state: state.to_owned(),
                substate,
                clip,
                notify,
                time: 0.0,
                sent: -1.0,
                done: false,
                weight: initial_weight,
                goal_weight: 1.0,
                goal_time: blend_time,
                anchor,
            });
        }
    }
    if anchor.is_some() {
        place(world, id);
    }
    pose(world, id);
    Ok(())
}

/// `G_AnimScripted_Think`: holds the actor at its scripted anchor plus the
/// clip's root motion, planting to the floor (or falling under gravity) in
/// `deathplant` mode and colliding with the world in `gravity` animmode.
fn place(world: &mut World, id: u64) {
    let (mode, Some(mut playing)) = ({
        let anim = anim_mut(world, id);
        (anim.mode, anim.playing.clone())
    }) else {
        return;
    };
    let Some(mut anchor) = playing.anchor else {
        return;
    };
    let delta = playing.clip.abs_delta_trans(playing.time);
    let (sin, cos) = anchor.angles[1].to_radians().sin_cos();
    let mut at = [
        anchor.origin[0] + delta[0] * cos - delta[1] * sin,
        anchor.origin[1] + delta[0] * sin + delta[1] * cos,
        anchor.origin[2] + delta[2],
    ];
    let yaw = anchor.angles[1] + playing.clip.abs_delta_yaw(playing.time);
    if anchor.deathplant {
        let top = [at[0], at[1], at[2] + DEATHPLANT_REACH];
        let bottom = [at[0], at[1], at[2] - DEATHPLANT_REACH];
        let hit = trace(world, top, bottom, [0.0; 3], [0.0; 3], DEATHPLANT_MASK);
        if hit.startsolid == 0 && hit.fraction < 1.0 {
            at[2] = top[2] + (bottom[2] - top[2]) * hit.fraction;
            anchor.fall_vel = 0.0;
        } else {
            let dt = crate::MATCH_TICK_MS as f32 / 1000.0;
            anchor.fall_vel -= DEATHPLANT_GRAVITY * dt;
            let next_z = at[2] + anchor.fall_vel * dt;
            let drop = trace(
                world,
                at,
                [at[0], at[1], next_z],
                [0.0; 3],
                [0.0; 3],
                DEATHPLANT_MASK,
            );
            at[2] += (next_z - at[2]) * drop.fraction;
            if drop.fraction < 1.0 {
                anchor.fall_vel = 0.0;
            }
            anchor.origin[2] = at[2] - delta[2];
        }
        playing.anchor = Some(anchor);
        anim_mut(world, id).playing = Some(playing);
    } else if mode == "gravity" {
        let from = super::movement::origin_of(world, id);
        at = super::movement::step_actor(world, id, from, at);
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.set_object_field(id, "origin", Value::Vector(at));
    runtime.set_object_field(
        id,
        "angles",
        Value::Vector([anchor.angles[0], yaw, anchor.angles[2]]),
    );
}

/// Plays every actor's clip one server frame on: its notetracks go out
/// under its state's notify, a looping clip starts over.
/// `setentityanimrate`: how fast an entity's animations play (0 to 1).
#[derive(bevy_ecs::prelude::Resource, Default)]
struct AnimRates(std::collections::BTreeMap<u64, f32>);

fn advance_clip_delta(clip: &AnimClip, from: f32, to: f32, cycles: f32) -> ([f32; 3], f32) {
    let (start, end, full) = (
        clip.abs_delta_trans(from),
        clip.abs_delta_trans(to),
        clip.abs_delta_trans(1.0),
    );
    let step = std::array::from_fn(|i| end[i] - start[i] + cycles * full[i]);
    let (start_yaw, end_yaw, full_yaw) = (
        clip.abs_delta_yaw(from),
        clip.abs_delta_yaw(to),
        clip.abs_delta_yaw(1.0),
    );
    let mut dyaw = end_yaw - start_yaw;
    if dyaw > 180.0 {
        dyaw -= 360.0;
    } else if dyaw < -180.0 {
        dyaw += 360.0;
    }
    (step, dyaw + cycles * full_yaw)
}

fn step_goal_weight(weight: f32, goal_weight: f32, goal_time: f32, dt: f32) -> (f32, f32) {
    if goal_time <= dt {
        return (goal_weight, 0.0);
    }
    let next = (goal_weight - weight) * dt / goal_time + weight;
    let next = if next < 1e-6 {
        goal_weight * 0.001
    } else {
        next
    };
    (next, goal_time - dt)
}

fn advance(world: &mut World) {
    let Some(anims) = world.get_resource::<ActorAnims>() else {
        return;
    };
    let ids: Vec<u64> = anims.0.keys().copied().collect();
    let dt = crate::MATCH_TICK_MS as f32 / 1000.0;
    for id in ids {
        if !world.resource::<RoundScript>().entities.contains_key(&id) {
            world.resource_mut::<ActorAnims>().0.remove(&id);
            continue;
        }
        {
            let anim = anim_mut(world, id);
            anim.step = [0.0; 3];
            anim.step_yaw = 0.0;
        }
        let rate = world
            .get_resource::<AnimRates>()
            .and_then(|rates| rates.0.get(&id).copied())
            .unwrap_or(1.0);
        let step = dt * rate;
        let mut blended_step = [0.0f32; 3];
        let mut blended_yaw = 0.0f32;
        let mut total_weight = 0.0f32;
        {
            let anim = anim_mut(world, id);
            for b in &mut anim.blending {
                (b.weight, b.goal_time) =
                    step_goal_weight(b.weight, b.goal_weight, b.goal_time, dt);
                if b.weight <= 0.0 || b.done {
                    continue;
                }
                let from = b.time;
                let length = b.clip.duration();
                let mut cycles = 0.0;
                let mut time = if length > 0.0 {
                    b.time + step / length
                } else {
                    1.0
                };
                if time >= 1.0 {
                    if b.clip.looping {
                        cycles = time.floor();
                        time -= cycles;
                    } else {
                        time = 1.0;
                        b.done = true;
                    }
                }
                b.time = time;
                let (dtrans, dyaw) = advance_clip_delta(&b.clip, from, time, cycles);
                for i in 0..3 {
                    blended_step[i] += b.weight * dtrans[i];
                }
                blended_yaw += b.weight * dyaw;
                total_weight += b.weight;
            }
            anim.blending.retain(|b| b.weight > 0.0);
        }
        let Some(mut playing) = anim_mut(world, id).playing.clone() else {
            continue;
        };
        (playing.weight, playing.goal_time) =
            step_goal_weight(playing.weight, playing.goal_weight, playing.goal_time, dt);
        if anim_mut(world, id).blending.is_empty() {
            playing.weight = 1.0;
        }
        if playing.done {
            if playing.anchor.is_some() {
                playing.anchor = None;
            }
            anim_mut(world, id).playing = Some(playing);
            pose(world, id);
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
            let w = playing.weight.max(0.01);
            let (dtrans, dyaw) = advance_clip_delta(&playing.clip, from, time, cycles);
            for i in 0..3 {
                blended_step[i] += w * dtrans[i];
            }
            blended_yaw += w * dyaw;
            total_weight += w;
            if total_weight > 0.0 {
                let inv = 1.0 / total_weight;
                anim_mut(world, id).step = blended_step.map(|c| c * inv);
                anim_mut(world, id).step_yaw = blended_yaw * inv;
            }
        }
        anim_mut(world, id).playing = Some(playing);
        place(world, id);
        pose(world, id);
        record_client_notes(world, id, &notes);
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

/// Poses the actor's body by its blended playing clips, so clients draw it and
/// traces meet it where the clips hold it (its bind pose lies on the floor).
fn pose(world: &mut World, id: u64) {
    use xmodel_runtime::{
        XAnimNodeId, XAnimNodeState, XAnimSemanticNode, XAnimSemanticNodeKind, XAnimTreeSnapshot,
    };
    let Some(presence) = world
        .resource::<RoundScript>()
        .entities
        .get(&id)
        .and_then(|entity| entity.presence)
    else {
        return;
    };
    let (playing, blending) = {
        let anim = anim_mut(world, id);
        let Some(playing) = anim.playing.clone() else {
            return;
        };
        (playing, anim.blending.clone())
    };
    let mut clips: BTreeMap<String, Arc<AnimClip>> = BTreeMap::new();
    clips.insert(playing.clip.name.clone(), Arc::clone(&playing.clip));
    for b in &blending {
        clips.insert(b.clip.name.clone(), Arc::clone(&b.clip));
    }
    let nodes = if blending.is_empty() {
        vec![XAnimSemanticNode {
            parent: None,
            kind: XAnimSemanticNodeKind::Leaf,
            clip: Some(playing.clip.name.clone()),
            parts: None,
            state: XAnimNodeState {
                time: playing.time,
                old_time: playing.time,
                goal_weight: 1.0,
                weight: 1.0,
                rate: 1.0,
                ..XAnimNodeState::default()
            },
        }]
    } else {
        let mut nodes = Vec::with_capacity(2 + blending.len());
        nodes.push(XAnimSemanticNode {
            parent: None,
            kind: XAnimSemanticNodeKind::Blend,
            clip: None,
            parts: None,
            state: XAnimNodeState {
                weight: 1.0,
                goal_weight: 1.0,
                ..XAnimNodeState::default()
            },
        });
        for item in std::iter::once(&playing).chain(&blending) {
            if item.weight <= 0.0 {
                continue;
            }
            nodes.push(XAnimSemanticNode {
                parent: Some(XAnimNodeId(0)),
                kind: XAnimSemanticNodeKind::Leaf,
                clip: Some(item.clip.name.clone()),
                parts: None,
                state: XAnimNodeState {
                    time: item.time,
                    old_time: item.time,
                    goal_weight: item.goal_weight,
                    weight: item.weight,
                    rate: 1.0,
                    ..XAnimNodeState::default()
                },
            });
        }
        nodes
    };
    let tree = XAnimTreeSnapshot {
        definition_revision: nodes.len() as u32,
        state_revision: 0,
        nodes,
    };
    let resolved = tree.resolve(|name| clips.get(name).cloned()).ok();
    if let Some(dobj) = FrameWorld::from_world(world)
        .collision_owner_mut(presence)
        .and_then(|row| row.dobj.as_mut())
    {
        dobj.set_tree_pose(tree, resolved);
        dobj.materialize();
    }
}

pub(crate) fn present(world: &mut World) {
    let Some(anims) = world.get_resource::<ActorAnims>() else {
        return;
    };
    let ids: Vec<u64> = anims.0.keys().copied().collect();
    for id in ids {
        pose(world, id);
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
    let alive = super::actor::in_use(world, id)
        && matches!(
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
    super::movement::enter_scripted(world, id);
    reset_orient(world, id);
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
    let deathplant = scripted.mode.as_deref() == Some("deathplant");
    play(
        world,
        id,
        &scripted.state,
        substate,
        Some(ScriptedAnchor {
            origin: scripted.origin,
            angles: scripted.angles,
            deathplant,
            fall_vel: 0.0,
        }),
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

fn quat_to_axis([x, y, z, w]: [f32; 4]) -> [[f32; 3]; 3] {
    let len2 = x * x + y * y + z * z + w * w;
    if len2 == 0.0 {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    let s = 2.0 / len2;
    let (xs, ys, zs) = (x * s, y * s, z * s);
    let (wx, wy, wz) = (w * xs, w * ys, w * zs);
    let (xx, xy, xz) = (x * xs, x * ys, x * zs);
    let (yy, yz, zz) = (y * ys, y * zs, z * zs);
    [
        [1.0 - (yy + zz), xy + wz, xz - wy],
        [xy - wz, 1.0 - (xx + zz), yz + wx],
        [xz + wy, yz - wx, 1.0 - (xx + yy)],
    ]
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
    let rotation = quat_to_axis(clip.abs_delta_quat(0.0));
    let angles = math_iw4::axis_to_angles(math_iw4::matrix_multiply(rotation, axis));
    Ok((start, angles))
}

/// The scripted clip's anchor, while one plays.
pub(super) fn scripted(world: &mut World, id: u64) -> bool {
    anim_mut(world, id)
        .playing
        .as_ref()
        .is_some_and(|playing| playing.anchor.is_some())
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

    // gscm_setentityanimrate(rate): the entity's animations play at `rate`
    // (0 to 1), with the engine's errors.
    registry.register(
        Namespace::Method,
        "setentityanimrate",
        |world, receiver, args| {
            let id = super::super::engine::entity_id(world, receiver)?;
            let rate = match args.first() {
                Some(Value::Int(n)) => *n as f32,
                Some(Value::Float(f)) => *f,
                _ => return Err("setentityanimrate expects a rate".into()),
            };
            if rate < 0.0 {
                return Err("must be > 0".into());
            }
            if rate > 1.0 {
                return Err("must be < 1".into());
            }
            world
                .get_resource_or_insert_with(AnimRates::default)
                .0
                .insert(id, rate);
            Ok(Value::Undefined)
        },
    );
    registry.register(Method, "getentityanimrate", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)?;
        let rate = world
            .get_resource::<AnimRates>()
            .and_then(|rates| rates.0.get(&id).copied())
            .unwrap_or(1.0);
        Ok(Value::Float(rate))
    });
    registry.register(Method, "isinscriptedstate", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let in_state = scripted(world, id) || super::movement::in_custom(world, id);
        Ok(Value::Int(in_state.into()))
    });
    registry.register(Method, "getmotionangle", |world, receiver, _| {
        let id = actor(world, receiver)?;
        let yaw = match world
            .resource_mut::<RoundScript>()
            .object_field(id, "angles")
        {
            Value::Vector(v) => v[1],
            _ => 0.0,
        };
        let Some(motion_yaw) = super::movement::path_yaw(world, id) else {
            return Ok(Value::Float(0.0));
        };
        let turns = (motion_yaw - yaw) * 0.002_777_777_8;
        let angle = (turns - (turns + 0.5).floor()) * 360.0;
        Ok(Value::Float(angle))
    });
    registry.register(Function, "precacheanimstatedef", |world, _, args| {
        let index = int(args, 0)?;
        match super::arg(args, 1)? {
            Value::AnimationTree(_) => {}
            other => {
                return Err(format!(
                    "type {} is not an animtree",
                    super::type_name(other)
                ));
            }
        }
        let name = string(args, 2)?.to_ascii_lowercase();
        world
            .get_resource_or_insert_with(PrecachedAsds::default)
            .by_index
            .insert(index, name);
        Ok(Value::Undefined)
    });
    registry.register(Method, "setanimstate", |world, receiver, args| {
        let id = actor(world, receiver)?;
        let state = state_name(args, 0)?;
        let substate = if args.len() > 1 {
            Substate::Index(int(args, 1)?)
        } else {
            Substate::Index(0)
        };
        if let Ok(table) = table(world, id)
            && let Ok(index) = pick(&table, &state, substate, &mut 0, "ActorCmd_SetAnimState")
        {
            let _ = play(world, id, &state, index, None);
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "getcorpseanim", |world, receiver, _| {
        let id = super::super::engine::entity_id(world, receiver)?;
        if !matches!(
            world.resource::<RoundScript>().entities[&id].kind,
            EntityKind::Corpse { .. }
        ) {
            return Err("Only valid on player corpses".into());
        }
        Ok(Value::Animation {
            tree: "multiplayer".into(),
            name: "pb_death_run_stumble".into(),
        })
    });
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
    registry.register(Function, "getmovedelta", |world, _, args| {
        let mut start = 0.0f32;
        let mut end = 1.0f32;
        if args.len() != 1 {
            if args.len() != 2 {
                end = super::float(args, 2)?;
                if !(0.0..=1.0).contains(&end) {
                    return Err("end time must be between 0 and 1".into());
                }
            }
            start = super::float(args, 1)?;
            if !(0.0..=1.0).contains(&start) {
                return Err("start time must be between 0 and 1".into());
            }
        }
        let clip = anim_clip(world, args, 0)?;
        let from = clip.abs_delta_trans(start);
        let mut to = clip.abs_delta_trans(end);
        if clip.looping && end < start {
            let base = clip.abs_delta_trans(0.0);
            let full = clip.abs_delta_trans(1.0);
            for i in 0..3 {
                to[i] += full[i] - base[i];
            }
        }
        let delta = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
        let (sin, cos) = (-clip.abs_delta_yaw(start).to_radians()).sin_cos();
        Ok(Value::Vector([
            delta[0] * cos - delta[1] * sin,
            delta[0] * sin + delta[1] * cos,
            delta[2],
        ]))
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
