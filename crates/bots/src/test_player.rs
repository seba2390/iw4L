//! The test player (`IW4L_TEST_PLAYER=1`): drives the first player who is
//! not a bot, for headless runs. It aims at the nearest living actor it can
//! see, fires (pressing and letting go each tick, so a semi-automatic gun
//! fires again), reloads an empty clip, knifes an actor within melee reach
//! when it has nothing to fire, and backs away when actors crowd it. Every
//! five seconds it logs the round, its score, kills, health (and the lowest
//! since the last line), whether it is down, the actors alive, its weapon
//! and ammo, and what its aim lines ended on. It only presses buttons; the
//! game decides the rest.
//!
//! `IW4L_TEST_PLAYER=rounds` is the rounds test: the player also asks for
//! god mode once (cheats must be on) and for an ammo resupply whenever his
//! gun is empty, as the `god` and `give ammo` console commands do, so he
//! lives and fights through the rounds.
//!
//! `IW4L_TEST_PLAYER=use` fights as the rounds test does and, once the
//! scripts have set the map up, uses one of each thing they offer, in this
//! order — a wall weapon, Quick Revive, a door, the box, debris, the power
//! switch — each
//! once he can pay for it and stand by it inside a zone the scripts have
//! enabled (one behind a closed door waits its turn): he is moved in front of it (as `setviewpos` does), looks at
//! its middle, holds use, and two and a half seconds later logs what the
//! game did (score, weapon, whether the trigger is gone, which use triggers
//! the scripts made there). Once the box has taken its price he waits for
//! its weapon to rise and holds use again to take it. Then he goes back and
//! fights on.
//!
//! `IW4L_TEST_USE=box,door,…` picks the kinds the use test tries (wall,
//! perk, door, box, debris, power, pap, rebuild), in that order; `IW4L_TEST_PERK` names
//! the perk it buys (Quick Revive by default; `any` for any other).
//!
//! In every mode, when the game runs its own movement, each change of the
//! movement flags the script builtins read or set (ladder, mantle, no
//! melee, no weapon cycling, being revived) is logged, and so is each hit
//! that winds the damage timer up, with the player's speed. In every mode
//! each hit that lowers his health is logged with the health before and
//! after.
//!
//! `IW4L_TEST_TIMING=1` logs each change of his weapon's state (the game's
//! own state when it runs its own movement) with how long the state before
//! it lasted: reload and fire times, and what a perk makes of them.
//!
//! `IW4L_TEST_ACTORS=field,…` logs each living actor's position, health
//! and those script fields every five seconds.
//!
//! `IW4L_TEST_WAITS=name,…` logs the receivers of the waits pending on
//! those notify names every five seconds.
//!
//! `IW4L_TEST_VARS=key,…` adds those `level.zombie_vars` entries to the log
//! every five seconds; `level:<field>` names a field of `level` itself.
//!
//! `IW4L_TEST_GOTO=<zone>` (with the use test): 5 s in, it goes to a path
//! node standing in that zone, before the use test starts.
//!
//! `IW4L_TEST_BUILD=<equipname>` (with `IW4L_TEST_USE=piece,build,…`):
//! the use test's `piece` picks up a part of that buildable, `build`
//! holds use at its table, and `take` takes a persistent one from its
//! table once built.
//!
//! `IW4L_TEST_WIELD=<weapon>,…`: once he holds one of those weapons (the
//! first listed), his commands ask for it, as a client switching to it
//! does.
//!
//! `IW4L_TEST_PLACE=1` (with `IW4L_TEST_WIELD`): two seconds after the
//! wielded weapon comes up, he looks level, a quarter turn further each
//! time, and holds fire for half a second, as a player setting down
//! equipment he carries does.
//!
//! `IW4L_TEST_THROW=1` (with any fighting mode): every 15 s it holds its
//! lethal grenade's button for half a second at an actor in sight farther
//! than 250 units, then lets go to throw it.
//!
//! `IW4L_TEST_REVIVE=1`: when a teammate is down, the player goes beside
//! him and holds use to revive him.
//!
//! `IW4L_TEST_RIDE=1|<seconds>` (with any fighting mode): 15 s (or the
//! seconds given) in, the player steps
//! onto TranZit's bus and stays, logging where he and the bus are.
//!
//! `IW4L_TEST_PLAYER=revive` is the go-down test with Quick Revive: once
//! he can pay for it he buys Quick Revive as the use test does (fighting
//! nothing), then stands where he spawned as the go-down test does.
//!
//! `IW4L_TEST_PLAYER=watch` presses nothing: it only writes the five-second
//! log (with `IW4L_TEST_ACTORS`, `IW4L_TEST_WAITS`, `IW4L_TEST_VARS`), from a
//! bot when no one else plays, so a headless `serve` run shows the actors.
//!
//! `IW4L_TEST_PLAYER=down` is the go-down test: no god mode, no fighting;
//! he stands where he spawned until the zombies reach him and down him.
//! Each change of his life state and last stand is logged, so a run shows
//! going down and what the game does next.
//!
//! `IW4L_TEST_GRAB=1` (with any fighting mode): when a power-up lies on
//! the ground, he is moved onto it (as `setviewpos` does) to pick it up,
//! and half a second later back where he stood; once a second at most. He
//! logs which.
//!
//! `IW4L_TEST_WALK=1` (with any fighting mode): every 20 s, 10 s in, he
//! walks toward a path node in a clear line 600 to 1200 units away: a jump, a walk, a
//! sprint, then standing still crouched and prone. He only presses buttons
//! and logs what the movement made of them: the distance and speed walked
//! and sprinted, the jump's height, and the view height in each stance.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use net::{
    ActionRequestIds, AuthorityClock, AuthorityWorld, ClientActionInbox, ClientCommandInbox,
    LocalCmdOverride, LocalPresentClient, look_angles_from_degrees,
};
use sim::script::{ScriptOwner, UseKind, UseSite};
use sim::{
    BulletTraceQuery, ClientAction, ClientId, ClientLifecycle, ColliderId, Tick, TraceOutcome,
};

use crate::motor::{UserCmd, buttons};
use crate::roster::BotRoster;

/// The height above an actor's origin it aims near: its chest. It aims at
/// the body box nearest there.
const AIM_HEIGHT: f32 = 48.0;
/// A shot line that ends this near the aim point reached the actor.
const HIT_SLACK: f32 = 32.0;
/// How many of the nearest actors it tries to see each tick.
const SIGHT_CANDIDATES: usize = 4;
/// It backs away when this many actors are nearer than `BACK_OFF`; one
/// alone it knifes.
const BACK_OFF: f32 = 90.0;
const SWARM: usize = 2;
/// It knifes an actor whose aim point is this near its eye: Black Ops 2's
/// `player_zombieMeleeRange` default.
const KNIFE_REACH: f32 = 85.0;
const LOG_PERIOD_MS: i32 = 5000;
/// The throw test throws this often, at an actor at least this far.
const THROW_PERIOD_MS: i32 = 15_000;
const THROW_HOLD_MS: i32 = 500;
const THROW_FROM: f32 = 250.0;
/// The place aid waits this long with the weapon up, then holds fire.
const PLACE_AFTER_MS: i32 = 2000;
const PLACE_HOLD_MS: i32 = 500;
/// The walk test: when and how often it walks, and the end of each of its
/// phases (jump, walk, sprint, crouch, prone), from the walk's start.
const WALK_START_MS: i32 = 10_000;
const WALK_PERIOD_MS: i32 = 20_000;
const WALK_PHASES_MS: [i32; 5] = [300, 1500, 3000, 4000, 5500];
const WALK_SPRINT_PHASES_MS: [i32; 5] = [300, 1500, 10_500, 11_500, 13_000];
const WALK_NODE_RANGE: core::ops::Range<f32> = 600.0..1200.0;
/// The grab test moves onto a power-up at most this often.
const GRAB_PERIOD_MS: i32 = 1000;
/// The rounds test asks for ammo at most this often.
const RESUPPLY_PERIOD_MS: i32 = 2000;
/// The use test starts this long after the player first lives, once the
/// scripts have made their unitrigger stubs.
const USE_START_MS: i32 = 10_000;
/// A kind no site of which is found by this long after the start (or the
/// last use) is dropped.
const USE_GIVE_UP_MS: i32 = 60_000;
/// How often it looks for a site it can pay for.
const TRY_PERIOD_MS: i32 = 1000;
/// How long it waits after being moved, holds use, and watches after.
const ARRIVE_MS: i32 = 500;
/// Long enough, at a buildable's part or table, for its zone to come alive
/// and `_zm_unitrigger` to spawn its trigger.
const ZONE_ARRIVE_MS: i32 = 3000;
const HOLD_MS: i32 = 600;
/// A window's boards go back one at a time while use is held.
const REBUILD_HOLD_MS: i32 = 4000;
/// A buildable's table builds over its stub's `usetime`, 3 s.
const BUILD_HOLD_MS: i32 = 4000;

