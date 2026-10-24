//! A game's four-line reverb, as its sound driver's reverb effect runs it
//! on its four-channel reverb mix: a feedback delay network of four
//! 32768-sample lines, each line's input low-passed and fed back with a
//! rotation of the others' late taps, two sets of sixteen taps (an early and
//! a late one, each with its own weights and delays) read across the lines,
//! and every parameter but the delays eased toward its target once per
//! block.

use crate::render_core::QUANTUM;

/// The samples a delay line holds.
const LINE: usize = 0x8000;
/// One over the square root of two.
const HALF_ROOT: f32 = 0.707_106_77;

/// The base delays of the two tap sets, by the reverb's matrix choice:
/// the early taps', then the late taps' (a late tap of delay 0 has weight 0).
const BASE_DELAYS: [[[u32; 16]; 2]; 2] = [
    [
        [
            0x3ad, 0x95, 0x2a1, 0x115, 0xd3, 0x31d, 0x1a5, 0x241, 0x2ef, 0x15d, 0x3f1, 0xad, 0x1f3,
            0x265, 0xef, 0x36d,
        ],
        [
            0x44f, 0x503, 0, 0, 0x59f, 0x653, 0, 0, 0, 0, 0x6df, 0x79d, 0, 0, 0x2ef, 0x3ad,
        ],
    ],
    [
        [
            0x24b, 0x59f, 0x55d, 0x283, 0x5d5, 0x175, 0x38f, 0x443, 0x4b1, 0x305, 0x209, 0x665,
            0x3e5, 0x511, 0x62b, 0x1b7,
        ],
        [
            0x125, 0x1df, 0, 0, 0x41b, 0x4d5, 0, 0, 0, 0, 0x491, 0x67f, 0, 0, 0x71, 0x25f,
        ],
    ],
];

/// The reverb's 25 parameters, as the sound driver hands them to the
/// effect: the sample rate, the room's values and the listener's
/// surroundings, and the matrix choice last.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ReverbParameters(pub [f32; 25]);

/// The parameters' limits: each value is held to them before a block.
fn clamped(mut p: [f32; 25]) -> [f32; 25] {
    let hold = |value: f32, low: f32, high: f32| {
        let value = if 0.0 <= value - high { high } else { value };
        if 0.0 <= low - value { low } else { value }
    };
    for value in &mut p[1..=3] {
        *value = hold(*value, 0.0, 1.0);
    }
    for value in &mut p[4..=10] {
        *value = hold(*value, 0.0, 4.0);
    }
    for value in &mut p[11..=19] {
        *value = hold(*value, 0.0, 1.0);
    }
    for value in &mut p[20..=21] {
        *value = hold(*value, 0.1, 16.0);
    }
    p[22] = hold(p[22], 0.0, 1.0);
    p
}

/// The reverb's state: its lines, the values it eases, its taps and its
/// filters.
pub(crate) struct Radverb {
    lines: Box<[[f32; LINE]; 4]>,
    write: usize,
    early_gain: f32,
    late_gain: f32,
    early_mix: f32,
    late_scale: f32,
    late_mix: [f32; 4],
    output_gain: f32,
    early_damp: f32,
    late_feedback_damp: f32,
    input_damp: f32,
    late_damp: f32,
    weights: [f32; 4],
    direct: f32,
    early_size: f32,
    late_size: f32,
    rotation: f32,
    early_weights: [f32; 16],
    late_weights: [f32; 16],
    early_delays: [usize; 16],
    late_delays: [usize; 16],
    early: [f32; 4],
    late: [f32; 4],
    input: [f32; 4],
    feedback: [f32; 4],
}

impl Default for Radverb {
    fn default() -> Self {
        Self {
            lines: vec![[0.0; LINE]; 4]
                .into_boxed_slice()
                .try_into()
                .expect("four lines"),
            write: 0,
            early_gain: 0.0,
            late_gain: 0.0,
            early_mix: 0.0,
            late_scale: 0.0,
            late_mix: [0.0; 4],
            output_gain: 0.0,
            early_damp: 0.0,
            late_feedback_damp: 0.0,
            input_damp: 0.0,
            late_damp: 0.0,
            weights: [0.0; 4],
            direct: 0.0,
            early_size: 0.0,
            late_size: 0.0,
            rotation: 0.0,
            early_weights: [0.0; 16],
            late_weights: [0.0; 16],
            early_delays: [0; 16],
            late_delays: [0; 16],
            early: [0.0; 4],
            late: [0.0; 4],
            input: [0.0; 4],
            feedback: [0.0; 4],
        }
    }
}

