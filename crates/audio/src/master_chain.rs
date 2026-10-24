//! A game's two submix effects ahead of its master voice, as its sound
//! driver runs them on each output channel: the bus every voice but the
//! master bus's own sends to runs a compressor; the master bus, which
//! takes that bus and the voices sent to it directly, runs a gain, four
//! equaliser bands (a low shelf, two peaks and a high shelf) and a
//! compressor, each with its own switch.

use crate::render_core::{QUANTUM, SAMPLE_RATE};

/// A parameter at or under this is off, and a gain within it of 1 is not
/// applied.
const EPSILON: f32 = f32::from_bits(0x3780_3e84);
/// A filter's history under this is cleared after each block.
const FILTER_FLOOR: f32 = f32::from_bits(0x2cd1_b717);
/// The compressor's look-ahead: the effect reads its input one
/// 480-sample pass late.
const LOOK_AHEAD: usize = 480;
/// The compressor's detector: each sample's level weighs in at 0.01.
const DETECT_NEW: f32 = f32::from_bits(0x3c23_d700);
const DETECT_OLD: f32 = f32::from_bits(0x3f7d_70a4);
/// The level, about -45 dB, an attack or release time brings a step down to.
const SETTLE: f32 = f32::from_bits(0x3bb8_449c);

/// The two effects' parameters, as the sound driver passes them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MasterChainParameters {
    /// Four bands of (on, gain in dB, frequency, Q), the gain, then the
    /// compressor's on, gain before and after, threshold, ratio, attack
    /// and release seconds.
    pub master_bus: [f32; 24],
    /// The compressor's on, gain before and after, threshold, ratio,
    /// attack and release seconds.
    pub no_voice_bus: [f32; 7],
}

/// A filter's coefficients: b0, b1, b2, a1, a2 over a0.
type Coefficients = [f32; 5];

#[derive(Clone, Copy, Default)]
struct Biquad {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    fn run(&mut self, [b0, b1, b2, a1, a2]: Coefficients, samples: &mut [f32]) {
        for sample in samples {
            let x = *sample;
            let y = ((b1 * self.x1 + b0 * x + b2 * self.x2) - a1 * self.y1) - a2 * self.y2;
            self.x2 = self.x1;
            self.x1 = x;
            self.y2 = self.y1;
            self.y1 = y;
            *sample = y;
        }
        for value in [&mut self.x1, &mut self.x2, &mut self.y1, &mut self.y2] {
            if value.abs() - FILTER_FLOOR < 0.0 {
                *value = 0.0;
            }
        }
    }
}

/// The shared start of the three filters: the gain as an amplitude, the
/// band's cosine and its alpha.
fn band(gain_db: f32, frequency: f32, q: f32) -> (f32, f32, f32) {
    let amplitude = 10f64.powf(f64::from(gain_db * 0.025)) as f32;
    let ratio = (frequency / SAMPLE_RATE as f32) * 2.0;
    let ratio = if 0.0 <= ratio - 1e-6 { ratio } else { 1e-6 };
    let w = (f64::from(ratio) * f64::from(std::f32::consts::PI)) as f32;
    let sin = f64::from(w).sin() as f32;
    let cos = f64::from(w).cos() as f32;
    (amplitude, cos, sin / (q * 2.0))
}

fn low_shelf(gain_db: f32, frequency: f32, q: f32) -> Coefficients {
    let (a, cos, alpha) = band(gain_db, frequency, q);
    let beta = a.sqrt() * alpha * 2.0;
    let plus = a + 1.0;
    let minus = a - 1.0;
    let minus_cos = minus * cos;
    let plus_cos = plus * cos;
    let low = plus - minus_cos;
    let b0 = (beta + low) * a;
    let b1 = (minus - plus_cos) * a * 2.0;
    let b2 = (low - beta) * a;
    let a0 = (beta + minus_cos) + plus;
    let a1 = (plus_cos + minus) * -2.0;
    let a2 = (minus_cos + plus) - beta;
    let scale = 1.0 / a0;
    [scale * b0, scale * b1, scale * b2, scale * a1, scale * a2]
}

fn peak(gain_db: f32, frequency: f32, q: f32) -> Coefficients {
    let (a, cos, alpha) = band(gain_db, frequency, q);
    let middle = cos * -2.0;
    let up = alpha * a;
    let down = alpha / a;
    let scale = 1.0 / (down + 1.0);
    [
        scale * (up + 1.0),
        scale * middle,
        scale * (1.0 - up),
        scale * middle,
        scale * (1.0 - down),
    ]
}

fn high_shelf(gain_db: f32, frequency: f32, q: f32) -> Coefficients {
    let (a, cos, alpha) = band(gain_db, frequency, q);
    let beta = a.sqrt() * alpha * 2.0;
    let plus = a + 1.0;
    let minus = a - 1.0;
    let plus_cos = plus * cos;
    let minus_cos = minus * cos;
    let b0 = ((plus + beta) + minus_cos) * a;
    let b1 = (plus_cos + minus) * a * -2.0;
    let b2 = ((plus + minus_cos) - beta) * a;
    let low = plus - minus_cos;
    let a0 = low + beta;
    let a1 = (minus - plus_cos) * 2.0;
    let a2 = low - beta;
    let scale = 1.0 / a0;
    [scale * b0, scale * b1, scale * b2, scale * a1, scale * a2]
}