fn hold_ms(kind: UseKind) -> i32 {
    if kind == UseKind::Build {
        BUILD_HOLD_MS
    } else if kind == UseKind::Rebuild {
        REBUILD_HOLD_MS
    } else {
        HOLD_MS
    }
}
const WATCH_MS: i32 = 2500;
/// TranZit's `powerevent` sets `power_on` 30 s after the lever is thrown.
const POWER_EVENT_MS: i32 = 35_000;
/// After paying for the box he waits this long for its weapon to rise, then
/// holds use again to take it.
const BOX_SPIN_MS: i32 = 6000;
/// How far from a site's middle it would stand, and the room it keeps from
/// a wall behind it.
const STAND_OFF: f32 = 40.0;
/// A wall weapon's or the box's trigger is a turned box the body must
/// touch: he walks up to it.
const STAND_OFF_TOUCH: f32 = 8.0;
/// The room he keeps from a wall walking up to one: just over half his
/// width.
const TOUCH_ROOM: f32 = 16.0;
const STAND_ROOM: f32 = 20.0;
/// How far from a site's middle a use trigger the scripts made counts as
/// that site's.
const OFFER_REACH: f32 = 96.0;
/// Path nodes this near a site (flat) are tried as places to come from, the
/// nearest this many.
const NODE_REACH: f32 = 256.0;
const NODE_CANDIDATES: usize = 16;
/// It walks toward a site this high above the floor (over steps).
const WAIST_HEIGHT: f32 = 32.0;
/// How far into a perk machine its trigger's middle lies: the trigger's
/// radius (`perk_machine_spawn_init` spawns it 40 wide).
const MACHINE_ROOM: f32 = 40.0;
/// With actors left but none in sight this long, it goes to the nearest,
/// and stands this far from it.
const HUNT_AFTER_MS: i32 = 20_000;
const HUNT_STAND_OFF: f32 = 48.0;
/// The revive test holds use this long beside a downed teammate (Black
/// Ops 2's revive takes 3 s), standing this far from him.
const REVIVE_HOLD_MS: i32 = 5_000;
const REVIVE_STAND_OFF: f32 = 32.0;
/// The ride test boards the bus this long after it lives; the bus floor
/// stands this high above its origin.
const RIDE_AFTER_MS: i32 = 15_000;
const BUS_FLOOR: f32 = 36.0;
/// The perk the use test buys.
const QUICK_REVIVE: &str = "specialty_quickrevive";
/// The eye height it plans with before it stands there.
const EYE_HEIGHT: f32 = 60.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Off,
    Fight,
    Rounds,
    Use,
    Down,
    Revive,
    Watch,
}

impl Mode {
    fn from_env() -> Self {
        match std::env::var("IW4L_TEST_PLAYER").as_deref() {
            Ok("1") => Self::Fight,
            Ok("rounds") => Self::Rounds,
            Ok("use") => Self::Use,
            Ok("down") => Self::Down,
            Ok("revive") => Self::Revive,
            Ok("watch") => Self::Watch,
            _ => Self::Off,
        }
    }

