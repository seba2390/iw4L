use bevy::prelude::*;

use crate::{ConsoleCommand, ConsoleDispatch};

use super::echo::ConsoleEcho;

pub(crate) fn route_session_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut echo: ConsoleEcho,
    mut transition: ResMut<::session::SessionSwapRequest>,
    has_world: Res<State<frame::MatchScope>>,
    role: Res<frame::RuntimeRole>,
    playback: Option<Res<::replay::ReplayPlayback>>,
    bridge: Option<Res<net::MasterBridge>>,
    manifest: Option<Res<::session::SessionContentManifest>>,
    mut dispatch: ResMut<ConsoleDispatch>,
    mut authority: Option<ResMut<net::AuthorityWorld>>,
    mut commands: Commands,
) {
    for cmd in events.read() {
        match cmd.name.as_str() {
            "end_match" => {
                if !cmd.args.is_empty() {
                    echo.write("usage: end_match");
                } else if *has_world.get() != frame::MatchScope::Live
                    || !matches!(
                        *role,
                        frame::RuntimeRole::Listen | frame::RuntimeRole::Dedicated
                    )
                {
                    echo.write("Only the match host can end the match");
                } else {
                    match transition.request_menu() {
                        Ok(id) => echo.write(format!(
                            "end_match: returning the session to its lobby (swap #{id})"
                        )),
                        Err(error) => echo.write(format!("end_match: {error}")),
                    }
                }
                dispatch.release();
            }
            "round_restart" => {
                let result = if !cmd.args.is_empty() {
                    Err("usage: round_restart".into())
                } else if let Some(authority) = authority.as_deref_mut() {
                    authority.0.request_round_restart()
                } else {
                    Err("round restart needs the match host".into())
                };
                match result {
                    Ok(()) => echo.write("round_restart: requested script round restart"),
                    Err(error) => echo.write(format!("round_restart: {error}")),
                }
            }
            "map_restart" | "fast_restart" => {
                // `fast_restart`: the level comes back as the save it made
                // at its start, when its game keeps one (Black Ops' zombies,
                // the pause menu's restart).
                if cmd.name == "fast_restart"
                    && let Some(authority) = authority.as_deref_mut()
                    && authority.0.request_save_load()
                {
                    echo.write("fast_restart: the level's save loads with it");
                }
                let zone = manifest.as_ref().and_then(|manifest| match &manifest.map {
                    ::session::ManifestFact::Known(map)
                        if (*has_world.get() == frame::MatchScope::Live) =>
                    {
                        Some(format!("{}:{}", map.namespace.as_str(), map.name))
                    }
                    _ => None,
                });
                match (cmd.args.is_empty(), zone) {
                    (false, _) => {
                        dispatch.release();
                        echo.write(format!("usage: {}", cmd.name));
                    }
                    (true, None) => {
                        dispatch.release();
                        echo.write(format!("{}: no map to restart", cmd.name));
                    }
                    (true, Some(zone)) => match transition.request_zone(zone.clone()) {
                        Ok(id) => {
                            echo.write(format!("{}: requested `{zone}` (swap #{id})", cmd.name))
                        }
                        Err(error) => {
                            dispatch.release();
                            echo.write(format!("{}: {error}", cmd.name));
                        }
                    },
                }
            }
            // `map <zone> [gametype]`: a zone without its game's prefix is
            // the current map's game's; a gametype also switches the mode
            // the level loads under.
            "map" => match cmd.args.as_slice() {
                [zone, gametype @ ..] if gametype.len() <= 1 => {
                    let zone = match manifest.as_ref().map(|manifest| &manifest.map) {
                        Some(::session::ManifestFact::Known(current)) if !zone.contains(':') => {
                            format!("{}:{zone}", current.namespace.as_str())
                        }
                        _ => zone.clone(),
                    };
                    let mode = match gametype.first() {
                        Some(token) => match sim::HostGameModeSelection::from_token(token) {
                            Some(mode) => Some((token.clone(), mode)),
                            None => {
                                dispatch.release();
                                echo.write(format!("map: unknown gametype `{token}`"));
                                continue;
                            }
                        },
                        None => None,
                    };
                    match transition.request_zone(zone.clone()) {
                        Ok(id) => {
                            if let Some((gametype, mode)) = mode {
                                asset_transport::select_zombies_start(Some(
                                    asset_transport::ZombiesStart {
                                        gametype,
                                        location: None,
                                    },
                                ));
                                commands.insert_resource(mode);
                            }
                            echo.write(format!("map: requested `{zone}` (swap #{id})"))
                        }
                        Err(error) => {
                            dispatch.release();
                            echo.write(format!("map: {error}"));
                        }
                    }
                }
                _ => {
                    dispatch.release();
                    echo.write("usage: map <zone> [gametype]");
                }
            },
            "disconnect" => {
                let in_session = (*has_world.get() == frame::MatchScope::Live)
                    || playback.is_some()
                    || transition.dump_id().is_some()
                    || bridge.is_some();
                if !cmd.args.is_empty() {
                    dispatch.release();
                    echo.write("usage: disconnect");
                } else if !in_session {
                    dispatch.release();
                    echo.write("disconnect: no session to leave");
                } else {
                    match transition.request_leave() {
                        Ok(id) => {
                            diag::lifecycle_boundary(
                                "disconnect_requested",
                                &format!(" swap={id}"),
                            );
                            echo.write(format!(
                                "disconnect: waiting for session teardown (swap #{id})"
                            ))
                        }
                        Err(error) => {
                            dispatch.release();
                            echo.write(format!("disconnect: {error}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