/// The share of the way to its target the compressor's gain moves each
/// step, for an attack or release time; none for a time within a sample.
fn step(seconds: f32) -> f32 {
    let sample = (1.0 / f64::from(SAMPLE_RATE)) as f32;
    if seconds <= sample {
        return 0.0;
    }
    (-(std::f64::consts::LN_2 * f64::from(SETTLE).log2()) / f64::from(seconds) * f64::from(sample))
        as f32
}

fn pow(base: f32, exponent: f32) -> f64 {
    (f64::from(base).log2() * f64::from(exponent)).exp2()
}

struct Compressor {
    delay: [f32; LOOK_AHEAD],
    at: usize,
    level: f32,
    gain: f32,
    target: f32,
}

impl Default for Compressor {
    fn default() -> Self {
        Self {
            delay: [0.0; LOOK_AHEAD],
            at: 0,
            level: 0.0,
            gain: 0.0,
            target: 0.0,
        }
    }
}

impl Compressor {
    /// `[on, before, after, threshold, ratio, attack, release]`.
    fn run(&mut self, parameters: &[f32], samples: &mut [f32]) {
        if parameters[0] <= EPSILON {
            return;
        }
        let scale = |samples: &mut [f32], gain: f32| {
            if EPSILON < (gain - 1.0).abs() {
                for sample in samples.iter_mut() {
                    *sample *= gain;
                }
            }
        };
        scale(samples, parameters[1]);
        self.compress(&parameters[3..7], samples);
        scale(samples, parameters[2]);
    }

    fn compress(&mut self, parameters: &[f32], samples: &mut [f32]) {
        let threshold = parameters[0];
        let slope = 1.0 / parameters[1];
        let release = step(parameters[3]);
        let attack = step(parameters[2]);
        let makeup = pow(threshold, slope - 1.0) as f32;
        let over_threshold = 1.0 / threshold;
        for pair in samples.chunks_mut(2) {
            let (first, rest) = pair.split_first_mut().expect("chunks are not empty");
            let input = *first;
            let delayed = self.delay(input);
            self.level = input.abs() * DETECT_NEW + self.level * DETECT_OLD;
            self.target = if self.level <= threshold {
                makeup
            } else {
                (f64::from(makeup) / f64::from(self.level)
                    * pow(self.level * over_threshold, slope)
                    * f64::from(threshold)) as f32
            };
            *first = delayed * self.gain;
            let rate = if self.target < self.gain {
                attack
            } else {
                release
            };
            let keep = 1.0 - rate;
            self.gain = self.target * rate + keep * self.gain;
            if let Some(second) = rest.first_mut() {
                let input = *second;
                let delayed = self.delay(input);
                self.level = input.abs() * DETECT_NEW + self.level * DETECT_OLD;
                *second = self.gain * delayed;
                self.gain = self.target * rate + keep * self.gain;
            }
        }
        for value in [&mut self.level, &mut self.gain, &mut self.target] {
            if *value < EPSILON {
                *value = 0.0;
            }
        }
    }

    /// The sample the look-ahead gives back for this one.
    fn delay(&mut self, input: f32) -> f32 {
        let delayed = std::mem::replace(&mut self.delay[self.at], input);
        self.at = (self.at + 1) % LOOK_AHEAD;
        delayed
    }
}

#[derive(Default)]
struct Channel {
    no_voice: Compressor,
    bands: [Biquad; 4],
    master: Compressor,
}

impl Channel {
    fn master_bus(&mut self, p: &[f32; 24], samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            *sample *= p[16];
        }
        let filters: [fn(f32, f32, f32) -> Coefficients; 4] = [low_shelf, peak, peak, high_shelf];
        for (index, (filter, design)) in self.bands.iter_mut().zip(filters).enumerate() {
            let at = index * 4;
            if p[at] <= EPSILON {
                *filter = Biquad::default();
            } else {
                filter.run(design(p[at + 1], p[at + 2], p[at + 3]), samples);
            }
        }
        self.master.run(&p[17..24], samples);
    }
}

/// The two effects on the two output channels.
#[derive(Default)]
pub(crate) struct MasterChain {
    channels: [Channel; 2],
}

impl MasterChain {
    /// The bus (every voice but the master bus's own, and the reverb)
    /// through its compressor, joined by the voices sent to the master bus
    /// directly, through the master bus.
    pub(crate) fn process(
        &mut self,
        parameters: &MasterChainParameters,
        bus: &mut [[f32; 2]; QUANTUM],
        direct: &[[f32; 2]; QUANTUM],
    ) {
        for (side, channel) in self.channels.iter_mut().enumerate() {
            let mut samples: [f32; QUANTUM] = std::array::from_fn(|at| bus[at][side]);
            channel.no_voice.run(&parameters.no_voice_bus, &mut samples);
            for (sample, direct) in samples.iter_mut().zip(direct) {
                *sample += direct[side];
            }
            channel.master_bus(&parameters.master_bus, &mut samples);
            for (frame, sample) in bus.iter_mut().zip(samples) {
                frame[side] = sample;
            }
        }
    }
}
