//! Black Ops multiplayer's scoreboard columns: `setScoreboardColumns` names
//! four of the player's score fields, and writing one of those fields also
//! writes its column's value, which every client's scoreboard shows.

use super::super::args::string;
use crate::frame::FrameWorld;
use crate::script::Namespace::Function;
use crate::script::{NativeRegistry, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::{Resource, World};

/// Black Ops' column types, by number: the names `setScoreboardColumns`
/// takes.
pub(crate) const COLUMN_TYPES: [&str; 18] = [
    "_invalid",
    "none",
    "kills",
    "deaths",
    "assists",
    "defends",
    "plants",
    "defuses",
    "returns",
    "captures",
    "destructions",
    "kdratio",
    "survived",
    "stabs",
    "tomahawks",
    "humiliated",
    "x2score",
    "headshots",
];

/// The `x2score` column holds a tenth of the field; the client shows it
/// times ten.
const X2SCORE: u8 = 16;

/// The four column types the scripts last named.
#[derive(Resource, Default, Clone, Copy)]
pub(crate) struct ScoreboardColumns(pub(crate) [u8; 4]);

pub(crate) fn register(registry: &mut NativeRegistry) {
    // Black Ops prints an error for a name that is no column and stores
    // `_invalid` for it; neither stops the script.
    registry.register(Function, "setscoreboardcolumns", |world, _, args| {
        if args.len() != 4 {
            return Err("Invalid param count. usage: setScoreboardColumns( <column1 name> <column2 name> <column3 name> <column4 name> ).".into());
        }
        let mut columns = [0; 4];
        for (index, column) in columns.iter_mut().enumerate() {
            let name = string(args, index)?;
            *column = COLUMN_TYPES
                .iter()
                .position(|known| known.eq_ignore_ascii_case(&name))
                .unwrap_or(0) as u8;
            if *column == 0 {
                diag::warn!(Sim, "Invalid param: {name} is not a valid column type.");
            }
        }
        world.insert_resource(ScoreboardColumns(columns));
        Ok(Value::Undefined)
    });
}

/// The column type a player field writes, when it writes one.
pub(crate) fn field_column(name: &str) -> Option<u8> {
    let column = COLUMN_TYPES.iter().position(|known| *known == name)?;
    // `kdratio` is no field: Black Ops' client works it out.
    (column >= 2 && column != 11).then_some(column as u8)
}

/// Black Ops' setter for a score field also writes the value into the first
/// of the four columns that shows the field (a tenth of it for `x2score`).
/// A column named after the field was last written shows nothing new until
/// the next write.
pub(crate) fn write_column(world: &mut World, id: ClientId, column: u8, value: i32) {
    let Some(columns) = world.get_resource::<ScoreboardColumns>().copied() else {
        return;
    };
    let Some(slot) = columns.0.iter().position(|&c| c == column) else {
        return;
    };
    let value = if column == X2SCORE { value / 10 } else { value };
    let mut frame = FrameWorld::from_world(world);
    if frame.client_meta(id).is_some() {
        frame.client_meta_mut(id).score_columns[slot] = value;
    }
}

/// The column types every client's scoreboard draws.
pub(crate) fn columns(world: &World) -> [u8; 4] {
    world
        .get_resource::<ScoreboardColumns>()
        .map_or([0; 4], |columns| columns.0)
}
