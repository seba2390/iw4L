use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use bevy_ecs::prelude::{Resource, World};

use super::args::{arg, float, int, string, vector};
use super::dynamic_paths;
use super::entities::EntityKind;
use crate::frame::FrameWorld;
use crate::script::runtime::raise;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};

/// Black Ops' actor clipmask (`MASK_ACTORSOLID`): solid world, monster clip
/// and body contents, without player clip.
const MASK_ACTOR_SOLID: u32 = 0x0282_0011;

/// A path node as actors use it.
#[derive(Clone, Debug)]
pub struct NavNode {
    pub kind: NavNodeKind,
    pub origin: [f32; 3],
    pub yaw: f32,
    pub targetname: String,
    pub target: String,
    pub animscript: String,
    /// Linked node, link length, and whether crossing it is a traversal.
    pub links: Vec<(u16, f32, bool)>,
    /// Every script field, when the game's decoder reads them all.
    pub fields: Option<NavNodeFields>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NavNodeFields {
    pub type_name: String,
    pub spawnflags: u32,
    pub script_linkname: String,
    pub script_noteworthy: String,
    pub radius: f32,
    pub min_use_dist_sq: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavNodeKind {
    Path,
    NegotiationBegin,
    NegotiationEnd,
    Other,
}

impl NavNodeKind {
    fn script_type(self) -> &'static str {
        match self {
            Self::Path => "Path",
            Self::NegotiationBegin => "Begin",
            Self::NegotiationEnd => "End",
            Self::Other => "Other",
        }
    }
}

/// The map's authored path network.
#[derive(Debug, Default)]
pub struct ActorPaths {
    nodes: Vec<NavNode>,
}

/// A badplace volume active in the level.
#[derive(Clone, Debug)]
struct BadPlace {
    name: Option<String>,
    expires_ms: Option<i64>,
    teams: u8,
    links: Vec<(u16, u16)>,
}

/// Links scripts made or broke since the map loaded, over its authored ones.
#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct LinkEdits {
    added: BTreeMap<u16, Vec<(u16, f32)>>,
    removed: BTreeSet<(u16, u16)>,
    /// The links each dynamic path entity cuts while it stands.
    cuts: BTreeMap<u64, Vec<(u16, u16)>>,
    /// How many entities cut each link.
    cut_by: BTreeMap<(u16, u16), u32>,
    /// Active badplaces and how many badplaces cover each link per team bit
    /// (`0..4`: free, axis, allies, neutral).
    badplaces: Vec<BadPlace>,
    badplace_by: BTreeMap<(u16, u16), [u32; 4]>,
    /// Whether the level's initial dynamic-path cuts ran.
    level_cut: bool,
}

impl LinkEdits {
    /// Links `from` to `to`, at the distance between them, unless linked.
    pub(crate) fn link(&mut self, paths: &ActorPaths, from: u16, to: u16) {
        self.removed.remove(&(from, to));
        if paths.links(from, self).any(|(next, _, _)| next == to) {
            return;
        }
        let length = distance_sq(
            paths.nodes[usize::from(from)].origin,
            paths.nodes[usize::from(to)].origin,
        )
        .sqrt();
        self.added.entry(from).or_default().push((to, length));
    }

    /// An entity cuts these links until it rejoins them.
    pub(crate) fn cut(&mut self, entity: u64, links: Vec<(u16, u16)>) {
        for link in &links {
            *self.cut_by.entry(*link).or_default() += 1;
        }
        self.cuts.entry(entity).or_default().extend(links);
    }

    /// How many links are cut.
    pub(crate) fn cut_count(&self) -> usize {
        self.cut_by.len()
    }

    /// Whether the level's dynamic paths made their first cuts.
    pub(crate) fn level_cut(&self) -> bool {
        self.level_cut
    }

    pub(crate) fn mark_level_cut(&mut self) {
        self.level_cut = true;
    }

    /// Rejoins the links an entity cut.
    pub(crate) fn rejoin(&mut self, entity: u64) {
        let Some(links) = self.cuts.remove(&entity) else {
            return;
        };
        for link in links {
            if let Some(count) = self.cut_by.get_mut(&link) {
                *count -= 1;
                if *count == 0 {
                    self.cut_by.remove(&link);
                }
            }
        }
    }

    /// Drops the link from `from` to `to`, if there is one.
    pub(crate) fn unlink(&mut self, from: u16, to: u16) {
        if let Some(links) = self.added.get_mut(&from) {
            links.retain(|&(next, _)| next != to);
        }
        self.removed.insert((from, to));
    }

    fn add_badplace(&mut self, place: BadPlace) {
        if let Some(name) = &place.name {
            self.remove_badplace_named(name);
        }
        for &link in &place.links {
            let counts = self.badplace_by.entry(link).or_default();
            for (bit, count) in counts.iter_mut().enumerate() {
                if place.teams & (1 << bit) != 0 {
                    *count += 1;
                }
            }
        }
        self.badplaces.push(place);
    }

    fn remove_badplace_named(&mut self, name: &str) -> Vec<(u16, u16)> {
        let mut removed_links = Vec::new();
        let mut i = 0;
        while i < self.badplaces.len() {
            if self.badplaces[i].name.as_deref() == Some(name) {
                let place = self.badplaces.remove(i);
                self.decrement_badplace(&place);
                removed_links.extend(place.links);
            } else {
                i += 1;
            }
        }
        removed_links
    }

    fn expire_badplaces(&mut self, now_ms: i64) {
        let mut i = 0;
        while i < self.badplaces.len() {
            if self.badplaces[i]
                .expires_ms
                .is_some_and(|expires| expires < now_ms)
            {
                let place = self.badplaces.remove(i);
                self.decrement_badplace(&place);
            } else {
                i += 1;
            }
        }
    }

    fn decrement_badplace(&mut self, place: &BadPlace) {
        for &link in &place.links {
            let Some(counts) = self.badplace_by.get_mut(&link) else {
                continue;
            };
            for (bit, count) in counts.iter_mut().enumerate() {
                if place.teams & (1 << bit) != 0 && *count > 0 {
                    *count -= 1;
                }
            }
            if counts.iter().all(|&c| c == 0) {
                self.badplace_by.remove(&link);
            }
        }
    }

    fn link_in_badplace(&self, link: (u16, u16), teams: u8) -> bool {
        teams != 0
            && self
                .badplace_by
                .get(&link)
                .is_some_and(|counts| (0..4).any(|bit| teams & (1 << bit) != 0 && counts[bit] > 0))
    }
}

/// The most nodes `getanynodearray` returns.
const ANY_NODES_MAX: usize = 1024;

/// How many of the closest nodes are traced for a straight walk.
const REACH_CANDIDATES: usize = 12;

/// Nodes farther above or below a position than this do not serve it.
const NODE_STEP_HEIGHT: f32 = 72.0;

impl ActorPaths {
    pub fn new(nodes: Vec<NavNode>) -> Self {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for node in &nodes {
            if node.kind == NavNodeKind::NegotiationBegin && !node.animscript.is_empty() {
                *counts.entry(node.animscript.clone()).or_default() += 1;
            }
        }
        if !counts.is_empty() {
            let summary: Vec<String> = counts
                .iter()
                .map(|(name, n)| format!("{name}={n}"))
                .collect();
            diag::info!(
                Sim,
                "actor paths: {} nodes, {} traversal scripts: {}",
                nodes.len(),
                counts.len(),
                summary.join(", ")
            );
        }
        Self { nodes }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Where the path nodes stand.
    pub fn origins(&self) -> impl Iterator<Item = [f32; 3]> + '_ {
        self.nodes.iter().map(|node| node.origin)
    }

    /// The closest node with nothing solid between it and `at`, or the
    /// closest node at all when none of the nearest few is in reach.
    fn nearest(
        &self,
        at: [f32; 3],
        reach: &mut dyn FnMut([f32; 3], [f32; 3]) -> bool,
    ) -> Option<u16> {
        let mut candidates: Vec<(f32, u16)> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| (node.origin[2] - at[2]).abs() <= NODE_STEP_HEIGHT)
            .map(|(index, node)| (distance_sq(node.origin, at), index as u16))
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        candidates
            .iter()
            .take(REACH_CANDIDATES)
            .find(|(_, node)| reach(at, self.nodes[*node as usize].origin))
            .or(candidates.first())
            .map(|(_, node)| *node)
    }

    pub(crate) fn nodes(&self) -> &[NavNode] {
        &self.nodes
    }

    /// A node's links as scripts left them: linked node, length, and
    /// whether crossing it is a traversal.
    pub(crate) fn links<'a>(
        &'a self,
        node: u16,
        edits: &'a LinkEdits,
    ) -> impl Iterator<Item = (u16, f32, bool)> + 'a {
        let authored = self
            .nodes
            .get(usize::from(node))
            .map_or(&[][..], |node| &node.links[..]);
        authored
            .iter()
            .copied()
            .chain(
                edits
                    .added
                    .get(&node)
                    .into_iter()
                    .flatten()
                    .map(|&(to, length)| (to, length, false)),
            )
            .filter(move |(to, _, _)| {
                !edits.removed.contains(&(node, *to)) && !edits.cut_by.contains_key(&(node, *to))
            })
    }

    /// The cheapest node sequence from `start` to `goal` over the links,
    /// by link length.
    pub(crate) fn route(&self, start: u16, goal: u16, edits: &LinkEdits) -> Option<Vec<u16>> {
        self.route_for_team(start, goal, edits, 0)
    }

    pub(crate) fn route_for_team(
        &self,
        start: u16,
        goal: u16,
        edits: &LinkEdits,
        badplace_teams: u8,
    ) -> Option<Vec<u16>> {
        let goal_origin = self.nodes.get(goal as usize)?.origin;
        let mut best: BTreeMap<u16, (f32, Option<u16>)> = BTreeMap::new();
        best.insert(start, (0.0, None));
        let mut open = BinaryHeap::new();
        open.push(Frontier {
            estimate: distance_sq(self.nodes.get(start as usize)?.origin, goal_origin).sqrt(),
            node: start,
        });
        while let Some(Frontier { node, .. }) = open.pop() {
            if node == goal {
                let mut path = vec![goal];
                let mut cursor = goal;
                while let Some(&(_, Some(previous))) = best.get(&cursor) {
                    path.push(previous);
                    cursor = previous;
                }
                path.reverse();
                return Some(path);
            }
            let cost = best[&node].0;
            for (next, length, _) in self.links(node, edits) {
                if edits.link_in_badplace((node, next), badplace_teams) {
                    continue;
                }
                let reached = cost + length.max(1.0);
                if best.get(&next).is_some_and(|(known, _)| *known <= reached) {
                    continue;
                }
                best.insert(next, (reached, Some(node)));
                open.push(Frontier {
                    estimate: reached
                        + distance_sq(self.nodes[next as usize].origin, goal_origin).sqrt(),
                    node: next,
                });
            }
        }
        None
    }

    /// The node sequence from the node serving `from` to the one serving `to`.
    fn plan(
        &self,
        from: [f32; 3],
        to: [f32; 3],
        edits: &LinkEdits,
        badplace_teams: u8,
        reach: &mut dyn FnMut([f32; 3], [f32; 3]) -> bool,
    ) -> Option<Vec<u16>> {
        let start = self.nearest(from, reach)?;
        let goal = self.nearest(to, reach)?;
        let mut path = self.route_for_team(start, goal, edits, badplace_teams)?;
        // Already past the first node: head for the second.
        if let [first, second, ..] = path[..] {
            let (a, b) = (
                self.nodes[first as usize].origin,
                self.nodes[second as usize].origin,
            );
            if distance_sq(from, b) < distance_sq(a, b)
                && !edits.link_in_badplace((first, second), badplace_teams)
                && reach(from, b)
            {
                path.remove(0);
            }
        }
        Some(path)
    }
}

