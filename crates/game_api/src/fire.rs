//! What a game's server does with its players' fire events: the game runs
//! its own shot (spread, bullets, penetration, damage) and the match traces
//! and applies it through [`FireWorld`].

use crate::Rule;
use crate::movement::{GameWeapon, MovePlayer};

/// What a bullet trace ran into, in the game's trace terms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BulletHitType {
    /// Nothing: the bullet reached its end.
    #[default]
    None,
    /// An entity, or the world itself as [`crate::movement::ENTITY_WORLD`].
    Entity(i32),
    /// A breakable glass piece, by number.
    Glass(i32),
}

/// One bullet trace.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BulletTrace {
    pub fraction: f32,
    pub normal: [f32; 3],
    /// The surface's flags, the surface type among them, in the game's bits.
    pub surface_flags: u32,
    /// The contents of what it hit, in the game's bits.
    pub contents: u32,
    pub hit: BulletHitType,
    /// The hit location the trace reports, in the game's numbering (0 none).
    pub hit_location: i32,
    /// The bone hit, as the match numbers it on the entity's model; handed
    /// back with the damage.
    pub bone: Option<u16>,
    pub start_solid: bool,
    pub all_solid: bool,
}

/// A team, as the match keeps one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireTeam {
    Axis,
    Allies,
    Neutral,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireEntityKind {
    Player,
    Actor,
    Other,
}

/// An entity a shot or blast reaches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FireEntity {
    pub kind: FireEntityKind,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    /// What it is solid to, in the game's bits.
    pub contents: u32,
    /// Its linked bounds in the world, as the engine links it (sentients
    /// are linked one unit wider); `None` when it is not linked by bounds.
    pub bounds: Option<([f32; 3], [f32; 3])>,
    /// A brush model's own bounds, in its model space.
    pub model_bounds: Option<([f32; 3], [f32; 3])>,
    /// An actor's eye.
    pub eye: Option<[f32; 3]>,
    /// A script model: a blast reaches it by its model's bounds.
    pub script_model: bool,
    /// A dead player's or actor's body.
    pub corpse: bool,
    /// It has its own blast damage rule (a destructible).
    pub blast_handler: bool,
    pub takes_damage: bool,
    pub health: i32,
    pub max_health: i32,
    pub team: Option<FireTeam>,
    /// An actor's `ignorelocationaldamage`.
    pub ignore_locational_damage: bool,
    /// An actor's `allowdeath`.
    pub allow_death: bool,
    /// An actor whose AI states hold off a lethal hit while it may not die.
    pub delays_death: bool,
    /// God mode: no damage at all.
    pub god: bool,
    /// Demigod: damage never takes the last point of health.
    pub demigod: bool,
    /// An actor's `type`, when it is one the game names.
    pub ai_type: Option<&'static str>,
}

/// Damage a shot does to one entity, as the game settled it.
#[derive(Clone, Debug, PartialEq)]
pub struct FireDamage {
    pub target: i32,
    pub inflictor: i32,
    pub attacker: i32,
    pub dir: [f32; 3],
    pub point: [f32; 3],
    pub amount: i32,
    pub flags: i32,
    /// The means of death, by its script name.
    pub means: &'static str,
    pub weapon: u32,
    /// The hit location, by its script name and its number.
    pub hit_location: &'static str,
    pub location: i32,
    pub bone: Option<u16>,
    /// How far behind the match the hit was settled, in milliseconds: the
    /// damage callbacks' time offset.
    pub time_offset: i32,
}

/// A bullet's mark on what it hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BulletImpact {
    pub attacker: i32,
    pub weapon: u32,
    pub start: [f32; 3],
    pub point: [f32; 3],
    pub normal: [f32; 3],
    pub surface_type: u8,
    pub surface_flags: u32,
    /// The entity hit; [`crate::movement::ENTITY_NONE`] for none.
    pub entity: i32,
    /// The game's impact flags (a head hit, a kill, an exit).
    pub flags: u8,
    /// Where the bullet left a surface it went through.
    pub exit: bool,
    /// The game's server sends this impact; otherwise its client draws it.
    pub from_server: bool,
}

