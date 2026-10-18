//! A game's own shots in the match: the bullet traces, entities, damage and
//! notifies its server fire rules ask for, through the runtime's collision
//! and script host.

use crate::bullet_collision::{BulletTraceQuery, ColliderId, LagcompQuery, TraceOutcome};
use crate::frame::FrameWorld;
use crate::identities::ShotId;
use crate::world::{ClientId, Tick};
use crate::{ScriptModelId, SimContent};
use game_api::fire::{
    BulletHitType, BulletImpact, BulletTrace, FireDamage, FireEntity, FireEntityKind, FireTeam,
    FireWorld, GameMissile, MissileExplosion, MissileRun, NotifyArg, WeaponEvent,
};
use game_api::movement::{ENTITY_NONE, ENTITY_WORLD, GameWeapon};
use std::collections::HashMap;
use std::sync::Arc;

/// The match around one player's shots in one command.
pub(crate) struct MatchFireWorld<'w, 'a> {
    world: &'w mut FrameWorld<'a>,
    content: Arc<SimContent>,
    tick: Tick,
    query: LagcompQuery,
    shot: ShotId,
    segment: u16,
    /// Script entities the traces reached, by the number the game knows.
    models: Vec<(i32, ScriptModelId)>,
}

impl<'w, 'a> MatchFireWorld<'w, 'a> {
    /// The match as `attacker` saw it when it fired; as it stands for a
    /// blast with no shooter.
    pub(crate) fn new(
        world: &'w mut FrameWorld<'a>,
        tick: Tick,
        attacker: Option<ClientId>,
    ) -> Self {
        world.record_collision_history(tick, 0, None);
        world.record_entity_collision_history(tick);
        let query = world.lagcomp_query_for(attacker.unwrap_or(ClientId(u32::MAX)), tick);
        let shot = world.alloc_shot_id();
        Self {
            content: world.content(),
            world,
            tick,
            query,
            shot,
            segment: 0,
            models: Vec::new(),
        }
    }

    fn model(&self, entity: i32) -> Option<ScriptModelId> {
        self.models
            .iter()
            .find(|(number, _)| *number == entity)
            .map(|(_, id)| *id)
    }

    fn client(&self, entity: i32) -> Option<ClientId> {
        let id = ClientId(u32::try_from(entity).ok()?);
        self.world.player(id).is_some().then_some(id)
    }

    /// The script object an entity number stands for: a player's, a hit script
    /// model's, or a live missile's once scripts adopted it.
    fn object_of(&mut self, entity: i32) -> Option<u64> {
        if let Some(client) = self.client(entity) {
            return crate::script::player_object_id(self.world.ecs(), client.0);
        }
        if let Some(id) = self.model(entity) {
            return self
                .world
                .ecs_ref()
                .get_resource::<crate::script::RoundScript>()?
                .presented_by(id);
        }
        if let Some(projectile) = self.world.projectile_by_number(entity)
            && let Some(object) = self
                .world
                .ecs_ref()
                .get_resource::<crate::script::RoundScript>()
                .and_then(|runtime| runtime.missiles.get(&projectile.id).copied())
        {
            return Some(object);
        }
        crate::script::object_by_number(self.world.ecs(), entity)
    }

    /// A missile the game just spawned, given its script entity.
    fn adopt_missile(&mut self, entity: i32) -> Option<u64> {
        let projectile = self.world.projectile_by_number(entity)?;
        crate::script::host::weapons::adopt_game_missile(self.world.ecs(), &projectile)
    }

    /// The number the game knows a script entity by.
    fn number_of(&mut self, id: ScriptModelId) -> i32 {
        let number = self.world.gentity_number(id).unwrap_or(ENTITY_NONE);
        if number != ENTITY_NONE && self.model(number).is_none() {
            self.models.push((number, id));
        }
        number
    }

    /// The number the game knows a script object by: a player's client, a
    /// presented entity's number, else its script number.
    pub(crate) fn number_of_object(&mut self, object: u64) -> i32 {
        let (client, presence, number) = {
            let Some(runtime) = self
                .world
                .ecs_ref()
                .get_resource::<crate::script::RoundScript>()
            else {
                return ENTITY_NONE;
            };
            let Some(entity) = runtime.entities.get(&object) else {
                return ENTITY_NONE;
            };
            (
                runtime.player_client(object),
                entity.presence,
                entity.number,
            )
        };
        if let Some(client) = client {
            return client as i32;
        }
        match presence {
            Some(id) => match self.number_of(id) {
                ENTITY_NONE => number,
                presented => presented,
            },
            None => number,
        }
    }

    /// The script object an entity number stands for.
    pub(crate) fn object(&mut self, entity: i32) -> Option<u64> {
        self.object_of(entity)
    }

    fn sentient_at(&mut self, object: u64) -> Option<game_api::turret::Sentient> {
        let view =
            crate::script::host::natives::t5_turret::sentient_view(self.world.ecs(), object)?;
        let entity = match (view.client, view.presence) {
            (Some(client), _) => client as i32,
            (None, Some(id)) => match self.number_of(id) {
                ENTITY_NONE => view.number,
                presented => presented,
            },
            (None, None) => view.number,
        };
        Some(game_api::turret::Sentient {
            entity,
            team: team_of(view.team.as_deref()),
            ignore_me: view.ignore_me,
            origin: view.origin,
            eye: view.eye,
            mins: view.mins,
            maxs: view.maxs,
            pm_type: view.pm_type,
        })
    }

