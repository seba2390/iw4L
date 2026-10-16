pub mod animation;
pub mod dvars;
pub mod elements;
pub mod engine_names;
pub mod layout;
pub mod profile;
pub mod scene;
pub mod script;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Axis {
    pub anchor_start: bool,
    pub anchor_end: bool,
    pub start: f32,
    pub end: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    pub horizontal: bool,
    pub vertical: bool,
    pub color: [bool; 3],
    pub opacity: bool,
    pub zoom: bool,
    pub rotation: [bool; 3],
    pub scale_delta: bool,
    pub spacing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    pub shader_vectors: [Option<[f32; 4]>; 6],
    pub material: Option<u32>,
    pub font: Option<u32>,
    pub alignment: Option<u8>,
    pub ui_3d_window: Option<i32>,
    pub horizontal: Axis,
    pub vertical: Axis,
    pub resolved_rectangle: [f32; 4],
    pub color: [f32; 3],
    pub opacity: f32,
    pub zoom: f32,
    pub rotation: [f32; 3],
    pub scale_delta: f32,
    pub spacing: f32,
    pub cached: bool,
    pub changes: Changes,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub current: State,
    pub animation_source: Option<State>,
    pub animation_target: Option<State>,
    pub animation_timer: Option<animation::AnimationTimer>,
    pub animation_name: Option<Vec<u8>>,
    pub animation_options: [bool; 2],
}

impl Default for Element {
    fn default() -> Self {
        Self {
            current: State {
                shader_vectors: [None; 6],
                material: None,
                font: None,
                alignment: None,
                ui_3d_window: None,
                horizontal: Axis::default(),
                vertical: Axis::default(),
                resolved_rectangle: [0.0; 4],
                color: [1.0; 3],
                opacity: 1.0,
                zoom: 0.0,
                rotation: [0.0; 3],
                scale_delta: 0.0,
                spacing: 0.0,
                cached: false,
                changes: Changes {
                    horizontal: true,
                    vertical: true,
                    color: [true; 3],
                    opacity: true,
                    ..Changes::default()
                },
            },
            animation_source: None,
            animation_target: None,
            animation_timer: None,
            animation_name: None,
            animation_options: [false; 2],
        }
    }
}

impl State {
    pub fn apply_target(&mut self, target: &Self) {
        self.cached = false;
        if target.changes.horizontal {
            self.horizontal = target.horizontal;
            self.changes.horizontal = true;
        }
        if target.changes.vertical {
            self.vertical = target.vertical;
            self.changes.vertical = true;
        }
        for i in 0..3 {
            if target.changes.color[i] {
                self.color[i] = target.color[i];
                self.changes.color[i] = true;
            }
            if target.changes.rotation[i] {
                self.rotation[i] = target.rotation[i];
                self.changes.rotation[i] = true;
            }
        }
        if target.changes.opacity {
            self.opacity = target.opacity;
            self.changes.opacity = true;
        }
        if target.changes.zoom {
            self.zoom = target.zoom;
            self.changes.zoom = true;
        }
        if target.changes.scale_delta {
            self.scale_delta = target.scale_delta;
            self.changes.scale_delta = true;
        }
        if target.changes.spacing {
            self.spacing = target.spacing;
            self.changes.spacing = true;
        }
        for (dst, src) in self.shader_vectors.iter_mut().zip(target.shader_vectors) {
            if src.is_some() {
                *dst = src;
            }
        }
        if target.material.is_some() {
            self.material = target.material;
        }
        if target.font.is_some() {
            self.font = target.font;
        }
        if target.alignment.is_some() {
            self.alignment = target.alignment;
        }
        if target.ui_3d_window.is_some() {
            self.ui_3d_window = target.ui_3d_window;
        }
    }
}

impl Element {
    fn destination(&mut self) -> &mut State {
        self.animation_target.as_mut().unwrap_or(&mut self.current)
    }

    pub fn begin_animation(
        &mut self,
        name: Vec<u8>,
        duration: i32,
        options: [bool; 2],
        target: State,
    ) {
        self.animation_options = options;
        self.animation_source = Some(self.current);
        for (dst, src) in self
            .current
            .shader_vectors
            .iter_mut()
            .zip(target.shader_vectors)
        {
            if src.is_some() {
                *dst = src;
            }
        }
        if target.material.is_some() {
            self.current.material = target.material;
        }
        if target.font.is_some() {
            self.current.font = target.font;
        }
        if target.alignment.is_some() {
            self.current.alignment = target.alignment;
        }
        if target.ui_3d_window.is_some() {
            self.current.ui_3d_window = target.ui_3d_window;
        }
        self.animation_target = Some(target);
        self.animation_timer = Some(animation::AnimationTimer::new(duration, options));
        self.animation_name = Some(name);
        self.current.cached = false;
    }

    pub fn finish_animation(&mut self, interrupted: bool) -> Option<(Vec<u8>, f32, bool)> {
        let target = self.animation_target.take()?;
        if let Some(source) = self.animation_source.take() {
            self.current.changes = source.changes;
        }
        self.current.apply_target(&target);
        let lateness =
            self.animation_timer
                .take()
                .map_or(0.0, |mut timer| match timer.interrupt() {
                    animation::AnimationStep::Complete { lateness, .. } => lateness,
                    animation::AnimationStep::Interpolate(_) => 0.0,
                });
        let name = self.animation_name.take().unwrap_or_else(Vec::new);
        Some((name, lateness, interrupted))
    }

    pub fn local_rectangle(&self) -> [f32; 4] {
        [
            self.current.horizontal.start,
            self.current.vertical.start,
            self.current.horizontal.end,
            self.current.vertical.end,
        ]
    }

    pub fn root_point_in_units(&self, point: [f32; 2]) -> [f32; 2] {
        let state = &self.current;
        let height = (state.vertical.end - state.vertical.start) * (1.0_f32 / 720.0);
        let scale = 1.0 / height;
        [
            (point[0] - state.horizontal.start) * scale,
            (point[1] - state.vertical.start) * scale,
        ]
    }

    pub fn set_horizontal(&mut self, axis: Axis) {
        let state = self.destination();
        state.horizontal = axis;
        state.cached = false;
        state.changes.horizontal = true;
    }

    pub fn set_vertical(&mut self, axis: Axis) {
        let state = self.destination();
        state.vertical = axis;
        state.cached = false;
        state.changes.vertical = true;
    }

    pub fn set_color(&mut self, color: [f32; 3]) {
        let state = self.destination();
        state.color = color;
        state.cached = false;
        state.changes.color = [true; 3];
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        let state = self.destination();
        state.opacity = opacity;
        state.cached = false;
        state.changes.opacity = true;
    }

    pub fn set_scale(&mut self, scale: f32) {
        let state = self.destination();
        state.scale_delta = scale - 1.0;
        state.cached = false;
        state.changes.scale_delta = true;
    }

    pub fn set_z_rot(&mut self, rotation: f32) {
        let state = self.destination();
        state.rotation[2] = rotation;
        state.cached = false;
        state.changes.rotation[2] = true;
    }
}
