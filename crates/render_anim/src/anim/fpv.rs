use asset_game::WeaponAnimSlot;

use crate::{EventResult, ViewmodelController, ViewmodelEvent, WeaponState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentFpvEvent {
    Fire,

    ReleaseAttack,
    RaiseFirst,
    Raise,
    Reload,
    ReloadEmpty,
    ReloadStart,
    ReloadEnd,
    Rechamber,
    Drop,
    SprintIn,
    SprintLoop,
    SprintOut,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AuthorityFpvCues {
    pub shot_accepted: bool,
    pub spawned: bool,
    pub attack_released: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FpvPoseSample {
    pub state: WeaponState,

    pub clip_name: Option<String>,
    pub clip_time: f32,
    pub fire_weight: f32,
    pub idle_weight: f32,
}

pub fn map_authority_to_fpv(cues: AuthorityFpvCues) -> Vec<PresentFpvEvent> {
    let mut out = Vec::new();

    if cues.shot_accepted {
        out.push(PresentFpvEvent::Fire);
    }
    if cues.attack_released {
        out.push(PresentFpvEvent::ReleaseAttack);
    }

    out
}

fn viewmodel_event_for(event: PresentFpvEvent) -> Option<ViewmodelEvent> {
    Some(match event {
        PresentFpvEvent::Fire => ViewmodelEvent::Fire,
        PresentFpvEvent::ReleaseAttack => return None,
        PresentFpvEvent::RaiseFirst => ViewmodelEvent::Raise {
            first: true,
            quick: false,
        },
        PresentFpvEvent::Raise => ViewmodelEvent::Raise {
            first: false,
            quick: false,
        },
        PresentFpvEvent::Reload => ViewmodelEvent::Reload { empty: false },
        PresentFpvEvent::ReloadEmpty => ViewmodelEvent::Reload { empty: true },
        PresentFpvEvent::ReloadStart => ViewmodelEvent::ReloadStart,
        PresentFpvEvent::ReloadEnd => ViewmodelEvent::ReloadEnd,
        PresentFpvEvent::Rechamber => ViewmodelEvent::Rechamber,
        PresentFpvEvent::Drop => ViewmodelEvent::Drop,
        PresentFpvEvent::SprintIn => ViewmodelEvent::SprintIn,
        PresentFpvEvent::SprintLoop => ViewmodelEvent::SprintLoop,
        PresentFpvEvent::SprintOut => ViewmodelEvent::SprintOut,
    })
}

fn apply_present_events(controller: &mut ViewmodelController, events: &[PresentFpvEvent]) {
    for event in events {
        match viewmodel_event_for(*event) {
            None => controller.settle_fire_to_idle(),
            Some(vm) => report_fpv_event_result(*event, controller.handle(vm)),
        }
    }
}

pub fn is_predicted_fire_weap_anim(masked: u32) -> bool {
    use weapon_iw4::weap_anim_event as event_id;
    matches!(
        masked,
        event_id::FIRE | event_id::LASTSHOT | event_id::ADS_FIRE | event_id::ADS_LASTSHOT
    )
}

fn report_dispatch_slot(slot: usize, result: EventResult) {
    match result {
        EventResult::Started(_) => {}
        EventResult::IgnoredState(state) => {
            diag::warn!(Fpv, "fpv: szXAnims[{slot}] ignored in state {state:?}");
        }
        EventResult::IgnoredMissingClip(named) => {
            diag::warn!(
                Fpv,
                "fpv: szXAnims[{slot}] ({named:?}) missing — not started"
            );
        }
    }
}

fn report_fpv_event_result(event: PresentFpvEvent, result: EventResult) {
    match result {
        EventResult::Started(_) => {}
        EventResult::IgnoredState(state) => {
            diag::warn!(Fpv, "fpv: {event:?} ignored in state {state:?}");
        }
        EventResult::IgnoredMissingClip(slot) => {
            diag::warn!(Fpv, "fpv: {event:?} ignored — szXAnims[{slot:?}] missing");
        }
    }
}

pub fn sample_pose(controller: &ViewmodelController) -> FpvPoseSample {
    let active = controller.active_anims().max_by(|a, b| {
        a.weight
            .partial_cmp(&b.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    FpvPoseSample {
        state: controller.state(),
        clip_name: active.map(|a| a.clip.name.clone()),
        clip_time: active.map(|a| a.time).unwrap_or(0.0),
        fire_weight: controller.animation_weight(WeaponAnimSlot::Fire),
        idle_weight: controller.animation_weight(WeaponAnimSlot::Idle),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FpvAuthoritySample {
    pub tick: u32,
    pub weaponstate: i32,
    pub cues: AuthorityFpvCues,

    pub sprinting: bool,

    pub ads_frac: f32,

    pub weap_anim: i32,

    pub weap_anim_secondary: i32,

    pub last_weapon_hand: i32,

    pub perks0: u32,
    pub perks1: u32,

    pub clip_ammo: Option<i32>,

    pub left_clip_ammo: Option<i32>,

    /// The player's own fields in the game's layout (a game's viewmodel
    /// rules read them).
    pub game_move: [u8; game_api::movement::GAME_MOVE_BYTES],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalShotIdentity {
    pub life_sequence: u32,
    pub hand: u8,
    pub weapon_shot_count: i32,

    pub weap_anim_restart: bool,
}

#[derive(Clone, Debug, Default)]
pub struct LocalShotCursor {
    last: Option<LocalShotIdentity>,
}

impl LocalShotCursor {
    pub fn clear(&mut self) {
        self.last = None;
    }

    pub fn observe(&mut self, identity: LocalShotIdentity) -> bool {
        match self.last {
            None => {
                self.last = Some(identity);
                false
            }
            Some(prev) if prev == identity => false,
            Some(prev) => {
                let count_edge = identity.weapon_shot_count != prev.weapon_shot_count;
                let restart_edge = identity.weap_anim_restart != prev.weap_anim_restart
                    && identity.weapon_shot_count == prev.weapon_shot_count;
                self.last = Some(identity);
                count_edge || restart_edge
            }
        }
    }

    pub fn last(&self) -> Option<LocalShotIdentity> {
        self.last
    }
}

pub fn local_shot_identity(
    life_sequence: u32,
    hand: u8,
    weapon_shot_count: i32,
    weap_anim: i32,
) -> LocalShotIdentity {
    LocalShotIdentity {
        life_sequence,
        hand,
        weapon_shot_count,
        weap_anim_restart: (weap_anim as u32 & 0x200) != 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WeapAnimEdge {
    Unchanged,

    Idle,

    ForceIdle,

    /// Idle now, blending in over this many seconds.
    ForceIdleOver(f32),

    Dispatch(usize),

    /// A game's own animation (`anim`, in its order) in a hand's `slot`.
    GameDispatch {
        slot: usize,
        anim: usize,
        left: bool,
    },
}

/// A game's own viewmodel rules, with the held weapon's own bytes.
#[derive(Clone, Debug)]
pub struct GameViewmodel {
    pub rules: game_api::ViewmodelRules,
    pub weapon: Option<std::sync::Arc<asset_game::GameWeaponBytes>>,
}

impl GameViewmodel {
    /// Whether the weapon names its animation `anim` (in the game's order).
    fn named(&self, anim: usize) -> bool {
        anim < 128
            && self
                .weapon
                .as_ref()
                .is_some_and(|weapon| weapon.named_anims & 1 << anim != 0)
    }

    fn slot(&self, anim: usize) -> Option<usize> {
        self.weapon
            .as_ref()?
            .anim_slots
            .get(anim)
            .copied()
            .flatten()
    }

    fn left_slot(&self, anim: usize) -> Option<usize> {
        self.weapon
            .as_ref()?
            .left_anim_slots
            .get(anim)
            .copied()
            .flatten()
    }

    /// The rate the game starts its animation `anim` of `length_ms` at;
    /// `None` when it has no rule.
    fn rate(
        &self,
        anim: usize,
        length_ms: i32,
        game: &[u8; game_api::movement::GAME_MOVE_BYTES],
    ) -> Option<f32> {
        let rate = self.rules.rate?;
        let weapon = self.weapon.as_ref()?;
        Some(rate(&game_api::ViewmodelStart {
            anim,
            length_ms,
            def: &weapon.def,
            variant: &weapon.variant,
            game,
        }))
    }

    /// Whether the player's own fields hide the viewmodel.
    pub fn hidden(&self, game: &[u8; game_api::movement::GAME_MOVE_BYTES]) -> bool {
        self.rules.hidden.is_some_and(|hidden| hidden(game))
    }
}

#[derive(Clone, Debug, Default)]
pub struct FpvPresentState {
    /// The client's random draws for a game's own picks.
    random: u32,

    pub last_tick: Option<u32>,

    pub last_ads_frac: Option<f32>,

    pub last_weap_anim: Option<i32>,

    pub last_weap_anim_secondary: Option<i32>,

    pub local_shot: LocalShotCursor,

    reported: Vec<&'static str>,
}

impl FpvPresentState {
    pub fn clear(&mut self) {
        self.last_tick = None;
        self.last_ads_frac = None;
        self.last_weap_anim = None;
        self.last_weap_anim_secondary = None;
        self.local_shot.clear();
    }

    pub fn forget_weap_anim(&mut self) {
        self.last_weap_anim = None;
        self.last_weap_anim_secondary = None;
    }

    pub fn observe_weap_anim_edge(&mut self, raw: i32) -> WeapAnimEdge {
        observe_weap_anim_edge_on(&mut self.last_weap_anim, raw)
    }

    pub fn observe_weap_anim_secondary_edge(&mut self, raw: i32) -> WeapAnimEdge {
        observe_weap_anim_edge_on(&mut self.last_weap_anim_secondary, raw)
    }

    /// The edge of the weapon animation number by the game's own rules,
    /// with what it does to the camera animations.
    fn observe_game_weap_anim_edge(
        &mut self,
        raw: i32,
        game: &GameViewmodel,
        player: &[u8; game_api::movement::GAME_MOVE_BYTES],
    ) -> (WeapAnimEdge, Option<game_api::CameraAnim>) {
        if self.last_weap_anim.replace(raw) == Some(raw) {
            return (WeapAnimEdge::Unchanged, None);
        }
        let named = |anim: usize| game.named(anim);
        let (anim, gap) = match (game.rules.pick, game.weapon.as_ref()) {
            (Some(pick), Some(weapon)) => {
                self.random = self
                    .random
                    .wrapping_mul(1_103_515_245)
                    .wrapping_add(12_345);
                pick(
                    raw as u32,
                    &game_api::ViewmodelPick {
                        named: &named,
                        def: &weapon.def,
                        variant: &weapon.variant,
                        game: player,
                        random: self.random >> 16,
                    },
                )
            }
            _ => (game.rules.anim)(raw as u32, &named),
        };
        if let Some(gap) = gap {
            self.report(gap);
        }
        let camera = game
            .rules
            .camera
            .and_then(|camera| (camera.anim)(raw as u32, &named));
        (self.game_edge(anim, game, false), camera)
    }

    /// The edge of a dual-wield weapon's left-hand number by the game's own
    /// rules; unchanged when it has none.
    fn observe_game_left_edge(&mut self, raw: i32, game: &GameViewmodel) -> WeapAnimEdge {
        let Some(left) = game.rules.left else {
            return WeapAnimEdge::Unchanged;
        };
        if self.last_weap_anim_secondary.replace(raw) == Some(raw) {
            return WeapAnimEdge::Unchanged;
        }
        let named = |anim: usize| game.named(anim);
        let (anim, gap) = left(raw as u32, &named);
        if let Some(gap) = gap {
            self.report(gap);
        }
        self.game_edge(anim, game, true)
    }

    fn game_edge(
        &mut self,
        anim: game_api::ViewmodelAnim,
        game: &GameViewmodel,
        left: bool,
    ) -> WeapAnimEdge {
        match anim {
            game_api::ViewmodelAnim::Idle => WeapAnimEdge::Idle,
            game_api::ViewmodelAnim::IdleNow => WeapAnimEdge::ForceIdle,
            game_api::ViewmodelAnim::IdleOver(secs) => WeapAnimEdge::ForceIdleOver(secs),
            game_api::ViewmodelAnim::Keep => WeapAnimEdge::Unchanged,
            game_api::ViewmodelAnim::Play(anim) => {
                let right = (!left).then(|| game.slot(anim)).flatten();
                match (right, game.left_slot(anim)) {
                    (Some(slot), _) => self.game_dispatch(slot, anim, false, game),
                    (None, Some(slot)) => self.game_dispatch(slot, anim, true, game),
                    (None, None) => {
                        if let Some(unslotted) = game.rules.unslotted {
                            self.report(unslotted);
                        }
                        WeapAnimEdge::Unchanged
                    }
                }
            }
        }
    }

    fn game_dispatch(
        &mut self,
        slot: usize,
        anim: usize,
        left: bool,
        game: &GameViewmodel,
    ) -> WeapAnimEdge {
        match (game.rules.rate, game.rules.playback) {
            (Some(_), _) => WeapAnimEdge::GameDispatch { slot, anim, left },
            (None, Some(playback)) => {
                self.report(playback);
                WeapAnimEdge::Dispatch(slot)
            }
            (None, None) => WeapAnimEdge::Dispatch(slot),
        }
    }

    fn report(&mut self, gap: &'static game_api::Unknown) {
        if !self.reported.contains(&gap.id) {
            diag::info!(Fpv, "game gap {}: {}", gap.id, gap.what);
            self.reported.push(gap.id);
        }
    }
}

#[derive(Debug)]
pub struct EquippedFpv {
    pub gun_xmodel: String,
    pub gun_index: assets::FpvMeshIndex,
    pub hands_index: assets::FpvMeshIndex,
    pub namespace: asset_core::AssetNamespace,
    pub hands: asset_model::FpvHands,
    pub controller: ViewmodelController,

    pub left: Option<ViewmodelController>,
}

impl EquippedFpv {
    pub fn new(
        gun_xmodel: impl Into<String>,
        gun_index: assets::FpvMeshIndex,
        hands_index: assets::FpvMeshIndex,
        namespace: asset_core::AssetNamespace,
        hands: asset_model::FpvHands,
        controller: ViewmodelController,
        left: Option<ViewmodelController>,
    ) -> Self {
        Self {
            gun_xmodel: gun_xmodel.into(),
            gun_index,
            hands_index,
            namespace,
            hands,
            controller,
            left,
        }
    }
}

pub fn events_for_authority_tick(
    present: &mut FpvPresentState,
    sample: FpvAuthoritySample,
) -> Vec<PresentFpvEvent> {
    if present.last_tick == Some(sample.tick) {
        return Vec::new();
    }
    let events = map_authority_to_fpv(sample.cues);
    present.last_tick = Some(sample.tick);
    present.last_ads_frac = Some(sample.ads_frac);
    events
}

#[derive(Clone, Debug)]
pub struct FpvNotetrack {
    pub hand: u8,
    pub notification: asset_anim::ClipNotification,
}

#[derive(Default)]
pub struct FpvNotetracks {
    pub records: Vec<FpvNotetrack>,
    pub discarded: u64,
}

impl FpvNotetracks {
    pub fn into_audio_batch(
        self,
        generation: frame::WorldStamp,
        timeline: u64,
        client: sim::ClientId,
        life: sim::LifeSequence,
        weapon: u32,
        tick: sim::Tick,
    ) -> audio::ViewmodelNotetracks {
        audio::ViewmodelNotetracks {
            generation,
            client,
            life,
            weapon,
            records: self
                .records
                .into_iter()
                .map(|record| {
                    let notify = record.notification;
                    audio::ViewmodelNotetrack {
                        event: audio::AudioEvent::from_animation(
                            generation,
                            timeline,
                            client,
                            tick,
                            audio::AnimationMarkerId {
                                controller: notify.scheduler,
                                playback: notify.playback,
                                node: notify.node,
                                cycle: notify.cycle,
                                marker: notify.marker,
                                hand: record.hand,
                                life: life.0,
                                weapon,
                            },
                        ),
                        name: notify.name,
                    }
                })
                .collect(),
            discarded: self.discarded,
        }
    }
}

pub fn tick_equipped_fpv_with_predicted_fire(
    equipped: &mut EquippedFpv,
    present: &mut FpvPresentState,
    sample: Option<FpvAuthoritySample>,
    predicted_local_fire: bool,
    game: Option<GameViewmodel>,
    dt_secs: f32,
) -> (FpvPoseSample, FpvNotetracks) {
    tick_equipped_fpv_with_extra_events(
        equipped,
        present,
        sample,
        predicted_local_fire,
        &[],
        game,
        dt_secs,
    )
}

/// With a game's own viewmodel rules, its weapon animation number alone
/// drives the right hand.
pub fn tick_equipped_fpv_with_extra_events(
    equipped: &mut EquippedFpv,
    present: &mut FpvPresentState,
    sample: Option<FpvAuthoritySample>,
    predicted_local_fire: bool,
    extra_events: &[PresentFpvEvent],
    game: Option<GameViewmodel>,
    dt_secs: f32,
) -> (FpvPoseSample, FpvNotetracks) {
    let mut events = match sample {
        Some(sample) => events_for_authority_tick(present, sample),
        None => Vec::new(),
    };
    if predicted_local_fire && !events.contains(&PresentFpvEvent::Fire) {
        events.push(PresentFpvEvent::Fire);
    }
    for event in extra_events {
        if !events.contains(event) {
            events.push(*event);
        }
    }
    if let Some(sample) = sample {
        equipped
            .controller
            .set_predicted_perks(sample.perks0, sample.perks1);
    }
    if game.is_none() {
        apply_present_events(&mut equipped.controller, &events);
    }
    if let Some(sample) = sample {
        let (edge, camera) = match &game {
            Some(game) => {
                present.observe_game_weap_anim_edge(sample.weap_anim, game, &sample.game_move)
            }
            None => (present.observe_weap_anim_edge(sample.weap_anim), None),
        };
        match edge {
            WeapAnimEdge::Unchanged => {}
            WeapAnimEdge::Idle => {
                let empty_mag = sample.clip_ammo == Some(0);
                if !equipped.controller.apply_idle_weap_anim(empty_mag) {
                    present.last_weap_anim = None;
                }
            }
            WeapAnimEdge::ForceIdle => {
                equipped
                    .controller
                    .apply_force_idle_weap_anim(sample.clip_ammo == Some(0));
            }
            WeapAnimEdge::ForceIdleOver(secs) => {
                equipped
                    .controller
                    .apply_force_idle_weap_anim_over(sample.clip_ammo == Some(0), secs);
            }
            WeapAnimEdge::Dispatch(slot) => {
                report_dispatch_slot(slot, equipped.controller.dispatch_sz_xanim_index(slot));
            }
            WeapAnimEdge::GameDispatch { slot, anim, left } => {
                let controller = if left {
                    equipped.left.as_mut()
                } else {
                    Some(&mut equipped.controller)
                };
                if let (Some(controller), Some(game)) = (controller, &game) {
                    start_game_anim(controller, game, slot, anim, &sample.game_move);
                }
            }
        }
        if let (Some(camera), Some(game)) = (camera, &game) {
            let cameras = asset_game::weap_anim_extra::CAMERAS;
            match camera {
                game_api::CameraAnim::Start(anim) => {
                    if let Some(slot) = game.slot(anim) {
                        equipped
                            .controller
                            .start_camera_anim(slot, cameras, |length_ms| {
                                game.rate(anim, length_ms, &sample.game_move).unwrap_or(1.0)
                            });
                    }
                }
                game_api::CameraAnim::Stop => equipped.controller.stop_slots(cameras),
            }
        }
    }
    let advance = equipped.controller.advance(dt_secs);
    let mut notetracks = FpvNotetracks {
        records: advance
            .notifications
            .into_iter()
            .map(|notification| FpvNotetrack {
                hand: 0,
                notification,
            })
            .collect(),
        discarded: advance.discarded,
    };
    let ads_frac = sample
        .map(|s| s.ads_frac)
        .or(present.last_ads_frac)
        .unwrap_or(0.0);
    equipped.controller.apply_ads_overlay_frame(ads_frac);
    if let (Some(left), Some(sample)) = (equipped.left.as_mut(), sample)
        && sample.last_weapon_hand == 1
    {
        left.set_predicted_perks(sample.perks0, sample.perks1);
        let edge = match &game {
            Some(game) => present.observe_game_left_edge(sample.weap_anim_secondary, game),
            None => present.observe_weap_anim_secondary_edge(sample.weap_anim_secondary),
        };
        // A game's left idle is its empty one only when the game says so.
        let empty = game.as_ref().is_none_or(|game| game.rules.left_empty_idle)
            && sample.left_clip_ammo == Some(0);
        match edge {
            WeapAnimEdge::Unchanged => {}
            WeapAnimEdge::Idle => {
                if !left.apply_idle_weap_anim(empty) {
                    present.last_weap_anim_secondary = None;
                }
            }
            WeapAnimEdge::ForceIdle => left.apply_force_idle_weap_anim(empty),
            WeapAnimEdge::ForceIdleOver(secs) => left.apply_force_idle_weap_anim_over(empty, secs),
            WeapAnimEdge::Dispatch(slot) => {
                report_dispatch_slot(slot, left.dispatch_sz_xanim_index(slot));
            }
            WeapAnimEdge::GameDispatch { slot, anim, .. } => {
                if let Some(game) = &game {
                    start_game_anim(left, game, slot, anim, &sample.game_move);
                }
            }
        }
        let advance = left.advance(dt_secs);
        notetracks
            .records
            .extend(
                advance
                    .notifications
                    .into_iter()
                    .map(|notification| FpvNotetrack {
                        hand: 1,
                        notification,
                    }),
            );
        notetracks.discarded = notetracks.discarded.saturating_add(advance.discarded);

        left.apply_ads_overlay_frame(0.0);
    }
    (sample_pose(&equipped.controller), notetracks)
}

/// Starts a game's own animation on a hand at the game's rate.
fn start_game_anim(
    controller: &mut ViewmodelController,
    game: &GameViewmodel,
    slot: usize,
    anim: usize,
    player: &[u8; game_api::movement::GAME_MOVE_BYTES],
) {
    let goal_time = game.rules.blend.map_or(0.0, |blend| blend(anim));
    let started = controller.start_game_anim(
        slot,
        |length_ms| game.rate(anim, length_ms, player).unwrap_or(1.0),
        goal_time,
    );
    if !started {
        diag::warn!(
            Fpv,
            "fpv: game animation {anim} (slot {slot}) missing — not started"
        );
    }
}

fn observe_weap_anim_edge_on(previous: &mut Option<i32>, raw: i32) -> WeapAnimEdge {
    let last = previous.replace(raw);
    if last == Some(raw) {
        return WeapAnimEdge::Unchanged;
    }
    let masked = raw as u32 & weapon_iw4::WEAP_ANIM_EVENT_MASK;
    match masked {
        0 => WeapAnimEdge::Idle,
        1 => WeapAnimEdge::ForceIdle,
        _ => match weapon_iw4::slot_for_weap_anim_event(masked) {
            Some(slot) => WeapAnimEdge::Dispatch(slot),
            None => WeapAnimEdge::Idle,
        },
    }
}
