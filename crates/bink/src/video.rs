//! Bink 1 video: each plane is read as 8x8 blocks whose kinds and values
//! come from nine bundles, refilled from the stream row by row.

use crate::bits::Bits;
use crate::tables::{INTER_QUANT, INTRA_QUANT, PATTERNS, SCAN, TREE_BITS, TREE_LENS};

const SKIP: u8 = 0;
const SCALED: u8 = 1;
const MOTION: u8 = 2;
const RUN: u8 = 3;
const RESIDUE: u8 = 4;
const INTRA: u8 = 5;
const FILL: u8 = 6;
const INTER: u8 = 7;
const PATTERN: u8 = 8;
const RAW: u8 = 9;

const BLOCK_TYPES: usize = 0;
const SUB_BLOCK_TYPES: usize = 1;
const COLORS: usize = 2;
const PATTERNS_SRC: usize = 3;
const X_OFF: usize = 4;
const Y_OFF: usize = 5;
const INTRA_DC: usize = 6;
const INTER_DC: usize = 7;
const RUNS: usize = 8;
const BUNDLES: usize = 9;

/// Run lengths of a block-type run symbol (12 to 15).
const RLE_LENS: [usize; 4] = [4, 8, 12, 32];
/// Bits of a DC bundle's first value.
const DC_START_BITS: u32 = 11;

/// One tree's decode table: for every 7-bit window, the symbol index and
/// its code length.
struct Vlc([(u8, u8); 128]);

impl Vlc {
    fn new(tree: usize) -> Self {
        let mut table = [(0, 0); 128];
        for symbol in 0..16 {
            let len = TREE_LENS[tree][symbol];
            let code = usize::from(TREE_BITS[tree][symbol]);
            // Every window whose low `len` bits are the code.
            for high in 0..(1usize << (7 - len)) {
                table[code | (high << len)] = (symbol as u8, len);
            }
        }
        Self(table)
    }
}

#[derive(Clone, Copy, Default)]
struct Tree {
    vlc: usize,
    syms: [u8; 16],
}

#[derive(Default)]
struct Bundle {
    /// Bits of a refill's value count.
    len: u32,
    tree: Tree,
    data: Vec<i16>,
    /// Where the next refill writes; `None` once the stream ended it.
    dec: Option<usize>,
    /// Where the next value is read.
    ptr: usize,
}

impl Bundle {
    /// Whether a refill is due: everything read so far was used.
    fn due(&self) -> bool {
        self.dec.is_some_and(|dec| dec <= self.ptr)
    }

    fn take(&mut self) -> i32 {
        let value = self.data.get(self.ptr).copied().unwrap_or(0);
        self.ptr += 1;
        i32::from(value)
    }

    fn push(&mut self, value: i32) -> Result<(), String> {
        let at = self.dec.unwrap_or(self.data.len());
        let slot = self
            .data
            .get_mut(at)
            .ok_or("bink: a bundle overflows its plane")?;
        *slot = value as i16;
        self.dec = Some(at + 1);
        Ok(())
    }

    /// A refill's value count, or `None` when none is due or the stream
    /// ended the bundle.
    fn count(&mut self, bits: &mut Bits) -> Option<usize> {
        if !self.due() {
            return None;
        }
        let count = bits.read(self.len) as usize;
        if count == 0 {
            self.dec = None;
            return None;
        }
        Some(count)
    }
}

/// One decoded picture: planar Y, Cb, Cr (4:2:0) and an optional alpha,
/// each padded to whole blocks.
#[derive(Clone)]
pub struct Picture {
    pub width: usize,
    pub height: usize,
    /// Y, Cb, Cr, alpha.
    pub planes: [Vec<u8>; 4],
    pub strides: [usize; 4],
}

