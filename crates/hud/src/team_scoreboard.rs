//! A scoreboard the game's executable draws by team
//! ([`game_api::hud::TeamScoreboard`]): a banner per team over its rows,
//! the column titles on the first banner's line, and the host's name.

use game_api::hud::{ScoreAlign, TeamScoreCell, TeamScoreboard};
use sim::{ClientId, ClientSnapshotMeta, Snapshot};

use crate::chrome::{ChromeAssets, layout_text_width};
use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance};
use crate::scoreboard::{ALIGN_SUB, ALIGN_VIEWABLE, client_state_name};
use crate::surface::Hud2dSurface;

/// A board column: a fixed cell or a score column's type.
#[derive(Clone, Copy)]
enum Cell {
    Fixed(TeamScoreCell),
    Score(u8),
}

#[derive(Clone, Copy)]
struct Column {
    cell: Cell,
    width: f32,
    title: &'static str,
    align: ScoreAlign,
}

/// The column table the client builds from the four column types: the
/// fixed cells, then each type the board names (an even share of what the
/// fixed cells and the ping leave), then the ping.
fn columns(board: &TeamScoreboard, types: [u8; 4]) -> Vec<Column> {
    let named: Vec<u8> = types
        .into_iter()
        .filter(|&t| t >= 2 && (t as usize) < board.column_titles.len())
        .collect();
    let mut used: f32 = board.columns.iter().map(|c| c.width).sum();
    used += board.ping.map_or(0.0, |c| c.width);
    let share = (1.0 - used.min(1.0)) / named.len().max(1) as f32;
    let mut out: Vec<Column> = board
        .columns
        .iter()
        .map(|c| Column {
            cell: Cell::Fixed(c.cell),
            width: c.width,
            title: c.title,
            align: c.align,
        })
        .collect();
    out.extend(named.into_iter().map(|t| Column {
        cell: Cell::Score(t),
        width: share,
        title: board.column_titles[t as usize],
        align: board.column_align,
    }));
    out.extend(board.ping.map(|c| Column {
        cell: Cell::Fixed(c.cell),
        width: c.width,
        title: c.title,
        align: c.align,
    }));
    out
}

/// The server's ranking: connecting players last, spectators after the
/// players, players by score and then fewer deaths. Equal players keep
/// their client order (the executable's sort leaves it unspecified).
fn ranked(snap: &Snapshot) -> Vec<&(ClientId, ClientSnapshotMeta)> {
    let mut rows: Vec<_> = snap.meta.clients.iter().collect();
    rows.sort_by(|(a_id, a), (b_id, b)| {
        let connecting = |m: &ClientSnapshotMeta| m.lifecycle == sim::ClientLifecycle::Connecting;
        let spectator = |m: &ClientSnapshotMeta| m.client_state_team == 3;
        connecting(a)
            .cmp(&connecting(b))
            .then(spectator(a).cmp(&spectator(b)))
            .then_with(|| {
                if spectator(a) {
                    std::cmp::Ordering::Equal
                } else {
                    b.score.cmp(&a.score).then(a.deaths.cmp(&b.deaths))
                }
            })
            .then(a_id.0.cmp(&b_id.0))
    });
    rows
}

struct Font<'a> {
    name: &'static str,
    def: &'a asset_game::FontDef,
    layout: &'static dyn game_api::MenuLayout,
}

impl Font<'_> {
    fn width(&self, text: &str, scale: f32) -> f32 {
        layout_text_width(self.layout, self.def, text, scale).trunc()
    }

    fn height(&self, scale: f32) -> f32 {
        self.layout.text_height(self.def.pixel_height, scale)
    }
}

struct Painter<'a> {
    surface: &'a Hud2dSurface,
    namespace: asset_core::AssetNamespace,
    cmds: Vec<Draw2dCmd>,
}