#[derive(PartialEq)]
struct Frontier {
    estimate: f32,
    node: u16,
}

impl Eq for Frontier {}

impl Ord for Frontier {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .total_cmp(&self.estimate)
            .then(other.node.cmp(&self.node))
    }
}

impl PartialOrd for Frontier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// How an actor's animation moves it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AnimMode {
    /// The engine walks the actor along its path at the animation's speed.
    #[default]
    Normal,
    /// The animation's translation moves the actor; gravity keeps it on the
    /// ground.
    PosDeltas,
    /// The animation moves and turns the actor; gravity keeps it on the ground.
    Gravity,
    /// The animation moves the actor horizontally; the ground sets its height.
    ZOnlyPhysics,
    /// The animation moves the actor in 3D, swept against the world and
    /// entities without gravity (`ePhysicsType = 4`).
    NoGravity,
    /// The animation moves the actor freely without collision (`noclip` /
    /// `nophysics`).
    NoClip,
    /// Only the animation's turn applies.
    AngleDeltas,
}

/// What an actor faces.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum Orient {
    Motion,
    Enemy,
    /// The enemy while standing (or with no path), else the motion.
    EnemyOrMotion,
    /// Its goal node's way.
    Goal,
    Angle(f32),
    Point([f32; 3]),
    /// The way it already faces.
    #[default]
    Current,
}

#[derive(Clone, Debug)]
pub(crate) enum Goal {
    Position([f32; 3]),
    Entity(u64),
}

/// An actor's goal, path and movement modes.
#[derive(Clone, Debug)]
pub(crate) struct ActorMove {
    goal: Option<Goal>,
    /// Optional facing yaw at the goal (`setgoalpos(pos, angles)` or goal node).
    pub(crate) goal_yaw: Option<f32>,
    /// Optional goal volume entity (`setgoalvolume`).
    pub(crate) goal_volume: Option<u64>,
    /// Where the engine's own think sends the actor ahead of its goal: a
    /// hellhound runs to the path node nearest its enemy.
    pursuit: Option<[f32; 3]>,
    /// Where the path was planned to; an entity goal replans when it moves.
    planned_to: Option<[f32; 3]>,
    pub(crate) waypoints: Vec<[f32; 3]>,
    /// The path node each waypoint stands on; the goal point has none.
    pub(crate) waypoint_nodes: Vec<Option<u16>>,
    /// The traversal the actor is playing between two negotiation nodes.
    pub(crate) traversal: Option<Traversal>,
    pub(crate) anim_mode: AnimMode,
    /// The script's `animmode`, kept until the animscript changes.
    pub(crate) anim_mode_override: Option<AnimMode>,
    /// The script's `orientmode`, over the AI's own until the animscript
    /// changes ("face default").
    pub(crate) orient: Option<Orient>,
    /// What the AI's state faces.
    pub(crate) code_orient: Orient,
    /// Stances a script allows; none recorded means all.
    stances: Option<Vec<String>>,
    /// Direction of travel over the last tick, in degrees.
    heading: Option<f32>,
    /// Velocity over the last tick, in units per second.
    pub(crate) velocity: [f32; 3],
    /// A scripted animation placing the actor, once started.
    scripted: Option<Scripted>,
    /// When the actor last struck with `melee`: once a frame.
    melee_time: Option<i64>,
    /// Custom physics bounds from `setphysparams`; defaults to
    /// `([-15, -15, 0], [15, 15, 72])`.
    pub(crate) bounds: Option<([f32; 3], [f32; 3])>,
    /// Whether the actor pitches to the slope under it (`allowpitchangle` or
    /// dog).
    pub(crate) allow_pitch: bool,
    /// What the last movement step hit (`"none"`, `"world"`, or `"obstacle"`).
    pub(crate) hit_ent_type: &'static str,
    /// Yaw away from the hit normal (`gethityaw`).
    pub(crate) hit_yaw: f32,
    /// What the actor stands on (`"none"`, `"world"`, or `"obstacle"`).
    pub(crate) ground_ent_type: &'static str,
    /// Whether the last movement step deflected off an obstacle (`isdeflected`).
    pub(crate) deflected: bool,
    /// The plane the actor stands on; `None` while airborne.
    ground_normal: Option<[f32; 3]>,
    /// Vertical speed while airborne.
    fall_speed: f32,
    /// The side the last deflection turned to (8 or 16), kept while the
    /// actor keeps hitting something.
    deflect_side: u8,
    /// Whether `"orientdone"` was already raised for the current turn.
    orient_notified: bool,
}

impl Default for ActorMove {
    fn default() -> Self {
        Self {
            goal: None,
            goal_yaw: None,
            goal_volume: None,
            pursuit: None,
            planned_to: None,
            waypoints: Vec::new(),
            waypoint_nodes: Vec::new(),
            traversal: None,
            anim_mode: AnimMode::Normal,
            anim_mode_override: None,
            orient: None,
            code_orient: Orient::Current,
            stances: None,
            heading: None,
            velocity: [0.0; 3],
            scripted: None,
            melee_time: None,
            bounds: None,
            allow_pitch: false,
            hit_ent_type: HIT_NONE,
            hit_yaw: 0.0,
            ground_ent_type: HIT_WORLD,
            deflected: false,
            ground_normal: Some([0.0, 0.0, 1.0]),
            fall_speed: 0.0,
            deflect_side: 0,
            orient_notified: false,
        }
    }
}

/// A negotiation link being crossed by its begin node's animscript.
#[derive(Clone, Debug)]
pub(crate) struct Traversal {
    pub(crate) begin: u16,
    pub(crate) end: u16,
    pub(crate) script: String,
}

/// A scripted animation aligned to a point: the actor stands where the
/// animation's root motion puts it relative to `origin` and `yaw`.
#[derive(Clone, Debug)]
struct Scripted {
    notify: String,
    origin: [f32; 3],
    yaw: f32,
    node: u16,
}

impl ActorMove {
    pub(crate) fn has_path(&self) -> bool {
        !self.waypoints.is_empty()
    }

    fn clear_path(&mut self) {
        self.waypoints.clear();
        self.waypoint_nodes.clear();
    }

    /// Ends the path the AI follows; the goal stays, replanned once it moves.
    pub(crate) fn drop_path(&mut self) {
        self.clear_path();
    }

    /// Where the path ends.
    pub(crate) fn path_end(&self) -> Option<[f32; 3]> {
        self.waypoints.last().copied()
    }

    /// "face default": the script's facing is let go of, the AI keeping the
    /// way it gave.
    pub(crate) fn clear_script_orient(&mut self) {
        if let Some(orient) = self.orient.take() {
            self.code_orient = orient;
        }
    }

    /// Ends a scripted animation.
    pub(crate) fn stop_scripted(&mut self) {
        self.scripted = None;
    }

    /// Whether a scripted animation still places the actor.
    pub(crate) fn scripted_playing(&self) -> bool {
        self.scripted.is_some()
    }

    fn drop_waypoint(&mut self) {
        self.waypoints.remove(0);
        if !self.waypoint_nodes.is_empty() {
            self.waypoint_nodes.remove(0);
        }
    }

    /// Ends a finished traversal; the path resumes past its end node.
    pub(crate) fn finish_traversal(&mut self) {
        let Some(traversal) = self.traversal.take() else {
            return;
        };
        if self.waypoint_nodes.first() == Some(&Some(traversal.end)) {
            self.drop_waypoint();
        }
    }
}

