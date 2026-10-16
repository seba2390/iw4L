//! Black Ops 2's client light mixers: a light entity's mixer blends two
//! sides (left and right), each a set of light values a hardcoded behavior
//! may animate, and runs each client frame as the engine's mixer update
//! does.
//!
//! Each frame, in order, for each mixer: its own behavior (`robot`), then
//! each side's behavior (the side the mixer rests on, while it is not
//! mixing, writes the light's intensity), then the mix: while mixing, the
//! fraction of its ramp (left-to-right from the left, right-to-left back)
//! blends the two sides' color, exponent, intensity, radius and cone into
//! the light; at the end it rests on the other side. A light whose
//! intensity crosses 0.3 runs the client scripts' light loop exploder
//! (`_fx::playlightloopexploder` going up, `stoplightloopexploder` going
//! down) when the mixer has an exploder id. A mixer becomes mixing on
//! `activatemixer`, on a level notify of its event, or on its `robot`
//! behavior's random period.
//!
//! A side's memory is kept as the engine lays it out (its parameters double
//! as behavior state), so each behavior reads and writes the offsets the
//! engine's does.

use super::super::super::{float, int, string, vector};
use super::{check_mixer_param_value, mixer_entity, mixer_light_entity, mixer_side, state};
use crate::script::client::ClientLights;
use crate::script::host::args::arg;
use crate::script::{Namespace, NativeRegistry};
use crate::script::{RoundScript, Value};
use bevy_ecs::prelude::{Resource, World};

/// A parameter the script left undefined (`-FLT_MAX`).
const UNDEFINED: f32 = f32::MIN;
/// The intensity a light crosses to run its light loop exploder.
const EXPLODER_INTENSITY: f32 = 0.3;
const PLAY_EXPLODER: &str = "clientscripts/mp/_fx::playlightloopexploder";
const STOP_EXPLODER: &str = "clientscripts/mp/_fx::stoplightloopexploder";

/// The engine's hardcoded light behaviors, by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Behavior {
    Firecaster,
    FireFlicker,
    ElectricalFlicker,
    FlickerThenStop,
    GenericDoubleStrobe,
    GenericStrobe,
    LightOff,
    Television,
    GenericFlickering,
    GenericPulsing,
    ExplodeThenFlicker,
}

impl Behavior {
    /// The behavior a name sets, `light_solid` none; `None` for an unknown
    /// name.
    fn named(name: &str) -> Option<Option<Self>> {
        Some(Some(match name {
            "firecaster" => Self::Firecaster,
            "fire_flicker" => Self::FireFlicker,
            "electrical_flicker" => Self::ElectricalFlicker,
            "flicker_then_stop" => Self::FlickerThenStop,
            "generic_double_strobe" => Self::GenericDoubleStrobe,
            "generic_strobe" => Self::GenericStrobe,
            "light_off" => Self::LightOff,
            "light_solid" => return Some(None),
            "television" => Self::Television,
            "generic_flickering" => Self::GenericFlickering,
            "generic_pulsing" => Self::GenericPulsing,
            "explode_then_flicker" => Self::ExplodeThenFlicker,
            _ => return None,
        }))
    }
}

/// One side of a mixer, as the engine lays it out from its color: words at
/// byte offsets `0x00..0x88` (color, exponent, intensity, radius, inner and
/// outer cone in degrees, then the behavior's 17 parameters from `0x28`,
/// which behaviors also use as state), its behavior state and next state.
#[derive(Clone, Debug)]
pub(super) struct Side {
    words: [u32; 0x88 / 4],
    state: u16,
    next: u16,
    behavior: Option<Behavior>,
    /// The sound alias a parameter holds, when the script gave a string.
    aliases: [Option<String>; 17],
}

impl Default for Side {
    fn default() -> Self {
        Self {
            words: [0; 0x88 / 4],
            state: 0,
            next: 0,
            behavior: None,
            aliases: Default::default(),
        }
    }
}

const COLOR: usize = 0x04;
const EXPONENT: usize = 0x10;
const INTENSITY: usize = 0x14;
const RADIUS: usize = 0x18;
const FOV_INNER: usize = 0x1c;
const FOV_OUTER: usize = 0x20;
const PARAMS: usize = 0x28;
/// The parameter a behavior plays its sound alias from.
const SOUND: usize = 0x4c;

impl Side {
    fn f(&self, at: usize) -> f32 {
        f32::from_bits(self.words[at / 4 - 1])
    }
    fn set_f(&mut self, at: usize, value: f32) {
        self.words[at / 4 - 1] = value.to_bits();
    }
    fn i(&self, at: usize) -> i32 {
        self.words[at / 4 - 1] as i32
    }
    fn set_i(&mut self, at: usize, value: i32) {
        self.words[at / 4 - 1] = value as u32;
    }
    /// Copies a word as it is.
    fn copy(&mut self, to: usize, from: usize) {
        self.words[to / 4 - 1] = self.words[from / 4 - 1];
    }
    /// Sets a parameter left undefined to its default.
    fn default_f(&mut self, at: usize, value: f32) {
        if self.f(at) == UNDEFINED {
            self.set_f(at, value);
        }
    }
    /// `*(uint32*)(side + 0x88) = state | next << 16`.
    fn states(&mut self, state: u16, next: u16) {
        self.state = state;
        self.next = next;
    }
}

/// A light's mixer.
#[derive(Clone, Debug, Default)]
pub(super) struct Mixer {
    /// The light entity it was made on.
    light: Option<u64>,
    /// The level notify that activates it (`""`: none).
    event: String,
    sides: [Side; 2],
    left_to_right_ms: f32,
    right_to_left_ms: f32,
    mixing: bool,
    mix_start: i32,
    /// The side it rests on (0 left, 1 right).
    position: u8,
    robot: bool,
    /// Its own behavior's 5 parameters; `robot` keeps its next time in the
    /// third and its state in the fifth's low half.
    params: [u32; 5],
    exploder: i32,
    exploder_on: bool,
    /// A full turn of the light every this many seconds (0: none), and the
    /// time the next turn starts after.
    rotate_seconds: f32,
    rotate_next: i32,
}