    /// A trace through the world, players and entities; each body's part
    /// chosen by `priority` when given.
    fn trace_bodies(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        pass_entity: i32,
        mask: u32,
        priority: Option<&[u8]>,
    ) -> BulletTrace {
        let glass: HashMap<u32, u16> = self
            .world
            .world_objects()
            .glass_damage_pairs()
            .into_iter()
            .collect();
        let outcome = crate::bullet_collision::bullet_trace_prioritized(
            self.world.clip_brushes(),
            self.world.clip_bsp(),
            self.world.clip_cmodels(),
            self.world.clip_mesh(),
            &self.query.players.poses,
            &self.query.entities.rows,
            &BulletTraceQuery {
                start,
                end,
                mask,
                ignore: self.client(pass_entity),
                ignore_hit: None,
                ignore_model: self.model(pass_entity),
            },
            &|piece| {
                crate::world_objects::glass_piece_is_solid(
                    glass.get(&u32::from(piece)).copied().unwrap_or(0),
                )
            },
            priority,
        );
        self.trace_of(outcome)
    }

    fn trace_of(&mut self, outcome: TraceOutcome) -> BulletTrace {
        let (fraction, normal, collider, start_solid, all_solid) = match outcome {
            TraceOutcome::Miss { .. } | TraceOutcome::Invalid { .. } => {
                return BulletTrace {
                    fraction: 1.0,
                    ..BulletTrace::default()
                };
            }
            TraceOutcome::Hit {
                fraction,
                normal,
                collider,
                ..
            } => (fraction, normal, Some(collider), false, false),
            TraceOutcome::StartSolid {
                collider,
                all_solid,
                ..
            } => (0.0, [0.0; 3], collider, true, all_solid),
        };
        let mut trace = BulletTrace {
            fraction,
            normal,
            start_solid,
            all_solid,
            ..BulletTrace::default()
        };
        match collider {
            None => trace.hit = BulletHitType::Entity(ENTITY_WORLD),
            Some(ColliderId::World {
                surface_flags,
                contents,
                glass_encoded,
            }) => {
                trace.surface_flags = surface_flags;
                trace.contents = contents;
                trace.hit = match glass_encoded {
                    0 => BulletHitType::Entity(ENTITY_WORLD),
                    encoded => BulletHitType::Glass(i32::from(encoded) - 1),
                };
            }
            Some(ColliderId::Player { client, hitloc, .. }) => {
                trace.hit = BulletHitType::Entity(client.0 as i32);
                trace.hit_location = i32::from(hitloc);
            }
            Some(ColliderId::EntityDObjBone {
                owner,
                bone,
                part_classification,
                surface_flags,
            }) => {
                trace.surface_flags = surface_flags;
                trace.hit_location = i32::from(part_classification);
                trace.bone = Some(bone);
                trace.hit = match owner.script_model() {
                    Some(id) => BulletHitType::Entity(self.number_of(id)),
                    None => BulletHitType::Entity(ENTITY_WORLD),
                };
            }
            Some(ColliderId::EntityLinkedBrush {
                owner,
                surface_flags,
                ..
            }) => {
                trace.surface_flags = surface_flags;
                trace.hit = match owner.script_model() {
                    Some(id) => BulletHitType::Entity(self.number_of(id)),
                    None => BulletHitType::Entity(ENTITY_WORLD),
                };
            }
        }
        trace
    }
}

fn team_of(name: Option<&str>) -> Option<FireTeam> {
    match name? {
        "axis" => Some(FireTeam::Axis),
        "allies" => Some(FireTeam::Allies),
        "neutral" => Some(FireTeam::Neutral),
        _ => None,
    }
}

impl FireWorld for MatchFireWorld<'_, '_> {
    fn level_time(&self) -> i32 {
        crate::corpse::level_time_ms(self.tick)
    }

    fn previous_level_time(&self) -> i32 {
        crate::corpse::level_time_ms(crate::world::Tick(self.tick.0.saturating_sub(1)))
    }

    fn dvar(&self, name: &str) -> Option<String> {
        self.world
            .ecs_ref()
            .get_resource::<crate::script::MatchScript>()?
            .dvars
            .get(name)
            .cloned()
    }

    fn rand(&mut self) -> i32 {
        let state = self.world.stuck_holdrand_mut();
        *state = state.wrapping_mul(0x0003_43fd).wrapping_add(0x0026_9ec3);
        (*state >> 17) as i32
    }

    fn weapon(&self, weapon: u32) -> Option<&GameWeapon> {
        self.content.game_weapon(weapon)
    }

    fn bullet_trace(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        pass_entity: i32,
        mask: u32,
    ) -> BulletTrace {
        self.trace_bodies(start, end, pass_entity, mask, None)
    }

    fn locational_trace(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        pass_entity: i32,
        mask: u32,
        priority: &[u8],
    ) -> BulletTrace {
        self.trace_bodies(start, end, pass_entity, mask, Some(priority))
    }

    fn run_missile_now(&mut self, missile: i32) {
        let Some(mut state) = self
            .world
            .ecs()
            .get_resource::<GameMissiles>()
            .and_then(|missiles| missiles.0.get(&missile).copied())
        else {
            return;
        };
        let (run, gaps) = if let Some(rules) = missile_rules(self.world) {
            rules.0.run(&mut state, self)
        } else if let Some(movement) = crate::game_move::rules(self.world) {
            movement.run_missile(&mut state, self)
        } else {
            return;
        };
        for gap in gaps {
            self.world.report_game_gap(gap);
        }
        apply_missile_run(self.world, missile, state, run);
    }