/// A goal set this close to the current one keeps the current path.
const SAME_GOAL_DISTANCE: f32 = 16.0;
/// A goal entity that moves farther than this is chased with a new path.
const REPLAN_DISTANCE: f32 = 64.0;
/// A waypoint closer than this is passed.
const WAYPOINT_REACHED: f32 = 16.0;
/// When a script names no radius.
const DEFAULT_GOAL_RADIUS: f32 = 32.0;
/// When a script names no goal height.
const DEFAULT_GOAL_HEIGHT: f32 = 80.0;
/// What the movement hit and stands on, as `gethitenttype` and
/// `getgroundenttype` answer.
const HIT_NONE: &str = "none";
const HIT_WORLD: &str = "world";
/// Degrees an actor turns per second toward what it faces (`ai_turnRate`).
const TURN_RATE: f32 = 220.0;
/// The actor's default collision box.
const ACTOR_MINS: [f32; 3] = [-15.0, -15.0, 0.0];
const ACTOR_MAXS: [f32; 3] = [15.0, 15.0, 72.0];
/// Step height an actor climbs.
const STEP_HEIGHT: f32 = 18.0;
/// Walkable floor normal z threshold.
const MIN_WALK_NORMAL: f32 = 0.7;
/// Gravity on an airborne actor (`bg_gravity`), units/s².
const ACTOR_GRAVITY: f32 = 800.0;
/// How far below its feet an actor looks for ground while standing on it.
const GROUND_PROBE: f32 = 0.25;
/// The slide's clip overbounce: a velocity into a plane loses a little more
/// than its part along the normal, one out of it a little less.
const OVERCLIP_INTO: f32 = 1.001;
const OVERCLIP_OUT: f32 = 0.999;
/// A plane the velocity enters slower than this does not clip it.
const CLIP_EPSILON: f32 = 0.1;
/// A walk that ends further than this share of its wish from the wished
/// point deflects off what it hit.
const DEFLECT_MISS: f32 = 0.1;
/// Each deflection's length against the last; the deflection blended into
/// one already made to the other side.
const DEFLECT_SCALE: f32 = 0.75;
const DEFLECT_BLEND: f32 = 0.33;
const DEFLECT_TRIES: usize = 2;
/// A walk that gains less than this share of its wish is blocked.
const BLOCKED_PROGRESS: f32 = 0.25;
/// Ground pitch: the samples ahead of and behind the actor, the height
/// window they look in, and the most the pitch turns per 50 ms tick.
const PITCH_SAMPLE_OFFSET: f32 = 10.9;
const PITCH_TRACE_WINDOW: f32 = 45.0;
const PITCH_STEP_PER_TICK: f32 = 6.0;
const PITCH_LIMIT: f32 = 40.0;
/// The body Black Ops sweeps along a dynamic path's links against its
/// brushes: `[-15, -15, 18]..[15, 15, 72]` above the node, with the actor's
/// clipmask.
const LINK_HULL: dynamic_paths::LinkHull = dynamic_paths::LinkHull {
    low: 18.0,
    high: 72.0,
    sweep_mask: Some(MASK_ACTOR_SOLID),
};

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::{Function, Method};
    registry.register(Function, "getnode", |world, _, args| {
        let found = find_nodes(world, args)?;
        Ok(found.into_iter().next().unwrap_or(Value::Undefined))
    });
    registry.register(Function, "getnodearray", |world, _, args| {
        let found = find_nodes(world, args)?;
        super::arrays::new_array(world, found)
    });
    // `GetAnyNodeArray(origin, radius)`: the path nodes of any type within
    // the radius across the ground (any height), nearest first, at most
    // 1024.
    registry.register(Function, "getanynodearray", |world, _, args| {
        let origin = vector(args, 0)?;
        let radius = float(args, 1)?;
        let Some(paths) = FrameWorld::from_world(world).actor_paths() else {
            return super::arrays::new_array(world, Vec::new());
        };
        let mut found: Vec<(f32, u16)> = paths
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                let dx = node.origin[0] - origin[0];
                let dy = node.origin[1] - origin[1];
                let dist_sq = dy * dy + dx * dx;
                (radius * radius >= dist_sq).then_some((dist_sq, index as u16))
            })
            .collect();
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        found.truncate(ANY_NODES_MAX);
        let nodes = found
            .into_iter()
            .map(|(_, index)| node_object(world, &paths, index))
            .collect::<Result<Vec<_>, _>>()?;
        super::arrays::new_array(world, nodes)
    });
    // `ai HasPath()`: whether the AI has a path.
    registry.register(Method, "haspath", |world, receiver, _| {
        let id = super::natives::engine::entity_id(world, receiver)?;
        let runtime = world.resource::<RoundScript>();
        if runtime.entities[&id].kind != EntityKind::Actor {
            let classname = runtime.entities[&id].classname.clone();
            return Err(format!(
                "haspath must be called on an AI, not on a '{classname}'"
            ));
        }
        let has = runtime
            .actor_moves
            .get(&id)
            .is_some_and(ActorMove::has_path);
        Ok(Value::Int(has.into()))
    });
    registry.register(Function, "getallnodes", |world, _, _| {
        let Some(paths) = FrameWorld::from_world(world).actor_paths() else {
            return super::arrays::new_array(world, Vec::new());
        };
        let nodes = (0..paths.nodes.len())
            .map(|index| node_object(world, &paths, index as u16))
            .collect::<Result<Vec<_>, _>>()?;
        super::arrays::new_array(world, nodes)
    });
    registry.register(Method, "setgoalpos", |world, receiver, args| {
        let goal = vector(args, 0)?;
        let yaw = if args.len() == 2 {
            Some(vector(args, 1)?[1])
        } else {
            None
        };
        set_goal(world, receiver, Goal::Position(goal), yaw)
    });
    registry.register(Method, "setgoalnode", |world, receiver, args| {
        let node = arg(args, 0)?.clone();
        let Value::Object(id) = node else {
            return Err("expects a path node".into());
        };
        let (origin, yaw) = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let origin = match runtime.object_field(id, "origin") {
                Value::Vector(origin) => origin,
                _ => return Err("path node has no origin".into()),
            };
            let yaw = match runtime.object_field(id, "angles") {
                Value::Vector(angles) => Some(angles[1]),
                _ => None,
            };
            (origin, yaw)
        };
        set_goal(world, receiver, Goal::Position(origin), yaw)
    });
    registry.register(Method, "setgoalentity", |world, receiver, args| {
        let target = super::natives::engine::entity_id(world, arg(args, 0)?)?;
        let id = actor_id(world, receiver)?;
        if let Some(movement) = world.resource_mut::<RoundScript>().actor_moves.get_mut(&id) {
            movement.goal_volume = None;
        }
        set_goal(world, receiver, Goal::Entity(target), None)
    });
    registry.register(Method, "setgoalvolume", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let volume = super::natives::engine::entity_id(world, arg(args, 0)?)?;
        with_move(world, &Value::Object(id), |movement| {
            movement.goal_volume = Some(volume);
        })
    });
    registry.register(Method, "getgoalvolume", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        let volume = world
            .resource::<RoundScript>()
            .actor_moves
            .get(&id)
            .and_then(|movement| movement.goal_volume);
        Ok(volume.map_or(Value::Undefined, Value::Object))
    });
    registry.register(Method, "cleargoalvolume", |world, receiver, _| {
        with_move(world, receiver, |movement| movement.goal_volume = None)
    });
    registry.register(Method, "animscripted", |world, receiver, args| {
        let Ok(id) = actor_id(world, receiver) else {
            return model_anim_scripted(world, receiver, args);
        };
        if args.len() > 8 {
            return Err("too many parameters".into());
        }
        if args.len() < 4 {
            return Err("Incorrect number of parameters for animscripted command".into());
        }
        super::actor_brain::anim_scripted(world, id, args)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "startscriptedanim", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let notify = string(args, 0)?;
        let origin = vector(args, 1)?;
        let angles = vector(args, 2)?;
        let anim = arg(args, 3)?.clone();
        let root = args.get(5).filter(|root| **root != Value::Undefined);
        let rate = match args.get(6) {
            Some(Value::Float(rate)) => *rate,
            Some(Value::Int(rate)) => *rate as f32,
            _ => 1.0,
        };
        let goal_time = match args.get(7) {
            Some(Value::Float(time)) => *time,
            Some(Value::Int(time)) => *time as f32,
            _ => 0.2,
        };
        let node =
            super::actor_anims::play_scripted(world, id, &notify, &anim, root, rate, goal_time)?;
        if notify == "rise" {
            let anim_name = match &anim {
                Value::Animation { name, .. } => name.as_ref(),
                _ => "-",
            };
            diag::info!(
                Sim,
                "actor {id}: scripted {notify} ({anim_name}) from ({:.0} {:.0} {:.0}) yaw {:.0}",
                origin[0],
                origin[1],
                origin[2],
                angles[1]
            );
        }
        with_move(world, receiver, |movement| {
            movement.scripted = Some(Scripted {
                notify,
                origin,
                yaw: angles[1],
                node,
            });
            movement.clear_path();
        })
    });
    registry.register(Method, "stopanimscripted", |world, receiver, _| {
        if let Ok(id) = actor_id(world, receiver) {
            super::actor_brain::stop_anim_scripted(world, id);
        }
        with_move(world, receiver, ActorMove::stop_scripted)
    });
    registry.register(Method, "forceteleport", force_teleport);
    registry.register(Method, "teleport", force_teleport);
    // Black Ops answers the found path's length as a whole number (it keeps
    // it as a short), and 0 when no path is found.
    registry.register(Method, "calcpathlength", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let to = vector(args, 0)?;
        let (from, _) = pose(&mut world.resource_mut::<RoundScript>(), id);
        let Some(paths) = FrameWorld::from_world(world).actor_paths() else {
            return Ok(Value::Int(0));
        };
        let badplace_teams = actor_badplace_teams(world, id);
        let edits = world
            .get_resource::<LinkEdits>()
            .cloned()
            .unwrap_or_else(LinkEdits::default);
        let mut reach = |a: [f32; 3], b: [f32; 3]| walkable(world, a, b);
        let Some(nodes) = paths.plan(from, to, &edits, badplace_teams, &mut reach) else {
            return Ok(Value::Int(0));
        };
        let mut length = 0.0;
        let mut at = from;
        for point in nodes
            .iter()
            .map(|&node| paths.nodes[node as usize].origin)
            .chain(std::iter::once(to))
        {
            length += distance_sq(at, point).sqrt();
            at = point;
        }
        Ok(Value::Int(i32::from(length as i16)))
    });
    for (name, end) in [
        ("getnegotiationstartnode", false),
        ("getnegotiationendnode", true),
    ] {
        let native: crate::script::Native = if end {
            |world, receiver, _| negotiation_node(world, receiver, true)
        } else {
            |world, receiver, _| negotiation_node(world, receiver, false)
        };
        registry.register(Method, name, native);
    }
    registry.register(Method, "traversemode", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let mode = string(args, 0)?;
        super::actor_brain::set_traverse_mode(world, id, &mode)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "animcustom", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let function = match arg(args, 0)? {
            Value::Function(function) => *function as usize,
            _ => return Err("animcustom expects a function".into()),
        };
        super::actor_brain::anim_custom(world, id, function);
        Ok(Value::Undefined)
    });
    registry.register(Method, "maymovetopoint", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let to = vector(args, 0)?;
        let (from, _) = pose(&mut world.resource_mut::<RoundScript>(), id);
        Ok(Value::Int(walkable(world, from, to).into()))
    });
    registry.register(Method, "maymovefrompointtopoint", |world, _, args| {
        let (from, to) = (vector(args, 0)?, vector(args, 1)?);
        Ok(Value::Int(walkable(world, from, to).into()))
    });
    registry.register(Method, "isingoal", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        if args.len() != 1 {
            return Err("illegal call to isingoal()\n".into());
        }
        let point = vector(args, 0)?;
        let radius = goal_radius(world, id);
        let height = goal_height(world, id);
        let volume = world
            .resource::<RoundScript>()
            .actor_moves
            .get(&id)
            .and_then(|movement| movement.goal_volume);
        let inside = goal_position(world, id).is_none_or(|goal| {
            let dz = point[2] - goal[2];
            dz * dz <= height * height && horizontal_distance(goal, point) <= radius
        }) && volume
            .is_none_or(|volume| super::triggers::is_touching(world, id, volume));
        Ok(Value::Int(inside.into()))
    });
    registry.register(Method, "gethitenttype", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        let kind = world
            .resource::<RoundScript>()
            .actor_moves
            .get(&id)
            .map_or(HIT_NONE, |movement| movement.hit_ent_type);
        Ok(Value::string(kind))
    });
    registry.register(Method, "gethityaw", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        let movement = world.resource::<RoundScript>().actor_moves.get(&id);
        match movement.filter(|m| m.hit_ent_type != HIT_NONE) {
            Some(m) => Ok(Value::Float(m.hit_yaw)),
            None => Err("nothing was hit".into()),
        }
    });
    registry.register(Method, "getgroundenttype", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        let kind = world
            .resource::<RoundScript>()
            .actor_moves
            .get(&id)
            .map_or(HIT_WORLD, |movement| movement.ground_ent_type);
        Ok(Value::string(kind))
    });
    registry.register(Method, "isdeflected", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        let deflected = world
            .resource::<RoundScript>()
            .actor_moves
            .get(&id)
            .is_some_and(|movement| movement.deflected);
        Ok(Value::Int(deflected.into()))
    });
    registry.register(Method, "allowpitchangle", |world, receiver, args| {
        let id = actor_id(world, receiver)
            .map_err(|_| "AllowPitchAngle() called on non-actor\n".to_owned())?;
        if args.is_empty() {
            return Err("AllowPitchAngle() called without an argument (0 or 1)\n".into());
        }
        let allow = int(args, 0)? != 0;
        let mut runtime = world.resource_mut::<RoundScript>();
        runtime.actor_moves.entry(id).or_default().allow_pitch = allow;
        if !allow {
            let (_, mut angles) = pose(&mut runtime, id);
            angles[0] = 0.0;
            runtime.set_object_field(id, "angles", Value::Vector(angles));
        }
        Ok(Value::Undefined)
    });
    // `setPhysParams(radius, bottom, top)`: the actor's box becomes
    // `(-radius, -radius, bottom)..(radius, radius, top)`.
    registry.register(Method, "setphysparams", |world, receiver, args| {
        if args.len() != 3 {
            return Err("setphysparams takes three parameters\n".into());
        }
        super::natives::engine::entity_id(world, receiver)?;
        let Ok(id) = actor_id(world, receiver) else {
            return Err("setphysparams must be called on an AI only.".into());
        };
        let radius = float(args, 0)?;
        let bottom = float(args, 1)?;
        let top = float(args, 2)?;
        with_move(world, &Value::Object(id), |movement| {
            movement.bounds = Some(([-radius, -radius, bottom], [radius, radius, top]));
        })
    });
    registry.register(Method, "disconnectpaths", |world, receiver, _| {
        let id = super::natives::engine::entity_id(world, receiver)?;
        if !dynamic_paths::dynamic_path(world, id) {
            return dynamic_paths::refuse(world, id, "disconnect paths");
        }
        dynamic_paths::disconnect(world, id, LINK_HULL);
        Ok(Value::Undefined)
    });
    registry.register(Method, "connectpaths", |world, receiver, _| {
        let id = super::natives::engine::entity_id(world, receiver)?;
        if !dynamic_paths::dynamic_path(world, id) {
            return dynamic_paths::refuse(world, id, "connect paths");
        }
        dynamic_paths::connect(world, id);
        Ok(Value::Undefined)
    });
    registry.on_tick(|world| dynamic_paths::cut_level(world, LINK_HULL));
    registry.register(Function, "badplace_delete", badplace_delete);
    registry.register(Function, "badplace_cylinder", badplace_cylinder);
    registry.register(Function, "badplace_arc", badplace_arc);
    registry.register(Function, "badplace_brush", badplace_brush);
    // `cansee(entity, [seconds])`: a sentient by a sight check no older
    // than that (a quarter second unless given), anything else by a new one.
    registry.register(Method, "cansee", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let target = super::natives::engine::entity_id(world, arg(args, 0)?)?;
        let seen = if super::actor_brain::is_sentient(world, target) {
            let max_age = match args.get(1) {
                Some(_) => i64::from((float(args, 1)? * 1000.0) as i32),
                None => super::actor_brain::SIGHT_MAX_AGE_MS,
            };
            super::actor_brain::can_see_sentient(world, id, target, max_age)
        } else {
            let (fov, max_dist_sq) = super::actor_brain::sight_limits(world, id);
            super::actor_brain::sight(world, id, target, fov, max_dist_sq)
        };
        Ok(Value::Int(i32::from(seen)))
    });
    registry.register(Method, "melee", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let direction = match args.first() {
            Some(_) => Some(vector(args, 0)?),
            None => None,
        };
        Ok(melee(world, id, direction).map_or(Value::Undefined, Value::Object))
    });
    registry.register(Method, "allowedstances", |world, receiver, args| {
        let stances = (0..args.len())
            .map(|index| string(args, index))
            .collect::<Result<Vec<_>, _>>()?;
        with_move(world, receiver, |movement| movement.stances = Some(stances))
    });
    registry.register(Method, "isstanceallowed", |world, receiver, args| {
        let stance = string(args, 0)?;
        let id = actor_id(world, receiver)?;
        let allowed = world
            .resource::<RoundScript>()
            .actor_moves
            .get(&id)
            .and_then(|movement| movement.stances.as_ref())
            .is_none_or(|stances| stances.contains(&stance));
        Ok(Value::Int(allowed.into()))
    });
    registry.register(Method, "getmotionangle", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        let (_, angles) = pose(&mut world.resource_mut::<RoundScript>(), id);
        let heading = world
            .resource::<RoundScript>()
            .actor_moves
            .get(&id)
            .and_then(|movement| movement.heading);
        Ok(Value::Float(heading.map_or(0.0, |heading| {
            math_iw4::angle_subtract(heading, angles[1])
        })))
    });
    registry.register(Method, "getclosestenemysqdist", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        let (origin, _) = pose(&mut world.resource_mut::<RoundScript>(), id);
        Ok(Value::Float(
            enemy_position(world, id).map_or(f32::MAX, |enemy| distance_sq(origin, enemy)),
        ))
    });
    // `animmode(mode, [clear_path])` sets the script's movement mode, which
    // the AI's state uses while its animscript runs ("none" lets it go);
    // a set mode with the flag ends the path.
    registry.register(Method, "animmode", |world, receiver, args| {
        let name = string(args, 0)?;
        let mode = match name.as_str() {
            "none" => None,
            "normal" => Some(AnimMode::Normal),
            "pos deltas" => Some(AnimMode::PosDeltas),
            "gravity" => Some(AnimMode::Gravity),
            "zonly_physics" => Some(AnimMode::ZOnlyPhysics),
            "nogravity" | "point relative" => Some(AnimMode::NoGravity),
            "noclip" | "nophysics" => Some(AnimMode::NoClip),
            "angle deltas" => Some(AnimMode::AngleDeltas),
            _ => return Err("illegal call to animmode()\n".into()),
        };
        let clear = name != "none" && matches!(args.get(1), Some(Value::Int(n)) if *n != 0);
        with_move(world, receiver, |movement| {
            if clear {
                movement.clear_path();
            }
            movement.anim_mode_override = mode;
            if let Some(mode) = mode {
                movement.anim_mode = mode;
            }
        })
    });
    registry.register(Method, "orientmode", |world, receiver, args| {
        let orient = match string(args, 0)?.as_str() {
            "face motion" => Some(Orient::Motion),
            "face enemy" => Some(Orient::Enemy),
            "face enemy or motion" => Some(Orient::EnemyOrMotion),
            "face goal" => Some(Orient::Goal),
            "face angle" => Some(Orient::Angle(float(args, 1)?)),
            "face direction" => {
                let dir = vector(args, 1)?;
                if dir[0] == 0.0 && dir[1] == 0.0 {
                    return Err("cannot face (0, 0, *)".into());
                }
                Some(Orient::Angle(dir[1].atan2(dir[0]).to_degrees()))
            }
            "face point" => Some(Orient::Point(vector(args, 1)?)),
            "face current" => Some(Orient::Current),
            "face default" => None,
            _ => {
                return Err(
                    "orientMode must be 'face angle', 'face current', 'face direction', \
                     'face enemy', 'face enemy or motion', 'face goal', 'face motion', \
                     'face point', or 'face default'\n'face direction' and 'face point' take \
                     a second argument that is a vector giving the way to face\n'face angle' \
                     takes a second argument that is a yaw angle\n"
                        .into(),
                );
            }
        };
        with_move(world, receiver, |movement| match orient {
            Some(orient) => movement.orient = Some(orient),
            None => movement.clear_script_orient(),
        })
    });
}