impl Picture {
    fn new(width: usize, height: usize, alpha: bool) -> Self {
        let luma = (width.div_ceil(8) * 8, height.div_ceil(8) * 8);
        let chroma = (width.div_ceil(16) * 8, height.div_ceil(16) * 8);
        let alpha_size = if alpha { luma } else { (0, 0) };
        Self {
            width,
            height,
            planes: [
                vec![0; luma.0 * luma.1],
                vec![0; chroma.0 * chroma.1],
                vec![0; chroma.0 * chroma.1],
                vec![0; alpha_size.0 * alpha_size.1],
            ],
            strides: [luma.0, chroma.0, chroma.0, alpha_size.0],
        }
    }

    /// The picture as RGBA (BT.601, video range).
    pub fn to_rgba(&self, out: &mut Vec<u8>) {
        out.resize(self.width * self.height * 4, 0);
        let [ys, us, vs, alpha_stride] = self.strides;
        for y in 0..self.height {
            let (yrow, crow) = (y * ys, (y / 2) * us);
            for x in 0..self.width {
                let luma = (i32::from(self.planes[0][yrow + x]) - 16) * 298;
                let cb = i32::from(self.planes[1][crow + x / 2]) - 128;
                let cr = i32::from(self.planes[2][(y / 2) * vs + x / 2]) - 128;
                let at = (y * self.width + x) * 4;
                out[at] = ((luma + 409 * cr + 128) >> 8).clamp(0, 255) as u8;
                out[at + 1] = ((luma - 100 * cb - 208 * cr + 128) >> 8).clamp(0, 255) as u8;
                out[at + 2] = ((luma + 516 * cb + 128) >> 8).clamp(0, 255) as u8;
                out[at + 3] = if alpha_stride > 0 {
                    self.planes[3][y * alpha_stride + x]
                } else {
                    255
                };
            }
        }
    }
}

pub struct VideoDecoder {
    revision: u8,
    swap_planes: bool,
    vlcs: Vec<Vlc>,
    bundles: [Bundle; BUNDLES],
    col_high: [Tree; 16],
    col_last: usize,
    current: Picture,
    last: Picture,
    has_alpha: bool,
}

/// A coefficient's magnitude and sign as Bink writes it.
fn signed_coefficient(bits: &mut Bits, nbits: u32) -> i32 {
    if nbits == 0 {
        1 - ((bits.read(1) as i32) << 1)
    } else {
        let t = (bits.read(nbits) | (1 << nbits)) as i32;
        if bits.bit() { -t } else { t }
    }
}

impl VideoDecoder {
    pub fn new(file: &crate::BinkFile) -> Self {
        let (width, height) = (file.width as usize, file.height as usize);
        let blocks = width.div_ceil(8) * height.div_ceil(8);
        let mut bundles: [Bundle; BUNDLES] = Default::default();
        for bundle in &mut bundles {
            bundle.data = vec![0; blocks * 64];
        }
        Self {
            revision: file.revision,
            swap_planes: file.revision >= b'h',
            vlcs: (0..16).map(Vlc::new).collect(),
            bundles,
            col_high: [Tree::default(); 16],
            col_last: 0,
            current: Picture::new(width, height, file.has_alpha),
            last: Picture::new(width, height, file.has_alpha),
            has_alpha: file.has_alpha,
        }
    }

    fn huff(&self, bits: &mut Bits, tree: Tree) -> i32 {
        let (symbol, len) = self.vlcs[tree.vlc].0[bits.peek(7) as usize];
        bits.skip(usize::from(len));
        i32::from(tree.syms[usize::from(symbol)])
    }

    /// Decodes one video packet; the picture stays valid until the next.
    pub fn decode(&mut self, packet: &[u8]) -> Result<&Picture, String> {
        std::mem::swap(&mut self.current, &mut self.last);
        let mut bits = Bits::new(packet);
        let total = packet.len() * 8;
        if self.has_alpha {
            if self.revision >= b'i' {
                bits.skip(32);
            }
            self.plane(&mut bits, 3, false)?;
        }
        if self.revision >= b'i' {
            bits.skip(32);
        }
        for plane in 0..3 {
            let index = if plane == 0 || !self.swap_planes {
                plane
            } else {
                plane ^ 3
            };
            self.plane(&mut bits, index, plane != 0)?;
            if bits.position() >= total {
                break;
            }
        }
        Ok(&self.current)
    }

