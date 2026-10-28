//! Bink audio, DCT variant: blocks of quantized DCT coefficients in bands,
//! inverse-transformed and overlapped into each other.

use crate::bits::Bits;

/// The band edges, in Hz, the coefficients' quantizers follow.
const CRITICAL_FREQS: [u32; 25] = [
    100, 200, 300, 400, 510, 630, 770, 920, 1080, 1270, 1480, 1720, 2000, 2320, 2700, 3150, 3700,
    4400, 5300, 6400, 7700, 9500, 12000, 15500, 24500,
];
/// Run lengths, in groups of 8 coefficients, of a coded run.
const RUN_LENGTHS: [usize; 16] = [2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 32, 64];
/// A band quantizer's step, as a power of e per level.
const QUANT_STEP: f32 = 0.152_891_65;

pub struct AudioDecoder {
    channels: usize,
    frame_len: usize,
    overlap: usize,
    root: f32,
    quant: [f32; 96],
    bands: Vec<usize>,
    previous: Vec<Vec<f32>>,
    first: bool,
    fft: Fft,
}

impl AudioDecoder {
    pub fn new(track: &crate::AudioTrack) -> Result<Self, String> {
        if !track.dct {
            return Err("bink: RDFT audio is not read".into());
        }
        let rate = track.sample_rate;
        let channels = usize::from(track.channels);
        let frame_bits = if rate < 22050 {
            9
        } else if rate < 44100 {
            10
        } else {
            11
        };
        let frame_len = 1usize << frame_bits;
        let half_rate = rate.div_ceil(2);
        let root = frame_len as f32 / ((frame_len as f32).sqrt() * 32768.0);
        let quant = std::array::from_fn(|i| (i as f32 * QUANT_STEP).exp() * root);
        let mut band_count = 1;
        while band_count < 25 && half_rate > CRITICAL_FREQS[band_count - 1] {
            band_count += 1;
        }
        let mut bands = vec![2];
        for &freq in &CRITICAL_FREQS[..band_count - 1] {
            bands.push((freq as usize * frame_len / half_rate as usize) & !1);
        }
        bands.push(frame_len);
        Ok(Self {
            channels,
            frame_len,
            overlap: frame_len / 16,
            root,
            quant,
            bands,
            previous: vec![vec![0.0; frame_len / 16]; channels],
            first: true,
            fft: Fft::new(frame_len * 2),
        })
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Decodes one packet into interleaved samples, appended to `out`.
    pub fn decode(&mut self, packet: &[u8], out: &mut Vec<f32>) -> Result<(), String> {
        if packet.len() < 4 {
            return Ok(());
        }
        let mut bits = Bits::new(packet);
        bits.skip(32);
        let mut channels = vec![vec![0.0f32; self.frame_len]; self.channels];
        while bits.left() > 0 {
            self.block(&mut bits, &mut channels)?;
            let samples = self.frame_len - self.overlap;
            for i in 0..samples {
                for channel in &channels {
                    out.push(channel[i]);
                }
            }
            bits.align32();
        }
        Ok(())
    }

    fn float(bits: &mut Bits) -> f32 {
        let power = bits.read(5) as i32;
        let mantissa = bits.read(23) as f32;
        let value = mantissa * 2f32.powi(power - 23);
        if bits.bit() { -value } else { value }
    }

    fn block(&mut self, bits: &mut Bits, out: &mut [Vec<f32>]) -> Result<(), String> {
        bits.skip(2);
        let band_count = self.bands.len() - 1;
        for coeffs in out.iter_mut() {
            coeffs[0] = Self::float(bits) * self.root;
            coeffs[1] = Self::float(bits) * self.root;
            if bits.left() < band_count as isize * 8 {
                return Err("bink: an audio block ends inside its bands".into());
            }
            let quant: Vec<f32> = (0..band_count)
                .map(|_| self.quant[(bits.read(8) as usize).min(95)])
                .collect();
            let mut band = 0;
            let mut q = quant[0];
            let mut i = 2;
            while i < self.frame_len {
                let end = if bits.bit() {
                    i + RUN_LENGTHS[bits.read(4) as usize] * 8
                } else {
                    i + 8
                }
                .min(self.frame_len);
                let width = bits.read(4);
                if width == 0 {
                    coeffs[i..end].fill(0.0);
                    i = end;
                    while self.bands[band] < i {
                        q = quant[band];
                        band += 1;
                    }
                } else {
                    while i < end {
                        if self.bands[band] == i {
                            q = quant[band];
                            band += 1;
                        }
                        let coeff = bits.read(width);
                        coeffs[i] = if coeff == 0 {
                            0.0
                        } else if bits.bit() {
                            -q * coeff as f32
                        } else {
                            q * coeff as f32
                        };
                        i += 1;
                    }
                }
            }
            coeffs[0] /= 0.5;
            self.fft.dct3(coeffs);
        }
        let count = (self.overlap * self.channels) as f32;
        for (ch, coeffs) in out.iter_mut().enumerate() {
            if !self.first {
                for (i, (sample, previous)) in coeffs
                    .iter_mut()
                    .zip(&self.previous[ch])
                    .take(self.overlap)
                    .enumerate()
                {
                    let j = (ch + i * self.channels) as f32;
                    *sample = (previous * (count - j) + *sample * j) / count;
                }
            }
            let tail = self.frame_len - self.overlap;
            self.previous[ch].copy_from_slice(&coeffs[tail..]);
        }
        self.first = false;
        Ok(())
    }
}

/// A radix-2 complex FFT, for the inverse DCT of a block.
struct Fft {
    size: usize,
    twiddles: Vec<(f32, f32)>,
    re: Vec<f32>,
    im: Vec<f32>,
}

impl Fft {
    fn new(size: usize) -> Self {
        let twiddles = (0..size / 2)
            .map(|k| {
                let angle = 2.0 * std::f64::consts::PI * k as f64 / size as f64;
                (angle.cos() as f32, angle.sin() as f32)
            })
            .collect();
        Self {
            size,
            twiddles,
            re: vec![0.0; size],
            im: vec![0.0; size],
        }
    }

