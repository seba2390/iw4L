//! The game's view kick on this client: its own player's shots kick the
//! view, each frame eases the kick back, and the kick turns the angles the
//! client's commands aim by (the view follows the aim).

use bevy::prelude::*;
use game_api::fire::{KickWeapon, ViewKick, ViewKickRules};

/// The client's view kick and the draws its shots take.
#[derive(Resource)]
pub(crate) struct GameViewKick {
    kick: ViewKick,
    /// The C library generator's state, as a thread starts it.
    holdrand: u32,
}

impl Default for GameViewKick {
    fn default() -> Self {
        Self {
            kick: ViewKick::default(),
            holdrand: 1,
        }
    }
}

impl GameViewKick {
    /// A draw from 0 to 1: the C library's `rand()` over 32767.
    fn rand(&mut self) -> f32 {
        self.holdrand = self.holdrand.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.holdrand >> 16) & 0x7fff) as f32 * (1.0 / 32767.0)
    }

    /// A shot of the client's own player.
    pub(crate) fn fire(
        &mut self,
        rules: &ViewKickRules,
        player: &game_api::movement::MovePlayer,
        registry: &asset_game::WeaponRegistry,
    ) {
        let weapons = |weapon: u32| kick_weapon(registry, weapon);
        let mut kick = self.kick;
        (rules.fire)(player, &weapons, &mut || self.rand(), &mut kick);
        self.kick = kick;
    }
}

fn kick_weapon(registry: &asset_game::WeaponRegistry, weapon: u32) -> Option<KickWeapon> {
    let bound = registry.bind_published_row(weapon).ok()?;
    let bytes = bound.game_bytes()?;
    Some(KickWeapon {
        def: Box::from(&bytes.def[..]),
        variant: Box::from(&bytes.variant[..]),
        dual_wield: Some(bound.dual_wield_wire_id()).filter(|&left| left != 0),
    })
}

/// Each frame the kick eases back and turns the aim; a client with no game
/// view kick, or no player, aims unturned.
pub(crate) fn run_view_kick(
    mode: Option<Res<game_api::ModeRules>>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    prediction: Res<net::ClientPredictionState>,
    clock: Res<net::FrameClock>,
    presented: Res<net::PresentedSnapshot>,
    local: Res<net::LocalPresentClient>,
    mut state: ResMut<GameViewKick>,
    mut aim: ResMut<net::AimKick>,
) {
    let rules = mode
        .as_deref()
        .and_then(|mode| mode.client_fire)
        .and_then(|rules| rules.view_kick);
    let registry = weapons
        .as_deref()
        .and_then(|weapons| weapons.for_snapshot(presented.weapon_epoch()).ok());
    let (Some(rules), Some(registry), Some(ps)) = (rules, registry, prediction.0.predicted_local())
    else {
        state.kick = ViewKick::default();
        *aim = net::AimKick::default();
        return;
    };
    let registry = registry.registry();
    let player = ps.move_player(i32::try_from(local.0.0).unwrap_or(0), false);
    let weapons = |weapon: u32| kick_weapon(registry, weapon);
    (rules.frame)(&player, &weapons, clock.frametime(), &mut state.kick);
    aim.angles = (rules.aim)(&player, &weapons, &state.kick);
    aim.viewmodel_angles = state.kick.viewmodel_angles;
}
