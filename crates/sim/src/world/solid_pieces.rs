//! The solid pieces near a sweep, for a game whose own collision reads the
//! map's convex point sets: the world's brushes and collision partitions,
//! the brush entities' and models' brushes, and the bodies.

use game_api::collision::{HullTables, Placement, SolidPiece, SolidShape};
use game_api::movement::ENTITY_WORLD;
use std::sync::Arc;

use super::{
    CONTENTS_BODY, ClipMapRef, CmodelTransform, SimBrush, SimClipBsp, SimClipCmodels, SimClipMesh,
    TraceExtents, clip_handle_to_model, walk_clip_tree,
};
use crate::bullet_collision::LinkedBrushCollisionBrush;

/// What a sweep may meet beyond the world.
pub(crate) struct SolidScene<'a> {
    pub(crate) brushes: &'a [SimBrush],
    pub(crate) bsp: &'a SimClipBsp,
    pub(crate) mesh: &'a SimClipMesh,
    pub(crate) hulls: &'a HullTables,
    pub(crate) cmodels: &'a SimClipCmodels,
    pub(crate) linked: &'a [LinkedBrushCollisionBrush],
    /// The solid models with movement brushes, each where it stands.
    pub(crate) models: &'a [(Placement, Arc<xmodel_runtime::RetainedModelCapability>)],
    pub(crate) bodies: &'a [crate::step::PlayerBodyClip],
}

thread_local! {
    static SEEN: std::cell::RefCell<(Vec<u32>, Vec<u32>, u32)> =
        const { std::cell::RefCell::new((Vec::new(), Vec::new(), 0)) };
}

fn overlaps(a_mins: [f32; 3], a_maxs: [f32; 3], b_mins: [f32; 3], b_maxs: [f32; 3]) -> bool {
    (0..3).all(|i| a_mins[i] <= b_maxs[i] && b_mins[i] <= a_maxs[i])
}

/// A brush's box from its first six planes (+x, -x, +y, -y, +z, -z).
fn brush_box(brush: &SimBrush) -> Option<([f32; 3], [f32; 3])> {
    let p = brush.planes.get(..6)?;
    Some((
        [-p[1][3], -p[3][3], -p[5][3]],
        [p[0][3], p[2][3], p[4][3]],
    ))
}

/// The axial sides' surface flags, min x, y, z then max x, y, z.
fn brush_axial_flags(brush: &SimBrush) -> [u32; 6] {
    let f = |i: usize| brush.plane_surface_flags.get(i).copied().unwrap_or(0);
    [f(1), f(3), f(5), f(0), f(2), f(4)]
}

fn placed_box(
    mins: [f32; 3],
    maxs: [f32; 3],
    placement: &Placement,
) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for corner in 0..8 {
        let p = [
            if corner & 1 == 0 { mins[0] } else { maxs[0] },
            if corner & 2 == 0 { mins[1] } else { maxs[1] },
            if corner & 4 == 0 { mins[2] } else { maxs[2] },
        ];
        for i in 0..3 {
            let w = placement.origin[i]
                + p[0] * placement.axis[0][i]
                + p[1] * placement.axis[1][i]
                + p[2] * placement.axis[2][i];
            lo[i] = lo[i].min(w);
            hi[i] = hi[i].max(w);
        }
    }
    (lo, hi)
}

fn brush_piece<'s>(
    brush: &'s SimBrush,
    points: &'s [[f32; 3]],
    placement: Option<Placement>,
    world_box: ([f32; 3], [f32; 3]),
) -> Option<SolidPiece<'s>> {
    let (mins, maxs) = brush_box(brush)?;
    Some(SolidPiece {
        shape: SolidShape::Brush {
            mins,
            maxs,
            axial_surface_flags: brush_axial_flags(brush),
            sides: &brush.planes[6..],
            side_surface_flags: brush.plane_surface_flags.get(6..).unwrap_or(&[]),
            points,
        },
        mins: world_box.0,
        maxs: world_box.1,
        placement,
        contents: brush.contents,
        entity: ENTITY_WORLD,
    })
}

