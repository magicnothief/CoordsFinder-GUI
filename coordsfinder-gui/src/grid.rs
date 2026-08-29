//! Click-to-paint editor for one Y layer of a filter.
//!
//! The grid is drawn in Minecraft's top-down map orientation: `+X` runs right
//! (east) and `+Z` runs down (south), so north is up, matching what a player
//! sees on the F3 screen.
//!
//! A painted cell shows its rotation as a colour and as the digit that will be
//! written to the config. What the colour cannot say is which *face* a row is —
//! so badges, the `side` bar, and the netherrack ring are the marks that
//! survive when cells get small, and the digit is the first thing dropped.

use egui::{
    Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, pos2,
};

use coordsfinder::types::{RotationInfo, RotationKind};

use crate::model::{Brush, EditableConfig, OFFSET_MAX, OFFSET_MIN, brush_of, row_text};

/// Empty cells kept around the painted area so the filter can always grow.
const AUTO_FIT_MARGIN: i32 = 3;
/// Smallest auto-fitted view, in cells.
const MIN_VIEW_CELLS: i32 = 10;
/// Width of the coordinate gutters along the top and left edges.
const GUTTER: f32 = 28.0;
/// Zoom limits, in points per cell.
pub const MIN_CELL: f32 = 10.0;
/// Largest cell size the zoom allows.
pub const MAX_CELL: f32 = 64.0;
/// A heavier guide line every this many cells, for counting offsets.
const GUIDE_EVERY: i32 = 5;
/// Below this cell size the rotation digit is dropped.
///
/// The digit is the first thing to go because the fill colour already carries
/// the rotation. A badge names the face, which nothing else encodes, so it
/// survives much further down.
const DIGIT_MIN_CELL: f32 = 21.0;
/// Below this cell size the badge is dropped too.
const BADGE_MIN_CELL: f32 = 11.0;
/// Room left for a scrollbar when fitting the board to the window.
const SCROLLBAR_ALLOWANCE: f32 = 18.0;
/// Smallest cell the automatic fit will choose.
///
/// Sitting just above [`DIGIT_MIN_CELL`], so an automatically sized board
/// always keeps its digits. Shrinking past this to fit a small window trades
/// away the readability the fit exists to protect, so the board is allowed to
/// overflow and scroll instead. Zooming by hand can still go smaller, for an
/// overview of a large filter.
const AUTO_FIT_MIN_CELL: f32 = 22.0;

/// Ink drawn on top of a painted cell.
const ON_FILL: Color32 = Color32::from_rgb(0x0E, 0x10, 0x14);
/// Ring around a netherrack cell: netherrack's own red.
///
/// Taken a little darker than the block's texture so it keeps its contrast
/// against every rotation fill — including rotation 3, which is itself a red.
/// Lightness, not hue, is what makes the ring read there.
const NETHERRACK_RING: Color32 = Color32::from_rgb(0x6B, 0x2F, 0x2C);

/// Background colour for each rotation value.
pub fn rotation_color(rotation: u8) -> Color32 {
    match rotation {
        0 => Color32::from_rgb(0x2F, 0x91, 0x8B),
        1 => Color32::from_rgb(0xC0, 0x87, 0x2E),
        2 => Color32::from_rgb(0x76, 0x62, 0xC4),
        _ => Color32::from_rgb(0xBE, 0x51, 0x66),
    }
}

/// Editor state that outlives a single frame.
pub struct GridView {
    /// Y layer currently being edited.
    pub layer: i8,
    /// Observation kind the next click paints.
    pub brush: Brush,
    /// Rotation the next click paints.
    pub rotation: u8,
    /// Side length of one cell, in points.
    pub cell: f32,
    /// Draw rows from other Y layers as dimmed ghosts.
    pub show_other_layers: bool,
    /// Keep the board fitted to the window: bounds around the painted rows,
    /// and a cell size that puts the whole thing on screen. Zooming by hand
    /// clears this.
    pub auto_fit: bool,
    /// Inclusive X/Z view bounds, used when `auto_fit` is off.
    pub bounds: (i32, i32, i32, i32),
    /// Cell the pointer is over, for the cursor readout.
    pub hovered: Option<(i8, i8)>,
}

