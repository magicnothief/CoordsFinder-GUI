//! Click-to-paint editor for one Y layer of a filter.
//!
//! The grid is drawn in Minecraft's top-down map orientation: `+X` runs right
//! (east) and `+Z` runs down (south), so north is up, matching what a player
//! sees on the F3 screen. Each painted cell shows the rotation digit that will
//! be written to the config file, so the picture and the text always agree.

use egui::{Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Vec2};

use coordsfinder::types::RotationInfo;

use crate::model::{Brush, EditableConfig, OFFSET_MAX, OFFSET_MIN, brush_of, row_text};

/// Empty cells kept around the painted area so the filter can always grow.
const AUTO_FIT_MARGIN: i32 = 3;
/// Smallest auto-fitted view, in cells.
const MIN_VIEW_CELLS: i32 = 10;
/// Width of the coordinate gutters along the top and left edges.
const GUTTER: f32 = 26.0;

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
    /// Keep the view fitted around the painted rows.
    pub auto_fit: bool,
    /// Inclusive X/Z view bounds, used when `auto_fit` is off.
    pub bounds: (i32, i32, i32, i32),
    /// Cell the pointer is over, for the status line.
    pub hovered: Option<(i8, i8)>,
}

impl Default for GridView {
    fn default() -> Self {
        Self {
            layer: 0,
            brush: Brush::FourWay,
            rotation: 0,
            cell: 30.0,
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

/// Rows at one cell, split into the current layer and other layers.
struct CellRows<'a> {
    here: Vec<&'a RotationInfo>,
    elsewhere: usize,
}

fn rows_at<'a>(config: &'a EditableConfig, x: i8, z: i8, layer: i8) -> CellRows<'a> {
    let mut here = Vec::new();
    let mut elsewhere = 0;
    for info in &config.filter {
        if info.x != x || info.z != z {
            continue;
        }
        if info.y == layer {
            here.push(info);
        } else {
            elsewhere += 1;
        }
    }
    CellRows { here, elsewhere }
}

