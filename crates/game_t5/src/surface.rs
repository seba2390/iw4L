//! Black Ops' surface types: its names by number, as its client builds
//! surface-suffixed sound aliases (`<prefix>_<surface>`).

/// Black Ops' surface names, by its surface numbers; any other is
/// `default`.
const NAMES: [&str; 31] = [
    "default",
    "bark",
    "brick",
    "carpet",
    "cloth",
    "concrete",
    "dirt",
    "flesh",
    "foliage",
    "glass",
    "grass",
    "gravel",
    "ice",
    "metal",
    "mud",
    "paper",
    "plaster",
    "rock",
    "sand",
    "snow",
    "water",
    "wood",
    "asphalt",
    "ceramic",
    "plastic",
    "rubber",
    "cushion",
    "fruit",
    "paintedmetal",
    "player",
    "tallgrass",
];

/// A surface's name by its number.
pub(crate) fn name(surface: u8) -> &'static str {
    NAMES
        .get(usize::from(surface))
        .copied()
        .unwrap_or("default")
}

/// The surface name a trace's surface flags carry (their bits 20 to 25).
pub(crate) fn name_of_flags(surface_flags: u32) -> &'static str {
    name(((surface_flags >> 20) & 0x3f) as u8)
}
