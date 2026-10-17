//! Black Ops 2's frame fog, as its client game builds it each frame and its
//! renderer turns it into the fog code constants.
//!
//! A frame's fog is a block of 20 words: two unused, the fog colour (already
//! scaled by its colour scale) and maximum opacity, the start distance, the
//! density (1 over the halfway distance), the height density (1 over the
//! halfway height), the base height, the sun fog colour (scaled) and its
//! opacity, the sun direction, the sun fog's start and end angles, and the
//! maximum density.
//!
//! A map with fog volumes draws them, and its script fog is not drawn: each
//! frame the camera's fog modifier volume (the first whose bank bits, in the
//! low 4 bits of the active bank mask, it is inside) scales the fog, and its
//! fog volume (the first in the active bank it is inside, else the map's
//! default fog) is the fog, both blended from the last ones over a
//! smoothstep at the target's rate. A view cut, or a camera jump of over
//! 1024 units, snaps both.
//!
//! A map without fog volumes draws its script fog: two slots per client (0
//! is no fog, 1 the fog the server's `setvolfog` or the client's scripts
//! set). A switch blends from the fog the last frame drew (or snaps when it
//! drew none) to the slot's, linearly over the transition, the sun
//! direction renormalized.

use game_api::fog::{FogFrame, FogRows, FrameFog, FrameFogRules, ScriptFogSwitch, WorldFogData};

pub const FRAME_FOG: FrameFogRules = FrameFogRules { start };

const DEG_TO_RAD: f32 = f32::from_bits(0x3c8e_fa35);
/// The sun fog slope when its start and end angles meet.
const FLAT_SLOPE: f32 = 10_000_000.0;
/// The world fog bank mask the client game sets for a map.
const INITIAL_BANK: u16 = 1;
/// A camera that moves further than this in a frame snaps the world fog.
const JUMP_SQ: f32 = 1_048_576.0;

/// A block word's index.
mod at {
    pub const COLOR: usize = 2;
    pub const COLOR_SCALE: usize = 5;
    pub const START: usize = 6;
    pub const DENSITY: usize = 7;
    pub const HEIGHT_DENSITY: usize = 8;
    pub const BASE_HEIGHT: usize = 9;
    pub const SUN_COLOR: usize = 10;
    pub const OPACITY: usize = 13;
    pub const SUN_DIR: usize = 14;
    pub const START_ANGLE: usize = 17;
    pub const END_ANGLE: usize = 18;
    pub const MAX_DENSITY: usize = 19;
}

pub type FogBlock = [f32; 20];

fn start(world: Option<&WorldFogData>) -> Box<dyn FrameFog> {
    Box::new(ClientFog {
        world: world.filter(|w| !w.volumes.is_empty()).cloned(),
        script: ScriptFog::default(),
        volumes: None,
        serial: None,
    })
}

struct ClientFog {
    /// The map's fog volumes, when it has any.
    world: Option<WorldFogData>,
    script: ScriptFog,
    /// The world fog's state, from the first frame.
    volumes: Option<VolumeFog>,
    /// The last script fog switch run.
    serial: Option<u32>,
}

impl FrameFog for ClientFog {
    fn frame(&mut self, frame: &FogFrame) -> FogRows {
        if let Some(switch) = frame.script
            && self.serial != Some(switch.serial)
        {
            self.serial = Some(switch.serial);
            self.script.switch(&switch);
        }
        let block = match &self.world {
            Some(world) => {
                let state = self
                    .volumes
                    .get_or_insert_with(|| VolumeFog::new(frame.eye));
                state.frame(world, frame)
            }
            None => self.script.frame(frame.time_ms),
        };
        rows(&block, frame.eye[2])
    }
}

/// A client's script fog: its slots' last switch and the fog drawn.
#[derive(Default)]
struct ScriptFog {
    slot: u8,
    from: FogBlock,
    to: FogBlock,
    current: FogBlock,
    start_ms: i32,
    end_ms: i32,
}

impl ScriptFog {
    fn switch(&mut self, switch: &ScriptFogSwitch) {
        let mut transition = switch.transition_ms;
        self.slot = switch.slot;
        self.from = if self.current[at::DENSITY] == 0.0 {
            transition = 0;
            switch.block
        } else {
            self.current
        };
        self.to = switch.block;
        if transition == 0 {
            self.start_ms = 0;
            self.end_ms = 0;
        } else {
            self.start_ms = switch.start_ms;
            self.end_ms = switch.start_ms.wrapping_add(transition);
        }
    }

