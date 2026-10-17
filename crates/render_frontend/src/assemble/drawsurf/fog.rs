use asset_world::{ExpFog, SunFog};
use bevy::prelude::*;

#[derive(Resource)]
pub struct FogDvars {
    pub enabled: bool,
    pub zfar: f32,
}
impl Default for FogDvars {
    fn default() -> Self {
        Self {
            enabled: true,
            zfar: 0.0,
        }
    }
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct MapFrameFog {
    pub fog: ExpFog,
    pub sliders: ExpFog,
    pub slider_sun: SunFog,
    pub readonly_sun: SunFog,
    pub art_tweak: bool,
    pub script_disabled: bool,
    previous: ExpFog,
    start_ms: i32,
    duration_ms: i32,
}
impl MapFrameFog {
    pub fn new(fog: ExpFog) -> Self {
        let sun = fog.sun.unwrap_or(SunFog {
            color_rgb: [1.0, 0.0, 0.0],
            sun_dir: [1.0, 0.0, 0.0],
            begin_angle_deg: 0.0,
            end_angle_deg: 180.0,
            scale: 1.0,
        });
        Self {
            fog,
            sliders: fog,
            slider_sun: sun,
            readonly_sun: sun,
            art_tweak: false,
            script_disabled: false,
            previous: fog,
            start_ms: 0,
            duration_ms: 0,
        }
    }

    pub fn set(&mut self, fog: ExpFog, time_ms: i32) {
        let current = self.sample(time_ms);
        self.previous = current;
        self.start_ms = time_ms;

        self.duration_ms = if current.density() == 0.0 {
            0
        } else {
            (fog.transition_time * 1000.0).round() as i32
        };
        if let Some(sun) = fog.sun {
            self.readonly_sun = sun;
        }
        self.fog = fog;
    }

    pub fn apply_sliders(&mut self, time_ms: i32) {
        let mut fog = self.sliders;
        fog.transition_time = 0.0;
        if fog.sun.is_some() {
            fog.sun = Some(self.slider_sun);
        }
        if self.script_disabled {
            fog = ExpFog {
                start_dist: 100_000_000_000.0,
                halfway_dist: 100_000_000_001.0,
                color_rgb: [0.0; 3],
                max_opacity: 0.0,
                transition_time: 0.0,
                sun: None,
                volumetric: None,
            };
        }
        self.set(fog, time_ms);
    }

    pub fn sample(&self, time_ms: i32) -> ExpFog {
        let sampled = sim::ScriptFog {
            from: fog_params(self.previous),
            to: fog_params(self.fog),
            start_ms: self.start_ms,
            duration_ms: self.duration_ms,
        }
        .sample(time_ms);
        let mut result = exp_fog(sampled);
        result.transition_time = self.fog.transition_time;
        result
    }
}

fn fog_params(fog: ExpFog) -> sim::ScriptFogParams {
    sim::ScriptFogParams {
        start_dist: fog.start_dist,
        halfway_dist: fog.halfway_dist,
        color_rgb: fog.color_rgb,
        max_opacity: fog.max_opacity,
        sun: fog.sun.map(|sun| sim::ScriptSunFog {
            color_rgb: sun.color_rgb,
            sun_dir: sun.sun_dir,
            begin_angle_deg: sun.begin_angle_deg,
            end_angle_deg: sun.end_angle_deg,
            scale: sun.scale,
        }),
        volumetric: fog.volumetric.map(|vol| sim::ScriptVolFog {
            halfway_height: vol.halfway_height,
            base_height: vol.base_height,
            color_scale: vol.color_scale,
            max_density: vol.max_density,
        }),
    }
}

fn exp_fog(fog: sim::ScriptFogParams) -> ExpFog {
    ExpFog {
        start_dist: fog.start_dist,
        halfway_dist: fog.halfway_dist,
        color_rgb: fog.color_rgb,
        max_opacity: fog.max_opacity,
        transition_time: 0.0,
        volumetric: fog.volumetric.map(|vol| asset_world::VolFog {
            halfway_height: vol.halfway_height,
            base_height: vol.base_height,
            color_scale: vol.color_scale,
            max_density: vol.max_density,
        }),
        sun: fog.sun.map(|sun| SunFog {
            color_rgb: sun.color_rgb,
            sun_dir: sun.sun_dir,
            begin_angle_deg: sun.begin_angle_deg,
            end_angle_deg: sun.end_angle_deg,
            scale: sun.scale,
        }),
    }
}

/// A game's own frame fog for the map (`game_api::ModeRules::frame_fog`):
/// its client state, run each frame in place of the renderer's fog.
#[derive(Resource)]
pub struct GameFrameFog {
    fog: Box<dyn game_api::fog::FrameFog>,
    /// The client and its teleport bit the last frame drew for.
    last_view: Option<(sim::ClientId, bool)>,
    /// The last script fog switch logged.
    logged: Option<u32>,
}

impl GameFrameFog {
    pub fn new(fog: Box<dyn game_api::fog::FrameFog>) -> Self {
        Self {
            fog,
            last_view: None,
            logged: None,
        }
    }