fn negotiation_node(world: &mut World, receiver: &Value, end: bool) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    let Some(traversal) = world
        .resource::<RoundScript>()
        .actor_moves
        .get(&id)
        .and_then(|movement| movement.traversal.clone())
    else {
        return Ok(Value::Undefined);
    };
    let paths = FrameWorld::from_world(world)
        .actor_paths()
        .ok_or("the map has no path network")?;
    node_object(
        world,
        &paths,
        if end { traversal.end } else { traversal.begin },
    )
}

fn actor_id(world: &World, receiver: &Value) -> Result<u64, String> {
    match world.resource::<RoundScript>().entity(receiver) {
        Some((id, entity)) if entity.kind == EntityKind::Actor => Ok(id),
        _ => Err("receiver is not an actor".into()),
    }
}

fn with_move(
    world: &mut World,
    receiver: &Value,
    apply: impl FnOnce(&mut ActorMove),
) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    apply(
        world
            .resource_mut::<RoundScript>()
            .actor_moves
            .entry(id)
            .or_default(),
    );
    Ok(Value::Undefined)
}

fn force_teleport(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    let origin = vector(args, 0)?;
    let angles = match args.get(1) {
        Some(Value::Vector(angles)) => Some(*angles),
        _ => None,
    };
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.set_object_field(id, "origin", Value::Vector(origin));
    if let Some(angles) = angles {
        runtime.set_object_field(id, "angles", Value::Vector(angles));
    }
    let movement = runtime.actor_moves.entry(id).or_default();
    movement.clear_path();
    movement.planned_to = None;
    Ok(Value::Undefined)
}

fn set_goal(
    world: &mut World,
    receiver: &Value,
    goal: Goal,
    yaw: Option<f32>,
) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    let mut runtime = world.resource_mut::<RoundScript>();
    let movement = runtime.actor_moves.entry(id).or_default();
    // Scripts re-issue the same goal every frame; only a goal that moved
    // replans, as an entity goal does once it strays from the plan.
    let same = match (&movement.goal, &goal) {
        (Some(Goal::Position(old)), Goal::Position(new)) => {
            distance_sq(*old, *new) < SAME_GOAL_DISTANCE * SAME_GOAL_DISTANCE
        }
        (Some(Goal::Entity(old)), Goal::Entity(new)) => old == new,
        _ => false,
    };
    movement.goal = Some(goal);
    movement.goal_yaw = yaw;
    if !same {
        movement.planned_to = None;
        movement.clear_path();
        raise(world, Value::Object(id), "goal_changed", Vec::new());
    }
    Ok(Value::Undefined)
}

/// `GetNode(value, key)` and `GetNodeArray(value, key)`: nodes whose key matches.
fn find_nodes(world: &mut World, args: &[Value]) -> Result<Vec<Value>, String> {
    let value = string(args, 0)?;
    let key = string(args, 1)?;
    let Some(paths) = FrameWorld::from_world(world).actor_paths() else {
        return Ok(Vec::new());
    };
    let matches: Vec<u16> = (0..paths.nodes.len())
        .filter(|&index| {
            let node = &paths.nodes[index];
            match key.as_str() {
                "targetname" => node.targetname == value,
                "target" => node.target == value,
                "animscript" => node.animscript == value,
                _ => false,
            }
        })
        .map(|index| index as u16)
        .collect();
    matches
        .into_iter()
        .map(|index| node_object(world, &paths, index))
        .collect()
}

/// The script objects of the nodes `keep` admits, in map order.
pub(crate) fn nodes_where(
    world: &mut World,
    keep: impl Fn(&NavNode) -> bool,
) -> Result<Vec<Value>, String> {
    let Some(paths) = FrameWorld::from_world(world).actor_paths() else {
        return Ok(Vec::new());
    };
    (0..paths.nodes.len())
        .filter(|&index| keep(&paths.nodes[index]))
        .map(|index| node_object(world, &paths, index as u16))
        .collect()
}

/// The script object of the node at `index`.
pub(crate) fn node_value(world: &mut World, index: u16) -> Result<Value, String> {
    let paths = FrameWorld::from_world(world)
        .actor_paths()
        .ok_or("no path nodes")?;
    if usize::from(index) >= paths.nodes.len() {
        return Err("no such node".into());
    }
    node_object(world, &paths, index)
}

/// The script object standing for a path node, made on first use.
fn node_object(world: &mut World, paths: &ActorPaths, index: u16) -> Result<Value, String> {
    let mut runtime = world.resource_mut::<RoundScript>();
    if let Some(&id) = runtime.path_node_objects.get(&index)
        && runtime.live(&id)
    {
        return Ok(Value::Object(id));
    }
    let node = &paths.nodes[index as usize];
    let id = runtime.new_object()?;
    runtime.set_object_field(id, "origin", Value::Vector(node.origin));
    runtime.set_object_field(id, "angles", Value::Vector([0.0, node.yaw, 0.0]));
    let type_name = node
        .fields
        .as_ref()
        .map_or(node.kind.script_type(), |fields| fields.type_name.as_str());
    runtime.set_object_field(id, "type", Value::string(type_name));
    if let Some(fields) = &node.fields {
        runtime.set_object_field(id, "spawnflags", Value::Int(fields.spawnflags as i32));
        runtime.set_object_field(id, "radius", Value::Float(fields.radius));
        runtime.set_object_field(id, "minusedistsq", Value::Float(fields.min_use_dist_sq));
        runtime.set_object_field(
            id,
            "suspended",
            Value::Int(i32::from(fields.spawnflags & 0x10000 != 0)),
        );
        for (name, text) in [
            ("script_linkname", &fields.script_linkname),
            ("script_noteworthy", &fields.script_noteworthy),
        ] {
            if !text.is_empty() {
                runtime.set_object_field(id, name, Value::string(text));
            }
        }
    }
    for (name, text) in [
        ("targetname", &node.targetname),
        ("target", &node.target),
        ("animscript", &node.animscript),
    ] {
        if !text.is_empty() {
            runtime.set_object_field(id, name, Value::string(text));
        }
    }
    runtime.path_node_objects.insert(index, id);
    Ok(Value::Object(id))
}

fn pose(runtime: &mut RoundScript, id: u64) -> ([f32; 3], [f32; 3]) {
    let vector = |runtime: &mut RoundScript, name| match runtime.object_field(id, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    };
    (vector(runtime, "origin"), vector(runtime, "angles"))
}