    fn read_tree(&self, bits: &mut Bits) -> Tree {
        let mut tree = Tree {
            vlc: bits.read(4) as usize,
            syms: [0; 16],
        };
        if tree.vlc == 0 {
            tree.syms = std::array::from_fn(|i| i as u8);
            return tree;
        }
        if bits.bit() {
            let mut len = bits.read(3) as usize;
            let mut seen = [false; 16];
            for i in 0..=len {
                let symbol = bits.read(4) as u8;
                tree.syms[i] = symbol;
                seen[usize::from(symbol)] = true;
            }
            for (i, &used) in seen.iter().enumerate() {
                if len >= 15 {
                    break;
                }
                if !used {
                    len += 1;
                    tree.syms[len] = i as u8;
                }
            }
        } else {
            let depth = bits.read(2);
            let mut order: [u8; 16] = std::array::from_fn(|i| i as u8);
            for i in 0..=depth {
                let size = 1usize << i;
                let mut merged = [0u8; 16];
                for start in (0..16).step_by(size * 2) {
                    let (mut a, mut b) = (start, start + size);
                    let (end_a, end_b) = (start + size, start + 2 * size);
                    let mut out = start;
                    while a < end_a && b < end_b {
                        if bits.bit() {
                            merged[out] = order[b];
                            b += 1;
                        } else {
                            merged[out] = order[a];
                            a += 1;
                        }
                        out += 1;
                    }
                    for &value in order[a..end_a].iter().chain(&order[b..end_b]) {
                        merged[out] = value;
                        out += 1;
                    }
                }
                order = merged;
            }
            tree.syms = order;
        }
        tree
    }

    fn read_bundle(&mut self, bits: &mut Bits, which: usize) {
        if which == COLORS {
            for i in 0..16 {
                self.col_high[i] = self.read_tree(bits);
            }
            self.col_last = 0;
        }
        if which != INTRA_DC && which != INTER_DC {
            self.bundles[which].tree = self.read_tree(bits);
        }
        let bundle = &mut self.bundles[which];
        bundle.dec = Some(0);
        bundle.ptr = 0;
    }

    fn init_lengths(&mut self, width: usize, bw: usize) {
        let log = |v: usize| (usize::BITS - 1 - v.leading_zeros()) + 1;
        let width = width.div_ceil(8) * 8;
        self.bundles[BLOCK_TYPES].len = log((width >> 3) + 511);
        self.bundles[SUB_BLOCK_TYPES].len = log((width >> 4) + 511);
        self.bundles[COLORS].len = log(bw * 64 + 511);
        for which in [INTRA_DC, INTER_DC, X_OFF, Y_OFF] {
            self.bundles[which].len = log((width >> 3) + 511);
        }
        self.bundles[PATTERNS_SRC].len = log((bw << 3) + 511);
        self.bundles[RUNS].len = log(bw * 48 + 511);
    }

    fn read_block_types(&mut self, bits: &mut Bits, which: usize) -> Result<(), String> {
        let Some(count) = self.bundles[which].count(bits) else {
            return Ok(());
        };
        if bits.bit() {
            let value = bits.read(4) as i32;
            for _ in 0..count {
                self.bundles[which].push(value)?;
            }
            return Ok(());
        }
        let tree = self.bundles[which].tree;
        let mut last = 0;
        let mut left = count;
        while left > 0 {
            let value = self.huff(bits, tree);
            if value < 12 {
                last = value;
                self.bundles[which].push(value)?;
                left -= 1;
            } else {
                let run = RLE_LENS[(value - 12) as usize];
                if run > left {
                    return Err("bink: a block type run passes its count".into());
                }
                for _ in 0..run {
                    self.bundles[which].push(last)?;
                }
                left -= run;
            }
        }
        Ok(())
    }

