//! Black Ops multiplayer's TAB scoreboard, read from BlackOpsMP.exe: the
//! layout dvars at their registered defaults, the column table its client
//! builds from the server's four `setScoreboardColumns` types, and the team
//! banners' dvars and colours.

use game_api::hud::{ScoreAlign, TeamScoreCell, TeamScoreColumn, TeamScoreboard};

pub(crate) static SCOREBOARD: TeamScoreboard = TeamScoreboard {
    // cg_scoreboardWidth, cg_scoreboardHeight.
    width: 500.0,
    height: 435.0,
    top_raise: 15.0,
    first_banner: 42.0 + 15.9,
    // cg_scoreboardBannerHeight.
    banner_height: 35.0,
    inset: 9.0,
    inner_trim: 6.0 + 4.0 + 8.0,
    bottom_room: 3.0 + 2.0 + 14.0 + 1.0,
    row_height: 12.0,
    row_gap: 4.0,
    icon_size: 13.0,
    // cg_scoreboardFont; the header's scale is a constant beside
    // cg_scoreboardHeaderFontScale's default (both 0.315).
    font: 3,
    header_scale: 0.315,
    banner_scale: 0.32,
    score_font: 6,
    score_scale: 0.65,
    score_drop: 6.0,
    icon_scale: 1.15,
    row_scale: 0.22,
    scale_step: 0.02,
    plain_below: 0.16,
    // cg_scoreboardTextOffset.
    text_offset: 0.6,
    // cg_scoreboardMyColor.
    my_color: [1.0, 0.8, 0.4, 1.0],
    row_back: [0.5, 0.5, 0.5, 0.5],
    // Free is g_ScoresColor_Free (`_scoreboard.gsc` sets the registered
    // default again); the client hardcodes the axis and allies rows';
    // spectators are g_ScoresColor_Spectator.
    team_colors: [
        [0.76, 0.78, 0.1, 1.0],
        [0.619, 0.043, 0.058, 1.0],
        [0.384, 0.725, 0.725, 1.0],
        [0.25, 0.25, 0.25, 1.0],
    ],
    columns: &[
        TeamScoreColumn {
            cell: TeamScoreCell::Rank,
            width: 0.07,
            title: "",
            align: ScoreAlign::Left,
        },
        TeamScoreColumn {
            cell: TeamScoreCell::Status,
            width: 0.05,
            title: "",
            align: ScoreAlign::Center,
        },
        TeamScoreColumn {
            cell: TeamScoreCell::Name,
            width: 0.2,
            title: "",
            align: ScoreAlign::Left,
        },
        TeamScoreColumn {
            cell: TeamScoreCell::Voice,
            width: 0.05,
            title: "",
            align: ScoreAlign::Left,
        },
        TeamScoreColumn {
            cell: TeamScoreCell::Score,
            width: 0.1,
            title: "CGAME_SB_SCORE",
            align: ScoreAlign::Right,
        },
    ],
    // cg_scoreboardPingText is on.
    ping: Some(TeamScoreColumn {
        cell: TeamScoreCell::Ping,
        width: 0.1,
        title: "CGAME_SB_PING",
        align: ScoreAlign::Right,
    }),
    column_titles: &[
        "",
        "",
        "MPUI_KILLS",
        "MPUI_DEATHS",
        "MPUI_ASSISTS",
        "MPUI_DEFENDS",
        "MPUI_PLANTS",
        "MPUI_DEFUSES",
        "MPUI_RETURNS",
        "MPUI_CAPTURES",
        "MPUI_DESTRUCTIONS",
        "MPUI_RATIO",
        "MPUI_SURVIVED",
        "MPUI_STABS",
        "MPUI_TOMAHAWKS",
        "MPUI_HUMILIATED",
        "MPUI_X2SCORE",
        "MPUI_HEADSHOTS",
    ],
    column_align: ScoreAlign::Right,
    ratio_column: 11,
    tenfold_column: 16,
    alive_gametype: "sd",
    alive_text: "MPUI_ALIVE",
    team_icons: [
        "g_TeamIcon_Free",
        "g_TeamIcon_Axis",
        "g_TeamIcon_Allies",
        "g_TeamIcon_Spectator",
    ],
    team_names: ["", "g_TeamName_Axis", "g_TeamName_Allies", ""],
    spectator_name: "CGAME_SPECTATORS",
    footer_height: 14.0,
    footer_bottom: 5.0,
    footer_scale: 0.2,
    footer_step: 0.01,
    footer_floor: 0.075,
    // sv_hostname's default on a server that is not dedicated.
    hostname: "BlackOpsPrivate",
    fade_ms: 100,
};