/// A bullet's first trace, from where it was fired to where it stopped (its
/// end when it hit nothing): the path the game's client draws its beam
/// along.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BulletPath {
    /// The entity that fired it.
    pub shooter: i32,
    pub weapon: u32,
    /// The pellet of a spread shot, from 0.
    pub pellet: u16,
    /// The hand that fired it: 1 the left.
    pub hand: u8,
    pub start: [f32; 3],
    pub end: [f32; 3],
}

/// The match around a shot.
pub trait FireWorld {
    /// The match's time, in milliseconds.
    fn level_time(&self) -> i32;

    /// A dvar's value; `None` when the match does not keep it.
    fn dvar(&self, name: &str) -> Option<String>;

    /// The server's shared random draw, 0 to 32767: the C library's
    /// generator over the match's state.
    fn rand(&mut self) -> i32;

    fn weapon(&self, weapon: u32) -> Option<&GameWeapon>;

    /// Traces a bullet from `start` to `end`, passing through `pass_entity`.
    fn bullet_trace(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        pass_entity: i32,
        mask: u32,
    ) -> BulletTrace;

    /// [`Self::bullet_trace`] of a box from `mins` to `maxs` around the path.
    fn box_trace(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        pass_entity: i32,
        mask: u32,
    ) -> BulletTrace {
        let _ = (mins, maxs);
        self.bullet_trace(start, end, pass_entity, mask)
    }

    /// [`Self::bullet_trace`] where each body's part is chosen by `priority`,
    /// one entry per hit location, as the game's locational trace does.
    fn locational_trace(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        pass_entity: i32,
        mask: u32,
        priority: &[u8],
    ) -> BulletTrace {
        let _ = priority;
        self.bullet_trace(start, end, pass_entity, mask)
    }

    /// Later traces see the bodies as the game's antilag leaves them:
    /// players `players_ms` and actors `actors_ms` behind the match, 0
    /// where it moves none back.
    fn antilag(&mut self, players_ms: i32, actors_ms: i32) {
        let _ = (players_ms, actors_ms);
    }

    /// Runs a missile the game just spawned for one frame now, at the
    /// match's time.
    fn run_missile_now(&mut self, missile: i32) {
        let _ = missile;
    }

    fn entity(&mut self, entity: i32) -> Option<FireEntity>;

    /// How deep a bullet of `penetrate_type` goes into `surface_type`, from
    /// the game's penetration table; `None` when the match has no table.
    fn penetration_depth(&self, penetrate_type: i32, surface_type: i32) -> Option<f32>;

    /// The damage multiplier at a hit location, from the game's table; `None`
    /// when the match has no table.
    fn hit_location_multiplier(&self, hit_location: i32) -> Option<f32>;

    /// Hands damage to the entity: an actor's damage callback, a player's,
    /// or the entity's own health and notifies.
    fn damage(&mut self, damage: &FireDamage);

    /// Damage triggers the bullet's path crosses.
    fn damage_triggers(
        &mut self,
        attacker: i32,
        start: [f32; 3],
        end: [f32; 3],
        amount: i32,
        means: &'static str,
    );

    fn impact(&mut self, impact: &BulletImpact);

    /// A bullet's path, for a game whose client draws its bullets' beams
    /// ([`ClientFireRules::beams`]).
    fn bullet_path(&mut self, path: &BulletPath) {
        let _ = path;
    }

    /// The entities whose bounds touch the box.
    fn entities_in_box(&mut self, mins: [f32; 3], maxs: [f32; 3]) -> Vec<i32>;

    /// True when a sight trace from `start` to `end` through what `mask`
    /// names reaches it, passing through the two entities.
    fn sight_clear(&mut self, start: [f32; 3], end: [f32; 3], pass: [i32; 2], mask: u32) -> bool;

    /// A player, as its movement keeps it.
    fn player(&self, entity: i32) -> Option<MovePlayer>;

    /// A script asked that script blasts pass players by.
    fn players_ignore_radius_damage(&self) -> bool;