    fn read_colors(&mut self, bits: &mut Bits) -> Result<(), String> {
        let Some(count) = self.bundles[COLORS].count(bits) else {
            return Ok(());
        };
        let tree = self.bundles[COLORS].tree;
        let revision = self.revision;
        let color = |this: &mut Self, bits: &mut Bits| {
            this.col_last = this.huff(bits, this.col_high[this.col_last]) as usize;
            let mut v = ((this.col_last as i32) << 4) | this.huff(bits, tree);
            if revision < b'i' {
                let sign = i32::from((v as u8 as i8) >> 7);
                v = (((v & 0x7f) ^ sign) - sign) + 0x80;
            }
            v & 0xff
        };
        if bits.bit() {
            let value = color(self, bits);
            for _ in 0..count {
                self.bundles[COLORS].push(value)?;
            }
        } else {
            for _ in 0..count {
                let value = color(self, bits);
                self.bundles[COLORS].push(value)?;
            }
        }
        Ok(())
    }

    fn read_patterns(&mut self, bits: &mut Bits) -> Result<(), String> {
        let Some(count) = self.bundles[PATTERNS_SRC].count(bits) else {
            return Ok(());
        };
        let tree = self.bundles[PATTERNS_SRC].tree;
        for _ in 0..count {
            let low = self.huff(bits, tree);
            let value = low | (self.huff(bits, tree) << 4);
            self.bundles[PATTERNS_SRC].push(value)?;
        }
        Ok(())
    }

    fn read_motion(&mut self, bits: &mut Bits, which: usize) -> Result<(), String> {
        let Some(count) = self.bundles[which].count(bits) else {
            return Ok(());
        };
        let signed = |bits: &mut Bits, v: i32| if v != 0 && bits.bit() { -v } else { v };
        if bits.bit() {
            let v = bits.read(4) as i32;
            let value = signed(bits, v);
            for _ in 0..count {
                self.bundles[which].push(value)?;
            }
        } else {
            let tree = self.bundles[which].tree;
            for _ in 0..count {
                let v = self.huff(bits, tree);
                let value = signed(bits, v);
                self.bundles[which].push(value)?;
            }
        }
        Ok(())
    }

    fn read_dcs(&mut self, bits: &mut Bits, which: usize, has_sign: bool) -> Result<(), String> {
        let Some(count) = self.bundles[which].count(bits) else {
            return Ok(());
        };
        let mut v = bits.read(DC_START_BITS - u32::from(has_sign)) as i32;
        if v != 0 && has_sign && bits.bit() {
            v = -v;
        }
        self.bundles[which].push(v)?;
        let rest = count - 1;
        let mut i = 0;
        while i < rest {
            let group = (rest - i).min(8);
            let size = bits.read(4);
            for _ in 0..group {
                if size != 0 {
                    let mut delta = bits.read(size) as i32;
                    if delta != 0 && bits.bit() {
                        delta = -delta;
                    }
                    v += delta;
                    if !(-32768..=32767).contains(&v) {
                        return Err("bink: a DC leaves 16 bits".into());
                    }
                }
                self.bundles[which].push(v)?;
            }
            i += 8;
        }
        Ok(())
    }

    fn read_runs(&mut self, bits: &mut Bits) -> Result<(), String> {
        let Some(count) = self.bundles[RUNS].count(bits) else {
            return Ok(());
        };
        if bits.bit() {
            let value = bits.read(4) as i32;
            for _ in 0..count {
                self.bundles[RUNS].push(value)?;
            }
        } else {
            let tree = self.bundles[RUNS].tree;
            for _ in 0..count {
                let value = self.huff(bits, tree);
                self.bundles[RUNS].push(value)?;
            }
        }
        Ok(())
    }

    fn value(&mut self, which: usize) -> i32 {
        self.bundles[which].take()
    }