/// The client's mixers, in `makemixerlight` order (a light's mixer index is
/// one more than its place).
#[derive(Resource, Default)]
pub(super) struct Mixers {
    mixers: Vec<Mixer>,
    allocated: usize,
    /// The first frame their lights changed was logged.
    reported: bool,
}

/// `Byte4PackRgba`: each channel times 255, rounded to nearest, kept to
/// 0..255 (a NaN packs to 0).
fn pack(channels: [f32; 4]) -> [u8; 4] {
    channels.map(|channel| {
        let scaled = f64::from(channel * 255.0) + 9.313_225_746_154_785e-10;
        let rounded = scaled.round_ties_even();
        let int = if rounded.is_nan() || rounded >= 2_147_483_648.0 || rounded < -2_147_483_648.0 {
            i32::MIN
        } else {
            rounded as i32
        };
        int.clamp(0, 255) as u8
    })
}

/// `(int)` of a float, truncating (out of range: `0x80000000`).
fn trunc(value: f32) -> i32 {
    if value.is_nan() || !(-2_147_483_648.0..2_147_483_648.0).contains(&value) {
        i32::MIN
    } else {
        value as i32
    }
}

/// `__ftol2` of an x87 value.
fn ftol(value: f64) -> i32 {
    if value.is_nan() || !(-2_147_483_648.0..2_147_483_648.0).contains(&value) {
        i32::MIN
    } else {
        value as i32
    }
}

/// The fraction `(now - start) / duration`, held to 0..1.
fn fraction(now: i32, start: i32, duration: i32) -> f32 {
    let t = (now as f32 - start as f32) / duration as f32;
    let mut held = 1.0;
    if t - 1.0 < 0.0 {
        held = t;
    }
    if 0.0 <= -t {
        held = 0.0;
    }
    held
}

/// What a frame's mixers ask of the client game beyond their lights.
#[derive(Default)]
struct Requests {
    /// Light loop exploder calls: play or stop, exploder number.
    exploders: Vec<(bool, i32)>,
    /// Sounds at a light: alias and origin.
    sounds: Vec<(String, [f32; 3])>,
}

fn light_origin(world: &mut World, light: u64) -> [f32; 3] {
    match world
        .resource_mut::<RoundScript>()
        .object_field(light, "origin")
    {
        Value::Vector(origin) => origin,
        _ => [0.0; 3],
    }
}

/// `random()` scaled to `min..max` with the client's random state.
fn frand(world: &mut World, min: f32, max: f32) -> f64 {
    super::super::pure::frand_wide(world, min, max)
}

/// Sets the light's intensity as a behavior does on its side
/// (`side.intensity` when not the side shown; else also the light's, and a
/// crossing of 0.3 runs the light loop exploder).
fn apply(
    world: &mut World,
    mixer: &mut Mixer,
    side: usize,
    value: f32,
    active: bool,
    out: &mut Requests,
) {
    let value = if value < 0.0 { 0.0 } else { value };
    if !active {
        mixer.sides[side].set_f(INTENSITY, value);
        return;
    }
    let current = mixer.sides[side].f(INTENSITY);
    if current <= EXPLODER_INTENSITY || EXPLODER_INTENSITY < value {
        if current <= EXPLODER_INTENSITY && EXPLODER_INTENSITY < value {
            exploder(mixer, true, out);
        }
    } else {
        exploder(mixer, false, out);
    }
    if let Some(light) = mixer.light
        && let Some(entry) = world.resource_mut::<ClientLights>().lights.get_mut(&light)
    {
        entry.intensity = value;
    }
    mixer.sides[side].set_f(INTENSITY, value);
}

/// The mixer's light loop exploder goes on or off, once each way.
fn exploder(mixer: &mut Mixer, on: bool, out: &mut Requests) {
    if mixer.exploder != 0 && mixer.exploder_on != on {
        mixer.exploder_on = on;
        out.exploders.push((on, mixer.exploder.wrapping_abs()));
    }
}

/// Packs a side's color and exponent into the light, keeping its exponent.
fn write_color(world: &mut World, mixer: &Mixer, side: usize) {
    let Some(light) = mixer.light else {
        return;
    };
    let s = &mixer.sides[side];
    let packed = pack([s.f(COLOR), s.f(COLOR + 4), s.f(COLOR + 8), s.f(EXPONENT)]);
    if let Some(entry) = world.resource_mut::<ClientLights>().lights.get_mut(&light) {
        entry.color = [packed[0], packed[1], packed[2], entry.color[3]];
    }
}

fn light_exponent(world: &World, mixer: &Mixer) -> u8 {
    mixer
        .light
        .and_then(|light| world.resource::<ClientLights>().lights.get(&light))
        .map_or(0, |entry| entry.color[3])
}

/// A side's sound alias plays at the light, when it has one.
fn sound(world: &mut World, mixer: &Mixer, side: usize, out: &mut Requests) {
    let s = &mixer.sides[side];
    if s.i(SOUND) == 0 {
        return;
    }
    if let (Some(alias), Some(light)) = (&s.aliases[(SOUND - PARAMS) / 4], mixer.light) {
        out.sounds.push((alias.clone(), light_origin(world, light)));
    }
}

/// The flicker behaviors' shared frame: a new random target intensity and
/// duration each time the last is reached, the intensity lerped toward it.
fn flicker_frame(
    world: &mut World,
    mixer: &mut Mixer,
    side: usize,
    now: i32,
    pick: impl Fn(&mut World, &mut Side) -> (f64, f64),
) {
    // The pick reads the parameters and gives the target intensity and the
    // duration in msec, before truncation.
    let s = &mut mixer.sides[side];
    let (target, duration) = pick(world, s);
    s.set_f(0x58, target as f32);
    s.set_i(0x60, now);
    let duration = ftol(duration);
    s.set_i(0x5c, if duration == 0 { 1 } else { duration });
    s.state = 3;
}