    /// Runs the frame for the presented client at `time_ms` from `eye`. The
    /// view is cut on the first frame, when the presented client changes and
    /// when its player's teleport bit flips.
    pub(crate) fn frame(
        &mut self,
        presented: &net::PresentedSnapshot,
        local: sim::ClientId,
        time_ms: i32,
        eye: [f32; 3],
    ) -> game_api::fog::FogRows {
        let snapshot = presented.snapshot();
        let screen = snapshot
            .and_then(|snapshot| snapshot.meta.for_client(local))
            .map(|meta| {
                (
                    meta.view_effects.client_screen.fog,
                    meta.view_effects.client_screen.fog_bank,
                )
            });
        let teleport = snapshot
            .and_then(|snapshot| snapshot.players.iter().find(|(id, _)| *id == local))
            .is_some_and(|(_, ps)| ps.e_flags & net::EFLAGS_TELEPORT != 0);
        let view_cut = self.last_view != Some((local, teleport));
        self.last_view = Some((local, teleport));
        let script = screen.and_then(|(fog, _)| fog);
        if let Some(switch) = script
            && self.logged != Some(switch.serial)
        {
            self.logged = Some(switch.serial);
            diag::info!(
                World,
                "frame fog: script fog switch {} to slot {} at {} over {}ms (start={:.3} density={:.6})",
                switch.serial,
                switch.slot,
                switch.start_ms,
                switch.transition_ms,
                switch.block[6],
                switch.block[7]
            );
        }
        self.fog.frame(&game_api::fog::FogFrame {
            time_ms,
            eye,
            bank: screen.and_then(|(_, bank)| bank),
            view_cut,
            script,
        })
    }
}

#[derive(Resource, Default)]
pub(crate) struct ScriptFogPresentation {
    applied: Option<sim::ScriptFog>,
    baseline: Option<MapFrameFog>,
    /// How many client light overrides the last frame drew with.
    light_overrides: usize,
}

pub(crate) fn sync_script_fog(
    scene: Res<crate::prepare::scene::world::WorldScene>,
    presented: Res<net::PresentedSnapshot>,
    local: Res<net::LocalPresentClient>,
    mut state: ResMut<ScriptFogPresentation>,
    mut map_fog: Option<ResMut<MapFrameFog>>,
    mut primary_lights: Option<ResMut<super::MapPrimaryLights>>,
    mut commands: Commands,
) {
    if !scene.spawned {
        return;
    }
    if let Some(primary) = primary_lights.as_deref_mut()
        && let Some(meta) = presented
            .snapshot()
            .and_then(|snapshot| snapshot.meta.for_client(local.0))
        && !meta.view_effects.client_lights.is_empty()
    {
        const INV_255: f32 = f32::from_bits(0x3b80_8081);
        if meta.view_effects.client_lights.len() != state.light_overrides {
            state.light_overrides = meta.view_effects.client_lights.len();
            let lights = &meta.view_effects.client_lights;
            diag::info!(
                World,
                "client light overrides: {} of {} primary lights ({} turned, {} moved)",
                state.light_overrides,
                primary.lights.len(),
                lights.iter().filter(|ov| ov.direction.is_some()).count(),
                lights.iter().filter(|ov| ov.origin.is_some()).count()
            );
        }
        for ov in &meta.view_effects.client_lights {
            let index = usize::from(ov.index);
            let Some(pack) = primary.lights.get_mut(index) else {
                continue;
            };
            let rgb = std::array::from_fn(|i| f32::from(ov.color[i]) * INV_255 * ov.intensity);
            pack.color = rgb;
            pack.radius = ov.radius;
            pack.cos_outer = ov.cos_half_fov_outer;
            pack.cos_inner = ov.cos_half_fov_inner;
            pack.exponent = ov.color[3];
            if let Some(direction) = ov.direction {
                pack.direction = direction;
            }
            if let Some(origin) = ov.origin {
                pack.origin = origin;
            }
            if let Some(overrides) = primary.overrides.get_mut(index) {
                let rgba = [rgb[0], rgb[1], rgb[2], 1.0];
                overrides.diffuse = Some(rgba);
                overrides.specular = Some(rgba);
            }
        }
    }
    let fog = presented.snapshot().and_then(|snapshot| {
        snapshot
            .meta
            .for_client(local.0)
            .and_then(|meta| meta.view_effects.fog)
            .or(snapshot.meta.objectives.fog)
    });
    if fog == state.applied {
        return;
    }
    if let Some(fog) = fog {
        diag::info!(
            World,
            "script fog: start={:.3} half={:.3} opacity={:.3} duration={}ms",
            fog.to.start_dist,
            fog.to.halfway_dist,
            fog.to.max_opacity,
            fog.duration_ms
        );
        if state.applied.is_none() {
            state.baseline = map_fog.as_deref().copied();
        }
        let mut target = MapFrameFog::new(exp_fog(fog.to));
        target.previous = exp_fog(fog.from);
        target.start_ms = fog.start_ms;
        target.duration_ms = fog.duration_ms;
        if let Some(map_fog) = map_fog.as_deref_mut() {
            *map_fog = target;
        } else {
            commands.queue(move |world: &mut World| {
                frame::scope::insert(world, target, frame::MatchScope::Live)
            });
        }
    } else if let Some(baseline) = state.baseline.take() {
        if let Some(map_fog) = map_fog.as_deref_mut() {
            *map_fog = baseline;
        } else {
            commands.queue(move |world: &mut World| {
                frame::scope::insert(world, baseline, frame::MatchScope::Live)
            });
        }
    } else {
        commands.remove_resource::<MapFrameFog>();
    }
    state.applied = fog;
}