/// Plans paths for new goals, walks each actor by its animation and reports
/// `goal` and `bad_path`.
/// `AnimScripted(notify, origin, angles, anim, ...)` on a script model: it
/// plays the clip once from that pose and reports `end` under `notify` when
/// the clip is done.
fn model_anim_scripted(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let id = super::natives::engine::entity_id(world, receiver)?;
    let notify = string(args, 0)?;
    let (origin, angles) = (vector(args, 1)?, vector(args, 2)?);
    let clip = match arg(args, 3)? {
        Value::Animation { name, .. } => name.clone(),
        other => return Err(format!("{other:?} is not an animation")),
    };
    let seconds = crate::frame::FrameWorld::from_world(world)
        .anim_clip_named(&clip)
        .map_or(0.0, |clip| {
            f32::from(clip.numframes.max(1)) / clip.framerate.max(1.0)
        });
    let now = super::players::now_ms(world);
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.set_object_field(id, "origin", Value::Vector(origin));
    runtime.set_object_field(id, "angles", Value::Vector(angles));
    if let Some(entity) = runtime.entities.get_mut(&id) {
        entity.anim_op = Some(Some(clip));
    }
    runtime.timed_notifies.push((
        now + (seconds * 1000.0) as i64,
        receiver.clone(),
        notify.into(),
        vec![Value::string("end")],
    ));
    Ok(Value::Undefined)
}

pub(crate) fn locomote(world: &mut World, seconds: f32) {
    let now = super::players::now_ms(world);
    if let Some(mut edits) = world.get_resource_mut::<LinkEdits>() {
        edits.expire_badplaces(now);
    }
    super::presence::settle_collision(world);
    let actors: Vec<u64> = world
        .resource::<RoundScript>()
        .actor_moves
        .keys()
        .copied()
        .collect();
    let paths = FrameWorld::from_world(world).actor_paths();
    for actor in actors {
        if !world
            .resource::<RoundScript>()
            .entities
            .contains_key(&actor)
        {
            world
                .resource_mut::<RoundScript>()
                .actor_moves
                .remove(&actor);
            continue;
        }
        let (mut origin, mut angles) = pose(&mut world.resource_mut::<RoundScript>(), actor);
        let before = origin;
        let goal_radius = goal_radius(world, actor);
        let goal_at = goal_position(world, actor);
        let mut movement = world.resource::<RoundScript>().actor_moves[&actor].clone();
        let mut events: Vec<&'static str> = Vec::new();
        let prev_ground = movement.ground_ent_type;
        movement.hit_ent_type = HIT_NONE;
        movement.deflected = false;
        // A pursuit ends where its path does; it is no script goal.
        let pursuing = movement.pursuit.is_some();
        let goal_radius = if pursuing {
            WAYPOINT_REACHED
        } else {
            goal_radius
        };

        let edits = world
            .get_resource::<LinkEdits>()
            .cloned()
            .unwrap_or_else(LinkEdits::default);
        let badplace_teams = actor_badplace_teams(world, actor);
        if let Some(goal_at) = goal_at {
            let path_blocked =
                path_uses_blocked_link(&movement.waypoint_nodes, &edits, badplace_teams);
            let stale = path_blocked
                || movement.planned_to.is_none_or(|planned| {
                    distance_sq(planned, goal_at) > REPLAN_DISTANCE * REPLAN_DISTANCE
                });
            if stale {
                movement.planned_to = Some(goal_at);
                movement.clear_path();
                if horizontal_distance(origin, goal_at) > goal_radius {
                    let mut reach = |from: [f32; 3], to: [f32; 3]| walkable(world, from, to);
                    let planned = paths.as_deref().and_then(|paths| {
                        paths.plan(origin, goal_at, &edits, badplace_teams, &mut reach)
                    });
                    match planned {
                        Some(nodes) => {
                            let paths = paths.as_deref().unwrap();
                            movement.waypoints = nodes
                                .iter()
                                .map(|&node| paths.nodes[node as usize].origin)
                                .collect();
                            movement.waypoint_nodes =
                                nodes.iter().map(|&node| Some(node)).collect();
                            movement.waypoints.push(goal_at);
                            movement.waypoint_nodes.push(None);
                        }
                        None => events.push("bad_path"),
                    }
                }
            }
        }

        if let Some(scripted) = movement.scripted.clone() {
            match super::actor_anims::node_clip(world, actor, scripted.node) {
                Some((clip, time)) => {
                    let local = clip.abs_delta_trans(time);
                    let (sin, cos) = scripted.yaw.to_radians().sin_cos();
                    origin = [
                        scripted.origin[0] + local[0] * cos - local[1] * sin,
                        scripted.origin[1] + local[0] * sin + local[1] * cos,
                        scripted.origin[2] + local[2],
                    ];
                    angles[1] = scripted.yaw + clip.abs_delta_yaw(time);
                    if !clip.looping && time >= 1.0 {
                        if scripted.notify == "rise" {
                            diag::info!(
                                Sim,
                                "actor {actor}: scripted rise done at ({:.0} {:.0} {:.0})",
                                origin[0],
                                origin[1],
                                origin[2]
                            );
                        }
                        movement.scripted = None;
                    }
                }
                None => movement.scripted = None,
            }
            if seconds > 0.0 {
                movement.velocity = std::array::from_fn(|i| (origin[i] - before[i]) / seconds);
            }
            let mut runtime = world.resource_mut::<RoundScript>();
            runtime.set_object_field(actor, "origin", Value::Vector(origin));
            runtime.set_object_field(actor, "angles", Value::Vector(angles));
            runtime.actor_moves.insert(actor, movement);
            continue;
        }
        let delta = super::actor_anims::root_delta(world, actor);
        let (local, turned) = delta.unwrap_or(([0.0; 3], 0.0));
        let scale = anim_translation_scale(world, actor);
        let (sin, cos) = angles[1].to_radians().sin_cos();
        let world_delta = [
            (local[0] * cos - local[1] * sin) * scale,
            (local[0] * sin + local[1] * cos) * scale,
            local[2] * scale,
        ];
        let mut heading = None;
        match movement.anim_mode {
            _ if movement.traversal.is_some() => {
                for axis in 0..3 {
                    origin[axis] += world_delta[axis];
                }
                angles[1] += turned;
            }
            // Black Ops adds a nogravity animation's delta as it is.
            AnimMode::NoGravity | AnimMode::NoClip => {
                for axis in 0..3 {
                    origin[axis] += world_delta[axis];
                }
                angles[1] += turned;
            }
            AnimMode::Normal => {
                while movement
                    .waypoints
                    .first()
                    .is_some_and(|&next| horizontal_distance(origin, next) < WAYPOINT_REACHED)
                    && movement.waypoints.len() > 1
                {
                    if let Some(traversal) = paths
                        .as_deref()
                        .and_then(|paths| traversal_at(paths, &movement.waypoint_nodes))
                    {
                        movement.drop_waypoint();
                        movement.traversal = Some(traversal);
                        break;
                    }
                    movement.drop_waypoint();
                }
                if let Some(&next) = movement.waypoints.first() {
                    let step = (world_delta[0].powi(2) + world_delta[1].powi(2)).sqrt();
                    let to = [next[0] - origin[0], next[1] - origin[1]];
                    let length = (to[0] * to[0] + to[1] * to[1]).sqrt();
                    let wish = if length > 0.001 {
                        let advance = step.min(length);
                        heading = Some(to[1].atan2(to[0]).to_degrees());
                        [to[0] / length * advance, to[1] / length * advance, 0.0]
                    } else {
                        [0.0; 3]
                    };
                    origin = walk_path(world, origin, wish, seconds, &mut movement);
                } else {
                    origin = physics_move(world, origin, [0.0; 3], seconds, &mut movement).0;
                }
            }
            AnimMode::PosDeltas => {
                origin = physics_move(
                    world,
                    origin,
                    [world_delta[0], world_delta[1], 0.0],
                    seconds,
                    &mut movement,
                )
                .0;
            }
            AnimMode::Gravity => {
                origin = physics_move(
                    world,
                    origin,
                    [world_delta[0], world_delta[1], 0.0],
                    seconds,
                    &mut movement,
                )
                .0;
                angles[1] += turned;
            }
            AnimMode::ZOnlyPhysics => {
                origin = zonly_move(world, origin, world_delta, seconds, &mut movement);
                angles[1] += turned;
            }
            AnimMode::AngleDeltas => angles[1] += turned,
        }

        if movement.ground_ent_type != prev_ground {
            events.push("groundEntChanged");
        }

        movement.heading = heading;
        let orient = movement
            .orient
            .clone()
            .unwrap_or_else(|| movement.code_orient.clone());
        // `isfacingmotion`: the facing took the motion's way this tick.
        let mut facing_motion = false;
        let facing = match orient {
            Orient::Motion => {
                facing_motion = true;
                heading
            }
            Orient::Angle(yaw) => Some(yaw),
            Orient::Point(point) => Some(yaw_to(origin, point)),
            Orient::Enemy => enemy_position(world, actor).map(|enemy| yaw_to(origin, enemy)),
            Orient::EnemyOrMotion => {
                // The enemy without a path or while standing; on the move the
                // motion, unless a seen enemy stands within 350 units, where
                // Black Ops' path test (not ported) may face it instead.
                let standing = movement.velocity[0] == 0.0 && movement.velocity[1] == 0.0;
                if !movement.has_path() || standing {
                    enemy_position(world, actor).map(|enemy| yaw_to(origin, enemy))
                } else {
                    facing_motion = true;
                    heading
                }
            }
            Orient::Goal => movement.goal_yaw,
            Orient::Current => None,
        };
        if let Some(wanted) = facing {
            let turn = math_iw4::angle_subtract(wanted, angles[1]);
            let rate = actor_turn_rate(world, actor);
            let limit = rate * seconds;
            angles[1] += turn.clamp(-limit, limit);
            if math_iw4::angle_subtract(wanted, angles[1]).abs() <= 0.01 {
                if !movement.orient_notified {
                    movement.orient_notified = true;
                    events.push("orientdone");
                }
            } else {
                movement.orient_notified = false;
            }
        }
        if movement.allow_pitch {
            let target_pitch = ground_pitch(world, origin, angles[1]);
            let diff = math_iw4::angle_subtract(target_pitch, angles[0]);
            angles[0] = (angles[0] + diff.clamp(-PITCH_STEP_PER_TICK, PITCH_STEP_PER_TICK))
                .clamp(-PITCH_LIMIT, PITCH_LIMIT);
        }

        if let Some(goal_at) = goal_at
            && horizontal_distance(origin, goal_at) <= goal_radius
            && (origin[2] - goal_at[2]).abs() <= goal_height(world, actor)
            && movement.planned_to.is_some()
            && (movement.waypoints.len() <= 1)
        {
            if !pursuing && (movement.has_path() || !matches!(movement.goal, Some(Goal::Entity(_))))
            {
                events.push("goal");
            }
            movement.clear_path();
            if !pursuing && matches!(movement.goal, Some(Goal::Position(_))) {
                movement.goal = None;
            }
        }

        if seconds > 0.0 {
            movement.velocity = std::array::from_fn(|i| (origin[i] - before[i]) / seconds);
        }
        // The way along the path ahead: toward the next waypoint.
        let (lookahead, lookahead_dist) = movement
            .waypoints
            .first()
            .and_then(|next| {
                let to = [next[0] - origin[0], next[1] - origin[1]];
                let length = (to[0] * to[0] + to[1] * to[1]).sqrt();
                (length > 0.001).then(|| ([to[0] / length, to[1] / length, 0.0], length))
            })
            .unzip();
        {
            let mut runtime = world.resource_mut::<RoundScript>();
            runtime.set_object_field(actor, "origin", Value::Vector(origin));
            runtime.set_object_field(actor, "angles", Value::Vector(angles));
            runtime.set_object_field(
                actor,
                "desiredangle",
                Value::Float(facing.unwrap_or(angles[1])),
            );
            runtime.set_object_field(actor, "isfacingmotion", Value::Int(facing_motion.into()));
            if let Some(dir) = lookahead {
                runtime.set_object_field(actor, "lookaheaddir", Value::Vector(dir));
            }
            runtime.set_object_field(
                actor,
                "lookaheaddist",
                Value::Float(lookahead_dist.unwrap_or(0.0)),
            );
            if let Some(end) = movement.path_end().or(goal_at) {
                runtime.set_object_field(actor, "pathgoalpos", Value::Vector(end));
            }
            runtime.actor_moves.insert(actor, movement);
        }
        for event in events {
            raise(world, Value::Object(actor), event, Vec::new());
        }
    }
}