/// The flicker behaviors' lerp step (state 3).
fn flicker_lerp(
    world: &mut World,
    mixer: &mut Mixer,
    side: usize,
    now: i32,
    active: bool,
    out: &mut Requests,
) {
    let s = &mixer.sides[side];
    let frac = fraction(now, s.i(0x60), s.i(0x5c));
    let value = (s.f(0x58) - s.f(0x64)) * frac + s.f(0x64);
    apply(world, mixer, side, value, active, out);
    if frac == 1.0 {
        let s = &mut mixer.sides[side];
        s.state = 2;
        s.copy(0x64, INTENSITY);
        sound(world, mixer, side, out);
    }
}

/// `firecaster`: flickers about its intensity at the time it starts, by
/// 0.63 to 1.2 times, each step 2 to 5 seconds.
fn firecaster(
    world: &mut World,
    mixer: &mut Mixer,
    side: usize,
    now: i32,
    active: bool,
    out: &mut Requests,
) {
    let s = &mut mixer.sides[side];
    if s.f(SOUND) == UNDEFINED {
        s.set_i(SOUND, 0);
    }
    match s.state {
        0 => {
            s.copy(0x54, INTENSITY);
            s.copy(0x64, INTENSITY);
        }
        2 => {}
        3 => return flicker_lerp(world, mixer, side, now, active, out),
        _ => return,
    }
    flicker_frame(world, mixer, side, now, |world, s| {
        let base = s.f(0x54);
        let target = frand(world, base * 0.63, base * 1.2);
        let duration = frand(world, 2.0, 5.0) * 1000.0;
        (target, duration)
    });
    flicker_lerp(world, mixer, side, now, active, out);
}

/// `fire_flicker` and `electrical_flicker`: each step a random target
/// between the third and fourth parameters, over a random time between the
/// first and second (seconds).
fn flicker(
    world: &mut World,
    mixer: &mut Mixer,
    side: usize,
    now: i32,
    active: bool,
    out: &mut Requests,
    defaults: [f32; 3],
) {
    let s = &mut mixer.sides[side];
    if s.f(SOUND) == UNDEFINED {
        s.set_i(SOUND, 0);
    }
    match s.state {
        0 => {
            s.default_f(0x28, defaults[0]);
            s.default_f(0x2c, defaults[1]);
            let low = s.f(0x28);
            if s.f(0x2c) < low {
                s.copy(0x28, 0x2c);
                s.set_f(0x2c, low);
            }
            s.default_f(0x30, defaults[2]);
            s.default_f(0x34, 1.0);
            s.copy(0x64, INTENSITY);
        }
        2 => {}
        3 => return flicker_lerp(world, mixer, side, now, active, out),
        _ => return,
    }
    flicker_frame(world, mixer, side, now, |world, s| {
        let target = frand(world, s.f(0x30), s.f(0x34));
        let duration = frand(world, s.f(0x28), s.f(0x2c)) * 1000.0;
        (target, duration)
    });
    flicker_lerp(world, mixer, side, now, active, out);
}

fn fire_flicker(
    world: &mut World,
    mixer: &mut Mixer,
    side: usize,
    now: i32,
    active: bool,
    out: &mut Requests,
) {
    flicker(world, mixer, side, now, active, out, [0.1, 0.2, 0.5]);
}

/// `flicker_then_stop`: `fire_flicker` until the eighth parameter's seconds
/// (default 2) have passed since the side was first shown, then off for
/// good.
fn flicker_then_stop(
    world: &mut World,
    mixer: &mut Mixer,
    side: usize,
    now: i32,
    active: bool,
    out: &mut Requests,
) {
    let s = &mut mixer.sides[side];
    if s.state == 0 {
        s.default_f(0x40, 2.0);
        s.set_i(0x54, 0);
        return fire_flicker(world, mixer, side, now, active, out);
    }
    if active && s.i(0x54) == 0 {
        s.set_i(0x54, trunc(s.f(0x40) * 1000.0 + now as f32));
    }
    let stop = s.i(0x54);
    if stop != 0 && stop <= now {
        apply(world, mixer, side, 0.0, active, out);
        mixer.sides[side].behavior = None;
        return;
    }
    fire_flicker(world, mixer, side, now, active, out);
}

/// `generic_strobe` and `generic_double_strobe`: off for the first
/// parameter's seconds, at its intensity for the second's, after the
/// twelfth parameter's delay.
fn strobe(
    world: &mut World,
    mixer: &mut Mixer,
    side: usize,
    now: i32,
    active: bool,
    out: &mut Requests,
    on_default: f32,
) {
    let s = &mut mixer.sides[side];
    let mut state = s.state;
    if state == 0 {
        s.default_f(0x28, 0.1);
        s.default_f(0x2c, on_default);
        s.copy(0x60, INTENSITY);
        if s.f(0x54) != UNDEFINED {
            s.set_i(0x64, now.wrapping_sub(trunc(s.f(0x54) * -1000.0)));
            s.state = 1;
            return;
        }
        state = 2;
    }
    match state {
        2 => {
            s.set_i(0x64, trunc(s.f(0x28) * 1000.0 + now as f32));
            apply(world, mixer, side, 0.0, active, out);
            mixer.sides[side].states(3, 4);
        }
        1 => {
            if s.i(0x64) < now {
                s.state = 2;
            }
        }
        3 => {
            if s.i(0x64) <= now {
                s.state = s.next;
            }
        }
        4 => {
            s.set_i(0x64, trunc(s.f(0x2c) * 1000.0 + now as f32));
            let on = s.f(0x60);
            apply(world, mixer, side, on, active, out);
            mixer.sides[side].states(3, 2);
        }
        _ => {}
    }
}

/// `light_off`: black at intensity 0, then no behavior.
fn light_off(world: &mut World, mixer: &mut Mixer, side: usize, active: bool, out: &mut Requests) {
    apply(world, mixer, side, 0.0, active, out);
    let exponent = light_exponent(world, mixer);
    let s = &mut mixer.sides[side];
    for at in [COLOR, COLOR + 4, COLOR + 8] {
        s.set_f(at, 0.0);
    }
    s.set_f(EXPONENT, f32::from(exponent));
    if active {
        write_color(world, mixer, side);
    }
    mixer.sides[side].behavior = None;
}