    fn antilag(&mut self, players_ms: i32, actors_ms: i32) {
        // A body the game leaves in place is traced where it stands; one it
        // moves back stays where the runtime's lag compensation puts it.
        if players_ms != 0 && actors_ms != 0 {
            return;
        }
        let current = self.world.current_bodies(self.tick);
        if players_ms == 0 {
            self.query.players = current.players;
        }
        if actors_ms == 0 {
            self.query.entities = current.entities;
        }
    }

    fn entity(&mut self, entity: i32) -> Option<FireEntity> {
        if let Some(client) = self.client(entity) {
            let meta = self.world.client_meta(client)?;
            let alive = meta.lifecycle == crate::match_state::ClientLifecycle::Alive;
            let team = team_of(crate::spawn::team_name(meta.client_state_team));
            let ps = self.world.player(client)?;
            // A sentient links one unit wider than its bounds.
            let bounds = self.world.player_area_bounds(client).map(|b| {
                let (mid, half) = (b.mid(), b.half());
                (
                    core::array::from_fn(|i| mid[i] - half[i] - 1.0),
                    core::array::from_fn(|i| mid[i] + half[i] + 1.0),
                )
            });
            return Some(FireEntity {
                kind: FireEntityKind::Player,
                origin: ps.origin,
                angles: ps.viewangles,
                contents: 0,
                bounds,
                model_bounds: None,
                eye: None,
                script_model: false,
                blast_handler: false,
                takes_damage: alive,
                health: ps.health,
                max_health: ps.max_health,
                team,
                ignore_locational_damage: false,
                allow_death: true,
                delays_death: false,
                god: false,
                demigod: false,
                ai_type: None,
            });
        }
        let id = self.model(entity)?;
        let target = crate::script::shot_target(self.world.ecs(), id)?;
        Some(FireEntity {
            kind: if target.actor {
                FireEntityKind::Actor
            } else {
                FireEntityKind::Other
            },
            origin: target.origin,
            angles: target.angles,
            contents: target.contents,
            bounds: target.brush.map(|(world, _)| world),
            model_bounds: target.brush.map(|(_, local)| local),
            eye: target.eye,
            script_model: target.script_model,
            blast_handler: target.destructible,
            takes_damage: target.takes_damage,
            health: target.health,
            max_health: target.health,
            team: team_of(target.team),
            ignore_locational_damage: target.ignore_locational_damage,
            allow_death: target.allow_death,
            delays_death: target.delays_death,
            god: false,
            demigod: false,
            ai_type: target.ai_type,
        })
    }

    fn penetration_depth(&self, penetrate_type: i32, surface_type: i32) -> Option<f32> {
        if !self.world.pen_table_loaded() {
            // A game whose scripts the runtime runs keeps its table in the
            // zones' raw files.
            return crate::script::host::natives::t6::zone_penetration_depth(
                self.world.ecs_ref(),
                penetrate_type,
                surface_type,
            );
        }
        Some(
            self.world
                .penetration_table()
                .depth(penetrate_type, u32::try_from(surface_type).ok()?),
        )
    }

    fn hit_location_multiplier(&self, hit_location: i32) -> Option<f32> {
        let location = usize::try_from(hit_location).ok()?;
        self.content.hit_location_multiplier(location).or_else(|| {
            crate::script::host::natives::t6::zone_location_scale(self.world.ecs_ref(), location)
        })
    }

    fn damage(&mut self, damage: &FireDamage) {
        if let Some(victim) = self.client(damage.target) {
            let attacker = self.client(damage.attacker);
            let commit = attacker.and_then(|attacker| {
                Some(crate::damage::DeathCommit {
                    victim,
                    victim_life: self.world.client_meta(victim)?.life_sequence,
                    attacker,
                    attacker_life: self.world.client_meta(attacker)?.life_sequence,
                    source: crate::DamageSource::Shot(self.shot),
                    pellet: crate::identities::PelletId(0),
                    weapon: damage.weapon,
                    amount: damage.amount,
                    killcam_entity_start_time: 0,
                    inflictor_origin: Some(damage.point),
                    hitloc: 0,
                })
            });
            let attacker_object = match attacker {
                Some(_) => None,
                None => self.object_of(damage.attacker),
            };
            let hit = crate::script_player::Hit {
                victim,
                attacker,
                attacker_object,
                amount: damage.amount,
                flags: damage.flags,
                means: damage.means,
                weapon: damage.weapon,
                point: damage.point,
                dir: damage.dir,
                hitloc: u8::try_from(damage.location).unwrap_or(0),
                inflictor: None,
                commit,
            };
            crate::script::game_player_damage(self.world.ecs(), self.tick, &hit);
            return;
        }
        let Some(target) = self.model(damage.target) else {
            return;
        };
        if let Some(bone) = damage.bone
            && crate::t5_destructible::apply_hit(
                self.world,
                self.tick,
                crate::AuthorityModelOwner::ScriptModel(target),
                bone,
                damage.amount.max(0) as u32,
                self.client(damage.attacker),
            )
        {
            return;
        }
        let attacker = self.client(damage.attacker);
        let attacker_object = match attacker {
            Some(_) => None,
            None => self.object_of(damage.attacker),
        };
        let inflictor = if damage.inflictor == damage.attacker {
            None
        } else {
            self.object_of(damage.inflictor)
        };
        crate::script::damage_entity(
            self.world.ecs(),
            &crate::script::EntityHit {
                target,
                amount: damage.amount,
                attacker,
                attacker_object,
                means: damage.means,
                weapon: damage.weapon,
                point: damage.point,
                dir: damage.dir,
                bone: damage.bone.map(usize::from),
                flags: damage.flags,
                hit_location: Some(damage.hit_location),
                part: None,
                inflictor,
                time_offset: damage.time_offset,
            },
        );
    }