/// Draws the grid and applies any painting the user did. Returns whether the
/// filter changed.
pub fn show(ui: &mut egui::Ui, config: &mut EditableConfig, view: &mut GridView) -> bool {
    if view.auto_fit {
        view.fit(config);
    }
    let (min_x, max_x, min_z, max_z) = view.bounds;
    let columns = (max_x - min_x + 1).max(1);
    let rows = (max_z - min_z + 1).max(1);
    let size = Vec2::new(
        GUTTER + columns as f32 * view.cell,
        GUTTER + rows as f32 * view.cell,
    );
    let (response, painter) = ui.allocate_painter(size, Sense::click_and_drag());
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

    // Board background, so empty cells read as part of one surface.
    painter.rect_filled(
        Rect::from_min_size(
            origin,
            Vec2::new(columns as f32 * view.cell, rows as f32 * view.cell),
        ),
        CornerRadius::same(3),
        visuals.extreme_bg_color,
    );

    let label_font = FontId::proportional((view.cell * 0.34).clamp(9.0, 12.0));
    let digit_font = FontId::proportional((view.cell * 0.5).clamp(11.0, 20.0));
    let badge_font = FontId::proportional((view.cell * 0.3).clamp(8.0, 11.0));
    // Thin out gutter labels when cells are too small to hold them.
    let label_step = if view.cell >= 26.0 {
        1
    } else if view.cell >= 16.0 {
        2
    } else {
        5
    };

    for x in min_x..=max_x {
        if x.rem_euclid(label_step) != 0 && x != 0 {
            continue;
        }
        let rect = cell_rect(x, min_z);
        painter.text(
            egui::pos2(rect.center().x, origin.y - GUTTER * 0.5),
            Align2::CENTER_CENTER,
            x.to_string(),
            label_font.clone(),
            if x == 0 {
                visuals.strong_text_color()
            } else {
                visuals.weak_text_color()
            },
        );
    }
    for z in min_z..=max_z {
        if z.rem_euclid(label_step) != 0 && z != 0 {
            continue;
        }
        let rect = cell_rect(min_x, z);
        painter.text(
            egui::pos2(origin.x - GUTTER * 0.5, rect.center().y),
            Align2::CENTER_CENTER,
            z.to_string(),
            label_font.clone(),
            if z == 0 {
                visuals.strong_text_color()
            } else {
                visuals.weak_text_color()
            },
        );
    }

    let grid_line = Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color);
    for x in min_x..=max_x {
        for z in min_z..=max_z {
            let rect = cell_rect(x, z).shrink(0.5);
            let cell = rows_at(config, x as i8, z as i8, view.layer);
            if let Some(primary) = cell.here.first() {
                painter.rect_filled(
                    rect,
                    CornerRadius::same(2),
                    rotation_color(primary.rotation),
                );
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    primary.rotation.to_string(),
                    digit_font.clone(),
                    Color32::from_rgb(0x10, 0x12, 0x16),
                );
                let badge = brush_of(primary).badge();
                if !badge.is_empty() {
                    painter.text(
                        rect.right_top() + Vec2::new(-2.0, 2.0),
                        Align2::RIGHT_TOP,
                        badge,
                        badge_font.clone(),
                        Color32::from_black_alpha(190),
                    );
                }
                if cell.here.len() > 1 {
                    painter.text(
                        rect.left_bottom() + Vec2::new(2.0, -2.0),
                        Align2::LEFT_BOTTOM,
                        format!("+{}", cell.here.len() - 1),
                        badge_font.clone(),
                        Color32::from_black_alpha(190),
                    );
                }
            } else {
                painter.rect_stroke(rect, CornerRadius::ZERO, grid_line, StrokeKind::Inside);
                if view.show_other_layers && cell.elsewhere > 0 {
                    painter.circle_filled(
                        rect.center(),
                        (view.cell * 0.12).max(2.0),
                        visuals.weak_text_color().gamma_multiply(0.6),
                    );
                }
            }
            if x == 0 && z == 0 {
                painter.rect_stroke(
                    rect,
                    CornerRadius::same(2),
                    Stroke::new(2.0, visuals.strong_text_color()),
                    StrokeKind::Inside,
                );
            }
        }
    }

    // Painting.
    let mut changed = false;
    view.hovered = None;
    let pointer = response
        .hover_pos()
        .or_else(|| response.interact_pointer_pos());
    let cell_at = |position: egui::Pos2| -> Option<(i8, i8)> {
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

    if let Some(position) = pointer
        && let Some((x, z)) = cell_at(position)
    {
        view.hovered = Some((x, z));
        let rect = cell_rect(i32::from(x), i32::from(z)).shrink(0.5);
        painter.rect_stroke(
            rect,
            CornerRadius::same(2),
            Stroke::new(1.5, visuals.strong_text_color().gamma_multiply(0.7)),
            StrokeKind::Outside,
        );

        let secondary = ui.input(|input| input.pointer.secondary_down());
        if response.secondary_clicked() || (response.dragged() && secondary) {
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
        } else if response.dragged() {
            let already = config
                .index_at(x, view.layer, z, view.brush)
                .is_some_and(|index| config.filter[index].rotation == view.rotation);
            if !already {
                config.paint(x, view.layer, z, view.brush, view.rotation);
                changed = true;
            }
        }
    }

    if let Some((x, z)) = view.hovered {
        let cell = rows_at(config, x, z, view.layer);
        if !cell.here.is_empty() {
            let tooltip = cell
                .here
                .iter()
                .map(|info| row_text(info))
                .collect::<Vec<_>>()
                .join("\n");
            // Anchored to the pointer: this widget is the whole board, and the
            // default placement would drop the tooltip below the entire grid,
            // far from the cell it describes.
            response.clone().on_hover_text_at_pointer(tooltip);
        }
    }

    changed
}
