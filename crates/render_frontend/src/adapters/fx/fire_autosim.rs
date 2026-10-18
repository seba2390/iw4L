//! The game's automatic fire simulation on this client: shots its fire
//! rules do not play at once (another player's automatic fire, a turret's)
//! are handed to the game's simulation, which says each frame which of them
//! play.

use std::collections::HashMap;

use asset_core::AssetNamespace;
use bevy::prelude::*;
use game_api::fire::{AutoFireShot, FireAutoSim, FireSoundShot};
use net::{FrameClock, PresentedSnapshot};

/// The client's simulation and, for each shooter, weapon and tag it runs,
/// the sound its shots play.
#[derive(Resource, Default)]
pub(crate) struct AutoFireSounds {
    sim: Option<Box<dyn FireAutoSim>>,
    sounds: HashMap<(i32, u32, u32), (AssetNamespace, String)>,
}

impl AutoFireSounds {
    /// Hands a shot to the game's simulation; gives it back when it plays
    /// at once.
    pub(crate) fn queue(
        &mut self,
        rules: &game_api::fire::FireAutoSimRules,
        shot: AutoFireShot,
        namespace: AssetNamespace,
        alias: &str,
    ) -> Option<AutoFireShot> {
        self.sounds.insert(
            (shot.entity, shot.weapon, shot.tag),
            (namespace, alias.to_owned()),
        );
        self.sim.get_or_insert_with(rules.new).queue(shot)
    }

    fn play(&self, shot: &AutoFireShot, out: &mut MessageWriter<audio::WeaponSound>) {
        let Some((namespace, alias)) = self.sounds.get(&(shot.entity, shot.weapon, shot.tag))
        else {
            return;
        };
        out.write(audio::WeaponSound {
            event: None,
            namespace: *namespace,
            alias: alias.clone(),
            origin_inches: Some(shot.origin),
            snd_ent: audio::ent_from_number(shot.entity),
        });
    }
}

/// A turret's shot reaches every client as the server's fire record: its
/// fire sound, picked by the game's fire rules as for a shot this client
/// does not view, plays at once or goes to the simulation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn turret_fire_shots(
    mut events: MessageReader<net::SvcScriptAudio>,
    mode: Option<Res<game_api::ModeRules>>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    bank: Option<Res<audio::SoundBank>>,
    generation: Res<frame::WorldGeneration>,
    view: Res<frame::ViewSubject>,
    mut state: ResMut<AutoFireSounds>,
    mut sounds: MessageWriter<audio::WeaponSound>,
) {
    for event in events.read() {
        let sim::ScriptAudioCommand::TurretFire {
            entity,
            weapon,
            barrel,
            origin,
        } = &event.0
        else {
            continue;
        };
        let Some(rules) = mode.as_deref().and_then(|mode| mode.client_fire) else {
            continue;
        };
        let (Some(weapons), Some(bank)) = (weapons.as_deref(), bank.as_deref()) else {
            continue;
        };
        let Ok(weapons) = weapons.for_event(generation.stamp()) else {
            continue;
        };
        let registry = weapons.registry();
        let Some(namespace) =
            registry.component_namespace_of(*weapon, asset_game::WeaponComponent::Sound)
        else {
            continue;
        };
        let shot = FireSoundShot {
            player_view: false,
            left: false,
            last_shot: false,
            killcam: view.in_killcam(),
            last_shot_sound: registry
                .authored_weapon_sound(*weapon, asset_game::WeaponSoundSlot::FireLast)
                .is_some(),
            parm: 0,
        };
        let Some((alias, simulated)) =
            super::system::game_fire_alias(&rules, registry, &bank.0, *weapon, &shot, &mut |gap| {
                diag::info!(Audio, "game gap {}: {}", gap.id, gap.what);
            })
        else {
            continue;
        };
        let shot = AutoFireShot {
            entity: *entity,
            weapon: *weapon,
            tag: barrel.unsigned_abs(),
            origin: *origin,
            player: false,
            last_shot: false,
            fire: game_api::fire::AutoFire {
                fire_time_ms: 0,
                burst: false,
            },
        };
        let now = match (simulated, rules.autosim.as_ref()) {
            (Some(fire), Some(autosim)) => {
                state.queue(autosim, AutoFireShot { fire, ..shot }, namespace, alias)
            }
            _ => {
                state.sounds.insert(
                    (shot.entity, shot.weapon, shot.tag),
                    (namespace, alias.to_owned()),
                );
                Some(shot)
            }
        };
        if let Some(shot) = now {
            state.play(&shot, &mut sounds);
        }
    }
}

/// One client frame of the game's simulation: the shots it plays now.
pub(crate) fn run_fire_autosim(
    clock: Res<FrameClock>,
    presented: Res<PresentedSnapshot>,
    camera: Option<Res<render_anim::occupancy::view_kick::SessionViewKick>>,
    mut state: ResMut<AutoFireSounds>,
    mut sounds: MessageWriter<audio::WeaponSound>,
) {
    let view = camera.map_or([0.0; 3], |camera| camera.refdef_vieworg);
    let flags = |entity: i32| {
        u32::try_from(entity)
            .ok()
            .and_then(|number| presented.player(sim::ClientId(number)))
            .map_or(0, |ps| ps.e_flags)
    };
    let frame_ms = clock.frametime();
    let Some(sim) = state.sim.as_mut() else {
        return;
    };
    let shots = sim.update(frame_ms, view, &flags);
    for shot in &shots {
        state.play(shot, &mut sounds);
    }
}