impl Default for GridView {
    fn default() -> Self {
        Self {
            layer: 0,
            brush: Brush::FourWay,
            rotation: 0,
            cell: 32.0,
            show_other_layers: true,
            auto_fit: true,
            bounds: (-6, 6, -6, 6),
            hovered: None,
        }
    }
}

impl GridView {
    /// Recomputes the view bounds from the rows in `config`.
    pub fn fit(&mut self, config: &EditableConfig) {
        let (mut min_x, mut max_x, mut min_z, mut max_z) = config.extent().unwrap_or((0, 0, 0, 0));
        min_x -= AUTO_FIT_MARGIN;
        max_x += AUTO_FIT_MARGIN;
        min_z -= AUTO_FIT_MARGIN;
        max_z += AUTO_FIT_MARGIN;
        // Grow the shorter axis so a one-row filter still gets a usable canvas.
        while max_x - min_x + 1 < MIN_VIEW_CELLS {
            min_x -= 1;
            max_x += 1;
        }
        while max_z - min_z + 1 < MIN_VIEW_CELLS {
            min_z -= 1;
            max_z += 1;
        }
        self.bounds = (
            min_x.max(OFFSET_MIN),
            max_x.min(OFFSET_MAX),
            min_z.max(OFFSET_MIN),
            max_z.min(OFFSET_MAX),
        );
    }

    /// Sets the rotation to the next value this brush accepts.
    pub fn cycle_rotation(&mut self) {
        self.rotation = (self.rotation + 1) % self.brush.rotation_count();
    }

    /// Clamps the rotation after the brush changed to a narrower one.
    pub fn clamp_rotation(&mut self) {
        self.rotation %= self.brush.rotation_count();
    }
}

/// What one cell holds, split into the current layer and the others.
struct CellRows<'a> {
    here: Vec<&'a RotationInfo>,
    /// Rotation of a row on some other layer, for the ghost mark.
    ghost: Option<u8>,
    elsewhere: usize,
}

fn rows_at<'a>(config: &'a EditableConfig, x: i8, z: i8, layer: i8) -> CellRows<'a> {
    let mut here = Vec::new();
    let mut ghost = None;
    let mut elsewhere = 0;
    for info in &config.filter {
        if info.x != x || info.z != z {
            continue;
        }
        if info.y == layer {
            here.push(info);
        } else {
            ghost.get_or_insert(info.rotation);
            elsewhere += 1;
        }
    }
    CellRows {
        here,
        ghost,
        elsewhere,
    }
}

/// Draws the bar that shows which way a `side` face is mirrored.
///
/// A `side` row is a two-state mirror rather than a turn, and the bar is the
/// only cue that separates the two states at a glance. Ordinary and netherrack
/// rows get no mark: their rotation is already carried by the fill colour and
/// the digit, and a third encoding only added clutter.
fn draw_side_bar(painter: &egui::Painter, rect: Rect, rotation: u8, ink: Color32) {
    let size = rect.width();
    let y = if rotation == 0 {
        rect.top() + size * 0.17
    } else {
        rect.bottom() - size * 0.17
    };
    let bar = Rect::from_center_size(
        pos2(rect.center().x, y),
        Vec2::new(size * 0.46, (size * 0.10).max(1.5)),
    );
    painter.rect_filled(bar, CornerRadius::same(1), ink);
}

/// Where the rotation digit sits, nudged clear of the `side` bar when the cell
/// has one. `side_rotation` is the bar's value, or `None` when there is no bar.
fn digit_position(rect: Rect, side_rotation: Option<u8>) -> Pos2 {
    let centre = rect.center();
    let nudge = Vec2::new(0.0, rect.height() * 0.09);
    match side_rotation {
        // The bar sits at the top for 0 and at the bottom for 1.
        Some(0) => centre + nudge,
        Some(_) => centre - nudge,
        None => centre,
    }
}

