//! Bink's bit streams: bits are taken from the low end of each byte first.

pub(crate) struct Bits<'a> {
    data: &'a [u8],
    /// The read position in bits.
    pos: usize,
}

impl<'a> Bits<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub(crate) fn position(&self) -> usize {
        self.pos
    }

    pub(crate) fn left(&self) -> isize {
        (self.data.len() * 8) as isize - self.pos as isize
    }

    /// The next `n` bits (at most 32) without consuming them; past the end
    /// the stream reads zeros.
    pub(crate) fn peek(&self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let byte = self.pos >> 3;
        let shift = self.pos & 7;
        let mut word = 0u64;
        for i in 0..5 {
            if let Some(&b) = self.data.get(byte + i) {
                word |= u64::from(b) << (8 * i);
            }
        }
        ((word >> shift) & ((1u64 << n) - 1)) as u32
    }

    pub(crate) fn skip(&mut self, n: usize) {
        self.pos += n;
    }

    pub(crate) fn read(&mut self, n: u32) -> u32 {
        let value = self.peek(n);
        self.pos += n as usize;
        value
    }

    pub(crate) fn bit(&mut self) -> bool {
        self.read(1) != 0
    }

    /// Skips to the next multiple of 32 bits from the stream's start.
    pub(crate) fn align32(&mut self) {
        let rem = self.pos & 31;
        if rem != 0 {
            self.pos += 32 - rem;
        }
    }
}