    /// God mode and ammo resupplies.
    fn cheats(self) -> bool {
        matches!(self, Self::Rounds | Self::Use)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Arriving,
    Holding,
    Watching,
    Returning,
}

#[derive(Clone, Debug)]
struct Scenario {
    site: UseSite,
    center: [f32; 3],
    stand: [f32; 3],
    home: [f32; 3],
    home_angles: [f32; 3],
    phase: Phase,
    until_ms: i32,
    score: i32,
    weapon: String,
    clip: i32,
    stock: i32,
    offered: Vec<String>,
    /// How many times he has held use there.
    presses: u8,
    /// Until when he waits for `power_on` after throwing a lever.
    power_by_ms: i32,
}

#[derive(Resource, Default)]
pub(crate) struct TestPlayer {
    mode: Mode,
    /// Weapon, clip, and stock after the last wall buy so a repeat wall buy
    /// fires and reloads one round before buying ammo.
    wall_stock: Option<(String, i32, i32)>,
    /// The teammate the revive test is reviving, and until when it holds use.
    reviving: Option<u32>,
    revive_until_ms: i32,
    /// The ride test stepped onto the bus.
    boarded: bool,
    /// How many `IW4L_TEST_GOTO` zones it has gone to.
    went: usize,
    next_ride_log_ms: i32,
    /// When it last saw an actor to aim at.
    last_seen_ms: Option<i32>,
    god_asked: bool,
    next_resupply_ms: i32,
    /// The grab test moves onto a power-up no sooner than this, and back
    /// to where he stood at this time.
    next_grab_ms: i32,
    grab_return: Option<(i32, [f32; 3])>,
    pressed: bool,
    next_log_ms: i32,
    shots: u32,
    knifes: u32,
    /// The throw test's next throw, and until when it holds the button.
    next_throw_ms: i32,
    throw_until_ms: i32,
    throws: u32,
    /// The weapon its command last asked for, kept while nothing is up.
    selected: u16,
    /// What the sight lines to targets ended on: nothing, a model, the world.
    lines: [u32; 3],
    /// When the player first lived.
    alive_since_ms: Option<i32>,
    /// The use test's kinds still to try.
    pending: Vec<UseKind>,
    scenario: Option<Scenario>,
    sites_logged: bool,
    /// Kinds already logged as waiting for their zone.
    deferred: Vec<UseKind>,
    /// The lowest health since the last log line.
    lowest_health: Option<i32>,
    /// His health last tick, for the hit log.
    last_health: Option<i32>,
    /// Where the game's movement had him last tick, for the movement log.
    last_move: Option<sim::MoveStatus>,
    /// His weapon, its state and since when, for the weapon timing log.
    last_weapon_state: Option<(u32, i32, i32)>,
    /// The nearest an actor came to the eye since the last log line.
    nearest: Option<f32>,
    /// When the use test next looks for a site it can pay for.
    next_try_ms: i32,
    /// When its last use ended: a kind with no site gives up counting from
    /// here.
    last_done_ms: i32,
    /// The down test's last life state and last stand seen.
    seen: Option<(ClientLifecycle, i32, Option<i32>)>,
    /// The place aid: since when the wielded weapon is up, and until when it
    /// holds fire facing which way; how many it set down.
    wielded_since_ms: Option<i32>,
    placing: Option<(i32, f32)>,
    placements: u32,
    /// The walk test's walk under way, and when it next walks.
    walk: Option<Walk>,
    next_walk_ms: i32,
}

/// What the walk test has seen of one walk: where each phase started, the
/// jump's top and the lowest view height in each stance phase.
#[derive(Clone, Debug)]
struct Walk {
    target: [f32; 3],
    start_ms: i32,
    phase_from: [[f32; 3]; 5],
    phase: usize,
    top_z: f32,
    low_view: [f32; 2],
    sprint_ms: i32,
    sprint_end: Option<[f32; 3]>,
}

impl TestPlayer {
    pub(crate) fn from_env() -> Self {
        Self {
            mode: Mode::from_env(),
            pending: if let Some(kinds) = kinds_from_env() {
                kinds
            } else if Mode::from_env() == Mode::Revive {
                vec![UseKind::Perk]
            } else {
                vec![
                    UseKind::WallBuy,
                    UseKind::Perk,
                    UseKind::Door,
                    UseKind::Box,
                    UseKind::Debris,
                    UseKind::Power,
                ]
            },
            ..Self::default()
        }
    }
}

/// `IW4L_TEST_PERK=specialty_…`: the perk the use test buys (Quick Revive
/// unless named).
fn wanted_perk() -> String {
    std::env::var("IW4L_TEST_PERK").unwrap_or_else(|_| QUICK_REVIVE.to_owned())
}

/// `IW4L_TEST_USE=box,door,…`: the kinds the use test tries, in that order
/// (wall, perk, door, box, debris, power).
fn kinds_from_env() -> Option<Vec<UseKind>> {
    let list = std::env::var("IW4L_TEST_USE").ok()?;
    Some(
        list.split(',')
            .filter_map(|kind| match kind.trim() {
                "wall" => Some(UseKind::WallBuy),
                "perk" => Some(UseKind::Perk),
                "door" => Some(UseKind::Door),
                "box" => Some(UseKind::Box),
                "debris" => Some(UseKind::Debris),
                "power" => Some(UseKind::Power),
                "pap" => Some(UseKind::PackAPunch),
                "rebuild" => Some(UseKind::Rebuild),
                "piece" => Some(UseKind::Piece),
                "build" => Some(UseKind::Build),
                "take" => Some(UseKind::Take),
                "teller" => Some(UseKind::Teller),
                _ => None,
            })
            .collect(),
    )
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt()
}

fn flat_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// View angles (pitch down positive, yaw) from `from` toward `to`.
fn look(from: [f32; 3], to: [f32; 3]) -> [f32; 3] {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let flat = (d[0] * d[0] + d[1] * d[1]).sqrt();
    [
        -d[2].atan2(flat).to_degrees(),
        d[1].atan2(d[0]).to_degrees(),
        0.0,
    ]
}

fn fmt_point(p: [f32; 3]) -> String {
    format!("({:.0} {:.0} {:.0})", p[0], p[1], p[2])
}

/// How far a line from `start` toward `end` runs before it meets the world.
fn clear_run(world: &sim::SimWorld, id: ClientId, start: [f32; 3], end: [f32; 3]) -> f32 {
    let full = distance(start, end);
    match world.sensor_trace(BulletTraceQuery {
        start,
        end,
        mask: world.bullet_mask(),
        ignore: Some(id),
        ignore_hit: None,
        ignore_model: None,
    }) {
        TraceOutcome::Miss { .. } => full,
        TraceOutcome::Hit { end: hit, .. } => distance(start, hit),
        TraceOutcome::StartSolid { .. } | TraceOutcome::Invalid { .. } => 0.0,
    }
}

/// Where to stand to use a site: the nearest path node (a place a player
/// may stand) that sees the site's middle, moved toward it until it is
/// `STAND_OFF` away (a turned box he must touch: right up to it), on the
/// floor below, inside a zone the scripts have
/// enabled (anywhere else Black Ops 2 kills him as out of the playable
/// area). None when there is no such place.
fn stand_point(
    world: &sim::SimWorld,
    id: ClientId,
    site: &UseSite,
    center: [f32; 3],
) -> Option<[f32; 3]> {
    // A trigger brush reaches out from its middle; a line to its middle
    // may end on its edge (a door's own brush) and still see it.
    let slack = site
        .brush
        .and_then(|n| world.clip_cmodels().models.get(n as usize))
        .map_or(0.0, |model| {
            (0..3)
                .map(|i| (model.maxs[i] - model.mins[i]) * 0.5)
                .fold(0.0, f32::max)
        })
        + STAND_ROOM
        // A perk machine's trigger is in the machine: the line to its
        // middle ends on the machine.
        + if matches!(site.kind, UseKind::Perk | UseKind::Teller) {
            MACHINE_ROOM
        } else {
            0.0
        };
    let mut nodes: Vec<[f32; 3]> = world
        .path_node_origins()
        .into_iter()
        .filter(|node| flat_distance(*node, center) <= NODE_REACH)
        .collect();
    nodes.sort_by(|a, b| distance(*a, center).total_cmp(&distance(*b, center)));
    for node in nodes.into_iter().take(NODE_CANDIDATES) {
        let eye = [node[0], node[1], node[2] + EYE_HEIGHT];
        let to_center = distance(eye, center);
        if clear_run(world, id, eye, center) + slack < to_center {
            continue;
        }
        let flat = flat_distance(node, center);
        let (off, room) = if site.kind == UseKind::Teller && site.cost == Some(0) {
            (0.0, TOUCH_ROOM)
        } else if site.touch
            || matches!(
                site.kind,
                UseKind::WallBuy | UseKind::Box | UseKind::Rebuild
            )
        {
            (STAND_OFF_TOUCH, TOUCH_ROOM)
        } else if matches!(site.kind, UseKind::Build | UseKind::Take) {
            (24.0, TOUCH_ROOM)
        } else {
            (STAND_OFF, STAND_ROOM)
        };
        let towards = if flat >= 1.0 {
            [(center[0] - node[0]) / flat, (center[1] - node[1]) / flat]
        } else {
            [0.0, 1.0]
        };
        let (step, dir) = if site.weapon.as_deref() == Some("sloth") {
            let yaw = site.angles[1].to_radians();
            let goal = [
                center[0] + yaw.cos() * 44.0,
                center[1] + yaw.sin() * 44.0,
                center[2],
            ];
            let to_goal = flat_distance(node, goal);
            if to_goal >= 1.0 {
                (
                    to_goal,
                    [(goal[0] - node[0]) / to_goal, (goal[1] - node[1]) / to_goal],
                )
            } else {
                (0.0, [0.0, 1.0])
            }
        } else if flat >= off {
            (flat - off, towards)
        } else {
            (off - flat, [-towards[0], -towards[1]])
        };
        let waist = node[2] + WAIST_HEIGHT;
        let reach = clear_run(
            world,
            id,
            [node[0], node[1], waist],
            [
                node[0] + dir[0] * (step + room),
                node[1] + dir[1] * (step + room),
                waist,
            ],
        );
        let step = step.min((reach - room).max(0.0));
        let above = [node[0] + dir[0] * step, node[1] + dir[1] * step, waist];
        let drop = clear_run(
            world,
            id,
            above,
            [above[0], above[1], above[2] - 2.0 * WAIST_HEIGHT],
        );
        let stand = [above[0], above[1], above[2] - drop + 1.0];
        let stand_eye = [stand[0], stand[1], stand[2] + EYE_HEIGHT];
        if clear_run(world, id, stand_eye, center) + slack < distance(stand_eye, center) {
            continue;
        }
        if world.in_enabled_player_volume(stand) {
            return Some(stand);
        }
    }
    None
}

/// Starts the cheapest site of the first pending kind it can pay for.
fn next_scenario(
    test: &mut TestPlayer,
    world: &sim::SimWorld,
    id: ClientId,
    (origin, viewangles, weapon): ([f32; 3], [f32; 3], u32),
    score: i32,
    now_ms: i32,
    since_ms: i32,
) -> Option<Scenario> {
    let sites = world.use_sites();
    if !test.sites_logged {
        test.sites_logged = true;
        let count = |kind| sites.iter().filter(|site| site.kind == kind).count();
        diag::info!(
            Sim,
            "test player: use sites — wall weapons {} perks {} box {} doors {} debris {} power {}; enabled zone volumes {}",
            count(UseKind::WallBuy),
            count(UseKind::Perk),
            count(UseKind::Box),
            count(UseKind::Door),
            count(UseKind::Debris),
            count(UseKind::Power),
            world.enabled_player_volume_count()
        );
        let buildables: Vec<String> = sites
            .iter()
            .filter(|site| matches!(site.kind, UseKind::Piece | UseKind::Build))
            .map(|site| {
                format!(
                    "{:?}:{}@{}",
                    site.kind,
                    site.weapon.as_deref().unwrap_or("-"),
                    fmt_point(site.origin)
                )
            })
            .collect();
        if !buildables.is_empty() {
            diag::info!(Sim, "test player: buildable sites {buildables:?}");
        }
    }
    // In order: one it cannot pay for yet waits, and so do the rest.
    if let Some(&kind) = test.pending.first() {
        // A wall stub with no `cost` sells something else by its own rule
        // (claymores).
        let mut of_kind: Vec<&UseSite> = sites
            .iter()
            .filter(|site| site.kind == kind && (kind != UseKind::WallBuy || site.cost.is_some()))
            // `IW4L_TEST_BUILD=<equipname>`: that buildable's parts and table.
            .filter(|site| {
                !matches!(kind, UseKind::Piece | UseKind::Build | UseKind::Take)
                    || std::env::var("IW4L_TEST_BUILD").is_ok_and(|wanted| {
                        wanted.split(',').any(|w| {
                            site.weapon.as_deref() == Some(w.trim())
                                || matches!(kind, UseKind::Build | UseKind::Take)
                                    && site.weapon.as_deref() == Some("open_table")
                        })
                    })
            })
            // Of the perks, Quick Revive: the one a solo player's down asks for.
            .filter(|site| {
                let wanted = wanted_perk();
                let owned = world.player_perks(id);
                kind != UseKind::Perk
                    || (!owned
                        .iter()
                        .any(|p| site.weapon.as_deref() == Some(p.as_str()))
                        && (wanted
                            .split(',')
                            .any(|w| site.weapon.as_deref() == Some(w.trim()))
                            || wanted == "any" && site.weapon.as_deref() != Some(QUICK_REVIVE)))
            })
            .collect();
        // `_zm_unitrigger::main` offers only the stubs of active zones (and
        // those filed under none); the others wait for a door to open.
        let offered = of_kind.len();
        let filed: Vec<String> = of_kind
            .iter()
            .map(|site| {
                format!(
                    "{}@{}",
                    site.zone.as_deref().unwrap_or("-"),
                    fmt_point(site.origin)
                )
            })
            .collect();
        // A buildable's part or table or perk machine in an enabled zone is
        // gone to: standing there makes its zone active, as walking there would.
        of_kind.retain(|site| {
            site.zone.as_deref().is_none_or(|zone| {
                world.zone_is_active(zone)
                    || matches!(
                        kind,
                        UseKind::Piece | UseKind::Build | UseKind::Take | UseKind::Perk
                    ) && world.zone_is_enabled(zone)
            })
        });
        if of_kind.is_empty() && offered > 0 {
            if !test.deferred.contains(&kind) {
                test.deferred.push(kind);
                diag::info!(
                    Sim,
                    "test player: use {kind:?}: none in an active zone yet (filed under {filed:?})"
                );
            }
            if kind != UseKind::Piece {
                test.pending.rotate_left(1);
            }
            if (kind == UseKind::Piece
                || test
                    .pending
                    .iter()
                    .all(|k| test.deferred.contains(k) || sites.iter().all(|s| s.kind != *k)))
                && let Some(blocker) = [UseKind::Door, UseKind::Debris].into_iter().find(|k| {
                    sites.iter().any(|s| {
                        s.kind == *k
                            && stand_point(world, id, s, world.use_site_center(s)).is_some()
                    })
                })
            {
                test.pending.insert(0, blocker);
            }
            return None;
        }
        // The power lever a buildable makes waits for its build.
        if of_kind.is_empty()
            && kind == UseKind::Power
            && test
                .pending
                .iter()
                .any(|k| matches!(k, UseKind::Piece | UseKind::Build))
        {
            test.pending.rotate_left(1);
            return None;
        }
        if of_kind.is_empty() {
            if now_ms - since_ms.max(test.last_done_ms) >= USE_GIVE_UP_MS {
                test.pending.remove(0);
                diag::info!(Sim, "test player: use {kind:?}: none on this map");
            } else {
                test.pending.rotate_left(1);
            }
            return None;
        }
        // The cheapest, and of those the nearest (nearest first when
        // building with points to spare; ordered by IW4L_TEST_BUILD list).
        let build_order: Vec<String> = std::env::var("IW4L_TEST_BUILD")
            .ok()
            .map(|s| s.split(',').map(|w| w.trim().to_owned()).collect())
            .unwrap_or_else(Vec::new);
        let build_rank = |site: &UseSite| {
            site.weapon
                .as_deref()
                .and_then(|w| build_order.iter().position(|b| b == w))
                .unwrap_or(build_order.len())
        };
        let build_mode = !build_order.is_empty() && score >= 2000;
        of_kind.sort_by(|a, b| {
            build_rank(a).cmp(&build_rank(b)).then_with(|| {
                if build_mode {
                    distance(origin, a.origin).total_cmp(&distance(origin, b.origin))
                } else {
                    a.cost
                        .unwrap_or(0)
                        .cmp(&b.cost.unwrap_or(0))
                        .then(distance(origin, a.origin).total_cmp(&distance(origin, b.origin)))
                }
            })
        });
        let Some((site, center, stand)) = of_kind.iter().find_map(|site| {
            let center = world.use_site_center(site);
            Some((*site, center, stand_point(world, id, site, center)?))
        }) else {
            // None he may stand at yet: the rest go first, this waits for a
            // door to open its zone.
            if !test.deferred.contains(&kind) {
                test.deferred.push(kind);
                diag::info!(
                    Sim,
                    "test player: use {kind:?}: no place to stand by it in an enabled zone yet"
                );
            }
            test.pending.rotate_left(1);
            if test
                .pending
                .iter()
                .all(|k| test.deferred.contains(k) || sites.iter().all(|s| s.kind != *k))
                && let Some(blocker) = [UseKind::Door, UseKind::Debris].into_iter().find(|k| {
                    sites.iter().any(|s| {
                        s.kind == *k
                            && stand_point(world, id, s, world.use_site_center(s)).is_some()
                    })
                })
            {
                test.pending.insert(0, blocker);
            }
            return None;
        };
        if site.cost.unwrap_or(0) > score {
            return None;
        }
        if matches!(kind, UseKind::Door | UseKind::Debris) {
            test.deferred.clear();
        }
        test.pending.remove(0);
        if kind == UseKind::Build
            && !build_order.is_empty()
            && test.pending.first() != Some(&UseKind::Piece)
            && sites.iter().any(|s| {
                s.kind == UseKind::Piece
                    && s.weapon
                        .as_deref()
                        .is_some_and(|w| build_order.iter().any(|b| b == w))
            })
        {
            test.pending.insert(0, UseKind::Build);
            test.pending.insert(0, UseKind::Piece);
        }
        return Some(Scenario {
            site: site.clone(),
            center,
            stand,
            home: origin,
            home_angles: viewangles,
            phase: Phase::Arriving,
            until_ms: now_ms
                + if matches!(
                    kind,
                    UseKind::Piece | UseKind::Build | UseKind::Take | UseKind::Perk
                ) {
                    ZONE_ARRIVE_MS
                } else {
                    ARRIVE_MS
                },
            score,
            weapon: world.weapon_script_name(weapon).to_owned(),
            clip: 0,
            stock: 0,
            offered: Vec::new(),
            presses: 0,
            power_by_ms: 0,
        });
    }
    None
}

/// Where the test player's commands go: the server's inbox, or, when he
/// drives the window's own player, the local client's input, so the client
/// sends and predicts them and the view faces where he shoots.
#[derive(SystemParam)]
pub(crate) struct DriveCmds<'w> {
    inbox: ResMut<'w, ClientCommandInbox>,
    local: Res<'w, LocalPresentClient>,
    window: Option<ResMut<'w, LocalCmdOverride>>,
}