/// Adds the pieces whose box meets `mins`..`maxs` and whose contents `mask`
/// names: the world's first, then the entities'.
pub(crate) fn gather<'s>(
    scene: &SolidScene<'s>,
    mins: [f32; 3],
    maxs: [f32; 3],
    mask: u32,
    pass_entity: i32,
    out: &mut Vec<SolidPiece<'s>>,
) {
    SEEN.with(|seen| {
        let mut seen = seen.borrow_mut();
        let (brushes_seen, partitions_seen, stamp) = &mut *seen;
        *stamp = stamp.wrapping_add(1);
        if *stamp == 0 {
            brushes_seen.fill(0);
            partitions_seen.fill(0);
            *stamp = 1;
        }
        let stamp = *stamp;
        brushes_seen.resize(scene.brushes.len(), 0);
        partitions_seen.resize(scene.mesh.tables.partitions.len(), 0);
        world(
            scene,
            mins,
            maxs,
            mask,
            brushes_seen,
            partitions_seen,
            stamp,
            out,
        );
    });
    entities(scene, mins, maxs, mask, pass_entity, out);
}

#[allow(clippy::too_many_arguments)]
fn world<'s>(
    scene: &SolidScene<'s>,
    mins: [f32; 3],
    maxs: [f32; 3],
    mask: u32,
    brushes_seen: &mut [u32],
    partitions_seen: &mut [u32],
    stamp: u32,
    out: &mut Vec<SolidPiece<'s>>,
) {
    let map = ClipMapRef {
        nodes: &scene.bsp.nodes,
        leaves: &scene.bsp.leaves,
        leafbrushes: &scene.bsp.leafbrushes,
        brushes: scene.brushes,
    };
    let center: [f32; 3] = core::array::from_fn(|i| (mins[i] + maxs[i]) * 0.5);
    let half: [f32; 3] = core::array::from_fn(|i| (maxs[i] - mins[i]) * 0.5);
    let ext = TraceExtents::new(center, center, half.map(|h| -h), half, mask);
    let tables = &scene.mesh.tables;
    let mut leaves = Vec::new();
    walk_clip_tree(&map, &ext, &|| 1.0, &mut |leaf| leaves.push(*leaf));
    for leaf in leaves {
        let first = leaf.first_brush as usize;
        for &index in scene
            .bsp
            .leafbrushes
            .get(first..first + usize::from(leaf.num_brushes))
            .unwrap_or(&[])
        {
            let index = usize::from(index);
            let Some(brush) = scene.brushes.get(index) else {
                continue;
            };
            if brushes_seen[index] == stamp || brush.contents & mask == 0 {
                continue;
            }
            brushes_seen[index] = stamp;
            let Some((lo, hi)) = brush_box(brush) else {
                continue;
            };
            if !overlaps(lo, hi, mins, maxs) {
                continue;
            }
            if let Some(piece) = brush_piece(brush, scene.hulls.brush_points(index), None, (lo, hi))
            {
                out.push(piece);
            }
        }
        let first = usize::from(leaf.first_coll_aabb_index);
        for root in first..first + usize::from(leaf.coll_aabb_count) {
            let Some(node) = tables.aabb_trees.get(root) else {
                continue;
            };
            let material = scene.hulls.materials.get(usize::from(node.material_index));
            if material.is_none_or(|m| m.2 & mask == 0) {
                continue;
            }
            aabb_node(scene, root, mins, maxs, partitions_seen, stamp, out);
        }
    }
}

fn aabb_node<'s>(
    scene: &SolidScene<'s>,
    index: usize,
    mins: [f32; 3],
    maxs: [f32; 3],
    partitions_seen: &mut [u32],
    stamp: u32,
    out: &mut Vec<SolidPiece<'s>>,
) {
    let tables = &scene.mesh.tables;
    let Some(node) = tables.aabb_trees.get(index) else {
        return;
    };
    let lo: [f32; 3] = core::array::from_fn(|i| node.origin[i] - node.half_size[i]);
    let hi: [f32; 3] = core::array::from_fn(|i| node.origin[i] + node.half_size[i]);
    if !overlaps(lo, hi, mins, maxs) {
        return;
    }
    if node.child_count != 0 {
        let first = node.u.max(0) as usize;
        for child in first..first + usize::from(node.child_count) {
            aabb_node(scene, child, mins, maxs, partitions_seen, stamp, out);
        }
        return;
    }
    let Ok(partition) = usize::try_from(node.u) else {
        return;
    };
    let Some(part) = tables.partitions.get(partition) else {
        return;
    };
    if partitions_seen[partition] == stamp {
        return;
    }
    partitions_seen[partition] = stamp;
    let Some(&(_, surface_flags, contents)) =
        scene.hulls.materials.get(usize::from(node.material_index))
    else {
        return;
    };
    let first = part.first_tri.max(0) as usize * 3;
    let tris = tables
        .tri_indices
        .get(first..first + usize::from(part.tri_count) * 3)
        .unwrap_or(&[]);
    out.push(SolidPiece {
        shape: SolidShape::Partition {
            verts: &tables.verts,
            tris,
            points: scene.hulls.partition_points(partition),
            surface_flags,
        },
        mins: lo,
        maxs: hi,
        placement: None,
        contents,
        entity: ENTITY_WORLD,
    });
}