    /// A run block's 64 values in the order of one of the 16 patterns.
    fn run_block(&mut self, bits: &mut Bits) -> Result<[u8; 64], String> {
        let mut out = [0u8; 64];
        let scan = &PATTERNS[bits.read(4) as usize];
        let mut at = 0usize;
        while at < 63 {
            let run = self.value(RUNS) as usize + 1;
            if at + run > 64 {
                return Err("bink: a run block runs past its block".into());
            }
            if bits.bit() {
                let v = self.value(COLORS) as u8;
                for &place in &scan[at..at + run] {
                    out[usize::from(place)] = v;
                }
            } else {
                for &place in &scan[at..at + run] {
                    out[usize::from(place)] = self.value(COLORS) as u8;
                }
            }
            at += run;
        }
        if at == 63 {
            out[usize::from(scan[63])] = self.value(COLORS) as u8;
        }
        Ok(out)
    }

    /// The DCT coefficients after the DC, unquantized.
    fn dct_block(&mut self, bits: &mut Bits, dc: i32, quant: &[[u32; 64]; 16]) -> [i32; 64] {
        let mut block = [0i32; 64];
        block[0] = dc;
        let mut coef_list = [0i32; 128];
        let mut mode_list = [0u8; 128];
        let (mut start, mut end) = (64usize, 64usize);
        for (coef, mode) in [(4, 0), (24, 0), (44, 0), (1, 3), (2, 3), (3, 3)] {
            coef_list[end] = coef;
            mode_list[end] = mode;
            end += 1;
        }
        let mut placed = [0usize; 64];
        let mut count = 0usize;
        let mut nbits = bits.read(4) as i32 - 1;
        while nbits >= 0 {
            let nb = nbits as u32;
            let mut pos = start;
            while pos < end {
                if (mode_list[pos] == 0 && coef_list[pos] == 0) || !bits.bit() {
                    pos += 1;
                    continue;
                }
                let mut coef = coef_list[pos];
                let mode = mode_list[pos];
                match mode {
                    0 | 2 => {
                        if mode == 0 {
                            coef_list[pos] = coef + 4;
                            mode_list[pos] = 1;
                        } else {
                            coef_list[pos] = 0;
                            mode_list[pos] = 0;
                            pos += 1;
                        }
                        for _ in 0..4 {
                            if bits.bit() {
                                start -= 1;
                                coef_list[start] = coef;
                                mode_list[start] = 3;
                            } else if count < 64 {
                                block[usize::from(SCAN[coef as usize & 63])] =
                                    signed_coefficient(bits, nb);
                                placed[count] = coef as usize & 63;
                                count += 1;
                            }
                            coef += 1;
                        }
                    }
                    1 => {
                        mode_list[pos] = 2;
                        for _ in 0..3 {
                            coef += 4;
                            coef_list[end] = coef;
                            mode_list[end] = 2;
                            end += 1;
                        }
                    }
                    _ => {
                        if count < 64 {
                            block[usize::from(SCAN[coef as usize & 63])] =
                                signed_coefficient(bits, nb);
                            placed[count] = coef as usize & 63;
                            count += 1;
                        }
                        coef_list[pos] = 0;
                        mode_list[pos] = 0;
                        pos += 1;
                    }
                }
            }
            nbits -= 1;
        }
        let quant = &quant[bits.read(4) as usize];
        let scale = |value: i32, q: u32| ((value as u32).wrapping_mul(q) as i32) >> 11;
        block[0] = scale(block[0], quant[0]);
        for &coef in &placed[..count] {
            let at = usize::from(SCAN[coef]);
            block[at] = scale(block[at], quant[at]);
        }
        block
    }

