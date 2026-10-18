//! Black Ops' vision channels: ten visions a client holds at once (naked,
//! night, flare, infrared, missile camera, extra camera, last stand, damage,
//! mission failed, underwater), each blending from the vision it showed to
//! the one a script asked for, and the one a frame draws with picked by
//! priority from the client's state.

use crate::vision_file::{FieldKind, VISION_FIELDS, VisionSlot, read_vision};
use game_api::Unknown;
use game_api::post::{VisionCall, VisionChannels, VisionFiles, VisionView};
use game_api::unknown;

const NAKED: usize = 0;
const FLARE: usize = 2;
const INFRARED: usize = 3;
const MISSILE_CAMERA: usize = 4;
const EXTRA_CAMERA: usize = 5;
const LAST_STAND: usize = 6;
const DAMAGE: usize = 7;
const MISSION_FAILED: usize = 8;
const UNDERWATER: usize = 9;
const CHANNELS: usize = 10;

/// Visions held parsed, by name.
const CACHED: usize = 8;

/// Name buffers hold 64 bytes, the terminator included.
const NAME_BYTES: usize = 0x40;

/// How a channel moves to its new vision.
const STYLE_NONE: i32 = 0;
const STYLE_DONE: i32 = 1;
/// Every script call blends in with the sine ease.
const STYLE_EASE: i32 = 3;

const HALF_PI: f32 = f32::from_bits(0x3fc9_0fdb);

static GAPS: [&Unknown; 1] = [unknown!(
    "t5.vision.special_channels",
    "the channels Black Ops picks from its client state ahead of or after last stand and damage: flare (the player's vision lerp ratio), mission failed (the server's mission-failed command), underwater (the water check), infrared, missile camera, extra camera and night vision (weapon states); none of their conditions is passed in, so a frame never picks them",
    "the player state's vision lerp ratio and weapon states, the mission-failed command and the water check reaching the renderer"
)];

#[derive(Clone, Copy, Debug, Default)]
struct Blend {
    start: i32,
    duration: i32,
    style: i32,
}

#[derive(Clone, Copy, Debug, Default)]
struct Channel {
    from: VisionSlot,
    to: VisionSlot,
    result: VisionSlot,
    blend: Blend,
}

/// One client's channels and its vision cache.
pub(crate) struct Channels {
    started: bool,
    cache: [(String, VisionSlot); CACHED],
    channels: [Channel; CHANNELS],
    names: [String; CHANNELS],
    /// Client time the damage channel is picked until.
    damage_until: i32,
    /// The vision the last frame drew with, and its channel.
    presented: VisionSlot,
    picked: usize,
    messages: Vec<String>,
}

pub(crate) fn new_channels() -> Box<dyn VisionChannels> {
    Box::new(Channels {
        started: false,
        cache: Default::default(),
        channels: [Channel::default(); CHANNELS],
        names: Default::default(),
        damage_until: 0,
        presented: VisionSlot::default(),
        picked: NAKED,
        messages: Vec::new(),
    })
}

