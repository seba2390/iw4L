//! A full-screen cinematic before a game's front end: decoded on a worker
//! thread, shown at its own frame rate with its sound, and skipped by any key
//! or mouse button from its first frame.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bevy::prelude::*;

/// Decoded frames the worker may run ahead of the screen.
const FRAMES_AHEAD: usize = 8;

/// The launcher's cinematic: it asks for one with [`Cinematic::play`] and
/// learns it is over from [`Cinematic::take_finished`].
#[derive(Resource, Default)]
pub(crate) struct Cinematic {
    requested: Option<PathBuf>,
    playing: Option<Playing>,
    finished: bool,
}

impl Cinematic {
    pub(crate) fn play(&mut self, path: PathBuf) {
        self.requested = Some(path);
        self.finished = false;
    }

    /// Whether one is asked for or showing.
    pub(crate) fn active(&self) -> bool {
        self.requested.is_some() || self.playing.is_some()
    }

    pub(crate) fn take_finished(&mut self) -> bool {
        std::mem::take(&mut self.finished)
    }
}

struct Header {
    width: u32,
    height: u32,
    fps: f64,
    frames: usize,
    sound: Option<(Arc<[f32]>, u32, u16)>,
}

enum Decoded {
    Header(Header),
    Frame(Vec<u8>),
}

struct Playing {
    /// Behind a lock only so the resource is `Sync`; it is never shared.
    decoded: Mutex<Receiver<Decoded>>,
    stop: Arc<AtomicBool>,
    header: Option<Header>,
    image: Option<Handle<Image>>,
    root: Entity,
    picture: Entity,
    /// When the first frame showed.
    started: Option<Instant>,
    shown: usize,
    /// The first frame of the cinematic takes no skip from the press that
    /// started it.
    armed: bool,
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<Cinematic>()
        .add_systems(Update, (start, show).chain().in_set(frame::ClientSet::Ui));
}

/// Decodes the file: its whole sound first (so it starts with the first
/// frame), then each picture as RGBA.
fn decode(path: PathBuf, tx: std::sync::mpsc::SyncSender<Decoded>, stop: Arc<AtomicBool>) {
    let run = || -> Result<(), String> {
        let data = std::fs::read(&path).map_err(|err| format!("{}: {err}", path.display()))?;
        let file = bink::BinkFile::parse(data)?;
        let sound = match file.audio.first() {
            Some(track) => {
                let mut decoder = bink::AudioDecoder::new(track)?;
                let mut samples = Vec::new();
                for index in 0..file.frame_count() {
                    if let Some(packet) = file.frame(index)?.0.first() {
                        decoder.decode(packet, &mut samples)?;
                    }
                }
                Some((samples.into(), track.sample_rate, track.channels))
            }
            None => None,
        };
        let header = Header {
            width: file.width,
            height: file.height,
            fps: file.fps(),
            frames: file.frame_count(),
            sound,
        };
        if tx.send(Decoded::Header(header)).is_err() {
            return Ok(());
        }
        let mut video = bink::VideoDecoder::new(&file);
        for index in 0..file.frame_count() {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            let picture = video.decode(file.frame(index)?.1)?;
            let mut rgba = Vec::new();
            picture.to_rgba(&mut rgba);
            if tx.send(Decoded::Frame(rgba)).is_err() {
                return Ok(());
            }
        }
        Ok(())
    };
    if let Err(err) = run() {
        diag::warn!(Ui, "cinematic {}: {err}", path.display());
    }
}

fn start(mut cinematic: ResMut<Cinematic>, mut commands: Commands) {
    let Some(path) = cinematic.requested.take() else {
        return;
    };
    diag::info!(Ui, "cinematic: playing {}", path.display());
    let (tx, decoded) = sync_channel(FRAMES_AHEAD);
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    if std::thread::Builder::new()
        .name("cinematic".into())
        .spawn(move || decode(path, tx, worker_stop))
        .is_err()
    {
        cinematic.finished = true;
        return;
    }
    let mut picture = Entity::PLACEHOLDER;
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::BLACK),
            GlobalZIndex(20_000),
        ))
        .with_children(|root| {
            picture = root
                .spawn(Node {
                    max_width: Val::Percent(100.0),
                    max_height: Val::Percent(100.0),
                    ..default()
                })
                .id();
        })
        .id();
    cinematic.playing = Some(Playing {
        decoded: Mutex::new(decoded),
        stop,
        header: None,
        image: None,
        root,
        picture,
        started: None,
        shown: 0,
        armed: false,
    });
}

