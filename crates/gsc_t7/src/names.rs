/// Generated: `u32` count, then per name its hash and the offset of its text,
/// sorted by hash, then the NUL-terminated texts.
static NAMES: &[u8] = include_bytes!("names.bin");

/// The name a hash stands for, when the scripts' source names it.
pub fn name_of(hash: u32) -> Option<&'static str> {
    let word = |at: usize| u32::from_le_bytes(NAMES[at..at + 4].try_into().unwrap());
    let count = word(0) as usize;
    let texts = 4 + count * 8;
    let (mut low, mut high) = (0, count);
    while low < high {
        let mid = (low + high) / 2;
        let at = 4 + mid * 8;
        match word(at).cmp(&hash) {
            core::cmp::Ordering::Less => low = mid + 1,
            core::cmp::Ordering::Greater => high = mid,
            core::cmp::Ordering::Equal => {
                let start = texts + word(at + 4) as usize;
                let end = start + NAMES[start..].iter().position(|&b| b == 0)?;
                return core::str::from_utf8(&NAMES[start..end]).ok();
            }
        }
    }
    None
}