    /// A residue block's corrections to the motion-copied pixels.
    fn residue(bits: &mut Bits, mut masks: i32) -> [i32; 64] {
        let mut block = [0i32; 64];
        let mut coef_list = [0i32; 128];
        let mut mode_list = [0u8; 128];
        let (mut start, mut end) = (64usize, 64usize);
        for (coef, mode) in [(4, 0), (24, 0), (44, 0), (0, 2)] {
            coef_list[end] = coef;
            mode_list[end] = mode;
            end += 1;
        }
        let mut nonzero = [0usize; 64];
        let mut nonzero_count = 0usize;
        let mut mask = 1i32 << bits.read(3);
        while mask != 0 {
            for &at in &nonzero[..nonzero_count] {
                if !bits.bit() {
                    continue;
                }
                if block[at] < 0 {
                    block[at] -= mask;
                } else {
                    block[at] += mask;
                }
                masks -= 1;
                if masks < 0 {
                    return block;
                }
            }
            let mut pos = start;
            while pos < end {
                if (coef_list[pos] == 0 && mode_list[pos] == 0) || !bits.bit() {
                    pos += 1;
                    continue;
                }
                let mut coef = coef_list[pos];
                let mode = mode_list[pos];
                match mode {
                    0 | 2 => {
                        if mode == 0 {
                            coef_list[pos] = coef + 4;
                            mode_list[pos] = 1;
                        } else {
                            coef_list[pos] = 0;
                            mode_list[pos] = 0;
                            pos += 1;
                        }
                        for _ in 0..4 {
                            if bits.bit() {
                                start -= 1;
                                coef_list[start] = coef;
                                mode_list[start] = 3;
                            } else {
                                let at = usize::from(SCAN[coef as usize & 63]);
                                if nonzero_count < 64 {
                                    nonzero[nonzero_count] = at;
                                    nonzero_count += 1;
                                }
                                block[at] = if bits.bit() { -mask } else { mask };
                                masks -= 1;
                                if masks < 0 {
                                    return block;
                                }
                            }
                            coef += 1;
                        }
                    }
                    1 => {
                        mode_list[pos] = 2;
                        for _ in 0..3 {
                            coef += 4;
                            coef_list[end] = coef;
                            mode_list[end] = 2;
                            end += 1;
                        }
                    }
                    _ => {
                        let at = usize::from(SCAN[coef as usize & 63]);
                        if nonzero_count < 64 {
                            nonzero[nonzero_count] = at;
                            nonzero_count += 1;
                        }
                        block[at] = if bits.bit() { -mask } else { mask };
                        coef_list[pos] = 0;
                        mode_list[pos] = 0;
                        pos += 1;
                        masks -= 1;
                        if masks < 0 {
                            return block;
                        }
                    }
                }
            }
            mask >>= 1;
        }
        block
    }

    fn plane(&mut self, bits: &mut Bits, index: usize, chroma: bool) -> Result<(), String> {
        let (width, height) = (self.current.width, self.current.height);
        let bw = if chroma {
            width.div_ceil(16)
        } else {
            width.div_ceil(8)
        };
        let bh = if chroma {
            height.div_ceil(16)
        } else {
            height.div_ceil(8)
        };
        let stride = self.current.strides[index];
        let plane_width = width >> usize::from(chroma);
        if self.revision == b'k' && bits.bit() {
            let fill = bits.read(8) as u8;
            self.current.planes[index].fill(fill);
            return Ok(());
        }
        self.init_lengths(plane_width.max(8), bw);
        for which in 0..BUNDLES {
            self.read_bundle(bits, which);
        }
        let mut dst = std::mem::take(&mut self.current.planes[index]);
        let prev = std::mem::take(&mut self.last.planes[index]);
        let result = self.blocks(bits, &mut dst, &prev, stride, bw, bh);
        self.current.planes[index] = dst;
        self.last.planes[index] = prev;
        result?;
        bits.align32();
        Ok(())
    }