/// A string as a 64-byte name buffer keeps it.
fn bounded(text: &str) -> String {
    let mut end = text.len().min(NAME_BYTES - 1);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

impl Channels {
    /// The vision `name` names, through the cache: a vision not cached is
    /// read into the first free slot (the first slot when all are taken),
    /// over what that slot held; a missing file reads `default` in its
    /// place, cached under the name asked for.
    fn load(&mut self, name: &str, files: &dyn VisionFiles) -> Option<VisionSlot> {
        if name.is_empty() {
            return None;
        }
        if let Some((_, vision)) = self
            .cache
            .iter()
            .find(|(cached, _)| cached.eq_ignore_ascii_case(name))
        {
            return Some(*vision);
        }
        let index = self
            .cache
            .iter()
            .position(|(cached, _)| cached.is_empty())
            .unwrap_or(0);
        let path = bounded(&format!("vision/{name}.vision"));
        let text = match files.text(&path.to_ascii_lowercase()) {
            Some(text) => text,
            None => {
                self.messages.push(format!("couldn't open '{path}'."));
                let default = "vision/default.vision";
                let Some(text) = files.text(default) else {
                    self.messages.push(format!(
                        "couldn't open '{default}'. This is a default file that you should have."
                    ));
                    return None;
                };
                text
            }
        };
        let slot = &mut self.cache[index];
        self.messages.extend(read_vision(text, &path, &mut slot.1));
        slot.0 = bounded(name);
        Some(slot.1)
    }

    /// Points `channel` at the vision `name` names. With a duration, a
    /// channel already showing a vision blends from what it shows now;
    /// otherwise it shows the new vision at once. False when it does not
    /// load, the channel left as it was.
    fn set(
        &mut self,
        channel: usize,
        style: i32,
        name: &str,
        duration_ms: i32,
        now: i32,
        files: &dyn VisionFiles,
    ) -> bool {
        let blends = duration_ms > 0 && self.channels[channel].blend.style != STYLE_NONE;
        let Some(vision) = self.load(name, files) else {
            return false;
        };
        let target = &mut self.channels[channel];
        if blends {
            target.to = vision;
            target.from = target.result;
            target.blend = Blend {
                start: now,
                duration: duration_ms,
                style,
            };
        } else {
            target.result = vision;
            target.blend.style = STYLE_DONE;
        }
        true
    }

    /// What a client starts with: its flare, infrared and missile-camera
    /// visions and its underwater vision read, the other channels named.
    fn start(&mut self, files: &dyn VisionFiles) {
        self.started = true;
        for (channel, name) in [
            (INFRARED, "infrared"),
            (MISSILE_CAMERA, "tvguided_sp"),
            (EXTRA_CAMERA, ""),
            (FLARE, "flare"),
        ] {
            self.names[channel] = name.to_owned();
            self.preload(channel, files);
        }
        for (channel, name) in [
            (LAST_STAND, "laststand"),
            (DAMAGE, "death"),
            (MISSION_FAILED, "low_health"),
            (UNDERWATER, "creek_1_water"),
        ] {
            self.names[channel] = name.to_owned();
        }
        self.preload(UNDERWATER, files);
    }

    fn preload(&mut self, channel: usize, files: &dyn VisionFiles) {
        let name = self.names[channel].clone();
        if let Some(vision) = self.load(&name, files) {
            self.channels[channel].result = vision;
            self.channels[channel].blend.style = STYLE_DONE;
        }
    }

    fn pick(&self, view: VisionView) -> usize {
        if view.last_stand {
            LAST_STAND
        } else if view.now_ms < self.damage_until {
            DAMAGE
        } else {
            NAKED
        }
    }
}

/// One float of a blend: `style` 2 and 6 straight, 3 eased by a quarter
/// sine, 4 out and back, 5 eased out and back.
fn blend_float(style: i32, from: f32, to: f32, t: f32) -> f32 {
    let eased = || (f64::from(t * HALF_PI)).sin() as f32;
    match style {
        2 | 6 => (to - from) * t + from,
        3 => (to - from) * eased() + from,
        5 => out_and_back(from, to, eased()),
        _ => out_and_back(from, to, t),
    }
}

fn out_and_back(from: f32, to: f32, t: f32) -> f32 {
    if t < 0.5 {
        (to - from) * t * 2.0 + from
    } else {
        (from - to) * (t - 0.5) * 2.0 + to
    }
}

/// Moves `channel` on to `now`: past its end it shows its target (or, out
/// and back, its start again) and stops.
fn advance(channel: &mut Channel, now: i32) {
    let Blend {
        start,
        duration,
        style,
    } = channel.blend;
    if style == STYLE_NONE || style == STYLE_DONE {
        return;
    }
    if start.wrapping_add(duration) < now {
        channel.result = if (style - 2) as u32 <= 1 {
            channel.to
        } else {
            channel.from
        };
        channel.blend.style = STYLE_DONE;
        return;
    }
    let raw = now.wrapping_sub(start) as f32 / duration as f32;
    let mut t = if raw - 1.0 >= 0.0 { 1.0 } else { raw };
    if -raw >= 0.0 {
        t = 0.0;
    }
    let returning = (style - 4) as u32 <= 1 && t >= 0.5;
    let (from, to) = (&channel.from.words, &channel.to.words);
    let result = &mut channel.result.words;
    for &(_, offset, kind) in &VISION_FIELDS {
        let at = offset / 4;
        match kind {
            FieldKind::Bool => {
                let source = if returning { from[at] } else { to[at] };
                result[at] = (result[at] & !0xff) | (source & 0xff);
            }
            FieldKind::Lut => result[at] = if returning { from[at] } else { to[at] },
            FieldKind::Int => result[at] = to[at],
            FieldKind::Float | FieldKind::Vec3 | FieldKind::Vec4 => {
                let count = match kind {
                    FieldKind::Float => 1,
                    FieldKind::Vec3 => 3,
                    _ => 4,
                };
                for i in at..at + count {
                    result[i] =
                        blend_float(style, f32::from_bits(from[i]), f32::from_bits(to[i]), t)
                            .to_bits();
                }
            }
        }
    }
}

impl VisionChannels for Channels {
    fn call(&mut self, call: &VisionCall, now_ms: i32, files: &dyn VisionFiles) {
        if !self.started {
            self.start(files);
        }
        match call {
            VisionCall::ServerNaked { name, duration_ms } => {
                self.names[NAKED] = bounded(name);
                let name = self.names[NAKED].clone();
                self.set(NAKED, STYLE_EASE, &name, *duration_ms, now_ms, files);
            }
            VisionCall::ServerLastStand { name, duration_ms } => {
                self.names[LAST_STAND] = bounded(name);
                let name = self.names[LAST_STAND].clone();
                self.set(LAST_STAND, STYLE_EASE, &name, *duration_ms, now_ms, files);
            }
            VisionCall::ClientNaked { name, duration_ms } => {
                self.set(NAKED, STYLE_EASE, name, *duration_ms, now_ms, files);
            }
            // The script's name lands in the last-stand name buffer; the
            // channel itself blends to the mission-failed channel's name
            // (`low_health`) on, and back to the naked vision off.
            VisionCall::ClientDamage {
                on,
                name,
                duration_ms,
            } => {
                let target = if *on {
                    self.damage_until = i32::MAX;
                    self.names[LAST_STAND] = bounded(name);
                    self.names[MISSION_FAILED].clone()
                } else {
                    self.damage_until = now_ms.wrapping_add(*duration_ms);
                    self.names[LAST_STAND] = self.names[NAKED].clone();
                    self.names[NAKED].clone()
                };
                self.set(DAMAGE, STYLE_EASE, &target, *duration_ms, now_ms, files);
            }
            VisionCall::ClientUnderwater { name, duration_ms } => {
                if self.set(UNDERWATER, STYLE_EASE, name, *duration_ms, now_ms, files) {
                    self.names[UNDERWATER] = bounded(name);
                }
            }
        }
    }

    fn shellshock(
        &mut self,
        vision: Option<&str>,
        duration_ms: i32,
        now_ms: i32,
        files: &dyn VisionFiles,
    ) {
        if !self.started {
            self.start(files);
        }
        let name = vision.map_or_else(|| self.names[NAKED].clone(), str::to_owned);
        self.set(NAKED, STYLE_EASE, &name, duration_ms, now_ms, files);
    }

    fn frame(&mut self, view: VisionView, files: &dyn VisionFiles) -> &[u32] {
        if !self.started {
            self.start(files);
        }
        if self.names[NAKED].is_empty() {
            self.names[NAKED] = "default".to_owned();
            self.preload(NAKED, files);
        }
        for channel in &mut self.channels {
            advance(channel, view.now_ms);
        }
        self.picked = self.pick(view);
        let picked = &self.channels[self.picked];
        if picked.blend.style != STYLE_NONE {
            self.presented = picked.result;
        } else {
            // Film, revive effect and emissive brightening off; the rest
            // stays as the last frame drew it.
            self.presented.words[0] &= !0xff;
            self.presented.words[0x78 / 4] &= !0xff;
            self.presented.words[0xe8 / 4] = 0;
        }
        &self.presented.words
    }

    fn take_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.messages)
    }

    fn picked(&self) -> (usize, &str) {
        (self.picked, &self.names[self.picked])
    }

    fn gaps(&self) -> &'static [&'static Unknown] {
        &GAPS
    }
}