impl Painter<'_> {
    fn picture(&mut self, x: f32, y: f32, w: f32, h: f32, material: &str, color: [f32; 4]) {
        if material.is_empty() {
            return;
        }
        let r = self
            .surface
            .apply_rect(x, y, w, h, ALIGN_VIEWABLE, ALIGN_SUB);
        self.cmds.push(Draw2dCmd {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color,
            material: material.to_owned(),
            material_namespace: self.namespace,
            op: Draw2dOp::StretchPic,
            provenance: Draw2dProvenance::CgDraw { site: "scoreboard" },
            layer: 1,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        font: &Font<'_>,
        x: f32,
        y: f32,
        scale: f32,
        style: i32,
        text: &str,
        color: [f32; 4],
    ) {
        if text.is_empty() {
            return;
        }
        let draw_scale = font.layout.text_scale(font.def.pixel_height, scale);
        let r = self
            .surface
            .apply_rect(x, y, draw_scale, draw_scale, ALIGN_VIEWABLE, ALIGN_SUB);
        self.cmds.push(Draw2dCmd {
            x: (r.x + 0.5).floor(),
            y: (r.y + 0.5).floor(),
            w: r.w,
            h: r.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color,
            material: asset_core::AssetRef::bare_name(&font.def.material).to_owned(),
            material_namespace: self.namespace,
            op: Draw2dOp::TextRun {
                font: font.name.to_owned(),
                scale: draw_scale,
                text: text.to_owned(),
                loc_key: String::new(),
                style,
                fx: None,
                glow: None,
            },
            provenance: Draw2dProvenance::CgDraw { site: "scoreboard" },
            layer: 2,
        });
    }
}

/// Which banners and rows still fit: rows before the first shown one are
/// skipped, and the first that would cross the board's bottom ends the
/// drawing.
struct Fit {
    index: i32,
    first: i32,
    full: bool,
    limit: f32,
}

impl Fit {
    fn take(&mut self, y: f32, h: f32) -> bool {
        if self.full {
            return false;
        }
        if self.index < self.first {
            self.index += 1;
            return false;
        }
        if self.limit < y + h {
            self.full = true;
            return false;
        }
        self.index += 1;
        true
    }
}

pub(crate) struct BoardInput<'a> {
    pub snap: &'a Snapshot,
    /// The catalog holding the rank tables.
    pub tables: Option<&'a asset_game::MenuCatalog>,
    pub local: ClientId,
    pub alpha: f32,
}

