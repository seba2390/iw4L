//! `IW4L_SOUND_TRACE=1`: logs every sound the match asks for, without a
//! sound device — the client scripts' sound commands, the server's sound
//! events and local sounds, and each loop as it starts and stops — so a
//! headless run shows what would be heard.

use bevy_ecs::prelude::Resource;
use std::collections::BTreeMap;

pub(crate) fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("IW4L_SOUND_TRACE").is_ok_and(|v| v != "0" && !v.is_empty()))
}

pub(crate) fn script_audio(command: &crate::ScriptAudioCommand) {
    if enabled() {
        diag::info!(Sim, "sound trace: script {command:?}");
    }
}

pub(crate) fn entity_event(
    kind: &str,
    alias: Option<&str>,
    audience: &crate::EventAudience,
    number: i32,
    origin: [f32; 3],
) {
    if enabled() {
        diag::info!(
            Sim,
            "sound trace: event {kind} alias={} entity={number} at={origin:?} to={audience:?}",
            alias.unwrap_or("-")
        );
    }
}

pub(crate) fn local_sound(alias: Option<&str>, client: crate::ClientId, stop: bool) {
    if enabled() {
        let verb = if stop { "stop" } else { "play" };
        diag::info!(
            Sim,
            "sound trace: local {verb} alias={} client={}",
            alias.unwrap_or("-"),
            client.0
        );
    }
}

/// The local client's sound system said a client sound started or ended.
pub(crate) fn client_report(status: &crate::ClientSoundStatus) {
    diag::info!(
        Sim,
        "sound trace: report id={} playing={} at={}ms length={}ms",
        status.id,
        status.playing,
        status.playback_ms,
        status.length_ms
    );
}

/// The loops playing at the last trace, by owner.
#[derive(Resource, Default)]
pub(crate) struct TracedLoops(BTreeMap<crate::ScriptModelId, String>);

/// Logs the loops that started or stopped since the last call.
pub(crate) fn loops(
    world: &mut bevy_ecs::world::World,
    rows: &[(crate::ScriptModelId, &str, [f32; 3])],
) {
    if !enabled() {
        return;
    }
    let mut traced = world.get_resource_or_insert_with(TracedLoops::default);
    let now: BTreeMap<_, _> = rows
        .iter()
        .map(|(owner, alias, _)| (*owner, (*alias).to_owned()))
        .collect();
    for (owner, alias, origin) in rows {
        if traced.0.get(owner).map(String::as_str) != Some(*alias) {
            diag::info!(
                Sim,
                "sound trace: loop start alias={alias} owner={owner:?} at={origin:?}"
            );
        }
    }
    for (owner, alias) in &traced.0 {
        if now.get(owner) != Some(alias) {
            diag::info!(Sim, "sound trace: loop stop alias={alias} owner={owner:?}");
        }
    }
    traced.0 = now;
}
