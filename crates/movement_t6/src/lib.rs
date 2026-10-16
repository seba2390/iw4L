#![no_std]
#![forbid(unsafe_code)]

mod dive;
pub mod events;
mod footsteps;
mod ground;
mod inventory;
mod ladder;
pub mod math;
mod moves;
pub mod perks;
mod physics;
mod pml;
mod pmove;
mod prone;
mod slide;
mod special;
mod sprint;
mod stance;
pub mod state;
pub mod tuning;
mod view_angles;
mod view_height;
pub mod weapon;
mod world;

pub use events::MoveEvent;
pub use inventory::{HeldRounds, held_rounds, load_held};
pub use physics::effective_stance;
pub use pml::Pml;
pub use pmove::{MoveOutput, pmove};
pub use state::{Buttons, PlayerState, UserCmd};
pub use world::{
    MoveWorld, SURF_LADDER, SURF_NOFALLDAMAGE, SURF_NOSTEPS, SURF_SLICK, Trace, Weapon,
};

/// A part of Black Ops 2's movement the player reached that IW4L does not run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Gap {
    Mantle = 2,
    MountedView = 8,
    Shellshock = 16,
    Launched = 32,
    AnimLock = 64,
    /// The match cannot tell whether a spectator stays inside the map's
    /// height-lock volumes.
    HeightLock = 128,
}

const PLAYER_BODY_CONTENTS: u32 = 0x0200_c000;
const MAX_OUT: usize = 48;

/// One player's movement step: the state it moves, the command it runs and
/// the world it moves through.
pub struct Pm<'a, W: MoveWorld> {
    pub ps: &'a mut PlayerState,
    pub cmd: UserCmd,
    pub oldcmd: UserCmd,
    pub tracemask: u32,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub world: &'a W,
    /// The match is a zombies game.
    pub zombiemode: bool,
    /// The step runs on a client predicting its own player.
    pub client_side: bool,
    pub xyspeed: f32,
    pub step_smooth_time: i32,
    pub step_smooth_z: f32,
    touched: [i32; 32],
    numtouch: usize,
    out: [Option<MoveEvent>; MAX_OUT],
    numout: usize,
    gaps: core::cell::Cell<u32>,
}

impl<'a, W: MoveWorld> Pm<'a, W> {
    pub fn new(
        ps: &'a mut PlayerState,
        cmd: UserCmd,
        oldcmd: UserCmd,
        tracemask: u32,
        world: &'a W,
    ) -> Self {
        Self {
            ps,
            cmd,
            oldcmd,
            tracemask,
            mins: [-15.0, -15.0, 0.0],
            maxs: [15.0, 15.0, 70.0],
            world,
            zombiemode: false,
            client_side: false,
            xyspeed: 0.0,
            step_smooth_time: 0,
            step_smooth_z: 0.0,
            touched: [0; 32],
            numtouch: 0,
            out: [None; MAX_OUT],
            numout: 0,
            gaps: core::cell::Cell::new(0),
        }
    }

    pub(crate) fn client_num(&self) -> i32 {
        self.ps.client_num
    }

    pub(crate) fn trace(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
    ) -> Trace {
        self.trace_with_mask(start, end, mins, maxs, mask)
    }

    pub(crate) fn trace_with_mask(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
    ) -> Trace {
        let pass = self.client_num();
        let trace = self.world.trace(start, end, mins, maxs, pass, mask);
        if !trace.startsolid || trace.contents & PLAYER_BODY_CONTENTS == 0 {
            return trace;
        }
        self.touch(trace.entity);
        self.tracemask &= !PLAYER_BODY_CONTENTS;
        self.world
            .trace(start, end, mins, maxs, pass, mask & !PLAYER_BODY_CONTENTS)
    }

    pub(crate) fn trace_plain(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
    ) -> Trace {
        self.world
            .trace(start, end, mins, maxs, self.client_num(), mask)
    }

    pub(crate) fn touch(&mut self, entity: i32) {
        if entity == state::ENTITYNUM_WORLD || self.numtouch == self.touched.len() {
            return;
        }
        self.touched[self.numtouch] = entity;
        self.numtouch += 1;
    }

    pub(crate) fn out(&mut self, event: MoveEvent) {
        if self.numout < MAX_OUT {
            self.out[self.numout] = Some(event);
            self.numout += 1;
        }
    }

    pub(crate) fn event(&mut self, event: i32, parm: i32) {
        self.out(MoveEvent::Entity(event, parm));
    }

    pub(crate) fn anim_event(&mut self, event: i32, cont: bool, force: bool) {
        self.out(MoveEvent::Anim(event, cont, force));
    }

    pub(crate) fn prone_anim(&mut self) {
        self.out(MoveEvent::ProneAnim);
    }

    pub(crate) fn gap(&self, gap: Gap) {
        self.gaps.set(self.gaps.get() | gap as u32);
    }

    pub(crate) fn surface_sound(&self, pml: &Pml) -> i32 {
        if pml.ground_trace.surface_flags & SURF_NOSTEPS != 0 {
            0
        } else if self.ps.water_level != 0 {
            20
        } else {
            pml.ground_trace.surface_type()
        }
    }

    pub(crate) fn weapon_blocks_stance(&self) -> bool {
        let state = self.ps.weaponstate;
        (state::weapon_state::is_firing(state) || state::weapon_state::is_melee(state))
            && self.ps.weapon != 0
            && self
                .world
                .weapon(self.ps.weapon)
                .def
                .freeze_movement_when_firing()
    }

    pub(crate) fn reset_ads(&mut self) {
        self.event(events::event::RESET_ADS, 0);
        self.ps.pm_flags &= !state::pm_flags::ADS_INTENT;
    }

    pub fn touched(&self) -> &[i32] {
        &self.touched[..self.numtouch]
    }

    pub fn events(&self) -> impl Iterator<Item = MoveEvent> + '_ {
        self.out[..self.numout].iter().flatten().copied()
    }

    pub fn gaps(&self) -> impl Iterator<Item = Gap> + '_ {
        [
            Gap::Mantle,
            Gap::MountedView,
            Gap::Shellshock,
            Gap::Launched,
            Gap::AnimLock,
            Gap::HeightLock,
        ]
        .into_iter()
        .filter(|g| self.gaps.get() & *g as u32 != 0)
    }
}
