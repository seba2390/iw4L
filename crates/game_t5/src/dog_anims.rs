//! The animation tree Black Ops' multiplayer executable builds in code for
//! its dogs (`Dog_CreateAnims`), node for node: the zones hold no `dog` tree.

use game_api::{CodeAnimNode, CodeAnimTree};

const fn leaf(name: &'static str, parent: u16) -> CodeAnimNode {
    CodeAnimNode {
        name,
        parent: Some(parent),
        additive: false,
    }
}

const fn knob(name: &'static str) -> CodeAnimNode {
    leaf(name, 0)
}

const fn additive(name: &'static str) -> CodeAnimNode {
    CodeAnimNode {
        name,
        parent: Some(0),
        additive: true,
    }
}

pub(crate) const DOG_TREE: CodeAnimTree = CodeAnimTree {
    name: "dog",
    nodes: &[
        CodeAnimNode {
            name: "root",
            parent: None,
            additive: false,
        },
        leaf("german_shepherd_idle", 0),
        leaf("german_shepherd_walk", 0),
        leaf("german_shepherd_run_stop", 0),
        leaf("german_shepherd_run_pain", 0),
        leaf("german_shepherd_run_attack", 0),
        leaf("german_shepard_run_turn_90_left", 0),
        leaf("german_shepard_run_turn_90_right", 0),
        leaf("german_shepard_turn_90_left", 0),
        leaf("german_shepard_turn_90_right", 0),
        leaf("german_shepard_run_turn_180_left", 0),
        leaf("german_shepard_run_turn_180_right", 0),
        leaf("german_shepherd_run_start_180_L", 0),
        leaf("german_shepherd_run_start_180_R", 0),
        leaf("german_shepard_run_pain_hit_front", 0),
        leaf("german_shepard_run_pain_hit_front", 0),
        leaf("german_shepard_run_pain_hit_front", 0),
        leaf("german_shepard_run_pain_hit_front", 0),
        leaf("german_shepard_pain_hit_front", 0),
        leaf("german_shepard_pain_hit_back", 0),
        leaf("german_shepard_pain_hit_left", 0),
        leaf("german_shepard_pain_hit_right", 0),
        leaf("german_shepherd_death_front", 0),
        leaf("german_shepard_death_hit_back", 0),
        leaf("german_shepard_death_hit_left", 0),
        leaf("german_shepard_death_hit_right", 0),
        leaf("german_shepherd_run_flashbang", 0),
        leaf("german_shepherd_attack_player_miss", 0),
        leaf("german_shepherd_attack_player_miss_turnL", 0),
        leaf("german_shepherd_attack_player_miss_turnR", 0),
        leaf("german_shepard_run_attack_low", 0),
        leaf("german_shepherd_traverse_up_40", 0),
        leaf("german_shepherd_traverse_up_80", 0),
        leaf("german_shepherd_traverse_down_40", 0),
        leaf("german_shepherd_traverse_down_80", 0),
        leaf("german_shepherd_run_jump_40", 0),
        leaf("german_shepherd_run_jump_window_40", 0),
        leaf("german_shepherd_walk", 0),
        knob("german_shepherd_run_start_knob"),
        knob("german_shepherd_run_knob"),
        knob("german_shepherd_attackidle_knob"),
        additive("german_shepherd_look_2"),
        additive("german_shepherd_look_8"),
        additive("german_shepherd_look_4"),
        additive("german_shepherd_look_6"),
        leaf("german_shepherd_run_start", 0x26),
        leaf("german_shepherd_run", 0x27),
        leaf("german_shepherd_run_lean_L", 0x27),
        leaf("german_shepherd_run_lean_R", 0x27),
        leaf("german_shepherd_attackidle", 0x28),
        leaf("german_shepherd_attackidle_bark", 0x28),
        leaf("german_shepherd_attackidle_growl", 0x28),
        leaf("german_shepherd_look_down", 0x29),
        leaf("german_shepherd_attack_look_down", 0x29),
        leaf("german_shepherd_look_up", 0x2a),
        leaf("german_shepherd_attack_look_up", 0x2a),
        leaf("german_shepherd_look_right", 0x2b),
        leaf("german_shepherd_attack_look_right", 0x2b),
        leaf("german_shepherd_look_left", 0x2c),
        leaf("german_shepherd_attack_look_left", 0x2c),
    ],
};