    fn damage_triggers(
        &mut self,
        attacker: i32,
        start: [f32; 3],
        end: [f32; 3],
        amount: i32,
        means: &'static str,
    ) {
        let activator = match self.client(attacker) {
            Some(client) => crate::script::host::triggers::Activator::Client(client),
            None => match self.object_of(attacker) {
                Some(object) => crate::script::host::triggers::Activator::Object(object),
                None => return,
            },
        };
        crate::script::host::triggers::damage_line_by(
            self.world.ecs(),
            start,
            end,
            amount,
            activator,
            means,
        );
    }

    fn impact(&mut self, impact: &BulletImpact) {
        let segment = self.segment;
        self.segment = self.segment.saturating_add(1);
        let mut surface_flags = impact.surface_flags;
        if impact.exit {
            surface_flags |= crate::combat::IMPACT_EXIT_SURFACE;
        }
        self.world.push_pellet_fx(crate::PelletFxRecord {
            attacker: impact.attacker,
            weapon: impact.weapon,
            correlation: self.shot.0,
            fire_cause: None,
            pellet: 0,
            segment,
            hand: 0,
            start: impact.start,
            end: impact.point,
            normal: impact.normal,
            surf_type: impact.surface_type,
            surface_flags,
            flesh_flags: if impact.entity == ENTITY_NONE {
                0
            } else {
                impact.flags & 0x3
            },
        });
    }

    fn entities_in_box(&mut self, mins: [f32; 3], maxs: [f32; 3]) -> Vec<i32> {
        // Candidates: the game tests each one's own bounds. A body reaches at
        // most this far beyond its origin.
        const REACH: f32 = 128.0;
        let overlaps =
            |lo: [f32; 3], hi: [f32; 3]| (0..3).all(|i| lo[i] <= maxs[i] && hi[i] >= mins[i]);
        let mut out = Vec::new();
        for id in self.world.client_ids_sorted() {
            let Some(ps) = self.world.player(id) else {
                continue;
            };
            let o = ps.origin;
            if overlaps(o.map(|c| c - REACH), o.map(|c| c + REACH)) {
                out.push(id.0 as i32);
            }
        }
        for (id, origin) in crate::script::presented_origins(self.world.ecs()) {
            let near = overlaps(origin.map(|c| c - REACH), origin.map(|c| c + REACH));
            let brush = self
                .world
                .entity_collision_capabilities()
                .iter()
                .find(|row| row.owner.script_model() == Some(id))
                .and_then(|row| row.linked_brushes.first())
                .and_then(|brush| {
                    let model = self
                        .world
                        .clip_cmodels()
                        .models
                        .get(brush.cmodel_handle as usize)?;
                    let at = brush.origin;
                    Some(overlaps(
                        core::array::from_fn(|i| at[i] + model.mins[i] - model.radius),
                        core::array::from_fn(|i| at[i] + model.maxs[i] + model.radius),
                    ))
                });
            if near || brush == Some(true) {
                let number = self.number_of(id);
                if number != ENTITY_NONE {
                    out.push(number);
                }
            }
        }
        out
    }

    fn sight_clear(&mut self, start: [f32; 3], end: [f32; 3], pass: [i32; 2], mask: u32) -> bool {
        let ignore = pass.iter().find_map(|&e| self.client(e));
        let models: Vec<ScriptModelId> = pass.iter().filter_map(|&e| self.model(e)).collect();
        let ignore_model = models.first().copied();
        // The trace ignores one model; a second passed one leaves the rows.
        let others: Vec<_>;
        let rows = if models.len() > 1 {
            others = self
                .query
                .entities
                .rows
                .iter()
                .filter(|row| {
                    !row.owner
                        .script_model()
                        .is_some_and(|id| models.contains(&id))
                })
                .cloned()
                .collect();
            &others
        } else {
            &self.query.entities.rows
        };
        let outcome = crate::bullet_collision::bullet_trace_with_entity_models(
            self.world.clip_brushes(),
            self.world.clip_bsp(),
            self.world.clip_cmodels(),
            self.world.clip_mesh(),
            &self.query.players.poses,
            rows,
            &BulletTraceQuery {
                start,
                end,
                mask,
                ignore,
                ignore_hit: None,
                ignore_model,
            },
            &|_| true,
        );
        matches!(outcome, TraceOutcome::Miss { .. })
    }

    fn player(&self, entity: i32) -> Option<game_api::movement::MovePlayer> {
        let id = self.client(entity)?;
        let frozen = self
            .world
            .client_meta(id)
            .is_some_and(|m| m.controls.frozen);
        Some(self.world.player(id)?.move_player(id.0 as i32, frozen))
    }

