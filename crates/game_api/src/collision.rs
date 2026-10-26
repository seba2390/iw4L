//! The solid pieces a game's own collision sweep reads: the map's brushes and
//! collision partitions with the convex point sets the map stores for them,
//! and the entities' brushes and bodies.

/// The convex point sets a map stores beside its brushes and its collision
/// partitions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HullTables {
    /// The brushes' corner points; `brush_ranges[i]` is brush `i`'s first
    /// point and count (none: the brush is its box alone).
    pub brush_points: Vec<[f32; 3]>,
    pub brush_ranges: Vec<[u32; 2]>,
    /// The partitions' distinct vertex numbers into the collision mesh's
    /// vertices; `partition_ranges[i]` is partition `i`'s first and count.
    pub partition_points: Vec<u16>,
    pub partition_ranges: Vec<[u32; 2]>,
    /// Each clip material's name, surface flags and content flags.
    pub materials: Vec<(String, u32, u32)>,
}

impl HullTables {
    pub fn brush_points(&self, brush: usize) -> &[[f32; 3]] {
        self.brush_ranges.get(brush).map_or(&[], |&[first, count]| {
            let first = first as usize;
            self.brush_points
                .get(first..first + count as usize)
                .unwrap_or(&[])
        })
    }

    pub fn partition_points(&self, partition: usize) -> &[u16] {
        self.partition_ranges
            .get(partition)
            .map_or(&[], |&[first, count]| {
                let first = first as usize;
                self.partition_points
                    .get(first..first + count as usize)
                    .unwrap_or(&[])
            })
    }
}

/// Where a piece's own space sits in the world: `point_world = origin +
/// axis[0] * x + axis[1] * y + axis[2] * z`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub origin: [f32; 3],
    pub axis: [[f32; 3]; 3],
}

impl Placement {
    pub const fn at(origin: [f32; 3]) -> Self {
        Self {
            origin,
            axis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum SolidShape<'a> {
    /// A brush in its own space: its box, its axial sides' surface flags
    /// (min x, y, z, then max x, y, z), its other sides with their surface
    /// flags, and its corner points (none when the map keeps it as its box
    /// alone).
    Brush {
        mins: [f32; 3],
        maxs: [f32; 3],
        axial_surface_flags: [u32; 6],
        sides: &'a [[f32; 4]],
        side_surface_flags: &'a [u32],
        points: &'a [[f32; 3]],
    },
    /// A collision partition: its triangles (three vertex numbers each) and
    /// its distinct vertices, numbers into `verts`.
    Partition {
        verts: &'a [[f32; 3]],
        tris: &'a [u32],
        points: &'a [u16],
        surface_flags: u32,
    },
    /// A player's or an actor's body: its box in its own space.
    Body { mins: [f32; 3], maxs: [f32; 3] },
}

/// One thing a sweep may meet.
#[derive(Clone, Copy, Debug)]
pub struct SolidPiece<'a> {
    pub shape: SolidShape<'a>,
    /// Its box in the world.
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// Its own space; `None` when that is the world's.
    pub placement: Option<Placement>,
    pub contents: u32,
    /// [`crate::movement::ENTITY_WORLD`] for the map's own pieces.
    pub entity: i32,
}

/// What a game's own collision lists its pieces from.
pub trait SolidSource {
    /// Whether [`SolidSource::solid_pieces`] can list the pieces.
    fn lists_solid_pieces(&self) -> bool {
        false
    }

    /// Adds to `out` every solid piece whose box meets `mins`..`maxs` and
    /// whose contents `mask` names, but `pass_entity`'s own; `false` when
    /// the match cannot list them.
    fn solid_pieces<'s>(
        &'s self,
        _mins: [f32; 3],
        _maxs: [f32; 3],
        _mask: u32,
        _pass_entity: i32,
        _out: &mut Vec<SolidPiece<'s>>,
    ) -> bool {
        false
    }

    /// The surface flags of the clip material named `name`.
    fn clip_material_surface_flags(&self, _name: &str) -> Option<u32> {
        None
    }
}