    /// `x[n] = 2/N (1/2 X[0] + sum_{k>0} X[k] cos(pi k (2n + 1) / 2N))`, in
    /// place.
    fn dct3(&mut self, data: &mut [f32]) {
        let n = data.len();
        let scale = 2.0 / n as f64;
        self.re.fill(0.0);
        self.im.fill(0.0);
        for (k, &coeff) in data.iter().enumerate() {
            let angle = std::f64::consts::PI * k as f64 / (2 * n) as f64;
            let weight = if k == 0 { 0.5 } else { 1.0 } * scale;
            let value = f64::from(coeff) * weight;
            self.re[k] = (value * angle.cos()) as f32;
            self.im[k] = (value * angle.sin()) as f32;
        }
        self.inverse();
        data.copy_from_slice(&self.re[..n]);
    }

    /// `z[n] = sum_k Z[k] e^(2 pi i k n / size)`, unscaled.
    fn inverse(&mut self) {
        let size = self.size;
        let mut j = 0;
        for i in 1..size {
            let mut bit = size >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j |= bit;
            if i < j {
                self.re.swap(i, j);
                self.im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= size {
            let step = size / len;
            for start in (0..size).step_by(len) {
                for k in 0..len / 2 {
                    let (wr, wi) = self.twiddles[k * step];
                    let (a, b) = (start + k, start + k + len / 2);
                    let (br, bi) = (self.re[b], self.im[b]);
                    let tr = br * wr - bi * wi;
                    let ti = br * wi + bi * wr;
                    self.re[b] = self.re[a] - tr;
                    self.im[b] = self.im[a] - ti;
                    self.re[a] += tr;
                    self.im[a] += ti;
                }
            }
            len <<= 1;
        }
    }
}
