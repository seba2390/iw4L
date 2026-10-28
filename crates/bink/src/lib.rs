//! Bink 1 (`.bik`, revisions c to k): the container, video planes and DCT
//! audio, decoded to planar YUV and interleaved float samples.

mod audio;
mod bits;
mod container;
mod tables;
mod video;

pub use audio::AudioDecoder;
pub use container::{AudioTrack, BinkFile};
pub use video::{Picture, VideoDecoder};
