//! Black Ops 2's full-screen pieces its client scripts start, run each frame
//! as its client game's 2D draw runs them.
//!
//! The gump hide fade (`sethidegumpalpha`): the call starts the fade at the
//! client's time with alpha 1. Each frame, measured from that start: while
//! a gump zone loads the fade holds (at most 5000 msec); once 250 msec have
//! passed with none loading it restarts as the fade out, which holds alpha 1
//! for 500 msec and then falls to 0 over 250 msec. The fade is drawn as the
//! `white` material in its colour and alpha over the whole viewport (640x480
//! with both alignments 4), before the HUD elements. In theater playback the
//! engine holds 1000 msec and fades 500 msec on the real-time clock; IW4L's
//! Black Ops 2 has no theater playback.

/// How long a fade out holds alpha 1, in msec.
const HOLD_MS: f32 = 500.0;
/// How long a fade out takes to reach 0 after its hold, in msec.
const FADE_MS: f32 = 250.0;
/// How long a started fade waits before it fades out, in msec.
const START_MS: f32 = 250.0;
/// How long a fade waits on a loading gump at most, in msec.
const LOAD_WAIT_MS: f32 = 5000.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum HideState {
    #[default]
    Off,
    Started,
    WaitingOnLoad,
    FadingOut,
}

/// The gump hide fade as the client keeps it between frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HideFade {
    state: HideState,
    start_ms: i32,
    alpha: f32,
    color: [f32; 3],
}

impl HideFade {
    /// A `sethidegumpalpha` at the client's time `now_ms`, in `color`.
    pub fn start(&mut self, now_ms: i32, color: [f32; 3]) {
        self.state = HideState::Started;
        self.alpha = 1.0;
        self.start_ms = now_ms;
        self.color = color;
    }

    /// Whether the fade is running.
    #[must_use]
    pub fn active(&self) -> bool {
        self.state != HideState::Off
    }

    /// Runs the fade's frame at the client's time `now_ms`, `loading` while
    /// a gump zone loads, and gives the colour and alpha to draw over the
    /// viewport, if any.
    pub fn frame(&mut self, now_ms: i32, loading: bool) -> Option<[f32; 4]> {
        if self.state == HideState::Off {
            return None;
        }
        // Every step measures from the start the frame began with.
        let elapsed = now_ms.wrapping_sub(self.start_ms) as f32;
        if self.state == HideState::Started && (loading || elapsed > START_MS) {
            self.state = if loading {
                HideState::WaitingOnLoad
            } else {
                HideState::FadingOut
            };
            self.alpha = 1.0;
            self.start_ms = now_ms;
        }
        if self.state == HideState::WaitingOnLoad && (!loading || elapsed > LOAD_WAIT_MS) {
            self.alpha = 1.0;
            self.start_ms = now_ms;
            self.state = HideState::FadingOut;
        }
        if self.state == HideState::FadingOut {
            if loading {
                self.state = HideState::WaitingOnLoad;
                self.start_ms = now_ms;
                self.alpha = 1.0;
            } else if elapsed > HOLD_MS + FADE_MS {
                self.state = HideState::Off;
                self.alpha = 0.0;
            } else if elapsed > HOLD_MS {
                self.alpha = 1.0 - (elapsed - HOLD_MS) / FADE_MS;
            } else {
                self.alpha = 1.0;
            }
        }
        if self.alpha <= 0.0 {
            self.state = HideState::Off;
            return None;
        }
        let [r, g, b] = self.color;
        Some([r, g, b, self.alpha])
    }
}