    /// Where the moving platform the player numbered `player` stands on was
    /// at level time `time`; `None` when they stand on no platform the match
    /// keeps a history of.
    fn platform_at(&mut self, _player: i32, _time: i32) -> Option<[f32; 3]> {
        None
    }

    /// A script turned off grenade suicide: a grenade held past its fuse
    /// falls at the thrower's feet and hurts them instead of killing them
    /// outright.
    fn grenade_suicide_disabled(&self) -> bool;

    /// Puts a game missile in the match; returns its entity number, or
    /// `None` when the match has no room for it.
    fn spawn_missile(&mut self, missile: GameMissile) -> Option<i32>;

    /// Adds to a player's velocity.
    fn push_player(&mut self, entity: i32, velocity: [f32; 3]);

    /// Stores a player as its movement keeps it, after the game changed it.
    fn store_player(&mut self, entity: i32, player: &MovePlayer);

    /// A missile's explosion, as the clients show it.
    fn explosion(&mut self, explosion: &MissileExplosion);

    /// A grenade's bounce sound off a surface type, as clients play it.
    fn bounce(&mut self, missile: i32, surface_type: u8);

    /// The match's time one frame ago.
    fn previous_level_time(&self) -> i32;

    /// Notifies `name` on an entity with these arguments.
    fn notify(&mut self, entity: i32, name: &str, args: &[NotifyArg]);

    /// Records on an actor that a lethal hit was held off.
    fn set_delayed_death(&mut self, _entity: i32) {}

    /// The map's centre a script set; missiles too far from it are gone.
    fn map_center(&self) -> [f32; 3];

    /// The contents at a point, of the world and the entities there.
    fn point_contents(&mut self, point: [f32; 3], mask: u32) -> u32;

    /// A player's weapon event, sent to every client: the player's entity,
    /// the weapon in its hand when the event is sent.
    fn weapon_event(&mut self, entity: i32, weapon: u32, event: WeaponEvent);

    /// Updates an entity's script origin and angles before a notify wakes
    /// threads that read them.
    fn set_entity_pose(&mut self, _entity: i32, _origin: [f32; 3], _angles: [f32; 3]) {}

    /// Broadcasts an AI perception event to actors and script AI event
    /// listeners (`addAIEventListener`).
    fn ai_event(&mut self, _event: AiEvent) {}

    /// Applies a blast's world effects (dynamic entity damage and impulse,
    /// destructible pieces, glass, and client physics explosion event).
    fn blast_effects(&mut self, _blast: &RadiusDamage) {}
}