    fn blocks(
        &mut self,
        bits: &mut Bits,
        dst: &mut [u8],
        prev: &[u8],
        stride: usize,
        bw: usize,
        bh: usize,
    ) -> Result<(), String> {
        // A motion vector's block starts inside the reference plane, read as
        // one run of memory: a block left of a row's start takes the end of
        // the row above, as Bink's own copy does.
        let last_start = (bw - 1) * 8 + stride * (bh - 1) * 8;
        let reference = |at: usize, x: i32, y: i32| -> Result<usize, String> {
            let from = at as isize + x as isize + y as isize * stride as isize;
            if from < 0 || from as usize > last_start {
                return Err("bink: a motion vector leaves the picture".into());
            }
            Ok(from as usize)
        };
        let copy = |dst: &mut [u8], at: usize, from: usize| {
            for row in 0..8 {
                let (d, s) = (at + row * stride, from + row * stride);
                dst[d..d + 8].copy_from_slice(&prev[s..s + 8]);
            }
        };
        for by in 0..bh {
            self.read_block_types(bits, BLOCK_TYPES)?;
            self.read_block_types(bits, SUB_BLOCK_TYPES)?;
            self.read_colors(bits)?;
            self.read_patterns(bits)?;
            self.read_motion(bits, X_OFF)?;
            self.read_motion(bits, Y_OFF)?;
            self.read_dcs(bits, INTRA_DC, false)?;
            self.read_dcs(bits, INTER_DC, true)?;
            self.read_runs(bits)?;
            let mut bx = 0;
            while bx < bw {
                let at = by * 8 * stride + bx * 8;
                let kind = self.value(BLOCK_TYPES) as u8;
                // A scaled block's other three quarters were drawn with it.
                if ((by & 1) != 0 || (bx & 1) != 0) && kind == SCALED {
                    bx += 2;
                    continue;
                }
                match kind {
                    SKIP => copy(dst, at, at),
                    SCALED => {
                        let sub = self.value(SUB_BLOCK_TYPES) as u8;
                        let small = match sub {
                            RUN => self.run_block(bits)?,
                            INTRA => {
                                let dc = self.value(INTRA_DC);
                                let coefs = self.dct_block(bits, dc, &INTRA_QUANT);
                                idct(&coefs).map(|v| v as u8)
                            }
                            FILL => {
                                let v = self.value(COLORS) as u8;
                                for row in 0..16 {
                                    let d = at + row * stride;
                                    if let Some(line) = dst.get_mut(d..d + 16) {
                                        line.fill(v);
                                    }
                                }
                                bx += 2;
                                continue;
                            }
                            PATTERN => {
                                let colors = [self.value(COLORS) as u8, self.value(COLORS) as u8];
                                let mut out = [0u8; 64];
                                for row in 0..8 {
                                    let mut v = self.value(PATTERNS_SRC);
                                    for col in 0..8 {
                                        out[row * 8 + col] = colors[(v & 1) as usize];
                                        v >>= 1;
                                    }
                                }
                                out
                            }
                            RAW => std::array::from_fn(|_| self.value(COLORS) as u8),
                            _ => return Err(format!("bink: scaled sub-block kind {sub}")),
                        };
                        for row in 0..16 {
                            let d = at + row * stride;
                            if let Some(line) = dst.get_mut(d..d + 16) {
                                for (col, pixel) in line.iter_mut().enumerate() {
                                    *pixel = small[(row / 2) * 8 + col / 2];
                                }
                            }
                        }
                        bx += 2;
                        continue;
                    }
                    MOTION => {
                        let (x, y) = (self.value(X_OFF), self.value(Y_OFF));
                        copy(dst, at, reference(at, x, y)?);
                    }
                    RUN => {
                        let values = self.run_block(bits)?;
                        put(dst, at, stride, &values);
                    }
                    RESIDUE => {
                        let (x, y) = (self.value(X_OFF), self.value(Y_OFF));
                        copy(dst, at, reference(at, x, y)?);
                        let masks = bits.read(7) as i32;
                        let block = Self::residue(bits, masks);
                        add(dst, at, stride, &block);
                    }
                    INTRA => {
                        let dc = self.value(INTRA_DC);
                        let coefs = self.dct_block(bits, dc, &INTRA_QUANT);
                        put(dst, at, stride, &idct(&coefs).map(|v| v as u8));
                    }
                    FILL => {
                        let v = self.value(COLORS) as u8;
                        put(dst, at, stride, &[v; 64]);
                    }
                    INTER => {
                        let (x, y) = (self.value(X_OFF), self.value(Y_OFF));
                        copy(dst, at, reference(at, x, y)?);
                        let dc = self.value(INTER_DC);
                        let coefs = self.dct_block(bits, dc, &INTER_QUANT);
                        add(dst, at, stride, &idct(&coefs));
                    }
                    PATTERN => {
                        let colors = [self.value(COLORS) as u8, self.value(COLORS) as u8];
                        let mut out = [0u8; 64];
                        for row in 0..8 {
                            let mut v = self.value(PATTERNS_SRC);
                            for col in 0..8 {
                                out[row * 8 + col] = colors[(v & 1) as usize];
                                v >>= 1;
                            }
                        }
                        put(dst, at, stride, &out);
                    }
                    RAW => {
                        let out: [u8; 64] = std::array::from_fn(|_| self.value(COLORS) as u8);
                        put(dst, at, stride, &out);
                    }
                    _ => return Err(format!("bink: block kind {kind}")),
                }
                bx += 1;
            }
        }
        Ok(())
    }
}

