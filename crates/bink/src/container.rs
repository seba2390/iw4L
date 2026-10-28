//! The `.bik` file: its header, audio tracks and frame index.

/// A track's audio is stereo.
const AUDIO_STEREO: u16 = 0x2000;
/// A track's audio is coded with the DCT (otherwise the RDFT).
const AUDIO_DCT: u16 = 0x1000;
/// The video carries an alpha plane.
const VIDEO_ALPHA: u32 = 0x0010_0000;

#[derive(Clone, Debug)]
pub struct AudioTrack {
    pub sample_rate: u32,
    pub channels: u16,
    pub dct: bool,
}

/// One Bink 1 file, held in memory.
pub struct BinkFile {
    data: Vec<u8>,
    /// The format revision, the fourth byte of the signature (`b'i'`).
    pub revision: u8,
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub has_alpha: bool,
    pub audio: Vec<AudioTrack>,
    /// Each frame's byte range; a frame holds each track's audio packet,
    /// then its video packet.
    frames: Vec<(usize, usize)>,
}

fn u32_at(data: &[u8], at: usize) -> Result<u32, String> {
    data.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| format!("bink: truncated at byte {at}"))
}

fn u16_at(data: &[u8], at: usize) -> Result<u16, String> {
    data.get(at..at + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or_else(|| format!("bink: truncated at byte {at}"))
}

impl BinkFile {
    pub fn parse(data: Vec<u8>) -> Result<Self, String> {
        if data.get(..3) != Some(b"BIK") {
            return Err("bink: not a Bink 1 file".into());
        }
        let revision = data[3];
        if !(b'c'..=b'k').contains(&revision) {
            return Err(format!("bink: revision `{}` is not read", revision as char));
        }
        let file_size = (u32_at(&data, 4)? as usize + 8).min(data.len());
        let frame_count = u32_at(&data, 8)? as usize;
        let width = u32_at(&data, 20)?;
        let height = u32_at(&data, 24)?;
        let fps_num = u32_at(&data, 28)?;
        let fps_den = u32_at(&data, 32)?;
        let flags = u32_at(&data, 36)?;
        let tracks = u32_at(&data, 40)? as usize;
        if frame_count == 0 || width == 0 || height == 0 || fps_num == 0 || fps_den == 0 {
            return Err("bink: empty header".into());
        }
        if tracks > 256 || frame_count > 1_000_000 || width > 8192 || height > 8192 {
            return Err("bink: header out of range".into());
        }
        let mut at = 44 + 4 * tracks;
        let mut audio = Vec::with_capacity(tracks);
        for _ in 0..tracks {
            let sample_rate = u32::from(u16_at(&data, at)?);
            let flags = u16_at(&data, at + 2)?;
            audio.push(AudioTrack {
                sample_rate,
                channels: if flags & AUDIO_STEREO != 0 { 2 } else { 1 },
                dct: flags & AUDIO_DCT != 0,
            });
            at += 4;
        }
        at += 4 * tracks;
        let mut frames = Vec::with_capacity(frame_count);
        let mut next = u32_at(&data, at)? as usize & !1;
        for i in 0..frame_count {
            let start = next;
            next = if i + 1 == frame_count {
                file_size
            } else {
                u32_at(&data, at + 4 * (i + 1))? as usize & !1
            };
            if start > next || next > data.len() {
                return Err(format!("bink: frame {i} out of the file"));
            }
            frames.push((start, next));
        }
        Ok(Self {
            data,
            revision,
            width,
            height,
            fps_num,
            fps_den,
            has_alpha: flags & VIDEO_ALPHA != 0,
            audio,
            frames,
        })
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Frames per second.
    pub fn fps(&self) -> f64 {
        f64::from(self.fps_num) / f64::from(self.fps_den)
    }

    /// Frame `index`: each audio track's packet (empty when the frame has
    /// none for it), then the video packet.
    pub fn frame(&self, index: usize) -> Result<(Vec<&[u8]>, &[u8]), String> {
        let &(start, end) = self
            .frames
            .get(index)
            .ok_or_else(|| format!("bink: no frame {index}"))?;
        let mut at = start;
        let mut audio = Vec::with_capacity(self.audio.len());
        for _ in &self.audio {
            let size = u32_at(&self.data, at)? as usize;
            at += 4;
            if at + size > end {
                return Err(format!("bink: frame {index}'s audio runs past it"));
            }
            audio.push(if size >= 4 {
                &self.data[at..at + size]
            } else {
                &[][..]
            });
            at += size;
        }
        Ok((audio, &self.data[at..end]))
    }
}