impl DriveCmds<'_> {
    fn push(&mut self, id: ClientId, cmd: UserCmd) {
        match self.window.as_mut() {
            Some(window) if id == self.local.0 => window.0 = Some(cmd),
            _ => self.inbox.push(id, None, cmd, None),
        }
    }
}

pub(crate) fn drive_test_player(
    mut test: ResMut<TestPlayer>,
    clock: Res<AuthorityClock>,
    world: Res<AuthorityWorld>,
    roster: Res<BotRoster>,
    mut cmds: DriveCmds,
    mut actions: ResMut<ClientActionInbox>,
    mut request_ids: ResMut<ActionRequestIds>,
) {
    if test.mode == Mode::Off {
        return;
    }
    let snapshot = world.0.snapshot(Tick(clock.tick.saturating_sub(1)));
    // The watch test logs from a bot when no one else plays (`serve`).
    let Some((id, ps)) = snapshot
        .players
        .iter()
        .find(|(id, _)| roster.bots.iter().all(|bot| bot.id != *id))
        .or_else(|| {
            (test.mode == Mode::Watch)
                .then(|| snapshot.players.first())
                .flatten()
        })
        .map(|(id, ps)| (*id, *ps))
    else {
        return;
    };
    let Some(meta) = snapshot.meta.for_client(id) else {
        return;
    };
    // The zombies alone: an actor on another team (Buried's Leroy) is no
    // target.
    let actors: Vec<_> = world
        .0
        .living_actors()
        .into_iter()
        .filter(|actor| {
            world.0.object_field_text(actor.id, "team") != "allies"
                && world.0.object_field_text(actor.id, "script_noteworthy")
                    != "sloth_zombie_spawner"
        })
        .collect();
    let laststand = world.0.script_int(ScriptOwner::Player(id), "laststand");
    let eye = [
        ps.origin[0],
        ps.origin[1],
        ps.origin[2] + ps.view_height_current,
    ];
    if let Some(closest) = actors
        .iter()
        .map(|actor| {
            distance(
                eye,
                [
                    actor.origin[0],
                    actor.origin[1],
                    actor.origin[2] + AIM_HEIGHT,
                ],
            )
        })
        .min_by(f32::total_cmp)
    {
        test.nearest = Some(test.nearest.map_or(closest, |seen| seen.min(closest)));
    }
    if let Some(was) = test.last_health.replace(ps.health)
        && ps.health < was
    {
        diag::info!(
            Sim,
            "test player: hit t={}ms health {}->{} (max {})",
            clock.time_ms,
            was,
            ps.health,
            ps.max_health
        );
    }
    test.lowest_health = Some(
        test.lowest_health
            .map_or(ps.health, |low| low.min(ps.health)),
    );
    log_move_status(&mut test, &world.0, id, clock.time_ms, ps.velocity);
    if std::env::var("IW4L_TEST_TIMING").is_ok() {
        let state = test
            .last_move
            .map_or(ps.weaponstate_primary, |status| status.weapon_state);
        log_weapon_timing(&mut test, &world.0, (ps.weapon, state), clock.time_ms);
    }
    if clock.time_ms >= test.next_log_ms {
        test.next_log_ms = clock.time_ms + LOG_PERIOD_MS;
        let weapon = world.0.weapon_script_name(ps.weapon).to_owned();
        diag::info!(
            Sim,
            "test player: t={}s at={} round={} score={} kills={} perks={} health={} lowest={} life={:?} laststand={} actors={} nearest={} shots={} knifes={} weapon={weapon} clip={} stock={} state={} lines(clear/model/world)={:?}",
            clock.time_ms / 1000,
            fmt_point(ps.origin),
            world
                .0
                .script_int(ScriptOwner::Level, "round_number")
                .map_or_else(|| "-".to_owned(), |round| round.to_string()),
            meta.score,
            meta.kills,
            world
                .0
                .script_int(ScriptOwner::Player(id), "num_perks")
                .map_or_else(|| "-".to_owned(), |n| n.to_string()),
            ps.health,
            test.lowest_health.unwrap_or(ps.health),
            meta.lifecycle,
            laststand.map_or_else(|| "-".to_owned(), |n| n.to_string()),
            actors.len(),
            test.nearest
                .map_or_else(|| "-".to_owned(), |d| format!("{d:.0}")),
            test.shots,
            test.knifes,
            meta.ammo_clip,
            meta.ammo_stock,
            ps.weaponstate_primary,
            test.lines
        );
        let others: Vec<String> = snapshot
            .players
            .iter()
            .filter(|(other, _)| *other != id)
            .map(|(other, other_ps)| {
                format!(
                    "client {} {:?} health {} laststand {} sessionstate {} spectator_respawn {}",
                    other.0,
                    snapshot.meta.for_client(*other).map(|m| m.lifecycle),
                    other_ps.health,
                    world
                        .0
                        .script_int(ScriptOwner::Player(*other), "laststand")
                        .map_or_else(|| "-".to_owned(), |n| n.to_string()),
                    world.0.player_field_text(*other, "sessionstate"),
                    world.0.player_field_text(*other, "spectator_respawn")
                )
            })
            .collect();
        if !others.is_empty() {
            diag::info!(Sim, "test player: other players {others:?}");
        }
        if let Ok(fields) = std::env::var("IW4L_TEST_ACTORS") {
            for actor in &actors {
                let values: Vec<String> = fields
                    .split(',')
                    .map(|name| {
                        format!(
                            "{name}={}",
                            world.0.object_field_text(actor.id, name.trim())
                        )
                    })
                    .collect();
                diag::info!(
                    Sim,
                    "test player: actor {} at {} health {} {}",
                    actor.id,
                    fmt_point(actor.origin),
                    actor.health,
                    values.join(" ")
                );
            }
        }
        if let Ok(names) = std::env::var("IW4L_TEST_WAITS") {
            for name in names.split(',') {
                diag::info!(
                    Sim,
                    "test player: waits on {name}: {:?}",
                    world.0.waits_on(name.trim())
                );
            }
        }
        if let Ok(keys) = std::env::var("IW4L_TEST_VARS") {
            let vars: Vec<String> = keys
                .split(',')
                .map(|key| {
                    let value = match key.trim().strip_prefix("level:") {
                        Some(field) => Some(world.0.object_field_text(0, field)),
                        None => world.0.level_array_entry("zombie_vars", key.trim()),
                    };
                    format!("{key}={}", value.unwrap_or_else(|| "-".to_owned()))
                })
                .collect();
            diag::info!(Sim, "test player: zombie_vars {}", vars.join(" "));
        }
        test.nearest = None;
        test.lowest_health = None;
    }
    if test.mode == Mode::Watch {
        return;
    }
    if matches!(test.mode, Mode::Down | Mode::Revive) {
        let now = (meta.lifecycle, ps.health, laststand);
        if test
            .seen
            .is_none_or(|seen| (seen.0, seen.2) != (now.0, now.2))
        {
            diag::info!(
                Sim,
                "test player: down test t={}s life={:?} health={} laststand={} last_stand_vision={:?} blur={:?}",
                clock.time_ms / 1000,
                now.0,
                now.1,
                now.2.map_or_else(|| "-".to_owned(), |n| n.to_string()),
                snapshot
                    .meta
                    .objectives
                    .last_stand_vision
                    .as_ref()
                    .map(|v| (&v.name, v.duration_ms)),
                meta.view_effects
                    .blur
                    .map(|b| (b.from, b.to, b.duration_ms))
            );
        }
        test.seen = Some(now);
        if test.mode == Mode::Down || test.pending.is_empty() && test.scenario.is_none() {
            return;
        }
    }
    if meta.lifecycle != ClientLifecycle::Alive {
        return;
    }
    let since_ms = *test.alive_since_ms.get_or_insert(clock.time_ms);
    // `IW4L_TEST_GOTO=<zone>[,<zone>…]`: goes to the path node nearest each
    // zone's middle that stands in the zone, so the zone and those beside it
    // come alive.
    if test.scenario.is_none()
        && clock.time_ms - since_ms >= USE_START_MS / 2 + (test.went as i32) * 15_000
        && let Ok(zones) = std::env::var("IW4L_TEST_GOTO")
        && let Some(zone) = zones.split(',').nth(test.went).map(str::trim)
    {
        test.went += 1;
        let inside: Vec<[f32; 3]> = world
            .0
            .path_node_origins()
            .into_iter()
            .filter(|node| {
                world
                    .0
                    .in_zone_player_volume([node[0], node[1], node[2] + 1.0], zone)
            })
            .collect();
        let middle = inside.iter().fold([0.0f32; 3], |sum, node| {
            std::array::from_fn(|i| sum[i] + node[i] / inside.len() as f32)
        });
        match inside
            .iter()
            .min_by(|a, b| distance(**a, middle).total_cmp(&distance(**b, middle)))
        {
            Some(node) => {
                let stand = [node[0], node[1], node[2] + 1.0];
                diag::info!(Sim, "test player: going to {zone} at {}", fmt_point(stand));
                if let Err(error) = actions.push(
                    id,
                    ClientAction::Move {
                        request_id: request_ids.allocate(),
                        origin: stand,
                        angles: ps.viewangles,
                    },
                ) {
                    diag::warn!(Sim, "test player: request not queued — {error}");
                }
            }
            None => diag::info!(Sim, "test player: no path node stands in {zone}"),
        }
    }
    // `IW4L_TEST_GRAB=1`: steps onto a power-up lying on the ground, then
    // back to where he stood.
    if let Some((back_ms, back)) = test.grab_return
        && clock.time_ms >= back_ms
    {
        test.grab_return = None;
        if let Err(error) = actions.push(
            id,
            ClientAction::Move {
                request_id: request_ids.allocate(),
                origin: back,
                angles: ps.viewangles,
            },
        ) {
            diag::warn!(Sim, "test player: request not queued — {error}");
        }
    } else if test.mode.cheats()
        && test.grab_return.is_none()
        && clock.time_ms >= test.next_grab_ms
        && std::env::var_os("IW4L_TEST_GRAB").is_some()
        && let Some((name, at)) = world.0.active_powerups().into_iter().next()
    {
        test.next_grab_ms = clock.time_ms + GRAB_PERIOD_MS;
        test.grab_return = Some((clock.time_ms + GRAB_PERIOD_MS / 2, ps.origin));
        diag::info!(Sim, "test player: grabbing {name} at {}", fmt_point(at));
        if let Err(error) = actions.push(
            id,
            ClientAction::Move {
                request_id: request_ids.allocate(),
                origin: at,
                angles: ps.viewangles,
            },
        ) {
            diag::warn!(Sim, "test player: request not queued — {error}");
        }
    }
    let mut ask = |action: ClientAction| {
        if let Err(error) = actions.push(id, action) {
            diag::warn!(Sim, "test player: request not queued — {error}");
        }
    };
    if test.mode.cheats() {
        if !test.god_asked {
            test.god_asked = true;
            ask(ClientAction::ToggleGod {
                request_id: request_ids.allocate(),
            });
        }
        if meta.ammo_clip == 0 && meta.ammo_stock == 0 && clock.time_ms >= test.next_resupply_ms {
            test.next_resupply_ms = clock.time_ms + RESUPPLY_PERIOD_MS;
            ask(ClientAction::ResupplyAmmo {
                request_id: request_ids.allocate(),
            });
        }
    }

    let mut cmd = UserCmd {
        server_time: clock.time_ms,
        weapon: ps.weapon as u16,
        weapon_mapped: ps.weapon as u16,
        ..UserCmd::default()
    };
    // `IW4L_TEST_WIELD=<weapon>,…`: once he holds one, his command asks for
    // it, as a client switching to it does.
    let mut wielding = false;
    if let Ok(names) = std::env::var("IW4L_TEST_WIELD")
        && let Some(wanted) = names
            .split(',')
            .filter_map(|name| world.0.weapon_index_by_script_name(name))
            .find(|wanted| ps.weapons.iter().any(|&w| w as u32 == *wanted))
    {
        cmd.weapon = wanted as u16;
        cmd.weapon_mapped = wanted as u16;
        wielding = true;
    } else if ps.weapon == 0 && ps.weapons.contains(&i32::from(test.selected)) {
        // With nothing up (an offhand throw puts it away), he keeps asking
        // for what he held, as a client's weapon select does.
        cmd.weapon = test.selected;
        cmd.weapon_mapped = test.selected;
    } else if ps.weapon == 0
        && let Some(&first) = ps.weapons.iter().find(|&&w| w > 0)
    {
        // With nothing up (the scripts took the weapon he held), he asks
        // for his first, as a client's cgame changes weapon.
        cmd.weapon = first as u16;
        cmd.weapon_mapped = first as u16;
    }
    if cmd.weapon != 0 {
        test.selected = cmd.weapon;
    }
    let aim_cmd = |cmd: &mut UserCmd, view: [f32; 3]| {
        cmd.angles = look_angles_from_degrees(std::array::from_fn(|axis| {
            view[axis] - ps.delta_angles[axis]
        }));
    };
    // `IW4L_TEST_PLACE`: the wielded weapon up for a while, he looks level
    // and holds fire to set it down.
    if std::env::var("IW4L_TEST_PLACE").is_ok()
        && test.scenario.is_none()
        && wielding
        && u32::from(cmd.weapon) == ps.weapon
    {
        let up_since = *test.wielded_since_ms.get_or_insert(clock.time_ms);
        if clock.time_ms - up_since < PLACE_AFTER_MS {
            cmds.push(id, cmd);
            return;
        }
        {
            if test.placing.is_none() {
                test.placements += 1;
                let facing = ps.viewangles[1] + 90.0 * (test.placements - 1) as f32;
                diag::info!(
                    Sim,
                    "test player: place {} {} at {} facing {facing:.0}",
                    test.placements,
                    world.0.weapon_script_name(ps.weapon),
                    fmt_point(ps.origin)
                );
                test.placing = Some((clock.time_ms + PLACE_HOLD_MS, facing));
            }
            let Some((until, facing)) = test.placing else {
                return;
            };
            aim_cmd(&mut cmd, [0.0, facing, 0.0]);
            if clock.time_ms < until {
                cmd.buttons |= buttons::ATTACK;
            }
            cmds.push(id, cmd);
            return;
        }
    } else {
        test.wielded_since_ms = None;
        test.placing = None;
    }

    if std::env::var("IW4L_TEST_WALK").is_ok()
        && test.scenario.is_none()
        && clock.time_ms - since_ms >= WALK_START_MS
    {
        if test.walk.is_none() && clock.time_ms >= test.next_walk_ms {
            test.next_walk_ms = clock.time_ms + WALK_PERIOD_MS;
            let target = world
                .0
                .path_node_origins()
                .into_iter()
                .filter(|node| WALK_NODE_RANGE.contains(&flat_distance(ps.origin, *node)))
                .filter(|node| {
                    [20.0, WAIST_HEIGHT].iter().all(|&up| {
                        let from = [ps.origin[0], ps.origin[1], ps.origin[2] + up];
                        let to = [node[0], node[1], node[2] + up];
                        clear_run(&world.0, id, from, to) >= distance(from, to) - 1.0
                    })
                })
                .min_by(|a, b| {
                    (a[2] - ps.origin[2])
                        .abs()
                        .total_cmp(&(b[2] - ps.origin[2]).abs())
                });
            match target {
                Some(target) => {
                    diag::info!(
                        Sim,
                        "test player: walk from {} toward {}",
                        fmt_point(ps.origin),
                        fmt_point(target)
                    );
                    test.walk = Some(Walk {
                        target,
                        start_ms: clock.time_ms,
                        phase_from: [ps.origin; 5],
                        phase: 0,
                        top_z: ps.origin[2],
                        low_view: [ps.view_height_current; 2],
                        sprint_ms: 0,
                        sprint_end: None,
                    });
                }
                None => diag::info!(
                    Sim,
                    "test player: walk — no path node in reach of {} ({} nodes)",
                    fmt_point(ps.origin),
                    world.0.path_node_origins().len()
                ),
            }
        }
        if let Some(walk) = test.walk.as_mut() {
            let phases = if std::env::var("IW4L_TEST_WALK").as_deref() == Ok("sprint") {
                &WALK_SPRINT_PHASES_MS
            } else {
                &WALK_PHASES_MS
            };
            let t = clock.time_ms - walk.start_ms;
            let phase = phases
                .iter()
                .position(|&end| t < end)
                .unwrap_or(phases.len());
            while walk.phase < phase {
                walk.phase += 1;
                if walk.phase < walk.phase_from.len() {
                    walk.phase_from[walk.phase] = ps.origin;
                }
            }
            match phase {
                0 => walk.top_z = walk.top_z.max(ps.origin[2]),
                2 if world.0.move_status(id).is_some_and(|s| s.sprinting) => {
                    walk.sprint_ms += 50;
                    walk.sprint_end = Some(ps.origin);
                }
                3 | 4 => {
                    let low = &mut walk.low_view[phase - 3];
                    *low = low.min(ps.view_height_current);
                }
                _ => {}
            }
            if phase < phases.len() {
                if phase < 3 {
                    let eye = [ps.origin[0], ps.origin[1], ps.origin[2] + EYE_HEIGHT];
                    let ahead = [walk.target[0], walk.target[1], eye[2]];
                    aim_cmd(&mut cmd, look(eye, ahead));
                    cmd.forwardmove = 127;
                }
                cmd.buttons |= match phase {
                    0 => buttons::JUMP,
                    2 => buttons::SPRINT,
                    3 => buttons::CROUCH,
                    4 => buttons::PRONE,
                    _ => 0,
                };
                cmds.push(id, cmd);
                return;
            }
            let walked = |phase: usize, ms: i32| {
                let from = walk.phase_from[phase];
                let to = walk.phase_from.get(phase + 1).copied().unwrap_or(ps.origin);
                let d = flat_distance(from, to);
                format!("{d:.0} ({:.0}/s)", d * 1000.0 / ms as f32)
            };
            let sprint_window = phases[2] - phases[1];
            let sprinted = if walk.sprint_ms > 0 && walk.sprint_ms < sprint_window {
                let d = flat_distance(
                    walk.phase_from[2],
                    walk.sprint_end.unwrap_or(walk.phase_from[3]),
                );
                format!(
                    "{d:.0} ({:.0}/s over {}ms)",
                    d * 1000.0 / walk.sprint_ms as f32,
                    walk.sprint_ms
                )
            } else {
                format!(
                    "{} ({}ms)",
                    walked(2, sprint_window),
                    walk.sprint_ms.max(sprint_window)
                )
            };
            diag::info!(
                Sim,
                "test player: walked — jump {:.1} high, walk {}, sprint {sprinted}, crouch view {:.1}, prone view {:.1}, now at {}",
                walk.top_z - walk.phase_from[0][2],
                walked(1, phases[1] - phases[0]),
                walk.low_view[0],
                walk.low_view[1],
                fmt_point(ps.origin)
            );
            test.walk = None;
        }
    }

    if test.scenario.is_none()
        && test.pending.first() == Some(&UseKind::WallBuy)
        && let Some((ref wall_weapon, buy_clip, buy_stock)) = test.wall_stock
        && world.0.weapon_script_name(ps.weapon) == wall_weapon
        && meta.ammo_stock >= buy_stock
    {
        if meta.ammo_clip >= buy_clip {
            cmd.buttons |= buttons::ATTACK;
        } else {
            cmd.buttons |= buttons::RELOAD;
        }
        cmds.push(id, cmd);
        return;
    }

    if matches!(test.mode, Mode::Use | Mode::Revive)
        && test.scenario.is_none()
        && clock.time_ms - since_ms >= USE_START_MS
        && clock.time_ms >= test.next_try_ms
        && {
            test.next_try_ms = clock.time_ms + TRY_PERIOD_MS;
            true
        }
        && let Some(mut scenario) = next_scenario(
            &mut test,
            &world.0,
            id,
            (ps.origin, ps.viewangles, ps.weapon),
            meta.score,
            clock.time_ms,
            since_ms,
        )
    {
        scenario.clip = meta.ammo_clip;
        scenario.stock = meta.ammo_stock;
        diag::info!(
            Sim,
            "test player: use {:?}{} cost={} at {} — standing at {}",
            scenario.site.kind,
            scenario
                .site
                .weapon
                .as_deref()
                .map_or_else(String::new, |weapon| format!(" {weapon}")),
            scenario
                .site
                .cost
                .map_or_else(|| "-".to_owned(), |cost| cost.to_string()),
            fmt_point(scenario.center),
            fmt_point(scenario.stand)
        );
        let at_eye = [
            scenario.stand[0],
            scenario.stand[1],
            scenario.stand[2] + EYE_HEIGHT,
        ];
        ask(ClientAction::Move {
            request_id: request_ids.allocate(),
            origin: scenario.stand,
            angles: look(at_eye, scenario.center),
        });
        test.scenario = Some(scenario);
    }
    if let Some(mut scenario) = test.scenario.take() {
        aim_cmd(&mut cmd, look(eye, scenario.center));
        if clock.time_ms >= scenario.until_ms {
            match scenario.phase {
                Phase::Arriving => {
                    scenario.phase = Phase::Holding;
                    scenario.until_ms = clock.time_ms + hold_ms(scenario.site.kind);
                }
                Phase::Holding => {
                    scenario.presses += 1;
                    scenario.offered = world.0.use_triggers_near(scenario.center, OFFER_REACH);
                    let opening = matches!(scenario.site.kind, UseKind::Box | UseKind::PackAPunch)
                        && scenario.presses == 1;
                    scenario.phase = Phase::Watching;
                    scenario.power_by_ms = clock.time_ms + POWER_EVENT_MS;
                    scenario.until_ms =
                        clock.time_ms + if opening { BOX_SPIN_MS } else { WATCH_MS };
                }
                Phase::Watching
                    if scenario.site.kind == UseKind::Power
                        && !world.0.level_flag("power_on")
                        && clock.time_ms < scenario.power_by_ms =>
                {
                    scenario.until_ms = clock.time_ms + WATCH_MS;
                }
                Phase::Watching
                    if scenario.site.kind == UseKind::Box
                        && world.0.level_flag("moving_chest_now")
                        && clock.time_ms < scenario.power_by_ms =>
                {
                    scenario.until_ms = clock.time_ms + WATCH_MS;
                }
                Phase::Watching
                    if scenario.site.kind == UseKind::Perk
                        && (world
                            .0
                            .weapon_script_name(ps.weapon)
                            .starts_with("zombie_perk_bottle_")
                            || test.last_move.is_some_and(|s| s.no_weapon_cycling))
                        && clock.time_ms < scenario.power_by_ms =>
                {
                    scenario.until_ms = clock.time_ms + 500;
                }
                // The box took its price: its weapon has risen, take it.
                Phase::Watching
                    if matches!(scenario.site.kind, UseKind::Box | UseKind::PackAPunch)
                        && scenario.presses == 1
                        && meta.score < scenario.score =>
                {
                    diag::info!(
                        Sim,
                        "test player: {:?} took its price — score {}→{}; taking its weapon; its unitrigger stub — {}",
                        scenario.site.kind,
                        scenario.score,
                        meta.score,
                        world.0.unitrigger_state(scenario.site.object)
                    );
                    scenario.phase = Phase::Holding;
                    scenario.until_ms = clock.time_ms + hold_ms(scenario.site.kind);
                }
                Phase::Watching => {
                    let all = world.0.use_triggers_near(scenario.center, f32::MAX);
                    let boxes = all.iter().filter(|c| c.starts_with("trigger_box")).count();
                    let radii = all
                        .iter()
                        .filter(|c| c.starts_with("trigger_radius"))
                        .count();
                    diag::info!(
                        Sim,
                        "test player: use triggers on the map now: {} ({boxes} trigger_box[_use], {radii} trigger_radius[_use])",
                        all.len()
                    );
                    diag::info!(
                        Sim,
                        "test player: used {:?} — stood {:.0} from it, score {}→{}, weapon {}→{} (clip {}→{}, stock {}→{}, holds {:?}), trigger {}, use triggers there while holding {:?}",
                        scenario.site.kind,
                        distance(eye, scenario.center),
                        scenario.score,
                        meta.score,
                        scenario.weapon,
                        world.0.weapon_script_name(ps.weapon),
                        scenario.clip,
                        meta.ammo_clip,
                        scenario.stock,
                        meta.ammo_stock,
                        ps.weapons
                            .iter()
                            .filter(|&&w| w > 0)
                            .map(|&w| world.0.weapon_script_name(w as u32).to_owned())
                            .collect::<Vec<_>>(),
                        if world.0.script_object_lives(scenario.site.object) {
                            "still there"
                        } else {
                            "gone"
                        },
                        scenario.offered
                    );
                    if scenario.site.kind == UseKind::WallBuy {
                        test.wall_stock = Some((
                            world.0.weapon_script_name(ps.weapon).to_owned(),
                            meta.ammo_clip,
                            meta.ammo_stock,
                        ));
                    }
                    if scenario.site.kind == UseKind::Rebuild {
                        diag::info!(
                            Sim,
                            "test player: rebuild reward this round (rebuild_barrier_reward) {}",
                            world
                                .0
                                .script_int(ScriptOwner::Player(id), "rebuild_barrier_reward")
                                .map_or_else(|| "-".to_owned(), |n| n.to_string())
                        );
                    }
                    if matches!(
                        scenario.site.kind,
                        UseKind::Piece | UseKind::Build | UseKind::Take
                    ) {
                        diag::info!(
                            Sim,
                            "test player: {:?} {} — its stub {}; the buildable's table {}",
                            scenario.site.kind,
                            scenario.site.weapon.as_deref().unwrap_or("-"),
                            world.0.unitrigger_state(scenario.site.object),
                            world
                                .0
                                .use_sites()
                                .iter()
                                .find(|site| site.kind == UseKind::Build
                                    && site.weapon == scenario.site.weapon)
                                .map_or("built", |_| "not built")
                        );
                    }
                    if scenario.site.kind == UseKind::Power {
                        diag::info!(
                            Sim,
                            "test player: power_on {}",
                            world.0.level_flag("power_on")
                        );
                    }
                    if scenario.site.kind == UseKind::Perk {
                        diag::info!(
                            Sim,
                            "test player: perks drunk (num_perks) {} — max_health={} perks={:?}",
                            world
                                .0
                                .script_int(ScriptOwner::Player(id), "num_perks")
                                .map_or_else(|| "-".to_owned(), |n| n.to_string()),
                            ps.max_health,
                            world.0.player_perks(id)
                        );
                    }
                    if matches!(scenario.site.kind, UseKind::WallBuy | UseKind::Box) {
                        diag::info!(
                            Sim,
                            "test player: its unitrigger stub — {}",
                            world.0.unitrigger_state(scenario.site.object)
                        );
                    }
                    if scenario.site.kind == UseKind::Box {
                        let active_boxes: Vec<String> = world
                            .0
                            .use_sites()
                            .into_iter()
                            .filter(|s| s.kind == UseKind::Box)
                            .map(|s| format!("{}@{}", s.cost.unwrap_or(0), fmt_point(s.origin)))
                            .collect();
                        diag::info!(
                            Sim,
                            "test player: box state — chest_index={} chest_moves={} chest_accessed={} active={active_boxes:?}",
                            world.0.object_field_text(0, "chest_index"),
                            world.0.object_field_text(0, "chest_moves"),
                            world.0.object_field_text(0, "chest_accessed"),
                        );
                    }
                    ask(ClientAction::Move {
                        request_id: request_ids.allocate(),
                        origin: scenario.home,
                        angles: scenario.home_angles,
                    });
                    scenario.phase = Phase::Returning;
                    scenario.until_ms = clock.time_ms + ARRIVE_MS;
                }
                Phase::Returning => {
                    test.last_done_ms = clock.time_ms;
                    cmds.push(id, cmd);
                    return;
                }
            }
        }
        if scenario.phase == Phase::Holding {
            cmd.buttons |= buttons::USE;
        }
        test.scenario = Some(scenario);
        cmds.push(id, cmd);
        return;
    }
    // `IW4L_TEST_REVIVE`: a teammate down, he goes beside him, faces him
    // and holds use (Black Ops 2's revive trigger is around the downed
    // player).
    if std::env::var("IW4L_TEST_REVIVE").is_ok() {
        let downed = snapshot.players.iter().find(|(other, _)| {
            *other != id
                && world.0.script_int(ScriptOwner::Player(*other), "laststand") == Some(1)
                && snapshot
                    .meta
                    .for_client(*other)
                    .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
        });
        if let Some((other, other_ps)) = downed {
            let target = other_ps.origin;
            if test.reviving != Some(other.0) {
                test.reviving = Some(other.0);
                test.revive_until_ms = clock.time_ms + REVIVE_HOLD_MS;
                let away = [ps.origin[0] - target[0], ps.origin[1] - target[1]];
                let length = (away[0] * away[0] + away[1] * away[1]).sqrt().max(1.0);
                let stand = [
                    target[0] + away[0] / length * REVIVE_STAND_OFF,
                    target[1] + away[1] / length * REVIVE_STAND_OFF,
                    target[2] + 1.0,
                ];
                diag::info!(
                    Sim,
                    "test player: reviving client {} down at {}",
                    other.0,
                    fmt_point(target)
                );
                if let Err(error) = actions.push(
                    id,
                    ClientAction::Move {
                        request_id: request_ids.allocate(),
                        origin: stand,
                        angles: look([stand[0], stand[1], stand[2] + EYE_HEIGHT], target),
                    },
                ) {
                    diag::warn!(Sim, "test player: request not queued — {error}");
                }
            }
            if clock.time_ms < test.revive_until_ms {
                aim_cmd(&mut cmd, look(eye, target));
                cmd.buttons |= buttons::USE;
                cmds.push(id, cmd);
                return;
            }
        } else if let Some(client) = test.reviving.take() {
            diag::info!(Sim, "test player: client {client} is no longer down");
        }
    }
    // `IW4L_TEST_RIDE`: once, it steps onto the bus (`level.the_bus`, its
    // floor 36 above its origin), then logs where it and the bus are.
    if std::env::var("IW4L_TEST_RIDE").is_ok()
        && let Some((bus, yaw, number)) = world.0.level_entity_pose("the_bus")
    {
        let after = std::env::var("IW4L_TEST_RIDE")
            .ok()
            .and_then(|s| s.parse::<i32>().ok())
            .filter(|&s| s > 1)
            .map_or(RIDE_AFTER_MS, |s| s * 1000);
        if !test.boarded && clock.time_ms - since_ms >= after {
            test.boarded = true;
            let stand = [bus[0], bus[1], bus[2] + BUS_FLOOR + 1.0];
            diag::info!(Sim, "test player: boarding the bus at {}", fmt_point(stand));
            if let Err(error) = actions.push(
                id,
                ClientAction::Move {
                    request_id: request_ids.allocate(),
                    origin: stand,
                    angles: [0.0, yaw, 0.0],
                },
            ) {
                diag::warn!(Sim, "test player: request not queued — {error}");
            }
        }
        if test.boarded && clock.time_ms >= test.next_ride_log_ms {
            test.next_ride_log_ms = clock.time_ms + LOG_PERIOD_MS;
            diag::info!(
                Sim,
                "test player: ride — player {} bus {} (entity {number}) stands on {:?}",
                fmt_point(ps.origin),
                fmt_point(bus),
                world.0.player_ground_entity(id)
            );
        }
        if test.boarded {
            cmds.push(id, cmd);
            return;
        }
    }
    // The revive test fights nothing.
    if test.mode == Mode::Revive {
        return;
    }

    let mut near: Vec<_> = actors
        .iter()
        .map(|actor| (distance(eye, actor.origin), *actor))
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    let target = near.iter().take(SIGHT_CANDIDATES).find_map(|&(_, actor)| {
        let chest = [
            actor.origin[0],
            actor.origin[1],
            actor.origin[2] + AIM_HEIGHT,
        ];
        let aim = actor
            .body
            .and_then(|body| world.0.body_point(body, chest))
            .unwrap_or(chest);
        let outcome = world.0.sensor_trace(BulletTraceQuery {
            start: eye,
            end: aim,
            mask: world.0.bullet_mask(),
            ignore: Some(id),
            ignore_hit: None,
            ignore_model: None,
        });
        let seen = match outcome {
            TraceOutcome::Miss { .. } => true,
            TraceOutcome::Hit { end, .. } => distance(end, aim) <= HIT_SLACK,
            _ => false,
        };
        seen.then_some((aim, outcome, actor.id))
    });

    // Nothing in sight for a while with actors left (an inert zombie lying
    // outside, one that lost its way): it goes to stand by the nearest, as a
    // player would, inside an enabled zone.
    if target.is_some() {
        test.last_seen_ms = Some(clock.time_ms);
    } else if let Some((_, nearest)) = near.first()
        && clock.time_ms - *test.last_seen_ms.get_or_insert(clock.time_ms) >= HUNT_AFTER_MS
    {
        test.last_seen_ms = Some(clock.time_ms);
        let away = [
            ps.origin[0] - nearest.origin[0],
            ps.origin[1] - nearest.origin[1],
        ];
        let length = (away[0] * away[0] + away[1] * away[1]).sqrt().max(1.0);
        let stand = [
            nearest.origin[0] + away[0] / length * HUNT_STAND_OFF,
            nearest.origin[1] + away[1] / length * HUNT_STAND_OFF,
            nearest.origin[2] + 1.0,
        ];
        if world.0.in_enabled_player_volume(stand) {
            diag::info!(
                Sim,
                "test player: nothing in sight for {} s; going to the actor at {}",
                HUNT_AFTER_MS / 1000,
                fmt_point(nearest.origin)
            );
            if let Err(error) = actions.push(
                id,
                ClientAction::Move {
                    request_id: request_ids.allocate(),
                    origin: stand,
                    angles: look(stand, nearest.origin),
                },
            ) {
                diag::warn!(Sim, "test player: request not queued — {error}");
            }
        }
    }
    if let Some((_, outcome, _)) = target {
        let line = match outcome {
            TraceOutcome::Hit {
                collider: ColliderId::EntityDObjBone { .. },
                ..
            } => 1,
            TraceOutcome::Hit { .. } => 2,
            _ => 0,
        };
        test.lines[line] += 1;
    }
    let can_melee = target
        .is_some_and(|(_, _, actor_id)| world.0.object_field_text(actor_id, "cant_melee") != "1");
    let target = target.map(|(aim, _, _)| aim);
    aim_cmd(&mut cmd, target.map_or(ps.viewangles, |aim| look(eye, aim)));
    if let Ok(throw_arg) = std::env::var("IW4L_TEST_THROW") {
        let named = (throw_arg != "1").then_some(throw_arg.as_str());
        let button = if named.is_some() {
            buttons::SMOKE
        } else {
            buttons::FRAG
        };
        if clock.time_ms < test.throw_until_ms {
            cmd.buttons |= button;
            cmds.push(id, cmd);
            return;
        }
        let weapon_name = named.unwrap_or("frag_grenade_zm");
        let weap_idx = world.0.weapon_index_by_script_name(weapon_name);
        let held = weap_idx.is_some_and(|w| ps.weapons.contains(&(w as i32)));
        let ready = ps.weapon != 0
            && ps.weaponstate_primary == 0
            && if named.is_some() {
                held && target.is_some_and(|aim| distance(eye, aim) > KNIFE_REACH)
            } else {
                target.is_some_and(|aim| distance(eye, aim) > THROW_FROM)
            };
        if clock.time_ms >= test.next_throw_ms && ready {
            test.next_throw_ms = clock.time_ms + THROW_PERIOD_MS;
            test.throw_until_ms = clock.time_ms + THROW_HOLD_MS;
            test.throws += 1;
            diag::info!(
                Sim,
                "test player: throw {} at {} ({weapon_name} {:?} held={held} offhand={}/{} state={})",
                test.throws,
                fmt_point(target.unwrap_or(eye)),
                weap_idx,
                ps.offhand_primary,
                ps.offhand_secondary,
                ps.weaponstate_primary
            );
            cmd.buttons |= button;
            cmds.push(id, cmd);
            return;
        }
        if named.is_some() {
            cmds.push(id, cmd);
            return;
        }
    }
    test.pressed = !test.pressed;
    let hold_fire = wielding && std::env::var("IW4L_TEST_PLACE").is_err();
    if test.pressed || hold_fire {
        // It knifes only when it cannot fire: a gun with rounds fires, and
        // an empty clip with stock left reloads, even inside knife reach
        // (knifing alone stalls a late round).
        if !hold_fire && meta.ammo_clip == 0 && meta.ammo_stock > 0 {
            cmd.buttons |= buttons::RELOAD;
        } else if !hold_fire
            && meta.ammo_clip == 0
            && can_melee
            && target.is_some_and(|aim| distance(eye, aim) <= KNIFE_REACH)
        {
            cmd.buttons |= buttons::MELEE_CHARGE;
            test.knifes += 1;
        } else if !hold_fire && meta.ammo_clip == 0 {
            cmd.buttons |= buttons::RELOAD;
        } else if target.is_some() || hold_fire {
            cmd.buttons |= buttons::ATTACK;
            test.shots += 1;
        }
    }
    if near
        .iter()
        .filter(|&&(_, actor)| flat_distance(ps.origin, actor.origin) < BACK_OFF)
        .count()
        >= SWARM
    {
        cmd.forwardmove = -127;
    }
    cmds.push(id, cmd);
}

