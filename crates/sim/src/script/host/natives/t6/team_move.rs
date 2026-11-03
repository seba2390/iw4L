//! Black Ops 2's team move: each frame an actor with a path looks at the
//! friends around it and either travels, waits for one that is in its way, or
//! slows behind one running the same way. The engine's dodge around a blocker
//! is for actors whose `nododgemove` is off, and every zombies script turns it
//! on (`t6.actors.path_following`).

/// How close another actor counts as touching, and how far one that is in the
/// way is looked at; both squared.
const TOUCH_SQ: f32 = 900.0;
const NEAR_SQ: f32 = 1_406.25;
const MAX_HEIGHT: f32 = 80.0;
const SLOWED_LOOK: f32 = 37.5;
const SLOWED_FAR_SQ: f32 = 3_600.0;
/// A friend counts as moving when it is at least a fifth of the actor's speed.
const MOVING_SHARE_SQ: f32 = 25.0;
/// The share of the actor's speed² a friend ahead must be past it by.
const AHEAD_SHARE: f32 = 0.01;
const SMALL_AMOUNT: f32 = 0.001;
/// How long an actor waits for a friend, in ms.
pub(super) const WAIT_MS: i64 = 500;

/// `0` travel, `1` wait, `2` slow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TeamMove {
    Travel,
    Wait,
    Slow,
}

/// What the team move reads of the actor deciding.
#[derive(Clone, Copy, Debug)]
pub(super) struct Mover {
    pub(super) number: i32,
    pub(super) at: [f32; 3],
    pub(super) velocity: [f32; 2],
    pub(super) look_dir: [f32; 2],
    pub(super) look_dist: f32,
    pub(super) amount: f32,
    /// Its move mode is not "stop".
    pub(super) moving: bool,
    pub(super) nododge: bool,
    pub(super) piled: bool,
    /// How fast the species counts itself when it stands still.
    pub(super) species_speed: f32,
    /// The friend it last slowed behind (and whether that was just now), and
    /// where that friend is.
    pub(super) slowed: Slowed,
    pub(super) slowed_at: Option<[f32; 3]>,
    pub(super) negotiating: bool,
}

/// The friend an actor slows behind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Slowed {
    pub(super) by: Option<u64>,
    pub(super) fresh: bool,
}

/// What the team move reads of another actor on its team.
#[derive(Clone, Copy, Debug)]
pub(super) struct Friend {
    pub(super) id: u64,
    pub(super) number: i32,
    pub(super) at: [f32; 3],
    /// How fast it means to move: nothing while it is piled up.
    pub(super) velocity: [f32; 2],
    /// Its last move ran into a player or actor.
    pub(super) hit_sentient: bool,
    /// Its move mode is "walk", is "run", and its anim mode is `normal`.
    pub(super) walks: bool,
    pub(super) runs: bool,
    pub(super) normal: bool,
}

/// What the team move decided.
#[derive(Clone, Copy, Debug)]
pub(super) struct Outcome {
    pub(super) result: TeamMove,
    /// The actor it now piles up on.
    pub(super) pile: Option<u64>,
    /// It waits for this friend from now on.
    pub(super) wait: bool,
    /// It stands still with no direction to go: its path is over.
    pub(super) drop_path: bool,
    pub(super) slowed: Slowed,
}

fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

/// The team move for an actor that has a path and is not waiting already.
pub(super) fn decide(me: &Mover, friends: &[Friend]) -> Outcome {
    let mut out = Outcome {
        result: TeamMove::Travel,
        pile: None,
        wait: false,
        drop_path: false,
        slowed: me.slowed,
    };
    let speed_sq = dot(me.velocity, me.velocity);
    let (motion, motion_sq) = if speed_sq < 1.0 {
        if me.look_dir == [0.0; 2] {
            if me.negotiating {
                return out;
            }
            out.drop_path = true;
            out.result = TeamMove::Wait;
            return out;
        }
        let speed = me.species_speed;
        (
            [me.look_dir[0] * speed, me.look_dir[1] * speed],
            speed * speed,
        )
    } else {
        (me.velocity, speed_sq)
    };
    let ahead = if me.moving {
        motion_sq * AHEAD_SHARE
    } else {
        0.0
    };
    let reach_sq = TOUCH_SQ.max(motion_sq);
    let small_amount = me.amount <= SMALL_AMOUNT;
    let mut nearest_sq = TOUCH_SQ;
    let mut nearest: Option<&Friend> = None;
    let mut piled = me.piled;
    for friend in friends {
        if (me.at[2] - friend.at[2]).abs() >= MAX_HEIGHT {
            continue;
        }
        let own_reach = if me.nododge {
            dot(friend.velocity, friend.velocity)
        } else {
            0.0
        };
        let delta = [friend.at[0] - me.at[0], friend.at[1] - me.at[1]];
        let dist_sq = dot(delta, delta);
        if reach_sq.max(own_reach) <= dist_sq {
            continue;
        }
        let along = dot(delta, motion) - ahead;
        if along <= 0.0 && dot(me.look_dir, delta) <= 0.0 && !small_amount {
            continue;
        }
        let touching = dist_sq < TOUCH_SQ;
        if touching && !piled && friend.hit_sentient {
            piled = true;
            out.pile = Some(friend.id);
        }
        let friend_sq = dot(friend.velocity, friend.velocity);
        let moving = friend_sq >= 1.0 && friend_sq * MOVING_SHARE_SQ > speed_sq;
        let mut counts = true;
        if moving {
            if TOUCH_SQ <= dist_sq {
                continue;
            }
            if dot(motion, friend.velocity) < 0.0 {
                if speed_sq >= 1.0 || !touching {
                    if me.number <= friend.number {
                        continue;
                    }
                    counts = false;
                }
            } else {
                if ahead + along <= 0.0 {
                    continue;
                }
                if friend.number < me.number && dot(friend.velocity, delta) < ahead {
                    continue;
                }
            }
        }
        if moving && dist_sq < reach_sq && dist_sq < nearest_sq && counts {
            nearest_sq = dist_sq;
            nearest = Some(friend);
        }
    }
    if let Some(friend) = nearest
        && nearest_sq < reach_sq
    {
        if dot(motion, friend.velocity) < 0.0 {
            out.wait = true;
            out.result = TeamMove::Wait;
            return out;
        }
        let walking_clear = friend.walks && nearest_sq >= NEAR_SQ;
        if friend.normal && (friend.runs || walking_clear) {
            out.result = TeamMove::Slow;
        }
    }
    if let Some(friend) =
        nearest.filter(|friend| !small_amount && dot(friend.velocity, friend.velocity) < motion_sq)
    {
        out.slowed = Slowed {
            by: Some(friend.id),
            fresh: true,
        };
        out.result = TeamMove::Travel;
        return out;
    }
    let far = me.slowed_at.is_none_or(|at| {
        let delta = [at[0] - me.at[0], at[1] - me.at[1]];
        dot(delta, delta) >= SLOWED_FAR_SQ
    });
    if !me.slowed.fresh || me.slowed.by.is_none() || me.look_dist < SLOWED_LOOK || far {
        out.slowed.fresh = false;
        if me.slowed.by.is_some() {
            out.slowed.by = None;
            if !me.moving {
                out.result = TeamMove::Wait;
            }
        }
    }
    out
}
