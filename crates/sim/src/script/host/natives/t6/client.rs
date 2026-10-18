//! Black Ops 2's client builtins (its client script VM's two tables), each as
//! its client handler reads its arguments and answers. The client handlers
//! are the client's own code, not the server's: where one reads like the
//! server's handler of the same name it is still ported from the client's.

pub(crate) mod ambient;
mod effects;
pub(crate) mod entity;
mod fields;
pub(crate) mod fog;
pub(crate) mod game;
mod gump;
pub(crate) mod pure;
mod rope;
pub(crate) mod sound;
mod touch;

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
    fog::register(registry);
    sound::register(registry);
    ambient::register(registry);
    gump::register(registry);
    registry.on_client_sync(sync);
    registry.on_client_present(present);
}

/// The client game's snapshot: its entities and fields, then its fog, its
/// touches and its gump loads.
fn sync(client: &mut bevy_ecs::prelude::World) {
    game::sync(client);
    effects::sync_script_vectors(client);
    entity::sync_third_person_spectate(client);
    fog::sync(client);
    let now = client
        .get_resource::<crate::step::StepRequest>()
        .map_or(0, |r| i64::from(r.tick.0) * i64::from(crate::MATCH_TICK_MS));
    touch::frame(client, now);
    gump::frame(client, now);
    let eye = crate::script::host::client_scripts::with_server(client, |server| {
        crate::script_player::eye(
            &crate::frame::FrameWorld::from_world(server),
            crate::ClientId(game::LOCAL_CLIENT as u32),
        )
    })
    .flatten();
    if let Some(eye) = eye {
        rope::frame(client, now, eye);
    }
    let poison = super::poison_overlay::PoisonDvars::read(client);
    let revive = super::revive_overlay::read(client);
    crate::script::host::client_scripts::with_server(client, |server| {
        super::flame_overlay::frame(server, game::LOCAL_CLIENT as u32);
        super::poison_overlay::frame(server, game::LOCAL_CLIENT as u32, &poison);
        super::revive_overlay::publish(server, game::LOCAL_CLIENT as u32, revive);
    });
}

/// The client game's frame after its scripts: its light mixers, then what it
/// presents through the server.
fn present(client: &mut bevy_ecs::prelude::World) {
    effects::run_mixers(client);
    game::present(client);
}