/// `IW4L_TEST_TIMING`: logs each change of his weapon's state with how long
/// the state before it lasted, so a run shows reload and fire times (and
/// what a perk makes of them).
fn log_weapon_timing(test: &mut TestPlayer, world: &sim::SimWorld, now: (u32, i32), time_ms: i32) {
    match test.last_weapon_state {
        Some((weapon, state, _)) if (weapon, state) == now => {}
        was => {
            if let Some((_, state, since)) = was {
                diag::info!(
                    Sim,
                    "test player: weapon {} state {}->{} t={}ms after {}ms",
                    world.weapon_script_name(now.0),
                    state,
                    now.1,
                    time_ms,
                    time_ms - since,
                );
            }
            test.last_weapon_state = Some((now.0, now.1, time_ms));
        }
    }
}

/// Logs each change of the movement flags the script builtins set or read
/// (ladder, mantle, no melee, no weapon cycling, being revived), and each
/// hit that winds the damage timer up, with the player's speed.
fn log_move_status(
    test: &mut TestPlayer,
    world: &sim::SimWorld,
    id: ClientId,
    time_ms: i32,
    velocity: [f32; 3],
) {
    let Some(now) = world.move_status(id) else {
        return;
    };
    let flags = |s: &sim::MoveStatus| {
        (
            s.on_ladder,
            s.mantling,
            s.no_melee,
            s.no_weapon_cycling,
            s.being_revived,
        )
    };
    let was = test.last_move.replace(now);
    let hit = was.is_none_or(|was| now.damage_timer > was.damage_timer);
    if was.is_none_or(|was| flags(&was) != flags(&now)) || (hit && now.damage_timer > 0) {
        diag::info!(
            Sim,
            "test player: movement t={}ms ladder={} mantling={} damage_timer={} no_melee={} no_weapon_cycling={} being_revived={} speed={:.0}",
            time_ms,
            now.on_ladder,
            now.mantling,
            now.damage_timer,
            now.no_melee,
            now.no_weapon_cycling,
            now.being_revived,
            (velocity[0] * velocity[0] + velocity[1] * velocity[1]).sqrt(),
        );
    }
}