    fn frame(&mut self, now_ms: i32) -> FogBlock {
        if now_ms < self.end_ms {
            let duration = self.end_ms.wrapping_sub(self.start_ms).max(1);
            let f = (now_ms.wrapping_sub(self.start_ms) as f32 / duration as f32).min(1.0);
            for i in at::COLOR..20 {
                self.current[i] = (self.to[i] - self.from[i]) * f + self.from[i];
            }
            let dir = &mut self.current[at::SUN_DIR..at::SUN_DIR + 3];
            let mut len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
            if -len >= 0.0 {
                len = 1.0;
            }
            let inv = 1.0 / len;
            for v in dir {
                *v *= inv;
            }
        } else {
            self.current = self.to;
        }
        let scale = self.current[at::COLOR_SCALE];
        for i in (at::COLOR..at::COLOR + 3).chain(at::SUN_COLOR..at::SUN_COLOR + 3) {
            self.current[i] *= scale;
        }
        self.current[at::COLOR_SCALE] = self.current[at::OPACITY];
        if self.slot == 0 {
            [0.0; 20]
        } else {
            self.current
        }
    }
}

/// The world fog's state between frames.
struct VolumeFog {
    eye: [f32; 3],
    modifier: Option<usize>,
    modifier_start_ms: i32,
    modifier_rate: f32,
    modifier_from: [f32; 6],
    scales: [f32; 6],
    volume: Option<usize>,
    volume_start_ms: i32,
    from: [f32; 16],
    current: [f32; 16],
}

impl VolumeFog {
    /// As the client game sets it up for a map.
    fn new(eye: [f32; 3]) -> Self {
        Self {
            eye,
            modifier: None,
            modifier_start_ms: 0,
            modifier_rate: 1.0,
            modifier_from: [1.0; 6],
            scales: [1.0; 6],
            volume: None,
            volume_start_ms: 0,
            from: [0.0; 16],
            current: [0.0; 16],
        }
    }

    fn frame(&mut self, world: &WorldFogData, frame: &FogFrame) -> FogBlock {
        let now = frame.time_ms;
        let eye = frame.eye;
        let moved: [f32; 3] = std::array::from_fn(|i| eye[i] - self.eye[i]);
        self.eye = eye;
        let snap = frame.view_cut
            || JUMP_SQ < moved[0] * moved[0] + moved[1] * moved[1] + moved[2] * moved[2];
        self.run_modifier(world, frame, snap);
        self.find_volume(world, frame, snap);

        let (fog, rate) = match self.volume {
            Some(v) => (&world.volumes[v].fog, world.volumes[v].rate),
            None => (&world.default_fog, world.default_rate),
        };
        if frame.view_cut {
            self.current = *fog;
            self.from = *fog;
        } else {
            let f = smoothstep(now.wrapping_sub(self.volume_start_ms) as f32 * rate);
            for ((current, to), from) in self.current.iter_mut().zip(fog).zip(&self.from) {
                *current = (to - from) * f + from;
            }
        }
        let c = &self.current;
        let s = &self.scales;
        let mut block = [0.0; 20];
        block[at::START] = c[0];
        let halfway = s[0] * c[1];
        block[at::DENSITY] = if 1.0 <= halfway { 1.0 / halfway } else { 1.0 };
        block[at::BASE_HEIGHT] = c[2];
        block[at::HEIGHT_DENSITY] = if 1.0 <= c[3] { 1.0 / c[3] } else { 1.0 };
        for i in 0..4 {
            block[at::COLOR + i] = s[2 + i] * c[8 + i];
            block[at::SUN_COLOR + i] = c[12 + i] * s[2 + i];
        }
        let pitch = f64::from(c[4] * DEG_TO_RAD);
        let yaw = f64::from(c[5] * DEG_TO_RAD);
        let (cp, sp) = (pitch.cos() as f32, pitch.sin() as f32);
        let (cy, sy) = (yaw.cos() as f32, yaw.sin() as f32);
        block[at::SUN_DIR] = cy * cp;
        block[at::SUN_DIR + 1] = sy * cp;
        block[at::SUN_DIR + 2] = -sp;
        block[at::START_ANGLE] = c[6];
        block[at::END_ANGLE] = c[7];
        block[at::MAX_DENSITY] = 1.0;
        block
    }

