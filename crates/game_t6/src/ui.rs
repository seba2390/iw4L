pub mod animation;
pub mod elements;
pub mod layout;
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
    pub animation_target: Option<State>,
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
            animation_target: None,
            animation_options: [false; 2],
        }
    }
}

impl Element {
    fn destination(&mut self) -> &mut State {
        self.animation_target.as_mut().unwrap_or(&mut self.current)
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
}