/// An AI perception event a shot, missile, grenade or explosion broadcasts
/// to actors and script AI event listeners (`addAIEventListener`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AiEvent {
    /// A point event at `origin` from `originator`, for teams in `teams`
    /// (`None` = the originator's opposing team).
    Point {
        originator: i32,
        kind: AiPointEvent,
        teams: Option<u8>,
        origin: [f32; 3],
    },
    /// A line event from `start` to `end` from `originator`, for teams in
    /// `teams` (`None` = the originator's opposing team).
    Line {
        originator: i32,
        hit: i32,
        kind: AiLineEvent,
        teams: Option<u8>,
        start: [f32; 3],
        end: [f32; 3],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiPointEvent {
    /// Event 8: `explosion` (`ai_eventDistExplosion` 1024).
    Explosion,
    /// Event 9: `grenade_ping` (`ai_eventDistGrenadePing` 512, `"grenade danger"`).
    GrenadePing { no_ping: bool },
    /// Event 10: `projectile_ping` (`ai_eventDistProjPing` 128, `"grenade danger"`).
    ProjectilePing { no_ping: bool },
    /// Event 11: `gunshot` (`ai_eventDistGunShot` 2048).
    Gunshot,
    /// Event 12: `silenced_shot` (`ai_eventDistSilencedShot` 128).
    SilencedShot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiLineEvent {
    /// Event 15: `bullet` (`ai_eventDistBullet` 96, z 45, `"bulletwhizby"`).
    Bullet,
    /// Event 18: `projectile_impact` (`ai_eventDistProjImpact` 256, z 45, `"projectile_impact"`).
    ProjectileImpact,
}

/// How a game launches, moves and detonates its missiles, even while a mode
/// borrows the simulation's weapon state machine for the throw itself.
pub trait MissilePhysics: Sync {
    /// Launches a grenade from `origin` aimed along `angles` (`launcher` when
    /// fired from a grenade launcher, `remaining_fuse` when cooked in hand);
    /// returns the entity number `FireWorld::spawn_missile` gave it.
    #[allow(clippy::too_many_arguments)]
    fn launch_grenade(
        &self,
        owner: i32,
        weapon: u32,
        origin: [f32; 3],
        angles: [f32; 3],
        launcher: bool,
        remaining_fuse: Option<i32>,
        world: &mut dyn FireWorld,
    ) -> (Option<i32>, Vec<&'static crate::Unknown>);

    /// Spawns a grenade at `origin` with an explicit initial `velocity` and
    /// `fuse` in milliseconds (`MagicGrenadeType`).
    fn spawn_grenade(
        &self,
        owner: i32,
        weapon: u32,
        origin: [f32; 3],
        velocity: [f32; 3],
        fuse: i32,
        world: &mut dyn FireWorld,
    ) -> (Option<i32>, Vec<&'static crate::Unknown>);

    /// Launches a projectile weapon's rocket from `origin` aimed along
    /// `angles` with `spread` degrees and the shooter's `velocity`.
    #[allow(clippy::too_many_arguments)]
    fn launch_rocket(
        &self,
        owner: i32,
        weapon: u32,
        origin: [f32; 3],
        angles: [f32; 3],
        spread: f32,
        velocity: [f32; 3],
        world: &mut dyn FireWorld,
    ) -> (Option<i32>, Vec<&'static crate::Unknown>);

    /// Runs one frame of a missile.
    fn run(
        &self,
        missile: &mut GameMissile,
        world: &mut dyn FireWorld,
    ) -> (MissileRun, Vec<&'static crate::Unknown>);

    /// Detonates a missile (`detonate`), with an optional new owner.
    fn detonate(
        &self,
        missile: &mut GameMissile,
        attacker: Option<i32>,
        world: &mut dyn FireWorld,
    ) -> (MissileRun, Vec<&'static crate::Unknown>);

    /// Marks a grenade a dud (`makegrenadedud`).
    fn make_dud(&self, missile: &mut GameMissile);

    /// Whether a missile is a dud, as its state tells clients.
    fn is_dud(&self, _missile: &GameMissile) -> bool {
        false
    }

    /// Resets a missile's detonation time (`resetmissiledetonationtime`):
    /// `after_ms` from now when given, else its weapon's fuse time when it
    /// has timed detonation.
    fn reset_detonation_time(
        &self,
        missile: &mut GameMissile,
        after_ms: Option<i32>,
        world: &dyn FireWorld,
    );

    /// The entity number a resting missile sits on (`getgroundent`), or
    /// `ENTITY_NONE` while flying.
    fn ground_entity(&self, missile: &GameMissile) -> i32 {
        let _ = missile;
        crate::movement::ENTITY_NONE
    }
}

#[derive(Clone, Copy)]
pub struct MissileRules(pub &'static dyn MissilePhysics);

impl core::fmt::Debug for MissileRules {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("MissileRules")
    }
}

impl PartialEq for MissileRules {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::addr_eq(self.0, other.0)
    }
}

impl Eq for MissileRules {}

/// A player's weapon event the clients present with the weapon's sounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WeaponEvent {
    /// A shot, from the left hand or the right; the clip's last round.
    Fire {
        left: bool,
        last_shot: bool,
    },
    /// A shot carrying its burst/charge shot parameter (`eventParm`).
    FireBurst {
        left: bool,
        last_shot: bool,
        parm: i32,
    },
    NoAmmo,
    EjectBrass,
    MeleeSwipe,
    MeleeHit {
        point: [f32; 3],
        knife: bool,
    },
    MeleeMiss {
        point: [f32; 3],
        knife: bool,
    },
    MeleeBlood {
        point: [f32; 3],
    },
    Reload,
    ReloadFromEmpty,
    ReloadStart,
    ReloadEnd,
    Raise,
    FirstRaise,
    Putaway,
    /// The switch to the weapon's alternate mode.
    Alt,
    /// A grenade's pin is pulled.
    Pullback,
    Rechamber,
}

/// What a client does when a player fires, by the game's own rules.
#[derive(Clone, Copy, Debug)]
pub struct ClientFireRules {
    /// The fire sound a shot plays, from the weapon's definition and
    /// variant in the game's layout; `None` when the shot plays none.
    pub sound: fn(def: &[u8], variant: &[u8], shot: &FireSoundShot) -> Rule<Option<FireSound>>,
    /// The muzzle flash, shell and tracer a shot shows.
    pub effects: Rule<()>,
    /// A left-hand shot sounds the left-hand weapon of the weapon the
    /// client's own player holds, not the shooter's.
    pub left_from_own_weapon: bool,
    /// The game's automatic fire simulation, when it plays some shots'
    /// sounds through one.
    pub autosim: Option<FireAutoSimRules>,
    /// The game's view kick, when the client runs it.
    pub view_kick: Option<ViewKickRules>,
    /// The beams the game's client draws along its bullets' paths, in place
    /// of the weapons' tracer definitions.
    pub beams: Option<BulletBeamRules>,
    /// The sound the game's client plays where a bullet strikes, when it
    /// plays its own (the impact effects' sounds otherwise).
    pub impact_sound: Option<ImpactSoundFn>,
    /// The whizby and crack sounds a bullet passing the view plays.
    pub whizby: Option<WhizbyFn>,
}

/// The weapon fields [`ImpactSoundFn`] and [`WhizbyFn`] read.
pub struct ImpactSoundWeapon<'a> {
    pub def: &'a [u8],
    pub variant: &'a [u8],
    pub explosion_sounds: &'a [Option<String>; 3],
    pub bounce_sounds: Option<&'a [Option<String>; 31]>,
    pub whizby_sound: Option<&'a str>,
    pub crack_sound: Option<&'a str>,
}

/// What a bullet passing the view plays, and where along its segment.
#[derive(Clone, Debug, PartialEq)]
pub struct WhizbySound {
    pub origin: [f32; 3],
    pub alias: String,
    pub crack_alias: Option<String>,
}

/// The alias a bullet's impact plays, from the weapon's definition and
/// variant in the game's layout, the surface struck (by the game's
/// numbers) and whether it is the exit side; `None` plays none.
pub type ImpactSoundFn =
    fn(weapon: &ImpactSoundWeapon<'_>, surface: u8, exit: bool) -> Option<String>;

/// The whizby sound a bullet from `start` to `end` plays when passing
/// `view_origin`.
pub type WhizbyFn = fn(
    weapon: &ImpactSoundWeapon<'_>,
    start: [f32; 3],
    end: [f32; 3],
    view_origin: [f32; 3],
) -> Option<WhizbySound>;

/// A shooter as a game's beam rule reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BeamShooter {
    pub kind: FireEntityKind,
    pub team: Option<FireTeam>,
}

/// A beam a bullet draws along its path, from the shooter's flash point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BulletBeam {
    /// The beam's material, by name.
    pub material: &'static str,
    /// How fast its head moves along the path, in units per second.
    pub speed: f32,
    /// Its longest drawn length.
    pub length: f32,
    pub width: f32,
    /// The length of one turn of its screw, and its radius.
    pub screw_dist: f32,
    pub screw_radius: f32,
}

/// The beams a game's client draws along its bullets.
#[derive(Clone, Copy, Debug)]
pub struct BulletBeamRules {
    /// The beam a bullet of the weapon (its definition and variant in the
    /// game's layout) draws; `shots` is the client's count of the shooter's
    /// bullets, which the rule advances. `None` draws none.
    pub beam:
        fn(def: &[u8], variant: &[u8], shooter: BeamShooter, shots: &mut u8) -> Option<BulletBeam>,
    /// How many milliseconds before the client's frame a beam starts, from
    /// the frame's length and the client's `rand()` (0 to 32767).
    pub start_back: fn(frametime_ms: i32, rand: &mut dyn FnMut() -> i32) -> i32,
}

/// How far a client's own shots have kicked its view and its gun.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewKick {
    /// The view's kick (pitch, yaw, roll), in degrees; it turns the angles
    /// the client's commands aim by.
    pub angles: [f32; 3],
    /// How fast the view's kick moves, in degrees per second.
    pub velocity: [f32; 3],
    /// The gun's kick (pitch, yaw), in degrees.
    pub gun_angles: [f32; 2],
    /// How fast the gun's kick moves (pitch, yaw), in degrees per second.
    pub gun_velocity: [f32; 2],
    /// What the kick adds to the viewmodel's angles (pitch, yaw, roll).
    pub viewmodel_angles: [f32; 3],
    /// The view weapon the gun's kick belongs to.
    pub weapon: u32,
}

/// A weapon as the view kick reads it: its definition and variant in the
/// game's layout, and a dual-wield weapon's other-hand weapon.
pub struct KickWeapon {
    pub def: Box<[u8]>,
    pub variant: Box<[u8]>,
    pub dual_wield: Option<u32>,
}

/// The client's weapons, by number.
pub type KickWeapons<'a> = &'a dyn Fn(u32) -> Option<KickWeapon>;

/// A game's view kick, run by the client for its own player.
#[derive(Clone, Copy, Debug)]
pub struct ViewKickRules {
    /// A shot of the client's own player kicks its view and its gun; `rand`
    /// draws from 0 to 1.
    pub fire: fn(&MovePlayer, KickWeapons<'_>, rand: &mut dyn FnMut() -> f32, &mut ViewKick),
    /// A client frame of `frametime_ms` eases the view's kick back.
    pub frame: fn(&MovePlayer, KickWeapons<'_>, frametime_ms: i32, &mut ViewKick),
    /// What the kick adds to the client's aim angles.
    pub aim: fn(&MovePlayer, KickWeapons<'_>, &ViewKick) -> [f32; 3],
}

/// A game's automatic fire simulation: shots it takes are not played at
/// once; the simulation replays the shooter's fire sound at the weapon's
/// fire rate while the shooter keeps firing.
#[derive(Clone, Copy, Debug)]
pub struct FireAutoSimRules {
    /// How the simulation takes a shot, from the weapon's definition and
    /// variant in the game's layout; `None` plays the shot at once.
    pub simulated: fn(def: &[u8], variant: &[u8], shot: &FireSoundShot) -> Option<AutoFire>,
    /// A client's simulation.
    pub new: fn() -> Box<dyn FireAutoSim>,
}

/// What the simulation needs of a shot's weapon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoFire {
    /// The weapon's time between shots, in msec.
    pub fire_time_ms: i32,
    /// The weapon fires bursts.
    pub burst: bool,
}

/// A shot the simulation takes and, later, plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutoFireShot {
    /// The shooter's entity number.
    pub entity: i32,
    pub weapon: u32,
    /// The tag it fired from (a hand, a barrel), as the client numbers them.
    pub tag: u32,
    pub origin: [f32; 3],
    /// The shooter is a player: its shots stop once it stops firing.
    pub player: bool,
    pub last_shot: bool,
    pub fire: AutoFire,
}

/// One client's automatic fire simulation.
pub trait FireAutoSim: Send + Sync {
    /// Takes a shot the client did not play; gives it back when the
    /// simulation cannot take it and it plays at once.
    fn queue(&mut self, shot: AutoFireShot) -> Option<AutoFireShot>;
    /// Runs one client frame of `frame_ms` msec, the view at `view_origin`;
    /// `entity_flags` answers a player entity's flags as the client has them
    /// (0 for one it does not have). Gives the shots whose sound plays this
    /// frame.
    fn update(
        &mut self,
        frame_ms: i32,
        view_origin: [f32; 3],
        entity_flags: &dyn Fn(i32) -> u32,
    ) -> Vec<AutoFireShot>;
}

/// A shot as the client that plays its sound sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FireSoundShot {
    /// The client views the shooter in first person.
    pub player_view: bool,
    pub left: bool,
    pub last_shot: bool,
    /// The client is watching a killcam.
    pub killcam: bool,
    /// The weapon names a last shot sound for this view.
    pub last_shot_sound: bool,
    /// The fire event's shot parameter (`eventParm`).
    pub parm: i32,
}

/// Which of a weapon's fire sounds a shot plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireSound {
    Fire,
    FirePlayer,
    LastShot,
    LastShotPlayer,
    Killcam,
    KillcamPlayer,
    FireWithStart,
    FirePlayerWithStart,
}

/// A blast: damage falling off with distance from its origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadiusDamage {
    pub origin: [f32; 3],
    pub inflictor: i32,
    pub attacker: i32,
    /// Damage at the origin and at the edge.
    pub inner: f32,
    pub outer: f32,
    pub radius: f32,
    /// Only what lies within this cone (cosine, direction).
    pub cone: Option<(f32, [f32; 3])>,
    /// The entity the blast passes by.
    pub ignore: i32,
    /// The means of death, by its script name.
    pub means: &'static str,
    pub weapon: u32,
    /// The blast comes from a script's RadiusDamage.
    pub script: bool,
}