/// The traversal starting at the path's first node, when the link to the
/// second is a negotiation link.
fn traversal_at(paths: &ActorPaths, nodes: &[Option<u16>]) -> Option<Traversal> {
    let (Some(Some(begin)), Some(Some(end))) = (nodes.first(), nodes.get(1)) else {
        return None;
    };
    let node = &paths.nodes[*begin as usize];
    let negotiation = node
        .links
        .iter()
        .any(|&(to, _, negotiation)| to == *end && negotiation);
    (node.kind == NavNodeKind::NegotiationBegin && negotiation && !node.animscript.is_empty()).then(
        || Traversal {
            begin: *begin,
            end: *end,
            script: node.animscript.clone(),
        },
    )
}

fn goal_radius(world: &mut World, actor: u64) -> f32 {
    match world
        .resource_mut::<RoundScript>()
        .object_field(actor, "goalradius")
    {
        Value::Int(n) => n as f32,
        Value::Float(f) => f,
        _ => DEFAULT_GOAL_RADIUS,
    }
}

fn goal_height(world: &mut World, actor: u64) -> f32 {
    match world
        .resource_mut::<RoundScript>()
        .object_field(actor, "goalheight")
    {
        Value::Int(n) => n as f32,
        Value::Float(f) => f,
        _ => DEFAULT_GOAL_HEIGHT,
    }
}

fn anim_translation_scale(world: &mut World, actor: u64) -> f32 {
    match world
        .resource_mut::<RoundScript>()
        .object_field(actor, "animtranslationscale")
    {
        Value::Int(n) => n as f32,
        Value::Float(f) => f,
        _ => 1.0,
    }
}

fn actor_turn_rate(world: &mut World, actor: u64) -> f32 {
    let field = match world
        .resource_mut::<RoundScript>()
        .object_field(actor, "turnrate")
    {
        Value::Int(n) if n > 0 => Some(n as f32),
        Value::Float(f) if f > 0.0 => Some(f),
        _ => None,
    };
    if let Some(rate) = field {
        return rate;
    }
    world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get("ai_turnRate")
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|&r| r > 0.0)
        .unwrap_or(TURN_RATE)
}

fn actor_badplace_teams(world: &mut World, actor: u64) -> u8 {
    let mut runtime = world.resource_mut::<RoundScript>();
    let aware = match runtime.object_field(actor, "badplaceawareness") {
        Value::Int(n) => n as f32,
        Value::Float(f) => f,
        _ => 0.75,
    };
    if aware <= 0.0 {
        return 0;
    }
    match runtime.object_field(actor, "team") {
        Value::String(team) => match &*team {
            "free" => 1 << 0,
            "axis" => 1 << 1,
            "allies" => 1 << 2,
            "neutral" => 1 << 3,
            _ => 0,
        },
        _ => 0,
    }
}

fn path_uses_blocked_link(nodes: &[Option<u16>], edits: &LinkEdits, badplace_teams: u8) -> bool {
    nodes.windows(2).any(|pair| {
        let (Some(a), Some(b)) = (pair[0], pair[1]) else {
            return false;
        };
        edits.removed.contains(&(a, b))
            || edits.cut_by.get(&(a, b)).is_some_and(|&c| c > 0)
            || edits.link_in_badplace((a, b), badplace_teams)
    })
}

/// One actor move through the world, as Black Ops' actor physics runs it
/// with `phys_ai_collision_mode` 0: the box swept along its velocity and
/// slid along what it hits, stepping up stairs and down to the ground.
struct Phys {
    origin: [f32; 3],
    velocity: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    /// The walkable plane underfoot.
    ground: Option<[f32; 3]>,
    /// The last plane the move hit and where: `iHitEntnum`'s normal and
    /// origin.
    hit: Option<([f32; 3], [f32; 3])>,
}

/// What one sweep of the actor's box found.
struct Sweep {
    fraction: f32,
    normal: [f32; 3],
    endpos: [f32; 3],
    allsolid: bool,
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn clip_velocity(velocity: [f32; 3], normal: [f32; 3]) -> [f32; 3] {
    let into = dot3(velocity, normal);
    let backoff = into
        * if into < 0.0 {
            OVERCLIP_INTO
        } else {
            OVERCLIP_OUT
        };
    std::array::from_fn(|i| velocity[i] - normal[i] * backoff)
}

impl Phys {
    fn trace(&self, world: &mut World, start: [f32; 3], end: [f32; 3]) -> Sweep {
        let trace = FrameWorld::from_world(world).trace_clip(
            start,
            end,
            self.mins,
            self.maxs,
            MASK_ACTOR_SOLID,
        );
        Sweep {
            fraction: trace.fraction,
            normal: trace.normal,
            endpos: trace.endpos,
            allsolid: trace.allsolid != 0,
        }
    }

    /// Looks for ground under the actor: a step's height down while it
    /// stands and is not rising (which keeps it on stairs), else a hair.
    fn ground_trace(&mut self, world: &mut World) {
        let snap = self.ground.is_some() && self.velocity[2] <= 0.0;
        let depth = if snap { STEP_HEIGHT } else { GROUND_PROBE };
        let end = [self.origin[0], self.origin[1], self.origin[2] - depth];
        let trace = self.trace(world, self.origin, end);
        if trace.allsolid || trace.fraction >= 1.0 {
            self.ground = None;
            return;
        }
        if trace.normal[2] < MIN_WALK_NORMAL {
            self.ground = None;
            self.hit = Some((trace.normal, trace.endpos));
            return;
        }
        self.ground = Some(trace.normal);
        if snap {
            self.origin = trace.endpos;
        }
        if self.velocity[2] < 0.0 {
            self.velocity[2] = 0.0;
        }
    }

    /// Moves along the velocity for `dt`, sliding along up to four planes;
    /// whether anything was hit.
    fn slide(&mut self, world: &mut World, dt: f32, gravity: bool) -> bool {
        let mut end_velocity = self.velocity;
        if gravity {
            end_velocity[2] -= ACTOR_GRAVITY * dt;
            self.velocity[2] = (self.velocity[2] + end_velocity[2]) * 0.5;
            if let Some(normal) = self.ground {
                self.velocity = clip_velocity(self.velocity, normal);
            }
        }
        let mut planes: Vec<[f32; 3]> = Vec::with_capacity(5);
        if let Some(normal) = self.ground {
            planes.push(normal);
        }
        let speed = dot3(self.velocity, self.velocity).sqrt();
        if speed > 0.0 {
            planes.push(self.velocity.map(|c| c / speed));
        }
        let mut left = dt;
        let mut bumps = 0;
        while bumps < 4 {
            let end = std::array::from_fn(|i| self.origin[i] + self.velocity[i] * left);
            let trace = self.trace(world, self.origin, end);
            if trace.allsolid {
                self.velocity[2] = 0.0;
                return true;
            }
            if trace.fraction > 0.0 {
                self.origin = trace.endpos;
            }
            if trace.fraction >= 1.0 {
                break;
            }
            bumps += 1;
            self.hit = Some((trace.normal, trace.endpos));
            left -= left * trace.fraction;
            if planes.len() >= 5 {
                self.velocity = [0.0; 3];
                return true;
            }
            // The same plane again: nudge off it.
            if planes.iter().any(|plane| dot3(*plane, trace.normal) > 0.99) {
                self.velocity = std::array::from_fn(|i| self.velocity[i] + trace.normal[i]);
                continue;
            }
            planes.push(trace.normal);
            let mut stuck = true;
            for i in 0..planes.len() {
                if dot3(self.velocity, planes[i]) >= CLIP_EPSILON {
                    continue;
                }
                let mut clip = clip_velocity(self.velocity, planes[i]);
                let mut end_clip = clip_velocity(end_velocity, planes[i]);
                let mut blocked = false;
                for j in 0..planes.len() {
                    if j == i || dot3(clip, planes[j]) >= CLIP_EPSILON {
                        continue;
                    }
                    clip = clip_velocity(clip, planes[j]);
                    end_clip = clip_velocity(end_clip, planes[j]);
                    if dot3(clip, planes[i]) >= 0.0 {
                        continue;
                    }
                    // Into two planes: slide along their crease.
                    let (a, b) = (planes[i], planes[j]);
                    let crease = [
                        a[1] * b[2] - a[2] * b[1],
                        a[2] * b[0] - a[0] * b[2],
                        a[0] * b[1] - a[1] * b[0],
                    ];
                    let length = dot3(crease, crease).sqrt();
                    let dir = if length > 0.0 {
                        crease.map(|c| c / length)
                    } else {
                        crease
                    };
                    let along = dot3(dir, self.velocity);
                    clip = dir.map(|c| c * along);
                    let end_along = dot3(dir, end_velocity);
                    end_clip = dir.map(|c| c * end_along);
                    for (k, plane) in planes.iter().enumerate() {
                        if k != i && k != j && dot3(clip, *plane) < CLIP_EPSILON {
                            blocked = true;
                            break;
                        }
                    }
                    if blocked {
                        break;
                    }
                }
                if blocked {
                    self.velocity = [0.0; 3];
                    return true;
                }
                self.velocity = clip;
                end_velocity = end_clip;
                stuck = false;
                break;
            }
            if stuck {
                break;
            }
        }
        if gravity {
            self.velocity = end_velocity;
        }
        bumps != 0
    }

    /// The slide, then the same move a step higher and back down; the
    /// stepped one stands unless the plain one ended nearer the wished point.
    fn step_slide(&mut self, world: &mut World, dt: f32, gravity: bool) {
        let (start, start_velocity) = (self.origin, self.velocity);
        if !self.slide(world, dt, gravity) {
            return;
        }
        if self.velocity[2] > 0.0 {
            let below = [start[0], start[1], start[2] - STEP_HEIGHT];
            let trace = self.trace(world, start, below);
            if trace.fraction >= 1.0 || trace.normal[2] < MIN_WALK_NORMAL {
                return;
            }
        }
        let (plain, plain_velocity, plain_hit) = (self.origin, self.velocity, self.hit);
        let up = [start[0], start[1], start[2] + STEP_HEIGHT];
        let trace = self.trace(world, start, up);
        if trace.allsolid {
            return;
        }
        let lifted = trace.endpos[2] - start[2];
        self.origin = trace.endpos;
        self.velocity = start_velocity;
        self.slide(world, dt, gravity);
        let wished = [
            start[0] + start_velocity[0] * dt,
            start[1] + start_velocity[1] * dt,
        ];
        let miss = |p: [f32; 3]| (wished[0] - p[0]).powi(2) + (wished[1] - p[1]).powi(2);
        if miss(plain) < miss(self.origin) {
            self.origin = plain;
            self.velocity = plain_velocity;
            self.hit = plain_hit;
            return;
        }
        let down = [self.origin[0], self.origin[1], self.origin[2] - lifted];
        let trace = self.trace(world, self.origin, down);
        if !trace.allsolid {
            self.origin = trace.endpos;
        }
        if trace.fraction < 1.0 {
            self.velocity = clip_velocity(self.velocity, trace.normal);
        }
    }

