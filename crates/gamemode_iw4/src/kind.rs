use crate::{dd, dom, ffa};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum GameModeKind {
    #[default]
    FreeForAll = 0,

    Demolition = 1,

    Domination = 2,

    TeamDeathmatch = 3,

    SearchAndDestroy = 4,

    CaptureTheFlag = 5,

    Headquarters = 6,

    Sabotage = 7,

    Zombies = 8,

    Zclassic = 9,

    /// Black Ops 2's Survival: its zombies gametype on one start location.
    Zstandard = 10,

    /// Black Ops 2's Grief: two teams of survivors.
    Zgrief = 11,

    /// Black Ops 2's Turned: players as zombies.
    Zcleansed = 12,

    /// Black Ops' wager matches: One in the Chamber, Gun Game, Sticks and
    /// Stones, Sharpshooter.
    OneInTheChamber = 13,

    GunGame = 14,

    SticksAndStones = 15,

    Sharpshooter = 16,
}

impl GameModeKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().as_bytes() {
            b"dm" | b"ffa" | b"freeforall" | b"free_for_all" | b"deathmatch" => {
                Some(Self::FreeForAll)
            }

            b"dd" | b"dem" | b"demolition" => Some(Self::Demolition),
            b"dom" | b"domination" => Some(Self::Domination),
            b"war" | b"tdm" | b"teamdeathmatch" | b"team_deathmatch" => Some(Self::TeamDeathmatch),
            b"sd" | b"searchanddestroy" | b"search_and_destroy" => Some(Self::SearchAndDestroy),
            b"ctf" | b"capturetheflag" | b"capture_the_flag" => Some(Self::CaptureTheFlag),
            b"koth" | b"hq" | b"headquarters" => Some(Self::Headquarters),
            b"sab" | b"sabotage" => Some(Self::Sabotage),
            b"zom" | b"zombies" => Some(Self::Zombies),
            b"zclassic" => Some(Self::Zclassic),
            b"zstandard" => Some(Self::Zstandard),
            b"zgrief" => Some(Self::Zgrief),
            b"zcleansed" => Some(Self::Zcleansed),
            b"oic" => Some(Self::OneInTheChamber),
            b"gun" => Some(Self::GunGame),
            b"hlnd" => Some(Self::SticksAndStones),
            b"shrp" => Some(Self::Sharpshooter),
            _ => None,
        }
    }

    pub fn parse_ascii_ignore_case(s: &str) -> Option<Self> {
        let mut buf = [0u8; 32];
        let bytes = s.trim().as_bytes();
        if bytes.len() > buf.len() {
            return None;
        }
        for (i, &b) in bytes.iter().enumerate() {
            buf[i] = b.to_ascii_lowercase();
        }
        Self::parse(core::str::from_utf8(&buf[..bytes.len()]).ok()?)
    }

    pub fn token(self) -> &'static str {
        match self {
            Self::FreeForAll => ffa::GAMETYPE_TOKEN,
            Self::Demolition => dd::GAMETYPE_TOKEN,
            Self::Domination => dom::GAMETYPE_TOKEN,
            Self::TeamDeathmatch => "war",
            Self::SearchAndDestroy => "sd",
            Self::CaptureTheFlag => "ctf",
            Self::Headquarters => "koth",
            Self::Sabotage => "sab",
            Self::Zombies => "zom",
            Self::Zclassic => "zclassic",
            Self::Zstandard => "zstandard",
            Self::Zgrief => "zgrief",
            Self::Zcleansed => "zcleansed",
            Self::OneInTheChamber => "oic",
            Self::GunGame => "gun",
            Self::SticksAndStones => "hlnd",
            Self::Sharpshooter => "shrp",
        }
    }

    pub fn script_tokens(self) -> &'static [&'static str] {
        match self {
            Self::FreeForAll => &[ffa::GAMETYPE_TOKEN],
            Self::Demolition => &["dem", dd::GAMETYPE_TOKEN],
            Self::Domination => &[dom::GAMETYPE_TOKEN],
            Self::TeamDeathmatch => &["war", "tdm"],
            Self::SearchAndDestroy => &["sd"],
            Self::CaptureTheFlag => &["ctf"],
            Self::Headquarters => &["koth"],
            Self::Sabotage => &["sab"],
            Self::Zombies => &["zom"],
            Self::Zclassic => &["zclassic"],
            Self::Zstandard => &["zstandard"],
            Self::Zgrief => &["zgrief"],
            Self::Zcleansed => &["zcleansed"],
            Self::OneInTheChamber => &["oic"],
            Self::GunGame => &["gun"],
            Self::SticksAndStones => &["hlnd"],
            Self::Sharpshooter => &["shrp"],
        }
    }

    pub fn wire_tag(self) -> u8 {
        self as u8
    }

    pub fn from_wire_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::FreeForAll),
            1 => Some(Self::Demolition),
            2 => Some(Self::Domination),
            3 => Some(Self::TeamDeathmatch),
            4 => Some(Self::SearchAndDestroy),
            5 => Some(Self::CaptureTheFlag),
            6 => Some(Self::Headquarters),
            7 => Some(Self::Sabotage),
            8 => Some(Self::Zombies),
            9 => Some(Self::Zclassic),
            10 => Some(Self::Zstandard),
            11 => Some(Self::Zgrief),
            12 => Some(Self::Zcleansed),
            13 => Some(Self::OneInTheChamber),
            14 => Some(Self::GunGame),
            15 => Some(Self::SticksAndStones),
            16 => Some(Self::Sharpshooter),
            _ => None,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::FreeForAll => ffa::DISPLAY_NAME,
            Self::Demolition => dd::DISPLAY_NAME,
            Self::Domination => dom::DISPLAY_NAME,
            Self::TeamDeathmatch => "TEAM DEATHMATCH",
            Self::SearchAndDestroy => "SEARCH AND DESTROY",
            Self::CaptureTheFlag => "CAPTURE THE FLAG",
            Self::Headquarters => "HEADQUARTERS",
            Self::Sabotage => "SABOTAGE",
            Self::Zombies | Self::Zclassic | Self::Zstandard | Self::Zgrief | Self::Zcleansed => {
                "ZOMBIES"
            }
            Self::OneInTheChamber => "ONE IN THE CHAMBER",
            Self::GunGame => "GUN GAME",
            Self::SticksAndStones => "STICKS AND STONES",
            Self::Sharpshooter => "SHARPSHOOTER",
        }
    }

    /// Black Ops 2's zombies gametypes, which run on its zombies maps.
    pub fn is_t6_zombies(self) -> bool {
        matches!(
            self,
            Self::Zclassic | Self::Zstandard | Self::Zgrief | Self::Zcleansed
        )
    }

    pub fn is_team(self) -> bool {
        !matches!(
            self,
            Self::FreeForAll
                | Self::Zclassic
                | Self::Zstandard
                | Self::OneInTheChamber
                | Self::GunGame
                | Self::SticksAndStones
                | Self::Sharpshooter
        )
    }

    pub fn team_start_classname(self, axis: bool) -> Option<&'static str> {
        match self {
            Self::FreeForAll
            | Self::TeamDeathmatch
            | Self::SearchAndDestroy
            | Self::CaptureTheFlag
            | Self::Headquarters
            | Self::Sabotage
            | Self::Zombies
            | Self::Zclassic
            | Self::Zstandard
            | Self::Zgrief
            | Self::Zcleansed
            | Self::OneInTheChamber
            | Self::GunGame
            | Self::SticksAndStones
            | Self::Sharpshooter => None,
            Self::Domination => Some(if axis {
                dom::START_SPAWN_AXIS
            } else {
                dom::START_SPAWN_ALLIES
            }),
            Self::Demolition => Some(if axis {
                dd::START_SPAWN_DEFENDER
            } else {
                dd::START_SPAWN_ATTACKER
            }),
        }
    }

    pub fn team_grid_classnames(self, axis: bool) -> &'static [&'static str] {
        match self {
            Self::FreeForAll
            | Self::TeamDeathmatch
            | Self::SearchAndDestroy
            | Self::CaptureTheFlag
            | Self::Headquarters
            | Self::Sabotage
            | Self::Zombies
            | Self::Zclassic
            | Self::Zstandard
            | Self::Zgrief
            | Self::Zcleansed
            | Self::OneInTheChamber
            | Self::GunGame
            | Self::SticksAndStones
            | Self::Sharpshooter => &[],
            Self::Domination => &[dom::SPAWN_CLASSNAME],
            Self::Demolition => {
                if axis {
                    &[
                        dd::SPAWN_DEFENDER,
                        dd::SPAWN_DEFENDER_A,
                        dd::SPAWN_DEFENDER_B,
                    ]
                } else {
                    &[
                        dd::SPAWN_ATTACKER,
                        dd::SPAWN_ATTACKER_A,
                        dd::SPAWN_ATTACKER_B,
                    ]
                }
            }
        }
    }
}