/// `television`: the intensity wanders between 0.7 and 1.2 times its start
/// over 0.3 to 1.2 seconds a step, and the color between its own and random
/// channels of 0.5 to 1 over 0.3 to 1.2 seconds a step.
fn television(
    world: &mut World,
    mixer: &mut Mixer,
    side: usize,
    now: i32,
    active: bool,
    out: &mut Requests,
) {
    let state = mixer.sides[side].state;
    let pick = match state {
        0 => {
            mixer.sides[side].copy(0x28, INTENSITY);
            true
        }
        2 => true,
        3 => false,
        _ => {
            television_color(world, mixer, side, now, active);
            return;
        }
    };
    if pick {
        let base = mixer.sides[side].f(0x28);
        let target = frand(world, base * 0.7, base * 1.2);
        let s = &mut mixer.sides[side];
        s.set_f(0x2c, target as f32);
        s.copy(0x38, INTENSITY);
        s.set_i(0x30, now);
        let duration = ftol(frand(world, 0.3, 1.2) * 1000.0);
        let s = &mut mixer.sides[side];
        s.set_i(0x34, if duration == 0 { 1 } else { duration });
        s.state = 3;
    }
    let s = &mixer.sides[side];
    let t = (now as f32 - s.i(0x30) as f32) / s.i(0x34) as f32;
    let mut held = 1.0;
    if t - 1.0 < 0.0 {
        held = t;
    }
    let frac = if -t < 0.0 { held } else { 0.0 };
    let value = (s.f(0x2c) - s.f(0x38)) * frac + s.f(0x38);
    apply(world, mixer, side, value, active, out);
    if frac == 1.0 {
        mixer.sides[side].state = 2;
    }
    television_color(world, mixer, side, now, active);
}

fn television_color(world: &mut World, mixer: &mut Mixer, side: usize, now: i32, active: bool) {
    let next = mixer.sides[side].next;
    if next == 0 || next == 4 {
        let s = &mut mixer.sides[side];
        for k in 0..4 {
            s.copy(0x68 + 4 * k, COLOR + 4 * k);
        }
        for k in 0..3 {
            let channel = frand(world, 0.5, 1.0) as f32;
            mixer.sides[side].set_f(0x78 + 4 * k, channel);
        }
        mixer.sides[side].set_i(0x3c, now);
        let duration = ftol(frand(world, 0.3, 1.2) * 1000.0);
        let s = &mut mixer.sides[side];
        s.set_i(0x40, duration);
        s.next = 5;
    } else if next != 5 {
        return;
    }
    let exponent = light_exponent(world, mixer);
    let s = &mut mixer.sides[side];
    let t = (now as f32 - s.i(0x3c) as f32) / s.i(0x40) as f32;
    let mut frac = t;
    if 0.0 <= t - 1.0 {
        frac = 1.0;
    }
    if 0.0 <= -t {
        frac = 0.0;
    }
    for k in 0..3 {
        let from = s.f(0x68 + 4 * k);
        let to = s.f(0x78 + 4 * k);
        s.set_f(COLOR + 4 * k, (to - from) * frac + from);
    }
    s.set_f(EXPONENT, f32::from(exponent));
    if active {
        write_color(world, mixer, side);
    }
    if frac == 1.0 {
        mixer.sides[side].next = 4;
    }
}

/// `generic_flickering`: bursts of the seventh to eighth parameter's count
/// of flickers (each between the fifth and sixth parameter's intensity,
/// when the intensity is over half its start, for the first to second
/// parameter's seconds), then its start for the twelfth to thirteenth
/// parameter's seconds.
fn generic_flickering(
    world: &mut World,
    mixer: &mut Mixer,
    side: usize,
    now: i32,
    active: bool,
    out: &mut Requests,
) {
    let s = &mut mixer.sides[side];
    if s.f(SOUND) == UNDEFINED {
        s.set_i(SOUND, 0);
    }
    let mut state = s.state;
    if state == 0 {
        s.default_f(0x54, 0.2);
        s.default_f(0x58, 1.5);
        s.default_f(0x28, 0.05);
        s.default_f(0x2c, 0.1);
        s.default_f(0x30, 0.0);
        s.default_f(0x34, 0.3);
        s.default_f(0x38, 1.0);
        s.default_f(0x3c, 10.0);
        s.copy(0x64, INTENSITY);
        state = 2;
    }
    if state == 2 {
        let (low, high) = (trunc(s.f(0x38)), trunc(s.f(0x3c)));
        let count = super::super::pure::irand(world, low, high);
        let s = &mut mixer.sides[side];
        s.set_i(0x60, count);
        s.next = 3;
        sound(world, mixer, side, out);
        state = 3;
    }
    if state == 3 {
        let s = &mixer.sides[side];
        let mut value = s.f(0x64);
        if value * 0.5 < s.f(INTENSITY) {
            value = frand(world, s.f(0x30), s.f(0x34)) as f32;
        }
        apply(world, mixer, side, value, active, out);
        let s = &mut mixer.sides[side];
        s.set_i(0x60, s.i(0x60).wrapping_sub(1));
        let (low, high) = (s.f(0x28), s.f(0x2c));
        let wait = ftol(frand(world, low, high) * -1000.0);
        let s = &mut mixer.sides[side];
        s.set_i(0x5c, now.wrapping_sub(wait));
        s.state = 4;
        state = 4;
    }
    let s = &mut mixer.sides[side];
    match state {
        4 => {
            if s.i(0x5c) < now {
                if s.next == 3 && s.i(0x60) == 0 {
                    s.state = 5;
                } else {
                    s.state = s.next;
                }
            }
        }
        5 => {
            let start = s.f(0x64);
            apply(world, mixer, side, start, active, out);
            let s = &mixer.sides[side];
            let (low, high) = (s.f(0x54), s.f(0x58));
            let wait = ftol(frand(world, low, high) * -1000.0);
            let s = &mut mixer.sides[side];
            s.set_i(0x5c, now.wrapping_sub(wait));
            s.states(4, 2);
        }
        _ => {}
    }
}