fn show(
    mut cinematic: ResMut<Cinematic>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut sound: MessageWriter<frame::UiPlayCinematic>,
    mut stop_sound: MessageWriter<frame::UiStopCinematic>,
) {
    let Some(playing) = cinematic.playing.as_mut() else {
        return;
    };
    let skipped = playing.armed
        && (keys.get_just_pressed().next().is_some() || mouse.get_just_pressed().next().is_some());
    playing.armed = true;
    let mut ended = skipped;
    if !ended && playing.header.is_none() {
        match playing
            .decoded
            .get_mut()
            .map_or(Err(TryRecvError::Disconnected), |rx| rx.try_recv())
        {
            Ok(Decoded::Header(header)) => {
                let size = bevy::render::render_resource::Extent3d {
                    width: header.width,
                    height: header.height,
                    depth_or_array_layers: 1,
                };
                let mut image = Image::new_fill(
                    size,
                    bevy::render::render_resource::TextureDimension::D2,
                    &[0, 0, 0, 255],
                    bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                    bevy::asset::RenderAssetUsages::default(),
                );
                image.sampler = bevy::image::ImageSampler::linear();
                let handle = images.add(image);
                commands.entity(playing.picture).insert((
                    ImageNode::new(handle.clone()),
                    Node {
                        width: Val::Percent(100.0),
                        max_height: Val::Percent(100.0),
                        aspect_ratio: Some(header.width as f32 / header.height as f32),
                        ..default()
                    },
                ));
                playing.image = Some(handle);
                playing.header = Some(header);
            }
            Ok(Decoded::Frame(_)) | Err(TryRecvError::Disconnected) => ended = true,
            Err(TryRecvError::Empty) => {}
        }
    }
    if !ended && let Some(header) = &playing.header {
        let due = playing.started.map_or(0, |started| {
            (started.elapsed().as_secs_f64() * header.fps) as usize
        });
        let mut latest = None;
        while playing.shown <= due {
            match playing
                .decoded
                .get_mut()
                .map_or(Err(TryRecvError::Disconnected), |rx| rx.try_recv())
            {
                Ok(Decoded::Frame(rgba)) => {
                    latest = Some(rgba);
                    playing.shown += 1;
                }
                Ok(Decoded::Header(_)) | Err(TryRecvError::Disconnected) => {
                    ended = playing.shown >= header.frames || latest.is_none();
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        if let (Some(rgba), Some(handle)) = (latest, &playing.image)
            && let Some(mut image) = images.get_mut(handle)
        {
            image.data = Some(rgba);
            if playing.started.is_none() {
                playing.started = Some(Instant::now());
                if let Some((samples, rate, channels)) = header.sound.clone() {
                    sound.write(frame::UiPlayCinematic {
                        samples,
                        rate,
                        channels,
                    });
                }
            }
        }
        let length = header.frames as f64 / header.fps;
        if playing
            .started
            .is_some_and(|started| started.elapsed().as_secs_f64() >= length)
        {
            ended = true;
        }
    }
    if !ended {
        return;
    }
    let playing = cinematic.playing.take().expect("checked above");
    playing.stop.store(true, Ordering::Relaxed);
    commands.entity(playing.root).despawn();
    if let Some(handle) = playing.image {
        images.remove(&handle);
    }
    stop_sound.write(frame::UiStopCinematic);
    diag::info!(
        Ui,
        "cinematic: {} after {} frames",
        if skipped { "skipped" } else { "ended" },
        playing.shown
    );
    cinematic.finished = true;
}
