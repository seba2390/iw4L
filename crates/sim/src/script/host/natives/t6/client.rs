//! Black Ops 2's client builtins (its client script VM's two tables), each as
//! its client handler reads its arguments and answers. The client handlers
//! are the client's own code, not the server's: where one reads like the
//! server's handler of the same name it is still ported from the client's.

mod effects;
mod entity;
mod fields;
pub(crate) mod game;
mod pure;
mod sound;

use crate::script::{Namespace, NativeRegistry};

/// The builtins Black Ops 2's compiler emits as opcodes: one VM handler
/// serves both script VMs, each with its own time and dvars.
const OPCODE_BUILTINS: [&str; 16] = [
    "abs",
    "angleclamp180",
    "anglestoforward",
    "anglestoright",
    "anglestoup",
    "getdvar",
    "getdvarcoloralpha",
    "getdvarcolorblue",
    "getdvarcolorgreen",
    "getdvarcolorred",
    "getdvarfloat",
    "getdvarint",
    "getdvarvector",
    "gettime",
    "isdefined",
    "vectortoangles",
];

pub(crate) fn register(registry: &mut NativeRegistry) {
    let server = NativeRegistry::black_ops_2();
    for name in OPCODE_BUILTINS {
        if let Some(native) = server.get(Namespace::Function, name) {
            registry.register(Namespace::Function, name, native);
        }
    }
    pure::register(registry);
    entity::register(registry);
    fields::register(registry);
    effects::register(registry);
    sound::register(registry);
    registry.on_client_sync(game::sync);
    registry.on_client_present(game::present);
}