    /// A step on the ground: the wish turned along the ground plane at its
    /// own horizontal speed.
    fn ground_walk(&mut self, world: &mut World, wish: [f32; 3], dt: f32) {
        let flat = [wish[0] / dt, wish[1] / dt];
        let speed = (flat[0] * flat[0] + flat[1] * flat[1]).sqrt();
        self.velocity = [flat[0], flat[1], 0.0];
        if let Some(n) = self.ground {
            let along = [
                flat[0] * n[2],
                flat[1] * n[2],
                -(flat[0] * n[0]) - flat[1] * n[1],
            ];
            if along[0] * flat[0] + along[1] * flat[1] > 0.0 {
                let length = dot3(along, along).sqrt();
                self.velocity = along.map(|c| c / length * speed);
            }
        }
        if self.velocity[0] == 0.0 && self.velocity[1] == 0.0 {
            return;
        }
        self.step_slide(world, dt, false);
    }

    /// A step in the air: the wish across, gravity down.
    fn air_move(&mut self, world: &mut World, wish: [f32; 3], fall: f32, dt: f32) {
        self.velocity = [wish[0] / dt, wish[1] / dt, fall];
        self.step_slide(world, dt, true);
    }
}

fn actor_box(movement: &ActorMove) -> ([f32; 3], [f32; 3]) {
    movement.bounds.unwrap_or((ACTOR_MINS, ACTOR_MAXS))
}

/// What a move hit: the plane's normal and where.
type Hit = Option<([f32; 3], [f32; 3])>;

/// Runs one physics move from `origin` with the horizontal `wish`, on the
/// ground or falling, and records what it hit and stands on.
fn physics_move(
    world: &mut World,
    origin: [f32; 3],
    wish: [f32; 3],
    seconds: f32,
    movement: &mut ActorMove,
) -> ([f32; 3], Hit) {
    let (mins, maxs) = actor_box(movement);
    let mut phys = Phys {
        origin,
        velocity: [0.0, 0.0, movement.fall_speed],
        mins,
        maxs,
        ground: movement.ground_normal,
        hit: None,
    };
    phys.ground_trace(world);
    if phys.ground.is_some() {
        phys.ground_walk(world, wish, seconds);
    } else {
        phys.air_move(world, wish, movement.fall_speed, seconds);
    }
    phys.ground_trace(world);
    settle_physics(&phys, movement);
    (phys.origin, phys.hit)
}

/// `zonly_physics`: the animation's own motion slid through the world,
/// with the ground setting the height.
fn zonly_move(
    world: &mut World,
    origin: [f32; 3],
    delta: [f32; 3],
    seconds: f32,
    movement: &mut ActorMove,
) -> [f32; 3] {
    let (mins, maxs) = actor_box(movement);
    let mut phys = Phys {
        origin,
        velocity: delta.map(|c| c / seconds),
        mins,
        maxs,
        ground: movement.ground_normal,
        hit: None,
    };
    phys.slide(world, seconds, false);
    phys.velocity[2] = 0.0;
    phys.ground_trace(world);
    settle_physics(&phys, movement);
    phys.origin
}

fn settle_physics(phys: &Phys, movement: &mut ActorMove) {
    movement.ground_normal = phys.ground;
    movement.fall_speed = if phys.ground.is_some() {
        0.0
    } else {
        phys.velocity[2]
    };
    movement.ground_ent_type = if phys.ground.is_some() {
        HIT_WORLD
    } else {
        HIT_NONE
    };
    match phys.hit {
        Some((normal, _)) => {
            movement.hit_ent_type = HIT_WORLD;
            let yaw = (-normal[1]).atan2(-normal[0]).to_degrees();
            movement.hit_yaw = if yaw < 0.0 { yaw + 360.0 } else { yaw };
        }
        None => movement.hit_ent_type = HIT_NONE,
    }
}

/// The walk along a path: the physics move, and when it hits something and
/// falls short, up to two shorter tries turned aside along what it hit.
/// Blocked still, the path is planned again from where the actor stands.
fn walk_path(
    world: &mut World,
    origin: [f32; 3],
    wish: [f32; 3],
    seconds: f32,
    movement: &mut ActorMove,
) -> [f32; 3] {
    let (ground, fall) = (movement.ground_normal, movement.fall_speed);
    let (mut result, mut hit) = physics_move(world, origin, wish, seconds, movement);
    let wish_length = (wish[0] * wish[0] + wish[1] * wish[1]).sqrt();
    if hit.is_none() || wish_length <= 0.0 {
        movement.deflect_side = 0;
        return result;
    }
    let mut current = wish;
    let miss = ((result[0] - (origin[0] + wish[0])).powi(2)
        + (result[1] - (origin[1] + wish[1])).powi(2))
    .sqrt();
    if miss / wish_length > DEFLECT_MISS {
        let next = movement.waypoints.first().copied().unwrap_or(origin);
        let dir = [wish[0] / wish_length, wish[1] / wish_length];
        let mut length = wish_length;
        for _ in 0..DEFLECT_TRIES {
            let Some((normal, at)) = hit else {
                break;
            };
            let flat = (normal[0] * normal[0] + normal[1] * normal[1]).sqrt();
            let n = if flat > 0.0 {
                [normal[0] / flat, normal[1] / flat]
            } else {
                [1.0, 0.0]
            };
            length *= DEFLECT_SCALE;
            let left = dir[0] * (next[1] - at[1]) - dir[1] * (next[0] - at[0]) <= 0.0;
            let (side, other) = if left { (16, 8) } else { (8, 16) };
            let turn = if left {
                [-n[1] * length, n[0] * length]
            } else {
                [n[1] * length, -n[0] * length]
            };
            current = if movement.deflect_side == other {
                [
                    current[0] + turn[0] * DEFLECT_BLEND,
                    current[1] + turn[1] * DEFLECT_BLEND,
                    0.0,
                ]
            } else {
                [turn[0], turn[1], 0.0]
            };
            movement.deflect_side = side;
            movement.ground_normal = ground;
            movement.fall_speed = fall;
            (result, hit) = physics_move(world, origin, current, seconds, movement);
            movement.deflected = true;
            if hit.is_none() {
                return result;
            }
        }
    }
    let length = (current[0] * current[0] + current[1] * current[1]).sqrt();
    let gained = if length > 0.0 {
        ((result[0] - origin[0]) * current[0] + (result[1] - origin[1]) * current[1]) / length
    } else {
        0.0
    };
    if gained < length * BLOCKED_PROGRESS {
        // Planned again on the next step; the path stands until then.
        movement.planned_to = None;
    }
    result
}

/// The slope under the actor along its yaw: the ground height a little
/// ahead and behind, within a window above and below its feet.
fn ground_pitch(world: &mut World, origin: [f32; 3], yaw: f32) -> f32 {
    let (sin, cos) = yaw.to_radians().sin_cos();
    let mut height = |offset: f32| {
        let at = [
            origin[0] + cos * offset,
            origin[1] + sin * offset,
            origin[2],
        ];
        let start = [at[0], at[1], at[2] + PITCH_TRACE_WINDOW];
        let end = [at[0], at[1], at[2] - PITCH_TRACE_WINDOW];
        let trace = FrameWorld::from_world(world).trace_clip(
            start,
            end,
            [0.0; 3],
            [0.0; 3],
            MASK_ACTOR_SOLID,
        );
        (trace.allsolid == 0 && trace.fraction < 1.0).then_some(trace.endpos[2])
    };
    match (height(PITCH_SAMPLE_OFFSET), height(-PITCH_SAMPLE_OFFSET)) {
        (Some(front), Some(back)) => (back - front).atan2(2.0 * PITCH_SAMPLE_OFFSET).to_degrees(),
        _ => 0.0,
    }
}

/// A badplace's shape: a cylinder (optionally cut to an arc of yaws) or a
/// brush volume's box.
enum BadPlaceShape {
    Cylinder {
        origin: [f32; 3],
        radius: f32,
        half_height: f32,
        /// Yaws from and to, in degrees; `None` for the whole circle.
        arc: Option<(f32, f32)>,
    },
    Volume {
        mins: [f32; 3],
        maxs: [f32; 3],
    },
}

impl BadPlaceShape {
    fn covers_point(&self, p: [f32; 3]) -> bool {
        match *self {
            Self::Cylinder {
                origin,
                radius,
                half_height,
                arc,
            } => {
                let (dx, dy, dz) = (p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]);
                dx * dx + dy * dy <= radius * radius
                    && dz * dz <= half_height * half_height
                    && in_arc(arc, dx, dy)
            }
            Self::Volume { mins, maxs } => (0..3).all(|i| mins[i] <= p[i] && p[i] <= maxs[i]),
        }
    }

    /// Whether the link from `a` to `b` runs through the badplace.
    fn covers_link(&self, a: [f32; 3], b: [f32; 3]) -> bool {
        if self.covers_point(a) || self.covers_point(b) {
            return true;
        }
        let Self::Cylinder { origin, .. } = *self else {
            return false;
        };
        // The point of the link nearest the axis.
        let d = [b[0] - a[0], b[1] - a[1]];
        let length_sq = d[0] * d[0] + d[1] * d[1];
        if length_sq <= 0.0 {
            return false;
        }
        let t =
            (((origin[0] - a[0]) * d[0] + (origin[1] - a[1]) * d[1]) / length_sq).clamp(0.0, 1.0);
        self.covers_point(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t))
    }
}

fn in_arc(arc: Option<(f32, f32)>, dx: f32, dy: f32) -> bool {
    let Some((from, to)) = arc else {
        return true;
    };
    if dx == 0.0 && dy == 0.0 {
        return true;
    }
    let yaw = dy.atan2(dx).to_degrees().rem_euclid(360.0);
    if from <= to {
        (from..=to).contains(&yaw)
    } else {
        yaw >= from || yaw <= to
    }
}

/// The team bits a badplace's trailing team names name: every team but
/// "free" when none is named.
fn badplace_teams(args: &[Value], from: usize) -> Result<u8, String> {
    let mut teams = 0u8;
    for index in from..args.len() {
        teams |= match string(args, index)?.as_str() {
            "free" => 1 << 0,
            "axis" => 1 << 1,
            "allies" => 1 << 2,
            "neutral" => 1 << 3,
            other => return Err(format!("unknown team '{other}'")),
        };
    }
    Ok(if teams == 0 { 0b1110 } else { teams })
}

/// A badplace's name and when it ends: seconds from now, or never for a
/// duration of 0 or less, which only a named badplace may have.
fn badplace_life(
    world: &mut World,
    args: &[Value],
) -> Result<(Option<String>, Option<i64>), String> {
    let name = string(args, 0)?;
    let seconds = float(args, 1)?;
    let name = (!name.is_empty()).then_some(name);
    if seconds <= 0.0 {
        if name.is_none() {
            return Err("anonymous bad places must have a duration".into());
        }
        return Ok((name, None));
    }
    let now = super::players::now_ms(world);
    Ok((name, Some(now + (seconds * 1000.0) as i64)))
}

fn add_badplace(
    world: &mut World,
    name: Option<String>,
    expires_ms: Option<i64>,
    teams: u8,
    shape: &BadPlaceShape,
) {
    let mut links = Vec::new();
    if let Some(paths) = FrameWorld::from_world(world).actor_paths() {
        let edits = world.get_resource_or_insert_with(LinkEdits::default);
        for (index, node) in paths.nodes().iter().enumerate() {
            let from = index as u16;
            for (to, _, _) in paths.links(from, &edits) {
                if shape.covers_link(node.origin, paths.nodes()[usize::from(to)].origin) {
                    links.push((from, to));
                }
            }
        }
    }
    world
        .get_resource_or_insert_with(LinkEdits::default)
        .add_badplace(BadPlace {
            name,
            expires_ms,
            teams,
            links,
        });
}