    fn run_modifier(&mut self, world: &WorldFogData, frame: &FogFrame, snap: bool) {
        let now = frame.time_ms;
        let found = first_inside(
            world.modifiers.iter().map(|m| m.packed),
            &world.modifier_planes,
            frame.eye,
            u32::from(frame.bank.unwrap_or(INITIAL_BANK) & 0xf),
        );
        let target = found.map_or([1.0; 6], |m| world.modifiers[m].scales);
        if snap {
            self.modifier = found;
            self.modifier_start_ms = now;
            self.modifier_rate = found.map_or(1.0, |m| world.modifiers[m].rate);
            self.modifier_from = target;
            self.scales = target;
            return;
        }
        let s = smoothstep(now.wrapping_sub(self.modifier_start_ms) as f32 * self.modifier_rate);
        let inv = 1.0 - s;
        let blended: [f32; 6] = match self.modifier {
            None => std::array::from_fn(|i| inv * self.modifier_from[i] + s),
            Some(m) => {
                let to = world.modifiers[m].scales;
                std::array::from_fn(|i| to[i] * s + self.modifier_from[i] * inv)
            }
        };
        if found != self.modifier {
            self.modifier_start_ms = now;
            self.modifier = found;
            if let Some(m) = found {
                self.modifier_rate = world.modifiers[m].rate;
            }
            self.modifier_from = blended;
        }
        self.scales = blended;
    }

    fn find_volume(&mut self, world: &WorldFogData, frame: &FogFrame, snap: bool) {
        let found = first_inside(
            world.volumes.iter().map(|v| v.packed),
            &world.volume_planes,
            frame.eye,
            u32::from(frame.bank.unwrap_or(INITIAL_BANK)),
        );
        if snap || found != self.volume {
            diag::info!(
                World,
                "t6 world fog: volume {found:?} (was {:?}) bank {:?} snap={snap}",
                self.volume,
                frame.bank
            );
        }
        if snap {
            let fog = found.map_or(world.default_fog, |v| world.volumes[v].fog);
            self.volume_start_ms = frame.time_ms;
            self.current = fog;
            self.from = fog;
        } else if found != self.volume {
            self.volume_start_ms = frame.time_ms;
            self.from = self.current;
        }
        self.volume = found;
    }
}

/// `(f·-2 + 3)·f²` of the fraction, at most 1.
fn smoothstep(f: f32) -> f32 {
    let f = f.min(1.0);
    (f * -2.0 + 3.0) * f * f
}

/// The first of the packed volumes in `bank` that `eye` is inside of: on
/// no plane's front side.
fn first_inside(
    packed: impl Iterator<Item = u32>,
    planes: &[[f32; 4]],
    eye: [f32; 3],
    bank: u32,
) -> Option<usize> {
    packed.enumerate().position(|(_, word)| {
        word & bank != 0
            && (0..((word >> 8) & 0xff) + 1).all(|j| {
                planes
                    .get(((word >> 16) + j) as usize)
                    .is_some_and(|p| p[1] * eye[1] + p[0] * eye[0] + p[2] * eye[2] + p[3] <= 0.0)
            })
    })
}

/// The fog code constant rows the renderer sets from a frame's block at
/// the eye's height.
#[must_use]
pub fn rows(block: &FogBlock, eye_z: f32) -> FogRows {
    let b = block;
    let color = [b[2], b[3], b[4], b[5]];
    let sun_dir = [b[14], b[15], b[16], 0.0];
    let sun_color = [b[10], b[11], b[12], b[13]];
    let start = (b[at::START_ANGLE] * DEG_TO_RAD).cos();
    let end = (b[at::END_ANGLE] * DEG_TO_RAD).cos();
    let diff = start - end;
    let slope = if diff == 0.0 { FLAT_SLOPE } else { 1.0 / diff };
    let angles = [-(end * slope), slope, 0.0, 0.0];
    let density = b[at::DENSITY];
    let (fog, fog2) = if density == 0.0 {
        ([0.0; 4], [0.0; 4])
    } else {
        let mut max_density = b[at::MAX_DENSITY];
        if density * 100.0 < max_density {
            max_density = density * 100.0;
        }
        if max_density < density {
            max_density = density;
        }
        let ln2 = 2.0f64.ln() as f32;
        let height = b[at::HEIGHT_DENSITY];
        let x = (density / max_density).ln() - height * (eye_z - b[at::BASE_HEIGHT]) * ln2;
        let integral = if x < 0.0 { x.exp() } else { x + 1.0 };
        let scale = if height > 0.0 {
            -max_density * ln2
        } else {
            -max_density
        };
        (
            [x, scale, b[at::START] * density, -(height * ln2)],
            [integral, 0.0, 0.0, 0.0],
        )
    };
    [color, fog, fog2, sun_dir, sun_color, angles]
}
