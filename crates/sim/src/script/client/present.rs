//! The client's own entities, presented by the client itself: Black Ops'
//! client entities (numbered from `0x400`, the client-only map entities and
//! those the client spawns) take no server entity. Each frame the client VM's
//! script movers with a model join the snapshot as script mover rows at their
//! own numbers, shown to the local client alone, with their models' DObjs; a
//! brush model (`*N`) draws as a brush mover. The runtime's server numbers
//! its entities past `0x400` (it spawns map entities Black Ops' server has no
//! spawn function for), so a client entity whose number a server entity holds
//! is presented at the highest free number instead.

use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::super::host::entities::EntityKind;
use super::super::{RoundScript, Value};
use crate::ScriptModelId;
use crate::bullet_collision::AuthorityDObjState;
use crate::gentity::ScriptMoverGentity;

/// The first client entity number.
pub(crate) const CLIENT_ENTITY_BASE: i32 = 0x400;

/// Snapshot row ids of the client's entities, by number, apart from the
/// server's spawned script models (`SPAWNED_PRESENCE_BASE` up to here).
pub(crate) const CLIENT_PRESENTED_BASE: u32 = 0x5000_0000;

/// The highest number a client entity is presented at: below the world's.
const PRESENTED_TOP: i32 = trace_iw4::ENTITYNUM_WORLD as i32 - 1;

/// The local client the client VM runs for sees its entities alone.
const LOCAL_VIEWER: u64 = 1;

/// Whether a snapshot row is one of the client's own entities.
pub(crate) fn client_presented(id: ScriptModelId) -> bool {
    id.to_wire() & 0xf000_0000 == CLIENT_PRESENTED_BASE
}

#[derive(Debug)]
struct Presented {
    mover: ScriptMoverGentity,
    model: Arc<str>,
    dobj: Option<AuthorityDObjState>,
    hidden: bool,
    moving: bool,
}

/// The client entities the local client draws, by client entity.
#[derive(Resource, Default)]
pub(crate) struct ClientPresentation {
    rows: BTreeMap<u64, Presented>,
}

impl ClientPresentation {
    /// The rows the snapshot carries.
    pub(crate) fn movers(&self) -> impl Iterator<Item = &ScriptMoverGentity> {
        self.rows.values().map(|row| &row.mover)
    }

    /// The shown models' DObjs, by row.
    pub(crate) fn dobjs(
        &self,
    ) -> impl Iterator<
        Item = (
            crate::AuthorityModelOwner,
            &xmodel_runtime::DObjSemanticState,
        ),
    > {
        self.rows
            .values()
            .filter(|row| !row.hidden)
            .filter_map(|row| {
                row.dobj.as_ref().map(|dobj| {
                    (
                        crate::AuthorityModelOwner::ScriptModel(row.mover.id),
                        &dobj.semantic_state,
                    )
                })
            })
    }

    /// A presented client entity's number, its model's bone for a tag and
    /// that tag's world origin and forward direction.
    pub(crate) fn tag(&self, object: u64, tag: &str) -> Option<TagPlace> {
        let row = self.rows.get(&object)?;
        let dobj = row.dobj.as_ref();
        Some((
            row.mover.state.number,
            dobj.and_then(|dobj| dobj.tag_bone(tag)),
            dobj.and_then(|dobj| dobj.tag_world_pose(tag)),
        ))
    }
}

/// A presented entity's number, a tag's bone on its model and the tag's
/// world origin and forward direction.
pub(crate) type TagPlace = (i32, Option<usize>, Option<([f32; 3], [f32; 3])>);

struct Wanted {
    object: u64,
    number: i32,
    origin: [f32; 3],
    angles: [f32; 3],
    model: Arc<str>,
    hidden: bool,
}

fn wanted(client: &mut World, mirrored: &std::collections::BTreeSet<u64>) -> Vec<Wanted> {
    let mut runtime = client.resource_mut::<RoundScript>();
    let own: Vec<(u64, i32, bool)> = runtime
        .entities
        .iter()
        .filter(|(id, e)| {
            e.kind != EntityKind::HudElem
                && e.number >= CLIENT_ENTITY_BASE
                && !mirrored.contains(id)
        })
        .map(|(id, e)| (*id, e.number, e.hidden))
        .collect();
    own.into_iter()
        .filter_map(|(object, number, hidden)| {
            let mover = matches!(
                runtime.object_field(object, "type"),
                Value::String(kind) if &*kind == "scriptmover"
            );
            let model: Arc<str> = match runtime.object_field(object, "model") {
                Value::String(model) if mover && !model.is_empty() => model.as_ref().into(),
                _ => return None,
            };
            let mut vector = |field| match runtime.object_field(object, field) {
                Value::Vector(v) => v,
                _ => [0.0; 3],
            };
            Some(Wanted {
                object,
                number,
                origin: vector("origin"),
                angles: vector("angles"),
                model,
                hidden,
            })
        })
        .collect()
}