/// `generic_pulsing`: falls from its start to 0 over the fifth parameter's
/// seconds, holds for the first's, rises over the sixth's, then holds for
/// the fourth's (none: straight to falling), after the twelfth's delay.
fn generic_pulsing(
    world: &mut World,
    mixer: &mut Mixer,
    side: usize,
    now: i32,
    active: bool,
    out: &mut Requests,
) {
    let s = &mut mixer.sides[side];
    let mut state = s.state;
    if state == 0 {
        s.copy(0x58, INTENSITY);
        s.default_f(0x28, 0.25);
        s.default_f(0x2c, 0.75);
        s.default_f(0x38, EXPLODER_INTENSITY);
        s.default_f(0x3c, EXPLODER_INTENSITY);
        s.default_f(0x34, 0.0);
        if s.f(0x54) != UNDEFINED {
            s.state = 1;
            s.set_i(0x60, now.wrapping_sub(trunc(s.f(0x54) * -1000.0)));
            return;
        }
        state = 2;
    }
    if state == 2 {
        s.set_i(0x60, now);
        s.state = 3;
        state = 3;
    }
    if state == 4 {
        s.set_i(0x60, now);
        s.state = 5;
        state = 5;
    }
    match state {
        3 => {
            let frac = fraction(now, s.i(0x60), trunc(s.f(0x38) * 1000.0));
            let from = s.f(0x58);
            apply(world, mixer, side, (0.0 - from) * frac + from, active, out);
            if frac == 1.0 {
                mixer.sides[side].state = 9;
            }
        }
        5 => {
            let frac = fraction(now, s.i(0x60), trunc(s.f(0x3c) * 1000.0));
            let to = s.f(0x58);
            apply(world, mixer, side, (to - 0.0) * frac, active, out);
            if frac == 1.0 {
                let s = &mut mixer.sides[side];
                s.state = if s.f(0x34) <= 0.0 { 2 } else { 6 };
            }
        }
        1 => {
            if s.i(0x60) < now {
                s.state = 2;
            }
        }
        6 => {
            s.set_i(0x5c, trunc(s.f(0x34) * 1000.0 + now as f32));
            s.states(7, 2);
        }
        7 => {
            if s.i(0x5c) < now {
                s.state = s.next;
            }
        }
        8 => {
            s.set_i(0x5c, trunc(s.f(0x2c) * 1000.0 + now as f32));
            s.states(7, 2);
        }
        9 => {
            s.set_i(0x5c, trunc(s.f(0x28) * 1000.0 + now as f32));
            s.states(7, 4);
        }
        _ => {}
    }
}

/// `explode_then_flicker`: at the ninth parameter's intensity (default
/// 0.8), then once shown for the seventh's seconds, falls to the third's
/// over the eighth's, then `fire_flicker`.
fn explode_then_flicker(
    world: &mut World,
    mixer: &mut Mixer,
    side: usize,
    now: i32,
    active: bool,
    out: &mut Requests,
) {
    let mut state = mixer.sides[side].state;
    if state == 0 {
        let s = &mut mixer.sides[side];
        s.default_f(0x48, 0.8);
        s.default_f(0x44, 1.0);
        s.default_f(0x40, 2.0);
        s.default_f(0x30, 0.5);
        let start = s.f(0x48);
        apply(world, mixer, side, start, active, out);
        mixer.sides[side].state = 3;
        state = 3;
    }
    let s = &mut mixer.sides[side];
    if state == 5 {
        s.set_i(0x5c, now);
        s.set_i(0x60, trunc(s.f(0x44) * 1000.0));
        s.state = 6;
        state = 6;
    }
    match state {
        3 => {
            if active {
                s.set_i(0x60, trunc(s.f(0x40) * 1000.0 + now as f32));
                s.state = 4;
            }
        }
        4 => {
            if s.i(0x60) < now {
                s.state = 5;
            }
        }
        6 => {
            let frac = fraction(now, s.i(0x5c), s.i(0x60));
            let (to, from) = (s.f(0x30), s.f(0x48));
            apply(world, mixer, side, (to - from) * frac + from, active, out);
            if frac == 1.0 {
                mixer.sides[side].state = 7;
            }
        }
        7 => {
            s.state = 0;
            s.behavior = Some(Behavior::FireFlicker);
        }
        _ => {}
    }
}

fn run_behavior(
    world: &mut World,
    mixer: &mut Mixer,
    side: usize,
    now: i32,
    active: bool,
    out: &mut Requests,
) {
    let Some(behavior) = mixer.sides[side].behavior else {
        return;
    };
    match behavior {
        Behavior::Firecaster => firecaster(world, mixer, side, now, active, out),
        Behavior::FireFlicker => fire_flicker(world, mixer, side, now, active, out),
        Behavior::ElectricalFlicker => {
            flicker(world, mixer, side, now, active, out, [0.02, 0.1, 0.25]);
        }
        Behavior::FlickerThenStop => flicker_then_stop(world, mixer, side, now, active, out),
        Behavior::GenericDoubleStrobe => strobe(world, mixer, side, now, active, out, 0.05),
        Behavior::GenericStrobe => strobe(world, mixer, side, now, active, out, 0.1),
        Behavior::LightOff => light_off(world, mixer, side, active, out),
        Behavior::Television => television(world, mixer, side, now, active, out),
        Behavior::GenericFlickering => generic_flickering(world, mixer, side, now, active, out),
        Behavior::GenericPulsing => generic_pulsing(world, mixer, side, now, active, out),
        Behavior::ExplodeThenFlicker => {
            explode_then_flicker(world, mixer, side, now, active, out);
        }
    }
}

/// The `robot` mixer behavior: mixes again after a random period between
/// its first and second parameters (seconds).
fn robot(world: &mut World, mixer: &mut Mixer, now: i32) {
    let state = mixer.params[4] as u16;
    if state == 0 {
        let (low, high) = (
            f32::from_bits(mixer.params[0]),
            f32::from_bits(mixer.params[1]),
        );
        let wait = ftol(frand(world, low, high) * -1000.0);
        mixer.params[2] = now.wrapping_sub(wait) as u32;
        mixer.params[4] = (mixer.params[4] & 0xffff_0000) | 2;
    } else if state == 2 && (mixer.params[2] as i32) < now {
        if !mixer.mixing {
            mixer.mixing = true;
            mixer.mix_start = now;
        }
        mixer.params[4] &= 0xffff_0000;
    }
}