impl Radverb {
    /// Eases the values toward the block's parameters, by the room's
    /// smoothing over a tenth of a second, and sets the taps.
    fn prepare(&mut self, parameters: &ReverbParameters, frames: usize) {
        let p = clamped(parameters.0);
        let ease = (1.0 - p[1]) / ((p[0] / frames as f32) * 0.1);
        let keep = 1.0 - ease;
        let eased = |target: f32, value: f32| target * ease + keep * value;
        self.early_gain = eased(p[2], self.early_gain);
        self.late_gain = eased(p[3], self.late_gain);
        self.early_mix = eased(p[4], self.early_mix);
        self.late_scale = eased(p[5], self.late_scale);
        for (mix, at) in self.late_mix.iter_mut().zip(6..) {
            *mix = p[at] * self.late_scale;
        }
        self.output_gain = eased(p[10], self.output_gain);
        self.early_damp = eased(p[11], self.early_damp);
        self.late_feedback_damp = eased(p[12], self.late_feedback_damp);
        self.input_damp = eased(p[13], self.input_damp);
        self.late_damp = eased(p[14], self.late_damp);
        for (weight, at) in self.weights.iter_mut().zip(15..) {
            *weight = eased(p[at], *weight);
        }
        self.direct = eased(p[19], self.direct);
        self.rotation = eased(p[22], self.rotation);
        self.early_size = keep * self.early_size + ease * p[20];
        self.late_size = keep * self.late_size + p[21] * ease;
        let angle = f64::from(self.rotation * std::f32::consts::FRAC_PI_2);
        let (sin, cos) = (angle.sin() as f32, angle.cos() as f32);
        self.late_weights = [
            cos, sin, 0.0, 0.0, sin, -cos, 0.0, 0.0, 0.0, 0.0, -cos, sin, 0.0, 0.0, sin, cos,
        ];
        let weight = |value: f32, scale: f32| {
            let weight = (value - 1.0) * scale + HALF_ROOT;
            if 0.0 <= -weight { 0.0 } else { weight }
        };
        let a = weight(self.weights[0], 0.0);
        let b = weight(self.weights[1], 0.353_553_38);
        let c = weight(self.weights[2], HALF_ROOT);
        let d = weight(self.weights[3], 1.060_660_1);
        self.early_weights = [a, d, c, b, d, a, b, c, c, b, a, d, b, c, d, a];
        let [early, late] = &BASE_DELAYS[usize::from(p[24].to_bits() != 0)];
        let delay = |base: u32, size: f32| (base as f32 * size).round_ties_even() as usize;
        for at in 0..16 {
            self.early_delays[at] = delay(early[at], p[20]);
            self.late_delays[at] = delay(late[at], p[21]);
        }
    }

    /// Runs a block of the four-channel reverb mix through the reverb.
    pub(crate) fn process(
        &mut self,
        parameters: &ReverbParameters,
        input: &[[f32; 4]; QUANTUM],
        output: &mut [[f32; 4]; QUANTUM],
    ) {
        self.prepare(parameters, QUANTUM);
        let tap = |line: &[f32; LINE], write: usize, delay: usize| {
            line[(write + LINE - delay % LINE) % LINE]
        };
        for (sample, out) in input.iter().zip(output.iter_mut()) {
            for j in 0..4 {
                // The even lines and the odd lines are summed apart, then
                // the odd sum is added to the even one.
                let (mut early, mut late) = ([0.0; 2], [0.0; 2]);
                for (k, line) in self.lines.iter().enumerate() {
                    let at = j * 4 + k;
                    early[k & 1] +=
                        tap(line, self.write, self.early_delays[at]) * self.early_weights[at];
                    late[k & 1] +=
                        tap(line, self.write, self.late_delays[at]) * self.late_weights[at];
                }
                let (early, late) = (early[1] + early[0], late[1] + late[0]);
                self.early[j] = (1.0 - self.early_damp) * self.early_gain * early
                    + self.early_damp * self.early[j];
                self.late[j] =
                    (1.0 - self.late_damp) * self.late_gain * late + self.late_damp * self.late[j];
            }
            let [l0, l1, l2, l3] = self.late;
            let mixed = [
                (l3 + l2) * HALF_ROOT,
                (l3 - l2) * HALF_ROOT,
                (l0 - l1) * HALF_ROOT,
                (l1 + l0) * HALF_ROOT,
            ];
            for k in 0..4 {
                self.input[k] =
                    (1.0 - self.input_damp) * sample[k] + self.input_damp * self.input[k];
                self.lines[k][self.write] = self.input[k] + mixed[k];
                self.feedback[k] = (1.0 - self.late_feedback_damp) * mixed[k]
                    + self.late_feedback_damp * self.feedback[k];
                out[k] = (self.input[k] * self.direct
                    + self.feedback[k] * self.late_mix[k]
                    + self.early_mix * self.early[k])
                    * self.output_gain;
            }
            self.write = (self.write + 1) & (LINE - 1);
        }
    }
}