fn put(dst: &mut [u8], at: usize, stride: usize, values: &[u8; 64]) {
    for row in 0..8 {
        let d = at + row * stride;
        dst[d..d + 8].copy_from_slice(&values[row * 8..row * 8 + 8]);
    }
}

/// Adds corrections to a block; Bink wraps a pixel past 0 or 255.
fn add(dst: &mut [u8], at: usize, stride: usize, values: &[i32; 64]) {
    for row in 0..8 {
        for col in 0..8 {
            let d = at + row * stride + col;
            dst[d] = (i32::from(dst[d]) + values[row * 8 + col]) as u8;
        }
    }
}

/// Bink's integer 8x8 inverse DCT (12-bit fixed point constants).
fn idct(block: &[i32; 64]) -> [i32; 64] {
    const A1: i32 = 2896;
    const A2: i32 = 2217;
    const A3: i32 = 3784;
    const A4: i32 = -5352;
    fn transform(s: [i32; 8], out: &mut [i32; 8], round: bool) {
        let a0 = s[0] + s[4];
        let a1 = s[0] - s[4];
        let a2 = s[2] + s[6];
        let a3 = (A1 * (s[2] - s[6])) >> 11;
        let a4 = s[5] + s[3];
        let a5 = s[5] - s[3];
        let a6 = s[1] + s[7];
        let a7 = s[1] - s[7];
        let b0 = a4 + a6;
        let b1 = (A3 * (a5 + a7)) >> 11;
        let b2 = ((A4 * a5) >> 11) - b0 + b1;
        let b3 = ((A1 * (a6 - a4)) >> 11) - b2;
        let b4 = ((A2 * a7) >> 11) + b3 - b1;
        let values = [
            a0 + a2 + b0,
            a1 + a3 - a2 + b2,
            a1 - a3 + a2 + b3,
            a0 - a2 - b4,
            a0 - a2 + b4,
            a1 - a3 + a2 - b3,
            a1 + a3 - a2 - b2,
            a0 + a2 - b0,
        ];
        for (o, v) in out.iter_mut().zip(values) {
            *o = if round { (v + 0x7f) >> 8 } else { v };
        }
    }
    let mut temp = [0i32; 64];
    for col in 0..8 {
        let column: [i32; 8] = std::array::from_fn(|row| block[row * 8 + col]);
        let mut out = [0i32; 8];
        if column[1..].iter().all(|&v| v == 0) {
            out = [column[0]; 8];
        } else {
            transform(column, &mut out, false);
        }
        for row in 0..8 {
            temp[row * 8 + col] = out[row];
        }
    }
    let mut result = [0i32; 64];
    for row in 0..8 {
        let line: [i32; 8] = std::array::from_fn(|col| temp[row * 8 + col]);
        let mut out = [0i32; 8];
        transform(line, &mut out, true);
        result[row * 8..row * 8 + 8].copy_from_slice(&out);
    }
    result
}