    fn players_ignore_radius_damage(&self) -> bool {
        self.world
            .ecs_ref()
            .get_resource::<crate::script::RoundScript>()
            .is_some_and(|runtime| runtime.engine.players_ignore_radius_damage)
    }

    fn platform_at(&mut self, player: i32, time: i32) -> Option<[f32; 3]> {
        let client = self.client(player)?;
        crate::script::host::natives::t6::vehicle::ridden_platform_at(
            self.world.ecs(),
            client.0,
            time,
        )
    }

    fn grenade_suicide_disabled(&self) -> bool {
        self.world
            .ecs_ref()
            .get_resource::<crate::script::RoundScript>()
            .is_some_and(|runtime| runtime.engine.grenade_suicide_disabled)
    }

    fn notify(&mut self, entity: i32, name: &str, args: &[NotifyArg]) {
        let receiver = self.object_of(entity);
        let args: Vec<crate::script::NotifyValue> = args
            .iter()
            .map(|arg| match *arg {
                NotifyArg::Weapon(w) => crate::script::NotifyValue::String(
                    crate::script_player::weapon_name(self.world, w),
                ),
                NotifyArg::Entity(e) => crate::script::NotifyValue::Entity(
                    self.object_of(e).or_else(|| self.adopt_missile(e)),
                ),
                NotifyArg::Vector(v) => crate::script::NotifyValue::Vector(v),
                NotifyArg::Float(f) => crate::script::NotifyValue::Float(f),
                NotifyArg::Int(n) => crate::script::NotifyValue::Int(n),
                NotifyArg::String(text) => crate::script::NotifyValue::String(text.to_owned()),
            })
            .collect();
        if let Some(receiver) = receiver {
            crate::script::notify_entity(self.world.ecs(), receiver, name, &args);
        }
    }

    fn set_delayed_death(&mut self, entity: i32) {
        if let Some(object) = self.object_of(entity)
            && let Some(mut runtime) = self
                .world
                .ecs()
                .get_resource_mut::<crate::script::RoundScript>()
        {
            runtime.set_object_field(object, "delayeddeath", crate::script::Value::Int(1));
        }
    }

    fn map_center(&self) -> [f32; 3] {
        self.world
            .ecs_ref()
            .get_resource::<crate::script::RoundScript>()
            .map_or([0.0; 3], |runtime| runtime.engine.map_center)
    }

    fn point_contents(&mut self, point: [f32; 3], mask: u32) -> u32 {
        let outcome = crate::bullet_collision::bullet_trace_with_entity_models(
            self.world.clip_brushes(),
            self.world.clip_bsp(),
            self.world.clip_cmodels(),
            self.world.clip_mesh(),
            &[],
            &[],
            &BulletTraceQuery {
                start: point,
                end: point,
                mask,
                ignore: None,
                ignore_hit: None,
                ignore_model: None,
            },
            &|_| true,
        );
        match outcome {
            TraceOutcome::StartSolid {
                collider: Some(ColliderId::World { contents, .. }),
                ..
            } => contents & mask,
            _ => 0,
        }
    }

    fn spawn_missile(&mut self, mut missile: GameMissile) -> Option<i32> {
        let owner = self.client(missile.owner).unwrap_or(ClientId(0));
        let owner_life = self
            .world
            .client_meta(owner)
            .map_or(crate::identities::LifeSequence(0), |meta| {
                meta.life_sequence
            });
        let id = self.world.allocate_projectile_id();
        let entnum = match self
            .world
            .allocate_dynamic_entity(crate::gentity::EntityRunKind::Missile)
        {
            Ok(entity) => entity.number(),
            Err(error) => {
                diag::warn!(Sim, "game missile not spawned: {error:?}");
                return None;
            }
        };
        missile.entity = entnum;
        let time = crate::corpse::level_time_ms(self.tick);
        let pos = crate::equipment::game_trajectory(&missile.position);
        self.world.push_projectile(crate::ProjectileState {
            id,
            owner,
            owner_life,
            weapon: missile.weapon,
            origin: missile.origin,
            velocity: missile.position.delta,
            pos,
            apos: crate::equipment::game_trajectory(&missile.angles),
            entnum,
            launch_time: missile.visible_at,
            spawn_time_ms: time,
            detonate_at_ms: None,
            cleanup_at_ms: i32::MAX,
            travel_distance: 0.0,
            live: true,
            detonation_armed: false,
            stuck_pane: None,
            grounded: false,
            guide: crate::MissileGuide::default(),
            attached_to: None,
        });
        self.world
            .ecs()
            .get_resource_or_insert_with(GameMissiles::default)
            .0
            .insert(entnum, missile);
        Some(entnum)
    }

    fn push_player(&mut self, entity: i32, velocity: [f32; 3]) {
        let Some(id) = self.client(entity) else {
            return;
        };
        if let Some(ps) = self.world.player_mut(id) {
            for (v, add) in ps.velocity.iter_mut().zip(velocity) {
                *v += add;
            }
        }
    }

    fn bounce(&mut self, missile: i32, surface_type: u8) {
        let Some(projectile) = self.world.projectile_by_number(missile) else {
            return;
        };
        crate::equipment::push_grenade_bounce(self.world, self.tick, &projectile, surface_type);
    }