/// One mixer's frame.
fn frame(world: &mut World, mixer: &mut Mixer, now: i32, out: &mut Requests) {
    if mixer.robot {
        robot(world, mixer, now);
    }
    for side in 0..2 {
        let active = side == usize::from(mixer.position) && !mixer.mixing;
        run_behavior(world, mixer, side, now, active, out);
    }
    if !mixer.mixing {
        return;
    }
    let from = usize::from(mixer.position);
    let ramp = if from == 0 {
        mixer.left_to_right_ms
    } else {
        mixer.right_to_left_ms
    };
    let ramp_ms = trunc(ramp * 1000.0);
    let mut frac = 1.0f32;
    if ramp_ms == 0 {
        mixer.mixing = false;
    } else {
        let t = (now as f32 - mixer.mix_start as f32) / ramp_ms as f32;
        frac = t;
        if 0.0 <= t - 1.0 {
            frac = 1.0;
        }
        if -t < 0.0 {
            if frac == 1.0 {
                mixer.mixing = false;
            }
        } else {
            frac = 0.0;
        }
    }
    if from != 0 {
        frac = 1.0 - frac;
    }
    let [left, right] = &mixer.sides;
    let lerp = |at: usize| (right.f(at) - left.f(at)) * frac + left.f(at);
    let color = [
        lerp(COLOR),
        lerp(COLOR + 4),
        lerp(COLOR + 8),
        lerp(EXPONENT),
    ];
    let intensity = lerp(INTENSITY);
    let radius = lerp(RADIUS);
    let inner = lerp(FOV_INNER);
    let outer = lerp(FOV_OUTER);
    let leaving = mixer.sides[from].f(INTENSITY);
    if leaving <= EXPLODER_INTENSITY || EXPLODER_INTENSITY < intensity {
        if leaving <= EXPLODER_INTENSITY && EXPLODER_INTENSITY < intensity {
            exploder(mixer, true, out);
        }
    } else {
        exploder(mixer, false, out);
    }
    if let Some(light) = mixer.light
        && let Some(entry) = world.resource_mut::<ClientLights>().lights.get_mut(&light)
    {
        entry.color = pack(color);
        entry.intensity = if intensity < 0.0 { 0.0 } else { intensity };
        entry.radius = radius;
        entry.cos_half_fov_inner = (inner * 0.017_453_292 * 0.5).cos();
        entry.cos_half_fov_outer = (outer * 0.017_453_292 * 0.5).cos();
    }
    if !mixer.mixing {
        mixer.position = u8::from(mixer.position == 0);
        diag::info!(
            Sim,
            "csc: light mixer {:?} rests on side {} at {now} (intensity {:.2})",
            mixer.light,
            mixer.position,
            intensity
        );
    }
}

/// The mixer's post step: a rotating mixer starts the light's next full
/// yaw turn once the last is due and points the light along its entity's
/// angles, past its rotation limit.
fn rotate(world: &mut World, mixer: &mut Mixer, now: i32) {
    if mixer.rotate_seconds == 0.0 {
        return;
    }
    let Some(light) = mixer.light else {
        return;
    };
    let angles = match world
        .resource_mut::<RoundScript>()
        .object_field(light, "angles")
    {
        Value::Vector(angles) => angles,
        _ => [0.0; 3],
    };
    if mixer.rotate_next < now {
        mixer.rotate_next = now.wrapping_sub(trunc(mixer.rotate_seconds * -1000.0));
        let duration_ms = i64::from(trunc(mixer.rotate_seconds * 1000.0));
        if duration_ms > 0 {
            world
                .resource_mut::<crate::script::host::mechanics::Mechanics>()
                .start(
                    light,
                    crate::script::host::mechanics::Motion {
                        field: "angles",
                        path: crate::script::host::mechanics::MotionPath::Linear {
                            from: angles,
                            to: [angles[0], angles[1] + 360.0, angles[2]],
                            accel: 0.0,
                            decel: 0.0,
                        },
                        start_ms: i64::from(now),
                        duration_ms,
                        done: "",
                    },
                );
        }
    }
    let (forward, _, _) = math_iw4::angle_vectors(angles);
    if let Some(entry) = world.resource_mut::<ClientLights>().lights.get_mut(&light) {
        entry.direction = Some(forward.map(|v| -v));
    }
}

/// The client game's mixers run their frame: level notifies since the last
/// frame activate mixers of that event, then each mixer runs, then the
/// light loop exploders and sounds they asked for run.
pub(crate) fn run_frame(world: &mut World, now: i32) {
    let signals = crate::script::take_signals(world);
    let Some(mut mixers) = world.remove_resource::<Mixers>() else {
        return;
    };
    for signal in &signals {
        if signal.is_empty() {
            continue;
        }
        for mixer in &mut mixers.mixers {
            if *mixer.event == **signal && !mixer.mixing {
                mixer.mixing = true;
                mixer.mix_start = now;
            }
        }
    }
    let mut out = Requests::default();
    let before = light_values(world, &mixers);
    for mixer in &mut mixers.mixers {
        frame(world, mixer, now, &mut out);
        rotate(world, mixer, now);
    }
    if !mixers.reported {
        let after = light_values(world, &mixers);
        let changed = before.iter().zip(&after).filter(|(a, b)| a != b).count();
        if changed > 0 {
            mixers.reported = true;
            diag::info!(
                Sim,
                "csc: light mixers changed {changed} of {} lights at {now}",
                mixers.mixers.len()
            );
        }
    }
    world.insert_resource(mixers);
    for (alias, origin) in out.sounds {
        super::super::sound::play(world, &alias, Some(origin), 1.0, 0);
    }
    for (on, number) in out.exploders {
        let name = if on { PLAY_EXPLODER } else { STOP_EXPLODER };
        if let Err(fault) = crate::script::runtime::run_now(
            world,
            name,
            Value::level(),
            vec![Value::Int(number)],
            i64::from(now),
        ) {
            diag::warn!(Sim, "csc: {name}: {fault}");
        }
    }
}