pub(crate) fn draw(
    board: &TeamScoreboard,
    assets: &ChromeAssets<'_>,
    surface: &Hud2dSurface,
    input: &BoardInput<'_>,
) -> Option<Draw2dList> {
    let BoardInput {
        snap,
        tables,
        local,
        alpha,
    } = *input;
    let layout = assets.layout()?;
    let catalog = assets.catalog?;
    let font = |id: i32, scale: f32| {
        let name = assets.font_name(id, surface.scale_virtual_to_real()[1], scale)?;
        Some(Font {
            name,
            def: catalog.font(name)?,
            layout,
        })
    };
    let localize = |key: &str| -> String {
        let key = key.trim_start_matches(['@', '&']);
        assets
            .localize
            .and_then(|l| l.text(key))
            .unwrap_or(key)
            .to_owned()
    };
    let dvars = snap.meta.script_dvars(local);
    let mut paint = Painter {
        surface,
        namespace: assets.namespace(),
        cmds: Vec::new(),
    };

    let place = surface.placement();
    let viewable = place.virtual_viewable_max[0] - place.virtual_viewable_min[0];
    let left = ((viewable - board.width) * 0.5).max(0.0);
    let x0 = left + board.inset;
    let top = ((480.0 - board.height) * 0.5 - board.top_raise).max(0.0);
    let inner = board.width - board.inner_trim;
    let columns = columns(board, snap.meta.objectives.scoreboard_columns);
    let rows = ranked(snap);
    let mut counts = [0i32; 4];
    for (_, meta) in &rows {
        if let Some(n) = counts.get_mut(meta.client_state_team as usize) {
            *n += 1;
        }
    }
    let local_team = snap
        .meta
        .for_client(local)
        .map_or(0, |m| m.client_state_team);

    // The local player's row is kept on the board: with more rows before
    // it than fit, the board starts half a board above it.
    let capacity = ((board.height + top
        - board.bottom_room
        - (top + board.first_banner - 15.9)
        - board.banner_height
        - 11.9
        - board.row_gap)
        * 0.0625) as i32;
    let mut first = 1;
    if local_team != 3 {
        let position = 1 + rows
            .iter()
            .take_while(|(id, _)| *id != local)
            .filter(|(_, m)| m.client_state_team == local_team)
            .count() as i32;
        if position > capacity {
            first = position - capacity / 2 + 1;
        }
    }
    let mut fit = Fit {
        index: 1,
        first,
        full: false,
        limit: top + board.height - board.bottom_room,
    };
    let white = [1.0, 1.0, 1.0, alpha];

    // The column titles, centred, on the first banner's line.
    let mut y = top + board.first_banner;
    let header = font(board.font, board.header_scale)?;
    let mut x = x0;
    for column in &columns {
        let w = column.width * inner;
        if !column.title.is_empty() {
            let title = localize(column.title);
            let tw = header.width(&title, board.header_scale);
            paint.text(
                &header,
                x + (w - tw) * 0.5,
                y + board.banner_height,
                board.header_scale,
                3,
                &title,
                white,
            );
        }
        x += w;
    }

    let row_font = font(board.font, board.row_scale)?;
    let banner_font = font(board.font, board.banner_scale)?;
    let score_font = font(board.score_font, board.banner_scale)?;
    let gametype = dvars.string("g_gametype").unwrap_or("");
    let alive_counts = !gametype.is_empty() && gametype.eq_ignore_ascii_case(board.alive_gametype);

    let banner = |paint: &mut Painter<'_>, fit: &mut Fit, team: usize, y: f32| -> f32 {
        if !fit.take(y, board.banner_height) {
            return y;
        }
        let mut x = x0;
        let icon = dvars.string(board.team_icons[team]).unwrap_or("");
        if !icon.is_empty() && counts[team] > 0 {
            let size = board.banner_height * board.icon_scale;
            paint.picture(
                x - size - 8.0,
                y + board.banner_height - 3.0 + 8.0 - size * 0.5,
                size,
                size,
                icon,
                white,
            );
        }
        let base = y + board.banner_height;
        if team != 3 {
            let score = snap.meta.objectives.scores.get(team).copied().unwrap_or(0);
            let text = score.to_string();
            paint.text(
                &score_font,
                x,
                base + board.score_drop,
                board.score_scale,
                3,
                &text,
                white,
            );
            x += score_font.width(&text, board.score_scale) + 8.0;
        }
        let name = match board.team_names[team] {
            "" if team == 3 => localize(board.spectator_name),
            "" => String::new(),
            dvar => localize(dvars.string(dvar).unwrap_or("")),
        };
        paint.text(&banner_font, x, base, board.banner_scale, 3, &name, white);
        x += banner_font.width(&name, board.banner_scale) + 8.0;
        let count = format!("( {} )", counts[team]);
        paint.text(&banner_font, x, base, board.banner_scale, 3, &count, white);
        if alive_counts && team != 3 {
            let alive = rows
                .iter()
                .filter(|(_, m)| m.client_state_team as usize == team && m.status_icon.is_empty())
                .count();
            let text = format!("    {alive} {}", localize(board.alive_text));
            paint.text(
                &banner_font,
                x + banner_font.width(&count, board.banner_scale),
                base,
                board.banner_scale,
                3,
                &text,
                white,
            );
        }
        base + board.row_gap
    };

    let cell = |paint: &mut Painter<'_>, text: &str, x: f32, y: f32, w: f32, align, color| {
        let mut scale = board.row_scale;
        while row_font.width(text, scale) > w && scale > 0.0 {
            scale -= board.scale_step;
        }
        let style = if scale < board.plain_below { 0 } else { 3 };
        let tw = row_font.width(text, scale);
        let dx = match align {
            ScoreAlign::Left => 0.0,
            ScoreAlign::Center => (w - tw) * 0.5,
            ScoreAlign::Right => w - tw - 4.0,
        };
        let ty = y + (row_font.height(scale) + board.row_height) * board.text_offset;
        paint.text(&row_font, x + dx, ty, scale, style, text, color);
    };

    let section = |paint: &mut Painter<'_>, fit: &mut Fit, team: usize, y: f32| -> f32 {
        let mut y = banner(paint, fit, team, y);
        for (id, meta) in rows.iter().map(|(id, meta)| (*id, meta)) {
            if meta.client_state_team as usize != team || !fit.take(y, board.row_height) {
                continue;
            }
            let tint = board.team_colors[team];
            paint.picture(x0, y, inner, board.row_height, "white", board.row_back);
            paint.picture(
                x0,
                y,
                inner,
                board.row_height,
                "white",
                [tint[0], tint[1], tint[2], tint[3] * 0.5],
            );
            let base = if id == local { board.my_color } else { white };
            let color = [base[0], base[1], base[2], tint[3] * alpha];
            let icon_color = [1.0, 1.0, 1.0, tint[3] * alpha];
            let mut x = x0;
            for column in &columns {
                let w = column.width * inner;
                match column.cell {
                    Cell::Fixed(TeamScoreCell::Rank) => {
                        if let Some((icon, level)) = tables.and_then(|tables| {
                            crate::overhead_names::rank_presentation(
                                tables,
                                meta.rank,
                                meta.prestige,
                            )
                        }) {
                            cell(paint, level, x, y, board.icon_size, column.align, color);
                            paint.picture(
                                x + board.icon_size,
                                y,
                                board.icon_size,
                                board.icon_size,
                                icon,
                                icon_color,
                            );
                        }
                    }
                    Cell::Fixed(TeamScoreCell::Status) => {
                        let dx = match column.align {
                            ScoreAlign::Left => 0.0,
                            ScoreAlign::Center => (w - board.icon_size) * 0.5,
                            ScoreAlign::Right => w - board.icon_size,
                        };
                        paint.picture(
                            x + dx,
                            y,
                            board.icon_size,
                            board.icon_size,
                            &meta.status_icon,
                            icon_color,
                        );
                    }
                    Cell::Fixed(TeamScoreCell::Name) => {
                        let name = client_state_name(&meta.name).unwrap_or("");
                        cell(paint, name, x, y, w, column.align, color);
                    }
                    // No voice chat: no player talks or is muted.
                    Cell::Fixed(TeamScoreCell::Voice) => {}
                    Cell::Fixed(TeamScoreCell::Score) if team != 3 => {
                        cell(paint, &meta.score.to_string(), x, y, w, column.align, color);
                    }
                    Cell::Fixed(TeamScoreCell::Score) => {}
                    // The server's measured ping (`cg_ScoresPing_Interval`, its
                    // divisor, registers 1 and cannot change).
                    Cell::Fixed(TeamScoreCell::Ping) => {
                        cell(paint, &meta.ping.to_string(), x, y, w, column.align, color);
                    }
                    Cell::Score(t) if team != 3 => {
                        let text = if t == board.ratio_column {
                            let deaths = if meta.deaths == 0 { 1 } else { meta.deaths };
                            format!("{:.2}", meta.kills as f32 / deaths as f32)
                        } else {
                            let value = snap
                                .meta
                                .objectives
                                .scoreboard_columns
                                .iter()
                                .position(|&c| c == t)
                                .map_or(0, |slot| meta.score_columns[slot]);
                            let value = if t == board.tenfold_column {
                                value * 10
                            } else {
                                value
                            };
                            value.to_string()
                        };
                        cell(paint, &text, x, y, w, column.align, color);
                    }
                    Cell::Score(_) => {}
                }
                x += w;
            }
            y += board.row_height + board.row_gap;
        }
        y
    };

    if counts[1] > 0 || counts[2] > 0 {
        let own = if matches!(local_team, 1 | 2) {
            local_team as usize
        } else {
            2
        };
        y = section(&mut paint, &mut fit, own, y) + board.row_gap;
        y = section(&mut paint, &mut fit, 3 - own, y) + board.row_gap;
    }
    if counts[0] > 0 {
        y = section(&mut paint, &mut fit, 0, y) + board.row_gap;
    }
    if counts[3] > 0 {
        section(&mut paint, &mut fit, 3, y);
    }

    // The footer: the host's name, its scale stepped down until it fits.
    let mut scale = board.footer_scale;
    let mut footer = font(board.font, scale)?;
    while footer.width(board.hostname, scale) + 4.0 > board.width - 6.0 - 8.0
        && scale - board.footer_step > board.footer_floor
    {
        scale -= board.footer_step;
        footer = font(board.font, scale)?;
    }
    let text_h = footer.height(scale);
    paint.text(
        &footer,
        x0,
        top + board.height - board.footer_bottom - (board.footer_height - text_h) * 0.5,
        scale,
        3,
        board.hostname,
        white,
    );

    // Pictures under text.
    let (mut pictures, texts): (Vec<_>, Vec<_>) =
        paint.cmds.into_iter().partition(|c| c.layer == 1);
    pictures.extend(texts);
    Some(Draw2dList { cmds: pictures })
}