    fn store_player(&mut self, entity: i32, player: &game_api::movement::MovePlayer) {
        let Some(id) = self.client(entity) else {
            return;
        };
        if let Some(ps) = self.world.player_mut(id) {
            ps.store_move_player(player);
        }
    }

    fn explosion(&mut self, explosion: &MissileExplosion) {
        let Some(projectile) = self.world.projectile_by_number(explosion.missile) else {
            return;
        };
        crate::equipment::push_explosion_event(
            self.world,
            self.tick,
            &projectile,
            explosion.origin,
            explosion.normal,
            explosion.surface_type,
            false,
        );
    }

    fn weapon_event(&mut self, entity: i32, weapon: u32, event: WeaponEvent) {
        let Some(client) = self.client(entity) else {
            return;
        };
        crate::combat::push_game_weapon_event(
            self.world, self.tick, client, weapon, event, self.shot,
        );
    }

    fn set_entity_pose(&mut self, entity: i32, origin: [f32; 3], angles: [f32; 3]) {
        let Some(object) = self.object_of(entity) else {
            return;
        };
        if let Some(mut runtime) = self
            .world
            .ecs()
            .get_resource_mut::<crate::script::RoundScript>()
        {
            runtime.set_object_field(object, "origin", crate::script::Value::Vector(origin));
            runtime.set_object_field(object, "angles", crate::script::Value::Vector(angles));
        }
    }
}

/// A script's RadiusDamage by the game's own blast rules; false when the
/// match runs no game rules for it.
pub(crate) fn script_blast(
    world: &mut FrameWorld,
    tick: Tick,
    blast: &crate::script::ScriptBlast,
) -> bool {
    let Some(movement) = crate::game_move::rules(world) else {
        return false;
    };
    if !crate::game_move::game_weapons(world) {
        return false;
    }
    let mut around = MatchFireWorld::new(world, tick, blast.attacker);
    let inflictor = blast
        .inflictor
        .map_or(ENTITY_NONE, |id| around.number_of(id));
    let attacker = blast.attacker.map_or(ENTITY_WORLD, |id| id.0 as i32);
    let rules = game_api::fire::RadiusDamage {
        origin: blast.origin,
        inflictor,
        attacker,
        inner: blast.max,
        outer: blast.min,
        radius: blast.radius,
        cone: None,
        ignore: inflictor,
        means: blast.means,
        weapon: blast.weapon,
        script: true,
    };
    let Some(gaps) = movement.radius_damage(&rules, &mut around) else {
        return false;
    };
    for gap in gaps {
        world.report_game_gap(gap);
    }
    true
}

/// A sight trace by the match's collision as it stands, through two script
/// entities.
pub(crate) fn sight_clear(
    world: &mut FrameWorld,
    tick: Tick,
    start: [f32; 3],
    end: [f32; 3],
    pass: [u64; 2],
    mask: u32,
) -> bool {
    let mut around = MatchFireWorld::new(world, tick, None);
    let pass = pass.map(|object| around.number_of_object(object));
    around.sight_clear(start, end, pass, mask)
}

/// An actor's melee blow as the script host knows it: its eye, facing,
/// weapon flash and enemy, by script object.
pub(crate) struct ScriptActorMelee {
    pub actor: u64,
    pub eye: [f32; 3],
    pub angles: [f32; 3],
    pub flash_forward: Option<[f32; 3]>,
    /// The enemy, with an actor enemy's eye.
    pub enemy: Option<(u64, Option<[f32; 3]>)>,
    pub direction: Option<[f32; 3]>,
    pub weapon: u32,
    pub in_melee_charge: bool,
}

/// An actor's melee blow by the game's own rules: what it struck; `None`
/// when the match runs no game rules for it.
pub(crate) fn actor_melee(
    world: &mut FrameWorld,
    tick: Tick,
    melee: &ScriptActorMelee,
) -> Option<Option<u64>> {
    let movement = crate::game_move::rules(world)?;
    if !crate::game_move::game_weapons(world) {
        return None;
    }
    let (hit, gaps) = {
        let mut around = MatchFireWorld::new(world, tick, None);
        let attacker = around.number_of_object(melee.actor);
        let enemy = melee
            .enemy
            .map(|(object, eye)| (around.number_of_object(object), eye));
        let rules = game_api::fire::ActorMelee {
            attacker,
            eye: melee.eye,
            angles: melee.angles,
            flash_forward: melee.flash_forward,
            enemy,
            direction: melee.direction,
            weapon: melee.weapon,
            in_melee_charge: melee.in_melee_charge,
        };
        let (hit, gaps) = movement.actor_melee(&rules, &mut around)?;
        let hit = match hit {
            ENTITY_NONE => None,
            hit => around.object_of(hit),
        };
        (hit, gaps)
    };
    for gap in gaps {
        world.report_game_gap(gap);
    }
    Some(hit)
}

/// The missiles a game's rules run, by entity number.
#[derive(bevy_ecs::prelude::Resource, Default)]
pub(crate) struct GameMissiles(std::collections::BTreeMap<i32, GameMissile>);

fn missile_rules(world: &FrameWorld) -> Option<game_api::fire::MissileRules> {
    world.bootstrap_ref().mode?.missiles
}

pub(crate) fn has_missile_rules(world: &FrameWorld) -> bool {
    missile_rules(world).is_some()
}