/// Each mixer's light's color and intensity, as their bits.
fn light_values(world: &World, mixers: &Mixers) -> Vec<Option<([u8; 4], u32)>> {
    let lights = world.resource::<ClientLights>();
    mixers
        .mixers
        .iter()
        .map(|mixer| {
            let entry = lights.lights.get(&mixer.light?)?;
            Some((entry.color, entry.intensity.to_bits()))
        })
        .collect()
}

fn mixers(world: &mut World) -> bevy_ecs::prelude::Mut<'_, Mixers> {
    world.get_resource_or_insert_with(Mixers::default)
}

/// The mixer of a light entity `mixer_entity` checked.
fn mixer_of(world: &mut World, receiver: &Value) -> Result<usize, String> {
    let id = mixer_entity(world, receiver)?;
    let index = state(world).lights[&id].mixer.unwrap_or(1) - 1;
    Ok(index)
}

fn with_mixer<R>(world: &mut World, index: usize, f: impl FnOnce(&mut Mixer) -> R) -> R {
    f(&mut mixers(world).mixers[index])
}

/// Sets a mix's time one way, at least 0.
fn ramp_speed(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    right_to_left: bool,
) -> Result<Value, String> {
    let index = mixer_of(world, receiver)?;
    let seconds = float(args, 0)?;
    let seconds = if 0.0 <= seconds { seconds } else { 0.0 };
    with_mixer(world, index, |mixer| {
        if right_to_left {
            mixer.right_to_left_ms = seconds;
        } else {
            mixer.left_to_right_ms = seconds;
        }
    });
    Ok(Value::Undefined)
}

