#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixerGroup {
    pub parent: i32,
    pub attenuation: f32,
    /// The group's sound category, for a game that reads one.
    pub category: Option<i32>,
}

/// A Black Ops speaker pan, one of the sound driver's named routings for an
/// unplaced sound: its levels to the front, back, center and LFE speakers,
/// and to the left and right.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpeakerPan {
    pub front: f32,
    pub back: f32,
    pub center: f32,
    pub lfe: f32,
    pub left: f32,
    pub right: f32,
}

impl SpeakerPan {
    /// The left and right gains on stereo output, for a mono source and
    /// for a stereo one (each side from its own channel), as Black Ops'
    /// sound driver builds its output matrix: a mono source takes the
    /// front and back levels together, held to 0..1; a stereo source takes
    /// half of them. The center and LFE levels reach only surround output.
    pub fn stereo_gains(self) -> Option<[[f32; 2]; 2]> {
        let levels = [self.front, self.back, self.left, self.right];
        if levels.iter().any(|level| !level.is_finite()) {
            return None;
        }
        let mono = (self.front + self.back).clamp(0.0, 1.0);
        let stereo = (self.front + self.back) * 0.5;
        Some([
            [self.left * mono, self.right * mono],
            [self.left * stereo, self.right * stereo],
        ])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixerGroupError {
    Cycle { group: usize },
    InvalidParent { group: usize, parent: i32 },
    InvalidAttenuation { group: usize },
    NonFiniteGain { group: usize },
}

pub fn compile_mixer_groups(groups: &[MixerGroup]) -> Vec<Result<f32, MixerGroupError>> {
    let mut gains = vec![None; groups.len()];
    let mut active = vec![None; groups.len()];
    let mut path = Vec::new();
    for first in 0..groups.len() {
        if gains[first].is_some() {
            continue;
        }
        let mut current = first;
        let mut result = loop {
            if let Some(gain) = gains[current] {
                break gain;
            }
            if let Some(position) = active[current] {
                break Err(MixerGroupError::Cycle {
                    group: *path[position..].iter().min().unwrap(),
                });
            }
            active[current] = Some(path.len());
            path.push(current);
            let node = groups[current];
            if !node.attenuation.is_finite() || node.attenuation < 0.0 {
                break Err(MixerGroupError::InvalidAttenuation { group: current });
            }
            if node.parent < 0 {
                break Ok(1.0);
            }
            let parent = node.parent as usize;
            if parent >= groups.len() {
                break Err(MixerGroupError::InvalidParent {
                    group: current,
                    parent: node.parent,
                });
            }
            current = parent;
        };
        while let Some(group) = path.pop() {
            active[group] = None;
            result = result.and_then(|parent_gain| {
                let gain = groups[group].attenuation * parent_gain;
                if gain.is_finite() {
                    Ok(gain)
                } else {
                    Err(MixerGroupError::NonFiniteGain { group })
                }
            });
            gains[group] = Some(result);
        }
    }
    gains.into_iter().map(Option::unwrap).collect()
}
