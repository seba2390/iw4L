use std::collections::HashMap;

use asset_core::{AssetNamespace, AssetRef, MaterialKey};
use asset_material::{MaterialCatalog, TS_COLOR_MAP, TS_SPECULAR_MAP};

use crate::CapturedStringTable;

/// The namespace of Black Ops' weapon option tables.
const BLACK_OPS: AssetNamespace = AssetNamespace::T5;

#[derive(Clone, Debug)]
pub struct WeaponCamouflage {
    pub slot: u8,
    pub name: String,
    pub caption_key: String,
    pub preview: String,
    pub materials: Vec<(MaterialKey, MaterialKey)>,
}

/// A material variant one of Black Ops' optic options draws with: the
/// option's lens or reticle row.
#[derive(Clone, Debug)]
pub struct WeaponOptic {
    pub kind: OpticKind,
    pub index: u8,
    pub material: MaterialKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OpticKind {
    Lens,
    Reticle,
}

impl OpticKind {
    /// The rows' type in `weaponoptions.csv`, the first column holding an
    /// image and how many rows Black Ops reads.
    fn rows(self) -> (&'static str, i32, usize) {
        match self {
            Self::Lens => ("lens", 3, 16),
            Self::Reticle => ("reticle", 2, 64),
        }
    }
}

/// Black Ops' optic options, read from `weaponoptions.csv`: every material
/// one of whose images is a column's first lens or reticle image gets a
/// variant per row of that kind, the image swapped for that row's in the
/// same column. Black Ops swaps these images when it draws the first-person
/// weapon (ten columns of each kind).
pub(crate) fn prepare_t5_optics(
    table: &CapturedStringTable,
    materials: &mut MaterialCatalog,
) -> HashMap<MaterialKey, Vec<WeaponOptic>> {
    let ns = BLACK_OPS;
    let mut swaps: Vec<(usize, OpticKind, u8, Vec<(usize, usize)>)> = Vec::new();
    for kind in [OpticKind::Lens, OpticKind::Reticle] {
        let (name, first, count) = kind.rows();
        let rows: Vec<i32> = (0..table.rows as i32)
            .filter(|&row| table.cell(row, 1) == name)
            .take(count)
            .collect();
        let Some((&base, others)) = rows.split_first() else {
            continue;
        };
        for col in first..(first + 10).min(table.columns as i32) {
            let from = table.cell(base, col);
            let Some(from) = (!from.is_empty())
                .then(|| materials.image_index_by_key(ns, from))
                .flatten()
            else {
                continue;
            };
            for (i, &row) in others.iter().enumerate() {
                let to = table.cell(row, col);
                let Some(to) = (!to.is_empty())
                    .then(|| materials.image_index_by_key(ns, to))
                    .flatten()
                else {
                    continue;
                };
                let index = (i + 1) as u8;
                match swaps
                    .iter_mut()
                    .find(|(f, k, n, _)| *f == from && *k == kind && *n == index)
                {
                    Some((_, _, _, list)) => list.push((from, to)),
                    None => swaps.push((from, kind, index, vec![(from, to)])),
                }
            }
        }
    }
    let mut out: HashMap<MaterialKey, Vec<WeaponOptic>> = HashMap::new();
    if swaps.is_empty() {
        return out;
    }
    let sources: Vec<(MaterialKey, asset_material::AuthoredMaterial)> = materials
        .materials
        .iter()
        .filter(|material| {
            material.textures.iter().any(|texture| {
                texture
                    .image
                    .is_some_and(|image| swaps.iter().any(|(from, ..)| *from == image))
            })
        })
        .filter_map(|material| {
            let AssetRef::Real(name) = &material.name else {
                return None;
            };
            Some((
                MaterialKey {
                    namespace: ns,
                    name: name.clone(),
                },
                material.clone(),
            ))
        })
        .collect();
    for (key, source) in sources {
        let mut variants: Vec<(OpticKind, u8, asset_material::AuthoredMaterial)> = Vec::new();
        for (_, kind, index, list) in &swaps {
            for texture in &source.textures {
                let Some(image) = texture.image else {
                    continue;
                };
                let Some(&(_, to)) = list.iter().find(|(from, _)| *from == image) else {
                    continue;
                };
                let at = match variants
                    .iter()
                    .position(|(k, n, _)| k == kind && n == index)
                {
                    Some(at) => at,
                    None => {
                        variants.push((*kind, *index, source.clone()));
                        variants.len() - 1
                    }
                };
                for swapped in &mut variants[at].2.textures {
                    if swapped.image == Some(image) {
                        swapped.image = Some(to);
                    }
                }
            }
        }
        for (kind, index, mut material) in variants {
            let role = match kind {
                OpticKind::Lens => "lens",
                OpticKind::Reticle => "reticle",
            };
            let name = format!("{}/optic/{role}/{index}", key.name);
            material.name = AssetRef::Real(name.clone());
            materials.link_material(material);
            out.entry(key.clone()).or_default().push(WeaponOptic {
                kind,
                index,
                material: MaterialKey {
                    namespace: ns,
                    name,
                },
            });
        }
    }
    out
}

/// Black Ops' sixteen reticle colours from `weaponoptions.csv`: the
/// `reticle_color` rows, each `r g b` over 255 with its fourth component
/// minus one over 255; a row without a colour, and every slot past the
/// rows, stays minus one throughout. Empty when the table has no rows.
pub(crate) fn t5_reticle_colours(table: &CapturedStringTable) -> Vec<[f32; 4]> {
    const SCALE: f32 = f32::from_bits(0x3b80_8081);
    const SLOTS: usize = 16;
    let mut colours: Vec<[f32; 4]> = (0..table.rows as i32)
        .filter(|&row| table.cell(row, 1) == "reticle_color")
        .take(SLOTS)
        .map(|row| {
            let mut colour = [-1.0f32; 4];
            let cell = table.cell(row, 2);
            if !cell.is_empty() {
                for (slot, value) in colour.iter_mut().zip(cell.split_whitespace()) {
                    if let Ok(value) = value.parse::<f32>() {
                        *slot = value;
                    }
                }
                colour = colour.map(|c| c * SCALE);
            }
            colour
        })
        .collect();
    if !colours.is_empty() {
        colours.resize(SLOTS, [-1.0; 4]);
    }
    colours
}

pub(crate) fn prepare_t5(
    table: &CapturedStringTable,
    choices: Option<&CapturedStringTable>,
    parent: &str,
    keys: &[MaterialKey],
    materials: &mut MaterialCatalog,
    variants: &mut HashMap<(usize, u8, bool), MaterialKey>,
) -> Vec<WeaponCamouflage> {
    let ns = BLACK_OPS;
    let Some(weapon) = (0..table.rows as i32)
        .find(|&row| table.cell(row, 1) == "weapon" && table.cell(row, 2) == parent)
    else {
        return Vec::new();
    };
    let Some(base) = (0..table.rows as i32)
        .find(|&row| table.cell(row, 1) == "camo" && table.cell(row, 0) == "0")
    else {
        return Vec::new();
    };
    let image = |name: &str, materials: &MaterialCatalog| materials.image_index_by_key(ns, name);
    let source_materials: Vec<_> = keys
        .iter()
        .filter_map(|key| {
            let id = materials.material_index_by_key(key)?.order();
            Some((key.clone(), id, materials.materials.get(id)?.clone()))
        })
        .collect();
    let mut out = Vec::new();
    for row in 0..table.rows as i32 {
        if table.cell(row, 1) != "camo" {
            continue;
        }
        let Ok(slot @ 1..=63) = table.cell(row, 0).parse::<u8>() else {
            continue;
        };
        let choice = choices.and_then(|choices| {
            (0..choices.rows as i32).find(|&c| {
                choices.cell(c, 1) == "camo" && choices.cell(c, 0).parse::<u8>() == Ok(slot)
            })
        });
        if choices.is_some() && choice.is_none() {
            continue;
        }
        let gold = table.cell(row, 2) == "gold";
        if choices.is_none() && !gold {
            continue;
        }
        let mut swaps = Vec::new();
        for (key, id, source) in &source_materials {
            let mut material = source.clone();
            let mut changed = false;
            let mut complete = true;
            let mut gold_material = false;
            if gold {
                let group = if (5..=8)
                    .chain(13..=17)
                    .any(|col| key.name == format!("mc/{}", table.cell(weapon, col)))
                {
                    "gold"
                } else if (9..=12)
                    .chain(18..=22)
                    .any(|col| key.name == format!("mc/{}", table.cell(weapon, col)))
                {
                    "black"
                } else {
                    continue;
                };
                gold_material = group == "gold";
                let Some(palette) = (0..table.rows as i32).find(|&r| table.cell(r, 1) == group)
                else {
                    continue;
                };
                for texture in &mut material.textures {
                    let col = match texture.semantic {
                        TS_COLOR_MAP => 2,
                        TS_SPECULAR_MAP => 3,
                        27 => 4,
                        19 => 5,
                        _ => continue,
                    };
                    let name = table.cell(palette, col);
                    if name.is_empty() {
                        continue;
                    }
                    match image(name, materials) {
                        Some(to) => {
                            texture.image = Some(to);
                            changed = true;
                        }
                        None => complete = false,
                    }
                }
            } else {
                for texture in &mut material.textures {
                    let Some(from) = texture.image.and_then(|id| materials.images.get(id)) else {
                        continue;
                    };
                    let Some(column) = (2..table.columns as i32).find(|&col| {
                        let name = table.cell(base, col);
                        !name.is_empty()
                            && from.name.as_str() == name
                            && (name == table.cell(weapon, 3)
                                || name == table.cell(weapon, 4)
                                || col == 2)
                    }) else {
                        continue;
                    };
                    match image(table.cell(row, column), materials) {
                        Some(to) => {
                            texture.image = Some(to);
                            changed = true;
                        }
                        None => complete = false,
                    }
                }
            }
            if !changed || !complete {
                continue;
            }
            if let Some(variant) = variants.get(&(*id, slot, gold_material)) {
                swaps.push((key.clone(), variant.clone()));
                continue;
            }
            let role = if gold_material {
                "gold"
            } else if gold {
                "black"
            } else {
                "pattern"
            };
            let name = format!("{}/camouflage/{slot}/{role}", key.name);
            material.name = AssetRef::Real(name.clone());
            materials.link_material(material);
            let to = MaterialKey {
                namespace: ns,
                name,
            };
            variants.insert((*id, slot, gold_material), to.clone());
            swaps.push((key.clone(), to));
        }
        if !swaps.is_empty() {
            let (name, caption_key, preview) = match (choices, choice) {
                (Some(choices), Some(choice)) => (
                    choices
                        .cell(choice, 3)
                        .strip_prefix("MPUI_CAMO_")
                        .unwrap_or(choices.cell(choice, 4))
                        .to_ascii_lowercase(),
                    choices.cell(choice, 3).to_owned(),
                    format!("t5:material/{}", choices.cell(choice, 6)),
                ),
                _ => (
                    table.cell(row, 2).to_ascii_lowercase(),
                    String::new(),
                    String::new(),
                ),
            };
            out.push(WeaponCamouflage {
                slot,
                name,
                caption_key,
                preview,
                materials: swaps,
            });
        }
    }
    out
}
