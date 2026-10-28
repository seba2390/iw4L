pub const FX_RANDOM_VERSION: u32 = 2;

#[derive(Copy, Clone, Debug)]
#[repr(u32)]
pub enum FxRandomChannel {
    AngularPitch = 1,
    AngularRoll = 2,
    AngularYaw = 3,
    Atlas = 4,
    Color = 5,
    Delay = 6,
    EmitDistance = 7,
    GlassCrack = 8,
    GlassShatter = 9,
    Gravity = 10,
    InitialRotation = 11,
    Life = 12,
    OneShotCount = 13,
    Reflection = 14,
    RotationDelta = 15,
    Scale = 16,
    Size = 17,
    SpawnAngleYaw = 18,
    SpawnHeight = 19,
    SpawnOriginX = 20,
    SpawnOriginY = 21,
    SpawnOriginZ = 22,
    SpawnPitch = 23,
    SpawnRadius = 24,
    SpawnRoll = 25,
    SpawnYaw = 26,
    VelocityX = 27,
    VelocityY = 28,
    VelocityZ = 29,
    Visual = 30,
    /// A non-uniform sprite's height, drawn apart from its width.
    Size1 = 31,
}

// Each draw is a keyed integer mix, not a cryptographic digest: the FX
// update and draw take several samples per live element per frame, and a
// BLAKE3 digest per sample was the hottest leaf of a Kino frame. The domain
// constants keep the four kinds of key apart; bump FX_RANDOM_VERSION when
// the mix changes, since every effect's random look changes with it.
const EFFECT_DOMAIN: u64 = 0x6566_6665_6374_0001;
const ELEMENT_DOMAIN: u64 = 0x656c_656d_656e_0002;
const TRAIL_DOMAIN: u64 = 0x7472_6169_6c00_0003;
const SAMPLE_DOMAIN: u64 = 0x7361_6d70_6c65_0004;

/// The 64-bit finalizer of MurmurHash3: every input bit reaches every output bit.
const fn avalanche(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}

fn digest_word(domain: u64, a: u64, b: u64) -> u64 {
    let version = u64::from(FX_RANDOM_VERSION).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    avalanche(avalanche(a ^ domain ^ version).wrapping_add(b.wrapping_mul(0xd6e8_feb8_6659_fd93)))
}

pub fn effect_random_key(instance: u64, begin_msec: i32) -> u64 {
    digest_word(EFFECT_DOMAIN, instance, u64::from(begin_msec as u32))
}

pub fn elem_random_seed(effect_key: u64, sequence: u8, begin_msec: i32) -> u64 {
    digest_word(
        ELEMENT_DOMAIN,
        effect_key,
        u64::from(sequence) << 32 | u64::from(begin_msec as u32),
    )
}

pub fn trail_random_seed(effect_key: u64, sequence: i8) -> u64 {
    digest_word(TRAIL_DOMAIN, effect_key, u64::from(sequence as u8))
}

fn sample_word(key: u64, channel: FxRandomChannel, index: u64) -> u64 {
    digest_word(
        SAMPLE_DOMAIN,
        key,
        (u64::from(channel as u32) << 56) ^ index,
    )
}

pub fn sample_u16(key: u64, channel: FxRandomChannel) -> u16 {
    (sample_word(key, channel, 0) >> 48) as u16
}

pub fn sample_f32(key: u64, channel: FxRandomChannel) -> f32 {
    sample_at(key, channel, 0)
}

pub fn sample_at(key: u64, channel: FxRandomChannel, index: u64) -> f32 {
    (sample_word(key, channel, index) >> 40) as f32 / 16_777_216.0
}

pub fn elem_visual_index(count: u8, key: u64) -> usize {
    if count <= 1 {
        return 0;
    }
    ((u32::from(sample_u16(key, FxRandomChannel::Visual)) * u32::from(count)) >> 16) as usize
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FxMotionRandom {
    pub velocity: [f32; 3],
    pub gravity: f32,
}

impl FxMotionRandom {
    pub fn from_seed(seed: u64) -> Self {
        Self {
            velocity: velocity_random(seed),
            gravity: sample_f32(seed, FxRandomChannel::Gravity),
        }
    }
}

pub(crate) fn velocity_random(seed: u64) -> [f32; 3] {
    [
        sample_f32(seed, FxRandomChannel::VelocityX),
        sample_f32(seed, FxRandomChannel::VelocityY),
        sample_f32(seed, FxRandomChannel::VelocityZ),
    ]
}