fn entities<'s>(
    scene: &SolidScene<'s>,
    mins: [f32; 3],
    maxs: [f32; 3],
    mask: u32,
    pass_entity: i32,
    out: &mut Vec<SolidPiece<'s>>,
) {
    for linked in scene.linked {
        let Some(cmodel) = clip_handle_to_model(&scene.cmodels.models, linked.cmodel_handle)
        else {
            continue;
        };
        let Some(transform) = CmodelTransform::new(linked.origin, linked.angles)
        else {
            continue;
        };
        let placement = Placement {
            origin: linked.origin,
            axis: transform.axis(),
        };
        let first = cmodel.first_brush as usize;
        for &index in scene
            .bsp
            .leafbrushes
            .get(first..first + usize::from(cmodel.num_brushes))
            .unwrap_or(&[])
        {
            let index = usize::from(index);
            let Some(brush) = scene.brushes.get(index) else {
                continue;
            };
            if brush.contents & mask == 0 {
                continue;
            }
            let Some((lo, hi)) = brush_box(brush) else {
                continue;
            };
            let world_box = placed_box(lo, hi, &placement);
            if !overlaps(world_box.0, world_box.1, mins, maxs) {
                continue;
            }
            if let Some(piece) = brush_piece(
                brush,
                scene.hulls.brush_points(index),
                Some(placement),
                world_box,
            ) {
                out.push(piece);
            }
        }
    }
    for (placement, capability) in scene.models {
        let placement = *placement;
        for brush in &capability.movement_brushes {
            if brush.contents & mask == 0 || brush.planes.len() < 6 {
                continue;
            }
            let p = &brush.planes;
            let lo = [-p[0][3], -p[1][3], -p[2][3]];
            let hi = [p[3][3], p[4][3], p[5][3]];
            let world_box = placed_box(lo, hi, &placement);
            if !overlaps(world_box.0, world_box.1, mins, maxs) {
                continue;
            }
            let f = |i: usize| brush.plane_surface_flags.get(i).copied().unwrap_or(0);
            out.push(SolidPiece {
                shape: SolidShape::Brush {
                    mins: lo,
                    maxs: hi,
                    axial_surface_flags: [f(0), f(1), f(2), f(3), f(4), f(5)],
                    sides: &brush.planes[6..],
                    side_surface_flags: brush.plane_surface_flags.get(6..).unwrap_or(&[]),
                    points: &brush.points,
                },
                mins: world_box.0,
                maxs: world_box.1,
                placement: Some(placement),
                contents: brush.contents,
                entity: ENTITY_WORLD,
            });
        }
    }
    if mask & CONTENTS_BODY == 0 {
        return;
    }
    for body in scene.bodies {
        if i32::from(body.entnum) == pass_entity {
            continue;
        }
        let lo: [f32; 3] = core::array::from_fn(|i| body.origin[i] + body.mins[i]);
        let hi: [f32; 3] = core::array::from_fn(|i| body.origin[i] + body.maxs[i]);
        if !overlaps(lo, hi, mins, maxs) {
            continue;
        }
        out.push(SolidPiece {
            shape: SolidShape::Body {
                mins: body.mins,
                maxs: body.maxs,
            },
            mins: lo,
            maxs: hi,
            placement: Some(Placement::at(body.origin)),
            contents: CONTENTS_BODY,
            entity: i32::from(body.entnum),
        });
    }
}

/// The solid models that have movement brushes, each with where it stands.
pub(crate) fn solid_models(
    rows: &[crate::bullet_collision::EntityCollisionCapabilities],
) -> Vec<(Placement, Arc<xmodel_runtime::RetainedModelCapability>)> {
    rows.iter()
        .filter(|row| row.solid)
        .filter_map(|row| {
            let dobj = row.dobj.as_ref()?;
            let capability = dobj.capability.as_ref()?;
            if capability.movement_brushes.is_empty() {
                return None;
            }
            let m = dobj.world_from_model;
            Some((
                Placement {
                    origin: m.w_axis.truncate().to_array(),
                    axis: [
                        m.x_axis.truncate().to_array(),
                        m.y_axis.truncate().to_array(),
                        m.z_axis.truncate().to_array(),
                    ],
                },
                Arc::clone(capability),
            ))
        })
        .collect()
}
