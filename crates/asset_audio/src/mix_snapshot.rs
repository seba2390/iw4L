/// A mix snapshot of a game's sound banks: the volume each snapshot group
/// takes while it is in force, and how fast it comes and goes.
#[derive(Clone, Debug, PartialEq)]
pub struct MixSnapshot {
    pub name: String,
    /// The name's hash, as the game looks snapshots up.
    pub hash: u32,
    /// The snapshot that supplies its second (low-pass) values.
    pub linked: u32,
    /// Fade-in and fade-out times, in seconds.
    pub fade_in: f32,
    pub fade_out: f32,
    /// The distance a sound carrying it applies it over.
    pub distance: f32,
    /// The volume of each snapshot group.
    pub volumes: [f32; 64],
}

/// A game's mix snapshots and how many snapshot groups its sound driver
/// declares.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MixSnapshots {
    pub list: Vec<MixSnapshot>,
    pub groups: Option<usize>,
}

impl MixSnapshots {
    pub fn find(&self, hash: u32) -> Option<&MixSnapshot> {
        self.list.iter().find(|snapshot| snapshot.hash == hash)
    }

    pub(crate) fn absorb(&mut self, other: Self) {
        for snapshot in other.list {
            if self.find(snapshot.hash).is_none() {
                self.list.push(snapshot);
            }
        }
        if self.groups.is_none() {
            self.groups = other.groups;
        }
    }
}

/// A sound context of a game's sound driver: the values it may take. An
/// alias that names a context plays only while the context has its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundContextDef {
    /// The context's name hash.
    pub ty: u32,
    /// The hashes of the values it may take (0 for an unused slot).
    pub values: [u32; 8],
}

/// A game's sound contexts and the values they start the match with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundContexts {
    pub defs: Vec<SoundContextDef>,
    pub initial: Vec<(u32, u32)>,
}