/// `badplace_delete(name)`.
fn badplace_delete(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("Incorrect number of parameters\n".into());
    }
    let name = string(args, 0)?;
    if name.is_empty() {
        return Err("badplace_delete called with name \"\"".into());
    }
    world
        .get_resource_or_insert_with(LinkEdits::default)
        .remove_badplace_named(&name);
    Ok(Value::Undefined)
}

/// `badplace_cylinder(name, seconds, origin, radius, height, [teams…])`.
fn badplace_cylinder(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    if args.len() < 5 {
        return Err("Incorrect badplace_cylinder() call.\n".into());
    }
    let (name, expires) = badplace_life(world, args)?;
    let shape = BadPlaceShape::Cylinder {
        origin: vector(args, 2)?,
        radius: float(args, 3)?,
        half_height: float(args, 4)? * 0.5,
        arc: None,
    };
    let teams = badplace_teams(args, 5)?;
    add_badplace(world, name, expires, teams, &shape);
    Ok(Value::Undefined)
}

/// `badplace_arc(name, seconds, origin, radius, height, direction, left,
/// right, [teams…])`: the cylinder's yaws from `left` before the
/// direction's to `right` after it.
fn badplace_arc(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    if args.len() < 8 {
        return Err("Incorrect usage for badplace_arc()\n".into());
    }
    let (name, expires) = badplace_life(world, args)?;
    let direction = vector(args, 5)?;
    let left = float(args, 6)?;
    if left < 0.0 {
        return Err("left angle < 0 in badplace_arc\n".into());
    }
    let right = float(args, 7)?;
    if right < 0.0 {
        return Err("right angle < 0 in badplace_arc\n".into());
    }
    let yaw = direction[1].atan2(direction[0]).to_degrees();
    let arc = (left + right < 360.0).then(|| {
        (
            (yaw - left).rem_euclid(360.0),
            (yaw + right).rem_euclid(360.0),
        )
    });
    let shape = BadPlaceShape::Cylinder {
        origin: vector(args, 2)?,
        radius: float(args, 3)?,
        half_height: float(args, 4)? * 0.5,
        arc,
    };
    let teams = badplace_teams(args, 8)?;
    add_badplace(world, name, expires, teams, &shape);
    Ok(Value::Undefined)
}

/// `badplace_brush(name, seconds, volume, [teams…])`: the links with an end
/// inside the volume entity's bounds.
fn badplace_brush(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("Incorrect badplace_cylinder() call.\n".into());
    }
    let (name, expires) = badplace_life(world, args)?;
    let volume = super::natives::engine::entity_id(world, arg(args, 2)?)?;
    let origin = match world
        .resource_mut::<RoundScript>()
        .object_field(volume, "origin")
    {
        Value::Vector(origin) => origin,
        _ => [0.0; 3],
    };
    let (mins, maxs) = super::triggers::entity_bounds(world, volume);
    let shape = BadPlaceShape::Volume {
        mins: std::array::from_fn(|i| origin[i] + mins[i]),
        maxs: std::array::from_fn(|i| origin[i] + maxs[i]),
    };
    let teams = badplace_teams(args, 3)?;
    add_badplace(world, name, expires, teams, &shape);
    Ok(Value::Undefined)
}

/// `melee([direction])`: the actor's blow, once a frame, by the game's melee
/// rules from its eye towards its enemy; what it struck.
fn melee(world: &mut World, actor: u64, direction: Option<[f32; 3]>) -> Option<u64> {
    use super::natives::t5_turret::sentient_view;
    let now = super::players::now_ms(world);
    let (angles, weapon, enemy, in_melee_charge) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let movement = runtime.actor_moves.entry(actor).or_default();
        if movement.melee_time == Some(now) {
            return None;
        }
        movement.melee_time = Some(now);
        let (_, angles) = pose(&mut runtime, actor);
        let weapon = match runtime.object_field(actor, "weapon") {
            Value::String(name) => Some(name),
            _ => None,
        };
        let enemy = match runtime.object_field(actor, "enemy") {
            Value::Object(enemy) => Some(enemy),
            _ => None,
        };
        let charge =
            matches!(runtime.object_field(actor, "inmeleecharge"), Value::Int(n) if n != 0);
        (angles, weapon, enemy, charge)
    };
    // An unknown weapon is the game's "none".
    let weapon = weapon
        .and_then(|name| FrameWorld::from_world(world).weapon_index_by_script_name(&name))
        .unwrap_or(0);
    let eye = sentient_view(world, actor)?.eye;
    let enemy = enemy.map(|enemy| {
        let eye = sentient_view(world, enemy)
            .filter(|view| view.client.is_none())
            .map(|view| view.eye);
        (enemy, eye)
    });
    let flash_forward =
        super::presence::tag_world(world, actor, "tag_flash").map(|(_, axis)| axis[0]);
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let blow = crate::game_fire::ScriptActorMelee {
        actor,
        eye,
        angles: [angles[0], angles[1], 0.0],
        flash_forward,
        enemy,
        direction,
        weapon,
        in_melee_charge,
    };
    crate::game_fire::actor_melee(&mut FrameWorld::from_world(world), tick, &blow).flatten()
}

/// Where the actor's goal is.
pub(crate) fn goal_point(world: &mut World, actor: u64) -> Option<[f32; 3]> {
    goal_position(world, actor)
}

fn goal_position(world: &mut World, actor: u64) -> Option<[f32; 3]> {
    let movement = world.resource::<RoundScript>().actor_moves.get(&actor)?;
    if let Some(pursuit) = movement.pursuit {
        return Some(pursuit);
    }
    let goal = movement.goal.clone()?;
    match goal {
        Goal::Position(at) => Some(at),
        Goal::Entity(id) => as_vector(super::players::entity_field(world, id, "origin")),
    }
}

/// Sends the actor toward `pursuit` ahead of its goal, or back to its goal.
pub(crate) fn set_pursuit(world: &mut World, actor: u64, pursuit: Option<[f32; 3]>) {
    let (origin, _) = pose(&mut world.resource_mut::<RoundScript>(), actor);
    let paths = FrameWorld::from_world(world).actor_paths();
    let edits = world
        .get_resource::<LinkEdits>()
        .cloned()
        .unwrap_or_else(LinkEdits::default);
    let badplace_teams = actor_badplace_teams(world, actor);
    let Some(mut movement) = world
        .resource::<RoundScript>()
        .actor_moves
        .get(&actor)
        .cloned()
    else {
        return;
    };
    match pursuit {
        Some(goal_at) => {
            let stale = !movement.has_path()
                || movement.pursuit.is_none()
                || movement.planned_to.is_none_or(|planned| {
                    distance_sq(planned, goal_at) > REPLAN_DISTANCE * REPLAN_DISTANCE
                });
            movement.pursuit = Some(goal_at);
            if stale {
                movement.planned_to = Some(goal_at);
                movement.clear_path();
                if horizontal_distance(origin, goal_at) > WAYPOINT_REACHED {
                    let mut reach = |from: [f32; 3], to: [f32; 3]| walkable(world, from, to);
                    if let Some(paths) = paths.as_deref()
                        && let Some(nodes) =
                            paths.plan(origin, goal_at, &edits, badplace_teams, &mut reach)
                    {
                        movement.waypoints = nodes
                            .iter()
                            .map(|&node| paths.nodes[node as usize].origin)
                            .collect();
                        movement.waypoint_nodes = nodes.iter().map(|&node| Some(node)).collect();
                        movement.waypoints.push(goal_at);
                        movement.waypoint_nodes.push(None);
                    }
                }
            }
        }
        None => {
            if movement.pursuit.is_some() {
                movement.pursuit = None;
                movement.planned_to = None;
                movement.clear_path();
            }
        }
    }
    world
        .resource_mut::<RoundScript>()
        .actor_moves
        .insert(actor, movement);
}

/// Where the path node serving `at` stands.
pub(crate) fn nearest_node_origin(world: &mut World, at: [f32; 3]) -> Option<[f32; 3]> {
    let paths = FrameWorld::from_world(world).actor_paths()?;
    let mut reach = |from: [f32; 3], to: [f32; 3]| walkable(world, from, to);
    let node = paths.nearest(at, &mut reach)?;
    Some(paths.nodes[node as usize].origin)
}

/// Angles from `actor` toward the next path node toward its closest enemy,
/// when it is not ignoring everyone and a route exists.
pub(crate) fn likely_enemy_path_angles(world: &mut World, actor: u64) -> Option<[f32; 3]> {
    let (origin, angles) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        if matches!(runtime.object_field(actor, "ignoreall"), Value::Int(n) if n != 0) {
            return None;
        }
        pose(&mut runtime, actor)
    };
    let enemy_at = enemy_position(world, actor)?;
    let paths = FrameWorld::from_world(world).actor_paths()?;
    let mut reach = |from: [f32; 3], to: [f32; 3]| walkable(world, from, to);
    let start = paths.nearest(origin, &mut reach)?;
    let goal = paths.nearest(enemy_at, &mut reach)?;
    let route = paths.route(start, goal, &LinkEdits::default())?;
    let &node = route.get(1).or_else(|| route.first())?;
    let to = paths.nodes[node as usize].origin;
    let delta = [to[0] - origin[0], to[1] - origin[1], to[2] - origin[2]];
    let horiz_sq = delta[0] * delta[0] + delta[1] * delta[1];
    if horiz_sq >= 1.0 {
        let pitch = (-delta[2]).atan2(horiz_sq.sqrt()).to_degrees();
        let yaw = delta[1].atan2(delta[0]).to_degrees();
        Some([
            if pitch < 0.0 { pitch + 360.0 } else { pitch },
            if yaw < 0.0 { yaw + 360.0 } else { yaw },
            0.0,
        ])
    } else {
        Some(angles)
    }
}

fn enemy_position(world: &mut World, actor: u64) -> Option<[f32; 3]> {
    let enemy = match world
        .resource_mut::<RoundScript>()
        .object_field(actor, "favoriteenemy")
    {
        Value::Object(id) => id,
        _ => return None,
    };
    as_vector(super::players::entity_field(world, enemy, "origin"))
}

/// Nothing solid lies between two points at knee height.
fn walkable(world: &mut World, from: [f32; 3], to: [f32; 3]) -> bool {
    let lift = |p: [f32; 3]| [p[0], p[1], p[2] + 18.0];
    let trace = FrameWorld::from_world(world).trace_clip(
        lift(from),
        lift(to),
        [-8.0, -8.0, 0.0],
        [8.0, 8.0, 18.0],
        MASK_ACTOR_SOLID,
    );
    trace.startsolid == 0 && trace.fraction >= 1.0
}

fn yaw_to(from: [f32; 3], to: [f32; 3]) -> f32 {
    (to[1] - from[1]).atan2(to[0] - from[0]).to_degrees()
}

fn horizontal_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Keeps the engine's `enemy` field on the favorite enemy while it lives and
/// the actor does not ignore everyone.
pub(crate) fn choose_enemy(world: &mut World, actor: u64) {
    let mut runtime = world.resource_mut::<RoundScript>();
    let ignoring = matches!(runtime.object_field(actor, "ignoreall"), Value::Int(n) if n != 0);
    let favorite = match runtime.object_field(actor, "favoriteenemy") {
        Value::Object(id) if !ignoring && runtime.live(&id) => Some(id),
        _ => None,
    };
    let enemy = favorite.filter(
        |&id| matches!(super::players::entity_field(world, id, "health"), Value::Int(n) if n > 0),
    );
    world.resource_mut::<RoundScript>().set_object_field(
        actor,
        "enemy",
        enemy.map_or(Value::Undefined, Value::Object),
    );
}

fn as_vector(value: Value) -> Option<[f32; 3]> {
    match value {
        Value::Vector(v) => Some(v),
        _ => None,
    }
}
