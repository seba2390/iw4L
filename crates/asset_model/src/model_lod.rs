pub use fastfile_t5::xmodel_lod as t5_lod;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ModelLodSelector {
    Iw4 {
        lod_start: u8,
        num_lods: u8,
        lod_dist: [f32; 4],
    },

    T5 {
        num_lods: i16,
        lod_dist: [f32; 4],
    },

    /// Black Ops 2's: picked as Black Ops does (`t5_lod::xmodel_get_lod_for_dist`),
    /// from distances its own LOD parms ramp
    /// (`model_skel_t6::RIGID_LOD_FOV_HYSTERESIS`).
    T6 {
        num_lods: i16,
        lod_dist: [f32; 4],
    },
}

impl ModelLodSelector {
    #[must_use]
    pub fn lod_dist(self) -> [f32; 4] {
        match self {
            Self::Iw4 { lod_dist, .. } | Self::T5 { lod_dist, .. } | Self::T6 { lod_dist, .. } => {
                lod_dist
            }
        }
    }

    #[must_use]
    pub fn num_lods(self) -> i16 {
        match self {
            Self::Iw4 { num_lods, .. } => i16::from(num_lods),
            Self::T5 { num_lods, .. } | Self::T6 { num_lods, .. } => num_lods,
        }
    }
}