/// A parameter as the engine stores it: undefined as `-FLT_MAX`, a float or
/// int as its bits, a string as a nonzero id (its alias kept).
fn param(args: &[Value], index: usize) -> (u32, Option<String>) {
    match arg(args, index) {
        Ok(Value::Float(v)) => (v.to_bits(), None),
        Ok(Value::Int(v)) => (*v as u32, None),
        Ok(Value::String(s)) => (1, Some(s.to_string())),
        Ok(Value::LocalizedString(s)) => (1, Some(s.to_string())),
        _ => (UNDEFINED.to_bits(), None),
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};

    // allocatemixerlights(count): the client's mixer table for `count`
    // mixers, empty.
    registry.register(Function, "allocatemixerlights", |world, _, args| {
        let count = int(args, 0)?;
        let mut table = mixers(world);
        table.mixers.clear();
        table.allocated = count.max(0) as usize;
        Ok(Value::Undefined)
    });

    // ismixerlightbehaviorhardcoded(name): whether `name` is one of the
    // engine's 12 light behaviors.
    registry.register(Function, "ismixerlightbehaviorhardcoded", |_, _, args| {
        let name = string(args, 0)?;
        Ok(Value::Int(Behavior::named(&name).is_some().into()))
    });

    // ent makemixerlight(event): a new mixer on the light entity, activated
    // by the level notify `event`.
    registry.register(Method, "makemixerlight", |world, receiver, args| {
        let id = mixer_light_entity(world, receiver)?;
        let event = string(args, 0)?;
        let mut table = mixers(world);
        if table.mixers.len() >= table.allocated {
            return Err(format!(
                "t6.csc.light.mixer_table: more mixers than allocatemixerlights({}) allocated",
                table.allocated
            ));
        }
        table.mixers.push(Mixer {
            light: Some(id),
            event,
            ..Default::default()
        });
        let index = table.mixers.len();
        if let Some(light) = state(world).lights.get_mut(&id) {
            light.mixer = Some(index);
        }
        Ok(Value::Undefined)
    });

    // ent activatemixer(): the mixer starts mixing now, unless it is.
    registry.register(Method, "activatemixer", |world, receiver, _| {
        let index = mixer_of(world, receiver)?;
        let now = super::now_ms(world) as i32;
        with_mixer(world, index, |mixer| {
            if !mixer.mixing {
                mixer.mixing = true;
                mixer.mix_start = now;
            }
        });
        Ok(Value::Undefined)
    });

    // ent setmixerbehavior(name): `robot` gives the mixer its behavior;
    // other names do nothing.
    registry.register(Method, "setmixerbehavior", |world, receiver, args| {
        let index = mixer_of(world, receiver)?;
        if string(args, 0)? == "robot" {
            with_mixer(world, index, |mixer| mixer.robot = true);
        }
        Ok(Value::Undefined)
    });

    // ent setmixerexploderid(id): the light loop exploder the mixer runs.
    registry.register(Method, "setmixerexploderid", |world, receiver, args| {
        let index = mixer_of(world, receiver)?;
        let id = int(args, 0)?;
        with_mixer(world, index, |mixer| mixer.exploder = id);
        Ok(Value::Undefined)
    });

    // ent setmixerl2rrampspeed(seconds) / setmixerr2lrampspeed(seconds):
    // the mix's time each way, at least 0.
    registry.register(Method, "setmixerl2rrampspeed", |world, receiver, args| {
        ramp_speed(world, receiver, args, false)
    });
    registry.register(Method, "setmixerr2lrampspeed", |world, receiver, args| {
        ramp_speed(world, receiver, args, true)
    });

    // ent setmixerlightrotatetime(seconds): turns the light a full turn
    // every `seconds`.
    registry.register(
        Method,
        "setmixerlightrotatetime",
        |world, receiver, args| {
            let index = mixer_of(world, receiver)?;
            let seconds = float(args, 0)?;
            with_mixer(world, index, |mixer| mixer.rotate_seconds = seconds);
            Ok(Value::Undefined)
        },
    );

    // ent setmixerlightbehavior(side, name): a side's hardcoded behavior,
    // from its first state.
    registry.register(Method, "setmixerlightbehavior", |world, receiver, args| {
        let index = mixer_of(world, receiver)?;
        let side = mixer_side(args, 0)?;
        let name = string(args, 1)?;
        let Some(behavior) = Behavior::named(&name) else {
            return Err(format!("Light behavior '{name}' unknown."));
        };
        with_mixer(world, index, |mixer| {
            let s = &mut mixer.sides[side];
            s.behavior = behavior;
            s.state = 0;
        });
        Ok(Value::Undefined)
    });

    // ent setmixerlightcolor(side, rgb).
    registry.register(Method, "setmixerlightcolor", |world, receiver, args| {
        let index = mixer_of(world, receiver)?;
        let side = mixer_side(args, 0)?;
        let rgb = vector(args, 1)?;
        with_mixer(world, index, |mixer| {
            for (k, channel) in rgb.into_iter().enumerate() {
                mixer.sides[side].set_f(COLOR + 4 * k, channel);
            }
        });
        Ok(Value::Undefined)
    });

    // ent setmixerlightexponent(side, exponent): held to 0..100.
    registry.register(Method, "setmixerlightexponent", |world, receiver, args| {
        let index = mixer_of(world, receiver)?;
        let side = mixer_side(args, 0)?;
        let exponent = int(args, 1)?.clamp(0, 100);
        with_mixer(world, index, |mixer| {
            mixer.sides[side].set_f(EXPONENT, exponent as f32);
        });
        Ok(Value::Undefined)
    });

    // ent setmixerlightfovrange(side, outer, [inner]) in degrees: without an
    // inner angle, the side's is kept unless it is at least the outer's plus
    // 0.001, which it then becomes.
    registry.register(Method, "setmixerlightfovrange", |world, receiver, args| {
        let index = mixer_of(world, receiver)?;
        let side = mixer_side(args, 0)?;
        let outer = float(args, 1)?;
        let inner = if args.len() == 3 {
            let inner = float(args, 2)?;
            if !(inner >= -0.001_000_000_05 && inner < outer + 0.001_000_000_05) {
                return Err("inner fov must be in the range of 0 to outer fov".into());
            }
            Some(inner)
        } else {
            None
        };
        with_mixer(world, index, |mixer| {
            let s = &mut mixer.sides[side];
            let inner = inner.unwrap_or_else(|| {
                let kept = s.f(FOV_INNER);
                if kept - (outer + 0.001) < 0.0 {
                    kept
                } else {
                    outer + 0.001
                }
            });
            s.set_f(FOV_OUTER, outer);
            s.set_f(FOV_INNER, inner);
        });
        Ok(Value::Undefined)
    });

    // ent setmixerlightintensity(side, intensity): at least 0.
    registry.register(Method, "setmixerlightintensity", |world, receiver, args| {
        let index = mixer_of(world, receiver)?;
        let side = mixer_side(args, 0)?;
        let value = float(args, 1)?;
        let value = if 0.0 <= value { value } else { 0.0 };
        with_mixer(world, index, |mixer| {
            mixer.sides[side].set_f(INTENSITY, value)
        });
        Ok(Value::Undefined)
    });

    // ent setmixerlightradius(side, radius): 0 when not positive.
    registry.register(Method, "setmixerlightradius", |world, receiver, args| {
        let index = mixer_of(world, receiver)?;
        let side = mixer_side(args, 0)?;
        let value = float(args, 1)?;
        let value = if 0.0 <= -value { 0.0 } else { value };
        with_mixer(world, index, |mixer| mixer.sides[side].set_f(RADIUS, value));
        Ok(Value::Undefined)
    });

    // ent setmixerlightparam(side, index, value): a side behavior's
    // parameter `0..=16`.
    registry.register(Method, "setmixerlightparam", |world, receiver, args| {
        let index = mixer_of(world, receiver)?;
        let side = mixer_side(args, 0)?;
        let number = int(args, 1)?;
        if !(0..17).contains(&number) {
            return Err(
                "Please ensure light logic param number specified is in range 0 - 16\n".into(),
            );
        }
        check_mixer_param_value(
            args,
            2,
            "Light logic parameter specified is of an invalid type.",
        )?;
        let (bits, alias) = param(args, 2);
        let number = number as usize;
        with_mixer(world, index, |mixer| {
            let s = &mut mixer.sides[side];
            s.words[(PARAMS + 4 * number) / 4 - 1] = bits;
            s.aliases[number] = alias;
        });
        Ok(Value::Undefined)
    });

    // ent setmixerparam(index, value): the mixer behavior's parameter
    // `0..=4`.
    registry.register(Method, "setmixerparam", |world, receiver, args| {
        let index = mixer_of(world, receiver)?;
        let number = int(args, 0)?;
        if !(0..5).contains(&number) {
            return Err(
                "Please ensure light logic param number specified is in range 0 - 4\n".into(),
            );
        }
        check_mixer_param_value(
            args,
            1,
            "Mixer logic parameter specified is of an invalid type.",
        )?;
        let (bits, _) = param(args, 1);
        with_mixer(world, index, |mixer| mixer.params[number as usize] = bits);
        Ok(Value::Undefined)
    });

    // ent setmixerlightmodels(model_ent, on_model, off_model).
    registry.register(Method, "setmixerlightmodels", |world, receiver, args| {
        mixer_of(world, receiver)?;
        super::entity_arg(world, args, 0)?;
        string(args, 1)?;
        string(args, 2)?;
        // t6.csc.light.mixer_models: the model entity's model does not
        // follow the light's intensity.
        Ok(Value::Undefined)
    });

    // ent setmixerspinmodels(model_ent, on_model, off_model, [fx_id, tag]).
    registry.register(Method, "setmixerspinmodels", |world, receiver, args| {
        mixer_of(world, receiver)?;
        super::entity_arg(world, args, 0)?;
        string(args, 1)?;
        string(args, 2)?;
        if !matches!(args.get(3), None | Some(Value::Undefined)) {
            int(args, 3)?;
            string(args, 4)?;
        }
        // t6.csc.light.mixer_models: no spinning models are spawned.
        Ok(Value::Undefined)
    });
}
