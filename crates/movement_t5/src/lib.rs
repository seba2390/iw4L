#![no_std]
#![forbid(unsafe_code)]

mod dive;
pub mod events;
mod footsteps;
mod ground;
mod ladder;
pub mod math;
mod moves;
pub mod perks;
mod physics;
mod pml;
mod pmove;
mod prone;
mod slide;
mod sprint;
mod stance;
pub mod state;
pub mod tuning;
mod view_angles;
mod view_height;
mod world;

pub use events::MoveEvent;
pub use physics::effective_stance;
pub use pml::Pml;
pub use pmove::{MoveOutput, pmove};
pub use state::{Buttons, PlayerState, UserCmd};
pub use world::{
    MoveWorld, SURF_LADDER, SURF_NOFALLDAMAGE, SURF_NOSTEPS, SURF_SLICK, Trace, WeaponMove,
};

/// A part of Black Ops' movement the player reached that IW4L does not run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Gap {
    Ladder = 1,
    Mantle = 2,
    Swim = 4,
    MountedView = 8,
    MountedSpeed = 16,
    Launched = 32,
    AnimLock = 64,
    SpecialMove = 128,
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
    /// The match is a zombies game (the `zombiemode` dvar).
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
    gaps: u32,
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
            gaps: 0,
        }
    }

    pub(crate) fn client_num(&self) -> i32 {
        self.ps.client_num
    }

    /// The player's own sweep: when it starts inside another body, that
    /// body is touched and bodies stop blocking this player for the step.
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

    pub(crate) fn gap(&mut self, gap: Gap) {
        self.gaps |= gap as u32;
    }

    /// The surface a footstep, jump or dive sounds like.
    pub(crate) fn surface_sound(&self, pml: &Pml) -> i32 {
        if pml.ground_trace.surface_flags & SURF_NOSTEPS != 0 {
            0
        } else if self.ps.water_level != 0 {
            20
        } else {
            pml.ground_trace.surface_type()
        }
    }

    /// Firing a weapon that freezes movement holds the stance too.
    pub(crate) fn weapon_blocks_stance(&self) -> bool {
        matches!(self.ps.weaponstate, 6 | 7 | 8 | 0x20 | 0x12 | 0x13 | 0x14)
            && self.ps.weapon != 0
            && self
                .world
                .weapon(self.ps.weapon)
                .freeze_movement_when_firing
    }

    /// Leaves aim down the sights.
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
            Gap::Ladder,
            Gap::Mantle,
            Gap::Swim,
            Gap::MountedView,
            Gap::MountedSpeed,
            Gap::Launched,
            Gap::AnimLock,
            Gap::SpecialMove,
        ]
        .into_iter()
        .filter(|g| self.gaps & *g as u32 != 0)
    }
}