/// Brings the client's rows up to its entities this frame.
pub(super) fn present(server: &mut World, client: &mut super::ClientScripts) {
    let now = crate::level_time_ms(server.resource::<crate::step::StepRequest>().tick);
    let mut mirrored: std::collections::BTreeSet<u64> = client
        .mirrored
        .values()
        .map(|entry| entry.client_object)
        .collect();
    mirrored.extend(
        client
            .world
            .resource::<super::ClientView>()
            .locals
            .iter()
            .map(|local| local.object),
    );
    let wanted = wanted(&mut client.world, &mirrored);
    let Some(mut presentation) = server.remove_resource::<ClientPresentation>() else {
        return;
    };
    let frame = crate::frame::FrameWorld::from_world(server);
    let mut taken: std::collections::BTreeSet<i32> = frame
        .ecs_ref()
        .resource::<RoundScript>()
        .entities
        .values()
        .filter(|e| e.kind != EntityKind::HudElem)
        .map(|e| e.number)
        .collect();
    taken.extend(frame.dynamic_entity_numbers());
    let alive: std::collections::BTreeSet<u64> = wanted.iter().map(|w| w.object).collect();
    presentation.rows.retain(|object, _| alive.contains(object));
    // Rows keep their numbers while no server entity takes them.
    presentation
        .rows
        .retain(|_, row| !taken.contains(&row.mover.state.number));
    taken.extend(presentation.rows.values().map(|row| row.mover.state.number));
    for want in wanted {
        let row = presentation.rows.entry(want.object).or_insert_with(|| {
            let id = ScriptModelId::from_wire(CLIENT_PRESENTED_BASE | want.number as u32);
            let number = if taken.contains(&want.number) {
                (CLIENT_ENTITY_BASE..=PRESENTED_TOP)
                    .rev()
                    .find(|n| !taken.contains(n))
                    .unwrap_or(want.number)
            } else {
                want.number
            };
            taken.insert(number);
            Presented {
                mover: ScriptMoverGentity {
                    id,
                    state: crate::gentity::init_script_mover_state(
                        number,
                        want.origin,
                        want.angles,
                    ),
                    nonsolid: true,
                    ..Default::default()
                },
                model: "".into(),
                dobj: None,
                hidden: false,
                moving: false,
            }
        });
        let state = &mut row.mover.state;
        let posed = state.tr_base != want.origin || state.apos_tr_base != want.angles;
        if posed || row.moving {
            crate::frame::pose_script_mover_state(state, now, want.origin, want.angles);
        }
        row.moving = posed;
        // Shown to the local client alone; hidden, to no one.
        state.e_flags |= entity_iw4::CG_SCRIPT_MOVER_NODRAW;
        row.mover.shown_to = if want.hidden { 0 } else { LOCAL_VIEWER };
        row.hidden = want.hidden;
        if row.model != want.model {
            row.model = want.model.clone();
            match want.model.strip_prefix('*') {
                Some(brush) => {
                    row.dobj = None;
                    state.index = super::super::host::natives::iw4::atoi(brush);
                    state.solid = entity_iw4::SCRIPT_MOVER_BMODEL_SOLID;
                }
                None => {
                    state.index = 0;
                    state.solid = 0;
                    let capability = frame.model_capability(&want.model).flatten();
                    match row.dobj.as_mut() {
                        Some(dobj) => dobj.replace_model(&want.model, capability),
                        None => {
                            row.dobj = Some(AuthorityDObjState::at_pose(
                                &want.model,
                                capability,
                                want.origin,
                                want.angles,
                            ));
                        }
                    }
                }
            }
        }
        if let Some(dobj) = row.dobj.as_mut() {
            dobj.set_world_pose(want.origin, want.angles);
        }
    }
    server.insert_resource(presentation);
}

/// Adds the client's rows to a snapshot: each joins the script movers and
/// the entity states at its own number (one the server holds keeps the
/// server's), and a shown model its DObj.
pub(crate) fn present_in_snapshot(world: &World, snapshot: &mut crate::Snapshot) {
    let Some(presentation) = world.get_resource::<ClientPresentation>() else {
        return;
    };
    let meta = &mut snapshot.meta;
    let taken: std::collections::BTreeSet<i32> =
        meta.entities.iter().map(|state| state.number).collect();
    let mut shown = std::collections::BTreeSet::new();
    for mover in presentation.movers() {
        if taken.contains(&mover.state.number) {
            continue;
        }
        shown.insert(mover.id);
        meta.entities.push(mover.state);
        meta.script_movers.push(*mover);
    }
    meta.entity_dobjs.extend(
        presentation
            .dobjs()
            .filter(|(owner, _)| owner.script_model().is_some_and(|id| shown.contains(&id)))
            .map(|(owner, state)| (owner, state.clone())),
    );
}
