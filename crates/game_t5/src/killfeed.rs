//! Black Ops multiplayer's kill feed: the client's obituary line, printed
//! to the obituary channel and drawn by `hud_hardcore`'s `gamemessages`
//! menu in game message window 0, with the script prints of the
//! `gamenotify` channel (`iprintln`); the bold prints (`iprintlnbold`,
//! channel `boldgame`) go to window 1, `boldgamemessages`.

use game_api::hud::{KillIcon, KillfeedRules, MessageWindow, NameColor};

pub(crate) static KILLFEED: KillfeedRules = KillfeedRules {
    menu: "gamemessages",
    window: 0,
    // `con_gameMsgWindow0MsgTime`, `LineCount`, `ScrollTime`,
    // `FadeInTime`, `FadeOutTime` as registered.
    timing: MessageWindow {
        msg_time_ms: 5000,
        line_count: 4,
        scroll_time_ms: 250,
        fade_in_ms: 250,
        fade_out_ms: 500,
    },
    kill_icon,
    icon_size,
    name_color,
    suicide: "CGAME_QUAKE_SUICIDE",
    out_of_range: "CG_Obituary: target out of range",
    // `cg_gameMessageWidth` and `cg_gameBoldMessageWidth` as registered.
    print_width: 455.0,
    bold_menu: "boldgamemessages",
    bold_window: 1,
    // `con_gameMsgWindow1*` as registered.
    bold_timing: MessageWindow {
        msg_time_ms: 8000,
        line_count: 1,
        scroll_time_ms: 250,
        fade_in_ms: 250,
        fade_out_ms: 10,
    },
    bold_width: 390.0,
};

const MOD_MELEE: i32 = 7;
const MOD_HEAD_SHOT: i32 = 9;
const MOD_CRUSH: i32 = 10;
const MOD_FALLING: i32 = 12;
const MOD_SUICIDE: i32 = 13;
const MOD_TRIGGER_HURT: i32 = 14;
const MOD_IMPACT: i32 = 16;

fn kill_icon(parm: i32) -> KillIcon {
    if parm >= 0 {
        return KillIcon::Weapon {
            weapon: parm as u32,
            none: "hud_obit_death_suicide",
        };
    }
    KillIcon::Material(match -1 - parm {
        MOD_MELEE => "hud_obit_knife",
        MOD_HEAD_SHOT => "killiconheadshot",
        MOD_CRUSH => "hud_obit_death_crush",
        MOD_FALLING => "hud_obit_death_falling",
        MOD_SUICIDE | MOD_TRIGGER_HURT => "hud_obit_death_suicide",
        MOD_IMPACT => "hud_obit_death_grenade_round",
        _ => "killicondied",
    })
}

fn icon_size(ratio: i32) -> (f32, f32) {
    match ratio {
        0 => (1.4, 1.4),
        1 => (2.8, 1.4),
        _ => (2.8, 0.7),
    }
}

/// `^8` for the local player's team, `^9` for the other, white when
/// either has no team; the client takes `^8` and `^9` from the allies and
/// axis colours, which the team-set scripts set.
fn name_color(local_team: i32, team: i32) -> NameColor {
    if !matches!(local_team, 1 | 2) || !matches!(team, 1 | 2) {
        return NameColor::White;
    }
    if team == local_team {
        NameColor::Dvar {
            name: "g_TeamColor_Allies",
            default: [0.6, 0.64, 0.69],
        }
    } else {
        NameColor::Dvar {
            name: "g_TeamColor_Axis",
            default: [0.65, 0.57, 0.41],
        }
    }
}
