use super::{Axis, State};

impl Axis {
    pub fn resolve(&self, parent: [f32; 2]) -> [f32; 2] {
        match (self.anchor_start, self.anchor_end) {
            (true, true) => [parent[0] + self.start, parent[1] + self.end],
            (true, false) => [parent[0] + self.start, self.end + parent[0]],
            (false, true) => [parent[1] + self.start, parent[1] + self.end],
            (false, false) => {
                let center = (parent[1] - parent[0]) * 0.5 + parent[0];
                [self.start + center, self.end + center]
            }
        }
    }
}

impl State {
    pub fn resolve_rectangle(&mut self, parent: Option<[f32; 4]>) {
        let Some(parent) = parent else {
            return;
        };
        if self.changes.horizontal {
            let [start, end] = self.horizontal.resolve([parent[0], parent[2]]);
            self.resolved_rectangle[0] = start;
            self.resolved_rectangle[2] = end;
        }
        if self.changes.vertical {
            let [start, end] = self.vertical.resolve([parent[1], parent[3]]);
            self.resolved_rectangle[1] = start;
            self.resolved_rectangle[3] = end;
        }
    }
}