/// Cell size that fits the board into `available`, never below readable.
///
/// The result is clamped up to [`AUTO_FIT_MIN_CELL`], so a window too small for
/// the whole board gets a scrollable board of readable cells rather than a
/// complete but illegible one.
///
/// A scrollbar's width is always subtracted. Without it the board oscillates:
/// fitting exactly hides the scrollbar, which frees width, which grows the
/// cells, which brings the scrollbar back. Sizes are floored to whole points
/// for the same reason — so a fraction of a point cannot flip the fit.
fn fit_cell(available: Vec2, columns: i32, rows: i32) -> Option<f32> {
    if !available.x.is_finite() || !available.y.is_finite() {
        return None;
    }
    let room = available - Vec2::splat(GUTTER + SCROLLBAR_ALLOWANCE);
    if room.x <= 0.0 || room.y <= 0.0 {
        return None;
    }
    let by_width = room.x / columns as f32;
    let by_height = room.y / rows as f32;
    Some(
        by_width
            .min(by_height)
            .floor()
            .clamp(AUTO_FIT_MIN_CELL, MAX_CELL),
    )
}

/// Applies ctrl+scroll zoom, keeping the cell under the pointer in place.
///
/// The zoom has to be applied before the board is laid out, but the correction
/// needs the board origin, so this returns the previous cell size for the
/// caller to finish the adjustment once the origin is known.
fn apply_zoom(ui: &egui::Ui, view: &mut GridView) -> f32 {
    let previous = view.cell;
    if ui.ui_contains_pointer() {
        let zoom = ui.input(|input| input.zoom_delta());
        if (zoom - 1.0).abs() > f32::EPSILON {
            view.cell = (view.cell * zoom).clamp(MIN_CELL, MAX_CELL);
            // Zooming by hand is a request for a particular size, so the board
            // stops resizing itself to the window.
            view.auto_fit = false;
        }
    }
    previous
}