/// An actor's melee blow (its `melee` builtin), as the actor stands when it
/// strikes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActorMelee {
    /// The actor, by entity number.
    pub attacker: i32,
    /// Its eye.
    pub eye: [f32; 3],
    /// Its facing (pitch, yaw, roll).
    pub angles: [f32; 3],
    /// The forward axis of its weapon's flash tag, when its model has one.
    pub flash_forward: Option<[f32; 3]>,
    /// Its enemy, by entity number, with an actor enemy's eye; a player's
    /// comes from its movement.
    pub enemy: Option<(i32, Option<[f32; 3]>)>,
    /// The direction the script passed.
    pub direction: Option<[f32; 3]>,
    /// Its weapon.
    pub weapon: u32,
    /// It is in a melee charge (`inmeleecharge`).
    pub in_melee_charge: bool,
}

/// How a trajectory moves.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TrajectoryKind {
    #[default]
    Stationary,
    Linear,
    /// Falling at this many units per second squared.
    Gravity(f32),
}

/// A position (or angles) over time: from `base` at `time`, moving by
/// `delta` per second as `kind` says.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameTrajectory {
    pub kind: TrajectoryKind,
    pub time: i32,
    pub duration: i32,
    pub base: [f32; 3],
    pub delta: [f32; 3],
}