fn apply_missile_run(world: &mut FrameWorld, entnum: i32, missile: GameMissile, run: MissileRun) {
    match run {
        MissileRun::Flying => {
            if let Some(projectile) = world.projectile_mut_by_number(entnum) {
                projectile.origin = missile.origin;
                projectile.pos = crate::equipment::game_trajectory(&missile.position);
                projectile.apos = crate::equipment::game_trajectory(&missile.angles);
                projectile.velocity = missile.position.delta;
            }
            if let Some(mut missiles) = world.ecs().get_resource_mut::<GameMissiles>() {
                missiles.0.insert(entnum, missile);
            }
        }
        MissileRun::Exploded(_) | MissileRun::Gone => {
            if let Some(mut missiles) = world.ecs().get_resource_mut::<GameMissiles>() {
                missiles.0.remove(&entnum);
            }
            if let MissileRun::Exploded(origin) = run
                && let Some(projectile) = world.projectile_by_number(entnum)
            {
                world
                    .weapon_notes
                    .push(crate::equipment::WeaponNote::Detonated {
                        id: projectile.id,
                        origin,
                    });
            }
            let _ = world.despawn_projectile(entnum);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn launch_game_grenade(
    world: &mut FrameWorld,
    owner: ClientId,
    weapon: u32,
    tick: Tick,
    origin: [f32; 3],
    angles: [f32; 3],
    launcher: bool,
    remaining_fuse: Option<i32>,
    scripted_velocity: Option<[f32; 3]>,
) -> bool {
    let Some(rules) = missile_rules(world) else {
        return false;
    };
    let (entnum, gaps) = {
        let mut around = MatchFireWorld::new(world, tick, Some(owner));
        match scripted_velocity {
            Some(vel) => rules.0.spawn_grenade(
                owner.0 as i32,
                weapon,
                origin,
                vel,
                remaining_fuse.unwrap_or(crate::equipment::GRENADE_DEFAULT_FUSE_MS),
                &mut around,
            ),
            None => rules.0.launch_grenade(
                owner.0 as i32,
                weapon,
                origin,
                angles,
                launcher,
                remaining_fuse,
                &mut around,
            ),
        }
    };
    for gap in gaps {
        world.report_game_gap(gap);
    }
    entnum.is_some()
}

pub(crate) fn spawn_game_grenade(
    world: &mut FrameWorld,
    owner: i32,
    weapon: u32,
    tick: Tick,
    origin: [f32; 3],
    velocity: [f32; 3],
    fuse_ms: i32,
) -> Option<crate::ProjectileState> {
    let rules = missile_rules(world)?;
    let attacker = u32::try_from(owner).ok().map(ClientId);
    let (entnum, gaps) = {
        let mut around = MatchFireWorld::new(world, tick, attacker);
        rules
            .0
            .spawn_grenade(owner, weapon, origin, velocity, fuse_ms, &mut around)
    };
    for gap in gaps {
        world.report_game_gap(gap);
    }
    world.projectile_by_number(entnum?)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn launch_game_rocket(
    world: &mut FrameWorld,
    owner: ClientId,
    weapon: u32,
    tick: Tick,
    origin: [f32; 3],
    angles: [f32; 3],
    spread: f32,
    velocity: [f32; 3],
) -> Option<crate::ProjectileState> {
    let rules = missile_rules(world)?;
    let (entnum, gaps) = {
        let mut around = MatchFireWorld::new(world, tick, Some(owner));
        rules.0.launch_rocket(
            owner.0 as i32,
            weapon,
            origin,
            angles,
            spread,
            velocity,
            &mut around,
        )
    };
    for gap in gaps {
        world.report_game_gap(gap);
    }
    world.projectile_by_number(entnum?)
}

pub(crate) fn detonate_game_missile(
    world: &mut FrameWorld,
    tick: Tick,
    entnum: i32,
    attacker: Option<i32>,
) -> bool {
    let Some(rules) = missile_rules(world) else {
        return false;
    };
    let Some(mut missile) = world
        .ecs()
        .get_resource::<GameMissiles>()
        .and_then(|missiles| missiles.0.get(&entnum).copied())
    else {
        return false;
    };
    let owner = attacker
        .or(Some(missile.owner))
        .and_then(|o| u32::try_from(o).ok())
        .map(ClientId);
    let (run, gaps) = {
        let mut around = MatchFireWorld::new(world, tick, owner);
        rules.0.detonate(&mut missile, attacker, &mut around)
    };
    for gap in gaps {
        world.report_game_gap(gap);
    }
    apply_missile_run(world, entnum, missile, run);
    true
}

pub(crate) fn make_game_missile_dud(world: &mut FrameWorld, entnum: i32) -> bool {
    let Some(rules) = missile_rules(world) else {
        return false;
    };
    let Some(mut missiles) = world.ecs().get_resource_mut::<GameMissiles>() else {
        return false;
    };
    let Some(missile) = missiles.0.get_mut(&entnum) else {
        return false;
    };
    rules.0.make_dud(missile);
    true
}

pub(crate) fn reset_game_missile_detonation_time(
    world: &mut FrameWorld,
    tick: Tick,
    entnum: i32,
    after_ms: Option<i32>,
) -> bool {
    let rules = missile_rules(world);
    if rules.is_none() && !crate::game_move::game_weapons(world) {
        return false;
    }
    let Some(mut missile) = world
        .ecs()
        .get_resource::<GameMissiles>()
        .and_then(|missiles| missiles.0.get(&entnum).copied())
    else {
        return false;
    };
    let owner = u32::try_from(missile.owner).ok().map(ClientId);
    if let Some(rules) = rules {
        let around = MatchFireWorld::new(world, tick, owner);
        rules
            .0
            .reset_detonation_time(&mut missile, after_ms, &around);
    } else if let Some(movement) = crate::game_move::rules(world) {
        let around = MatchFireWorld::new(world, tick, owner);
        movement.reset_missile_detonation_time(&mut missile, after_ms, &around);
    } else {
        return false;
    }
    if let Some(mut missiles) = world.ecs().get_resource_mut::<GameMissiles>() {
        missiles.0.insert(entnum, missile);
    }
    true
}

pub(crate) fn set_game_missile_owner(world: &mut FrameWorld, entnum: i32, owner: i32) -> bool {
    let Some(mut missiles) = world.ecs().get_resource_mut::<GameMissiles>() else {
        return false;
    };
    let Some(missile) = missiles.0.get_mut(&entnum) else {
        return false;
    };
    missile.owner = owner;
    true
}

pub(crate) fn missile_ground_entity(world: &FrameWorld, entnum: i32) -> Option<i32> {
    let rules = missile_rules(world)?;
    let missile = world
        .ecs_ref()
        .get_resource::<GameMissiles>()?
        .0
        .get(&entnum)?;
    Some(rules.0.ground_entity(missile))
}

/// A frame of a missile the game spawned; false when the missile is the
/// runtime's own.
pub(crate) fn think_game_missile(world: &mut FrameWorld, tick: Tick, entnum: i32) -> bool {
    let rules = missile_rules(world);
    if rules.is_none() && !crate::game_move::game_weapons(world) {
        return false;
    }
    // Under the game's weapon or missile rules every missile is the game's;
    // one the match does not run here (a client's copy) flies on its trajectory.
    let Some(mut missile) = world
        .ecs()
        .get_resource::<GameMissiles>()
        .and_then(|missiles| missiles.0.get(&entnum).copied())
    else {
        return true;
    };
    let owner = u32::try_from(missile.owner).ok().map(ClientId);
    let (run, gaps) = if let Some(rules) = rules {
        let mut around = MatchFireWorld::new(world, tick, owner);
        rules.0.run(&mut missile, &mut around)
    } else {
        let Some(movement) = crate::game_move::rules(world) else {
            return false;
        };
        let mut around = MatchFireWorld::new(world, tick, owner);
        movement.run_missile(&mut missile, &mut around)
    };
    for gap in gaps {
        world.report_game_gap(gap);
    }
    apply_missile_run(world, entnum, missile, run);
    true
}

impl game_api::turret::TurretWorld for MatchFireWorld<'_, '_> {
    fn turret_pose(&mut self, turret: i32) -> ([f32; 3], [f32; 3]) {
        match self.object_of(turret) {
            Some(object) => {
                crate::script::host::natives::t5_turret::turret_pose(self.world.ecs(), object)
            }
            None => ([0.0; 3], [0.0; 3]),
        }
    }

    fn turret_tag(
        &mut self,
        turret: i32,
        tag: &str,
        gun_angles: [f32; 3],
    ) -> Option<game_api::turret::TurretTag> {
        let object = self.object_of(turret)?;
        let (origin, axis) = crate::script::host::natives::t5_turret::turret_tag(
            self.world.ecs(),
            object,
            tag,
            gun_angles,
        )?;
        Some(game_api::turret::TurretTag { origin, axis })
    }

    fn sentient(&mut self, slot: usize) -> Option<game_api::turret::Sentient> {
        let object =
            crate::script::host::natives::t5_turret::sentient_in(self.world.ecs_ref(), slot)?;
        self.sentient_at(object)
    }

    fn sentient_of(&mut self, entity: i32) -> Option<game_api::turret::Sentient> {
        let object = self.object_of(entity)?;
        crate::script::host::natives::t5_turret::sentients(self.world.ecs_ref())
            .contains(&object)
            .then(|| self.sentient_at(object))
            .flatten()
    }

    fn turret_loop_sound(&mut self, turret: i32, alias: Option<&str>) {
        if let Some(object) = self.object_of(turret) {
            crate::script::host::natives::t5_turret::set_loop_sound(
                self.world.ecs(),
                object,
                alias,
            );
        }
    }

    fn turret_sound(&mut self, turret: i32, alias: &str) {
        if let Some(object) = self.object_of(turret) {
            crate::script::host::natives::t5_turret::play_sound(self.world.ecs(), object, alias);
        }
    }

    fn turret_tag_effect(&mut self, turret: i32, effect: &str, tag: &str) {
        if let Some(object) = self.object_of(turret) {
            crate::script::host::natives::t5_turret::tag_effect(
                self.world.ecs(),
                object,
                effect,
                tag,
            );
        }
    }

    fn turret_fire_sound(&mut self, turret: i32, weapon: u32, barrel: i32, origin: [f32; 3]) {
        self.world
            .push_script_audio(crate::ScriptAudioCommand::TurretFire {
                entity: turret,
                weapon,
                barrel,
                origin,
            });
    }

    fn shake_camera(&mut self, origin: [f32; 3], scale: f32, duration_ms: i32, radius: f32) {
        crate::script::host::natives::t5_turret::shake_camera(
            self.world.ecs(),
            scale,
            duration_ms,
            origin,
            radius,
        );
    }
}