/// Draws the grid and applies any painting the user did. Returns whether the
/// filter changed.
pub fn show(ui: &mut egui::Ui, config: &mut EditableConfig, view: &mut GridView) -> bool {
    let available = ui.available_size();
    let previous_cell = apply_zoom(ui, view);
    if view.auto_fit {
        view.fit(config);
    }
    let (min_x, max_x, min_z, max_z) = view.bounds;
    let columns = (max_x - min_x + 1).max(1);
    let rows = (max_z - min_z + 1).max(1);
    // Fitting the zoom as well as the bounds is what keeps a filter legible in
    // a small window, where a fixed cell size would push most of it off-screen.
    if view.auto_fit
        && let Some(cell) = fit_cell(available, columns, rows)
    {
        view.cell = cell;
    }
    let board = Vec2::new(columns as f32 * view.cell, rows as f32 * view.cell);
    let (response, painter) =
        ui.allocate_painter(board + Vec2::splat(GUTTER), Sense::click_and_drag());
    let origin = response.rect.min + Vec2::splat(GUTTER);
    let visuals = ui.visuals().clone();

    let cell_rect = |x: i32, z: i32| {
        Rect::from_min_size(
            origin
                + Vec2::new(
                    (x - min_x) as f32 * view.cell,
                    (z - min_z) as f32 * view.cell,
                ),
            Vec2::splat(view.cell),
        )
    };
    let cell_at = |position: Pos2| -> Option<(i8, i8)> {
        let local = position - origin;
        if local.x < 0.0 || local.y < 0.0 {
            return None;
        }
        let x = min_x + (local.x / view.cell).floor() as i32;
        let z = min_z + (local.y / view.cell).floor() as i32;
        if x > max_x || z > max_z {
            return None;
        }
        Some((i8::try_from(x).ok()?, i8::try_from(z).ok()?))
    };

    let pointer = response
        .hover_pos()
        .or_else(|| response.interact_pointer_pos());
    // Finish a cursor-anchored zoom now that the board origin is known.
    if view.cell != previous_cell
        && let Some(position) = pointer
    {
        let anchor = (position - origin) / previous_cell;
        ui.scroll_with_delta(-(anchor * (view.cell - previous_cell)));
    }
    view.hovered = pointer.and_then(cell_at);

    painter.rect_filled(
        Rect::from_min_size(origin, board),
        CornerRadius::same(3),
        visuals.extreme_bg_color,
    );

    // A band down the hovered row and column, so a cell can be lined up with
    // its coordinates without counting squares.
    if let Some((hover_x, hover_z)) = view.hovered {
        let tint = visuals.strong_text_color().gamma_multiply(0.07);
        let column = cell_rect(i32::from(hover_x), min_z);
        painter.rect_filled(
            Rect::from_min_size(column.min, Vec2::new(view.cell, board.y)),
            CornerRadius::ZERO,
            tint,
        );
        let row = cell_rect(min_x, i32::from(hover_z));
        painter.rect_filled(
            Rect::from_min_size(row.min, Vec2::new(board.x, view.cell)),
            CornerRadius::ZERO,
            tint,
        );
    }

    let label_font = FontId::proportional((view.cell * 0.34).clamp(9.0, 12.0));
    // Without a direction mark competing for the cell, the digit can take the
    // space it needs to stay readable.
    let digit_font = FontId::proportional((view.cell * 0.52).clamp(11.0, 24.0));
    let badge_font = FontId::proportional((view.cell * 0.30).clamp(8.0, 12.0));
    let big_badge_font = FontId::proportional((view.cell * 0.62).clamp(8.0, 15.0));
    // Thin out gutter labels when cells are too small to hold them.
    let label_step = if view.cell >= 26.0 {
        1
    } else if view.cell >= 16.0 {
        2
    } else {
        GUIDE_EVERY
    };

    for x in min_x..=max_x {
        let hovered = view.hovered.is_some_and(|(hx, _)| i32::from(hx) == x);
        if !hovered && x != 0 && x.rem_euclid(label_step) != 0 {
            continue;
        }
        painter.text(
            pos2(cell_rect(x, min_z).center().x, origin.y - GUTTER * 0.5),
            Align2::CENTER_CENTER,
            x.to_string(),
            label_font.clone(),
            if hovered || x == 0 {
                visuals.strong_text_color()
            } else {
                visuals.weak_text_color()
            },
        );
    }
    for z in min_z..=max_z {
        let hovered = view.hovered.is_some_and(|(_, hz)| i32::from(hz) == z);
        if !hovered && z != 0 && z.rem_euclid(label_step) != 0 {
            continue;
        }
        painter.text(
            pos2(origin.x - GUTTER * 0.5, cell_rect(min_x, z).center().y),
            Align2::CENTER_CENTER,
            z.to_string(),
            label_font.clone(),
            if hovered || z == 0 {
                visuals.strong_text_color()
            } else {
                visuals.weak_text_color()
            },
        );
    }

    let hair = visuals.widgets.noninteractive.bg_stroke.color;
    let guide = Stroke::new(1.0, hair.gamma_multiply(2.0));
    let axis = Stroke::new(1.0, visuals.strong_text_color().gamma_multiply(0.45));
    for x in min_x..=max_x + 1 {
        let stroke = if x == 0 {
            axis
        } else if x.rem_euclid(GUIDE_EVERY) == 0 {
            guide
        } else {
            continue;
        };
        let left = origin.x + (x - min_x) as f32 * view.cell;
        painter.line_segment(
            [pos2(left, origin.y), pos2(left, origin.y + board.y)],
            stroke,
        );
    }
    for z in min_z..=max_z + 1 {
        let stroke = if z == 0 {
            axis
        } else if z.rem_euclid(GUIDE_EVERY) == 0 {
            guide
        } else {
            continue;
        };
        let top = origin.y + (z - min_z) as f32 * view.cell;
        painter.line_segment([pos2(origin.x, top), pos2(origin.x + board.x, top)], stroke);
    }

    let hairline = Stroke::new(1.0, hair);
    for x in min_x..=max_x {
        for z in min_z..=max_z {
            let rect = cell_rect(x, z).shrink(0.5);
            let cell = rows_at(config, x as i8, z as i8, view.layer);
            let Some(primary) = cell.here.first() else {
                painter.rect_stroke(rect, CornerRadius::ZERO, hairline, StrokeKind::Inside);
                if view.show_other_layers
                    && let Some(rotation) = cell.ghost
                {
                    // A ghost keeps its rotation colour but stays faint, so a
                    // structure can be traced across layers without competing
                    // with the layer being edited.
                    painter.rect_filled(
                        rect.shrink(rect.width() * 0.3),
                        CornerRadius::same(1),
                        rotation_color(rotation).gamma_multiply(0.4),
                    );
                }
                continue;
            };

            painter.rect_filled(
                rect,
                CornerRadius::same(2),
                rotation_color(primary.rotation),
            );
            // Netherrack uses a different model selector from ordinary blocks
            // and can never share one, so the family gets a ring in netherrack's
            // own red. It is the netherrack cue that still reads when a cell is
            // only a few points across.
            if matches!(primary.kind, RotationKind::Netherrack(_)) {
                let width = (view.cell * 0.10).clamp(1.5, 3.5);
                painter.rect_stroke(
                    rect,
                    CornerRadius::same(2),
                    Stroke::new(width, NETHERRACK_RING),
                    StrokeKind::Inside,
                );
            }
            // Any side row on the block, not just the one driving the fill: a
            // stone block commonly carries both its top face and a side face,
            // and the bar is now the only thing that marks the side row.
            let side = cell
                .here
                .iter()
                .find(|info| matches!(info.kind, RotationKind::StandardSide));
            if let Some(side) = side {
                draw_side_bar(&painter, rect, side.rotation, ON_FILL.gamma_multiply(0.8));
            }

            let digit = view.cell >= DIGIT_MIN_CELL;
            if digit {
                painter.text(
                    digit_position(rect, side.map(|info| info.rotation)),
                    Align2::CENTER_CENTER,
                    primary.rotation.to_string(),
                    digit_font.clone(),
                    ON_FILL,
                );
            }
            if view.cell >= BADGE_MIN_CELL {
                // Every badge when there is room, because which faces a block
                // carries is worth seeing at a glance. When the digit has been
                // dropped the badge moves to the middle and grows into the
                // space, rather than staying a corner mark nobody can read.
                let badges: String = cell
                    .here
                    .iter()
                    .map(|info| brush_of(info).badge())
                    .collect::<Vec<_>>()
                    .concat();
                if !badges.is_empty() {
                    let (position, align, font, ink) = if digit {
                        (
                            rect.right_bottom() + Vec2::new(-2.0, -1.0),
                            Align2::RIGHT_BOTTOM,
                            badge_font.clone(),
                            ON_FILL.gamma_multiply(0.8),
                        )
                    } else {
                        (
                            rect.center(),
                            Align2::CENTER_CENTER,
                            big_badge_font.clone(),
                            ON_FILL,
                        )
                    };
                    painter.text(position, align, badges, font, ink);
                }
            }
            if view.show_other_layers && cell.elsewhere > 0 {
                painter.circle_filled(
                    rect.left_top() + Vec2::splat(rect.width() * 0.16),
                    (rect.width() * 0.07).max(1.5),
                    ON_FILL.gamma_multiply(0.7),
                );
            }
        }
    }

    // The origin is the coordinate every offset is relative to, so it is marked
    // last and stays visible over a painted cell.
    let origin_rect = cell_rect(0, 0);
    if (min_x..=max_x).contains(&0) && (min_z..=max_z).contains(&0) {
        painter.rect_stroke(
            origin_rect.shrink(0.5),
            CornerRadius::same(2),
            Stroke::new(2.0, visuals.strong_text_color()),
            StrokeKind::Inside,
        );
    }

    let mut changed = false;
    if let Some((x, z)) = view.hovered {
        let rect = cell_rect(i32::from(x), i32::from(z)).shrink(0.5);
        painter.rect_stroke(
            rect,
            CornerRadius::same(2),
            Stroke::new(1.5, visuals.strong_text_color().gamma_multiply(0.8)),
            StrokeKind::Outside,
        );

        if response.dragged_by(egui::PointerButton::Secondary) || response.secondary_clicked() {
            changed |= config.erase(x, view.layer, z);
        } else if response.clicked() {
            // Clicking a cell that already holds exactly what the brush would
            // paint advances the rotation, so repeated clicks cycle a cell.
            let repeat = config
                .index_at(x, view.layer, z, view.brush)
                .is_some_and(|index| config.filter[index].rotation == view.rotation);
            if repeat {
                view.cycle_rotation();
            }
            config.paint(x, view.layer, z, view.brush, view.rotation);
            changed = true;
        } else if response.dragged_by(egui::PointerButton::Primary) {
            let already = config
                .index_at(x, view.layer, z, view.brush)
                .is_some_and(|index| config.filter[index].rotation == view.rotation);
            if !already {
                config.paint(x, view.layer, z, view.brush, view.rotation);
                changed = true;
            }
        }

        let cell = rows_at(config, x, z, view.layer);
        if !cell.here.is_empty() {
            let mut tooltip = cell
                .here
                .iter()
                .map(|info| row_text(info))
                .collect::<Vec<_>>()
                .join("\n");
            if cell.elsewhere > 0 {
                tooltip.push_str(&format!("\n({} row(s) on other layers)", cell.elsewhere));
            }
            // Anchored to the pointer: this widget is the whole board, and the
            // default placement would drop the tooltip below the entire grid,
            // far from the cell it describes.
            response.clone().on_hover_text_at_pointer(tooltip);
        }
    }

    // Middle-drag pans, which beats reaching for the scrollbars on a filter
    // that is larger than the viewport.
    if response.dragged_by(egui::PointerButton::Middle) {
        ui.scroll_with_delta(response.drag_delta());
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitting_uses_the_tighter_axis_and_respects_the_zoom_limits() {
        // Room for 50-point cells across but only 30-point cells down; the
        // tighter axis has to win, or the board would overflow the window.
        let room_for = |cell: f32, count: i32| cell * count as f32 + GUTTER + SCROLLBAR_ALLOWANCE;
        let available = Vec2::new(room_for(50.0, 10), room_for(30.0, 20));
        assert_eq!(fit_cell(available, 10, 20), Some(30.0));

        // A window too small for the whole board keeps cells readable and lets
        // the board overflow, rather than shrinking it into illegibility.
        assert_eq!(
            fit_cell(Vec2::new(200.0, 200.0), 80, 80),
            Some(AUTO_FIT_MIN_CELL)
        );
        // And a tiny filter in a large window stops at the maximum.
        assert_eq!(fit_cell(Vec2::new(4000.0, 4000.0), 2, 2), Some(MAX_CELL));

        // No room at all is not a fit.
        assert_eq!(fit_cell(Vec2::new(10.0, 400.0), 10, 10), None);
        assert_eq!(fit_cell(Vec2::new(f32::INFINITY, 400.0), 10, 10), None);
    }

    #[test]
    fn ghost_reports_rows_from_other_layers_only() {
        let mut config = EditableConfig::default();
        config.paint(1, 0, 2, Brush::FourWay, 3);
        config.paint(1, 4, 2, Brush::FourWay, 1);

        let on_layer = rows_at(&config, 1, 2, 0);
        assert_eq!(on_layer.here.len(), 1);
        assert_eq!(on_layer.elsewhere, 1);
        assert_eq!(on_layer.ghost, Some(1));

        let empty_layer = rows_at(&config, 1, 2, 9);
        assert!(empty_layer.here.is_empty());
        assert_eq!(empty_layer.elsewhere, 2);
    }
}