/// Bytes a game keeps per missile beyond the shared fields.
pub const GAME_MISSILE_BYTES: usize = 64;

/// A missile the game's rules move and explode; the match keeps it, shows
/// it and runs it every frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameMissile {
    /// Its entity number; set by the match when it spawns.
    pub entity: i32,
    /// The player that fired it.
    pub owner: i32,
    pub weapon: u32,
    pub position: GameTrajectory,
    pub angles: GameTrajectory,
    /// Where it is now.
    pub origin: [f32; 3],
    /// Clients draw it from this time on.
    pub visible_at: i32,
    pub game: [u8; GAME_MISSILE_BYTES],
}

/// What a missile's frame did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MissileRun {
    Flying,
    /// It exploded here (the match tells scripts) and is gone.
    Exploded([f32; 3]),
    /// It is gone without an explosion scripts hear of.
    Gone,
    /// It came to rest where it struck and stays this frame for the scripts
    /// it told (a retrievable bolt); the match removes it after.
    Stationary,
}

/// A missile's explosion effect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MissileExplosion {
    pub missile: i32,
    pub owner: i32,
    pub weapon: u32,
    pub origin: [f32; 3],
    pub normal: [f32; 3],
    pub surface_type: u8,
}

/// An argument a game's notify passes to scripts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NotifyArg {
    /// A weapon, by its script name.
    Weapon(u32),
    Entity(i32),
    Vector([f32; 3]),
    Float(f32),
    Int(i32),
    String(&'static str),
    /// The name of a bone of an entity's model, by the number the match
    /// reported it under; empty when it has none.
    Bone {
        entity: i32,
        bone: u16,
    },
    /// No value (`undefined`).
    Undefined,
}
