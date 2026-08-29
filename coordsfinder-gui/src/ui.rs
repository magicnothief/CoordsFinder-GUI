//! Panel layout and widgets for [`CoordsFinderApp`].

use std::path::PathBuf;

use egui::{Color32, RichText};

use coordsfinder::config::ScanOrder;

use crate::app::{CoordsFinderApp, Editor, Results, duration, thousands};
use crate::grid::{self, rotation_color};
use crate::model::{ALGORITHMS, Brush, DIRECTIONS, OFFSET_MAX, OFFSET_MIN};
use crate::runner::BackendChoice;

impl eframe::App for CoordsFinderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump();
        self.take_dropped_file(ctx);
        self.handle_shortcuts(ctx);

        egui::TopBottomPanel::top("menu").show(ctx, |ui| self.menu_bar(ui));
        egui::SidePanel::left("settings")
            .resizable(true)
            .default_width(300.0)
            .width_range(260.0..=440.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.settings_panel(ui));
            });
        egui::TopBottomPanel::bottom("results")
            .resizable(true)
            .default_height(280.0)
            // A bottom panel shrinks to its content, so an empty match list
            // would collapse the pane; the floor keeps it a stable size.
            .height_range(170.0..=560.0)
            .show(ctx, |ui| self.results_panel(ui));
        egui::CentralPanel::default().show(ctx, |ui| self.filter_panel(ui));

        // Validation is cheap but not free, so it runs once per changed frame
        // rather than inside every widget callback.
        self.revalidate_if_needed();

        if self.scanning() {
            // Keep the rate and ETA moving even when no update has arrived.
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Dropping the handle cancels and joins any running scan.
        self.scan = None;
    }
}

impl CoordsFinderApp {
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let (open, save) = ctx.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::COMMAND, egui::Key::O),
                input.consume_key(egui::Modifiers::COMMAND, egui::Key::S),
            )
        });
        if open {
            self.open_dialog();
        }
        if save {
            self.save_current();
        }
    }

    /// Opens a `.conf` dropped onto the window.
    fn take_dropped_file(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|input| input.raw.dropped_files.clone());
        if let Some(path) = dropped.into_iter().find_map(|file| file.path) {
            self.open(&path);
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    self.new_document();
                    ui.close();
                }
                if ui.button("Open…").clicked() {
                    self.open_dialog();
                    ui.close();
                }
                if ui.button("Save").clicked() {
                    self.save_current();
                    ui.close();
                }
                if ui.button("Save as…").clicked() {
                    self.save_as_dialog();
                    ui.close();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Filter", |ui| {
                if ui.button("Fit view to filter").clicked() {
                    self.view.fit(&self.config);
                    ui.close();
                }
                if ui.button("Clear this layer").clicked() {
                    let layer = self.view.layer;
                    self.config.filter.retain(|info| info.y != layer);
                    self.after_filter_change();
                    ui.close();
                }
                if ui.button("Clear all rows").clicked() {
                    self.config.filter.clear();
                    self.after_filter_change();
                    ui.close();
                }
            });
            ui.separator();
            let title = match &self.path {
                Some(path) => path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                ),
                None => "untitled.conf".to_owned(),
            };
            ui.label(RichText::new(title).strong());
            if self.unsaved {
                ui.label(RichText::new("• unsaved").weak());
            }
        });
    }

    fn settings_panel(&mut self, ui: &mut egui::Ui) {
        let running = self.scanning();
        ui.add_space(4.0);
        ui.heading("Search");
        ui.add_enabled_ui(!running, |ui| {
            egui::Grid::new("search")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Algorithm");
                    egui::ComboBox::from_id_salt("algorithm")
                        .width(150.0)
                        .selected_text(self.config.algorithm.to_string())
                        .show_ui(ui, |ui| {
                            for algorithm in ALGORITHMS {
                                ui.selectable_value(
                                    &mut self.config.algorithm,
                                    algorithm,
                                    algorithm.to_string(),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label("Scan order");
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.config.scan_order,
                            ScanOrder::Spiral,
                            "Spiral",
                        );
                        ui.selectable_value(
                            &mut self.config.scan_order,
                            ScanOrder::Linear,
                            "Linear",
                        );
                    });
                    ui.end_row();

                    ui.label("Directions")
                        .on_hover_text("Rotations of the filter to try. Use all four when the screenshot's facing is unknown.");
                    ui.horizontal(|ui| {
                        for (index, direction) in DIRECTIONS.iter().enumerate() {
                            ui.toggle_value(
                                &mut self.config.directions[index],
                                format!("{direction}°"),
                            );
                        }
                    });
                    ui.end_row();
                });

            ui.add_space(6.0);
            ui.label(RichText::new("Ranges (end is exclusive)").weak());
            egui::Grid::new("ranges")
                .num_columns(3)
                .spacing([6.0, 6.0])
                .show(ui, |ui| {
                    for (label, range) in [
                        ("X", &mut self.config.x_range),
                        ("Y", &mut self.config.y_range),
                        ("Z", &mut self.config.z_range),
                    ] {
                        ui.label(label);
                        ui.add(egui::DragValue::new(&mut range.start).speed(16.0));
                        ui.add(egui::DragValue::new(&mut range.end).speed(16.0));
                        ui.end_row();
                    }
                });

            ui.add_space(6.0);
            egui::Grid::new("tolerance")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Error tolerance")
                        .on_hover_text("Blocks allowed to mismatch. Above 3 this gets very slow.");
                    ui.add(
                        egui::DragValue::new(&mut self.config.error_tolerance)
                            .speed(0.05)
                            .range(0..=16),
                    );
                    ui.end_row();
                });

            ui.collapsing("Advanced", |ui| {
                egui::Grid::new("advanced")
                    .num_columns(3)
                    .spacing([6.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("CPU tile");
                        ui.add(
                            egui::DragValue::new(&mut self.config.cpu_tile_size.x)
                                .speed(8.0)
                                .range(1..=i32::MAX),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.config.cpu_tile_size.z)
                                .speed(8.0)
                                .range(1..=i32::MAX),
                        );
                        ui.end_row();
                        ui.label("GPU tile")
                            .on_hover_text("Lower this if a tile could hold more than 262,144 matches.");
                        ui.add(
                            egui::DragValue::new(&mut self.config.gpu_tile_size.x)
                                .speed(64.0)
                                .range(1..=i32::MAX),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.config.gpu_tile_size.z)
                                .speed(64.0)
                                .range(1..=i32::MAX),
                        );
                        ui.end_row();
                    });
                ui.checkbox(&mut self.config.verbose, "Verbose progress");
            });
        });

        ui.add_space(8.0);
        ui.separator();
        ui.heading("Run");
        ui.add_enabled_ui(!running, |ui| {
            egui::Grid::new("run")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Backend");
                    ui.horizontal(|ui| {
                        for choice in BackendChoice::ALL {
                            ui.selectable_value(&mut self.backend, choice, choice.label());
                        }
                    });
                    ui.end_row();

                    ui.label("CPU threads");
                    ui.add(
                        egui::DragValue::new(&mut self.threads)
                            .speed(0.2)
                            .range(1..=1024),
                    );
                    ui.end_row();
                });

            ui.horizontal(|ui| {
                if ui.button("Output file…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Text", &["txt"])
                        .set_file_name("matches.txt")
                        .save_file()
                {
                    self.output = Some(path);
                }
                if self.output.is_some() && ui.button("Clear").clicked() {
                    self.output = None;
                }
            });
            match &self.output {
                Some(path) => {
                    ui.label(RichText::new(path.display().to_string()).weak().small());
                }
                None => {
                    ui.label(
                        RichText::new("Matches are only kept in this window.")
                            .weak()
                            .small(),
                    );
                }
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.heading("Summary");
        self.summary_panel(ui);
    }

    fn summary_panel(&mut self, ui: &mut egui::Ui) {
        match &self.summary {
            Ok(summary) => {
                let candidates = if summary.saturated {
                    format!("more than {}", thousands(summary.candidates))
                } else {
                    thousands(summary.candidates)
                };
                egui::Grid::new("summary")
                    .num_columns(2)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("Filter rows");
                        ui.label(thousands(self.config.filter.len() as u64));
                        ui.end_row();
                        ui.label("Block constraints");
                        ui.label(thousands(summary.constraints as u64));
                        ui.end_row();
                        ui.label("Candidates");
                        ui.label(candidates);
                        ui.end_row();
                        ui.label("Work items");
                        ui.label(format!(
                            "{} CPU / {} GPU",
                            thousands(summary.cpu_items as u64),
                            thousands(summary.gpu_items as u64)
                        ));
                        ui.end_row();
                    });
                if let Some(warning) = &summary.warning {
                    ui.add_space(4.0);
                    ui.colored_label(
                        Color32::from_rgb(0xD8, 0x9E, 0x30),
                        format!("Warning: {warning}"),
                    );
                }
            }
            Err(error) => {
                ui.colored_label(Color32::from_rgb(0xD8, 0x5A, 0x5A), error);
            }
        }
    }

    fn filter_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.editor, Editor::Grid, "Grid");
            if ui
                .selectable_label(self.editor == Editor::Rows, "Rows")
                .clicked()
            {
                // Refresh the text from the rows the grid produced.
                self.rows_text = self.config.filter_text();
                self.rows_error = None;
                self.editor = Editor::Rows;
            }
        });
        ui.separator();
        match self.editor {
            Editor::Grid => self.grid_editor(ui),
            Editor::Rows => self.rows_editor(ui),
        }
    }

    fn grid_editor(&mut self, ui: &mut egui::Ui) {
        let layers = self.config.layers();
        ui.horizontal_wrapped(|ui| {
            ui.label("Y layer");
            ui.add(
                egui::DragValue::new(&mut self.view.layer)
                    .speed(0.1)
                    .range(OFFSET_MIN..=OFFSET_MAX),
            );
            if !layers.is_empty() {
                ui.label(RichText::new("used:").weak());
                for layer in &layers {
                    if ui
                        .selectable_label(self.view.layer == *layer, layer.to_string())
                        .clicked()
                    {
                        self.view.layer = *layer;
                    }
                }
            }
            ui.separator();
            ui.label("Zoom");
            ui.add(
                egui::Slider::new(&mut self.view.cell, 12.0..=56.0)
                    .show_value(false)
                    .trailing_fill(true),
            );
            ui.checkbox(&mut self.view.auto_fit, "Auto-fit")
                .on_hover_text("Keep three empty cells around the painted area.");
            ui.checkbox(&mut self.view.show_other_layers, "Ghost other layers");
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Paint");
            let mut brush = self.view.brush;
            egui::ComboBox::from_id_salt("brush")
                .width(180.0)
                .selected_text(brush.label())
                .show_ui(ui, |ui| {
                    for option in Brush::ALL {
                        ui.selectable_value(&mut brush, option, option.label());
                    }
                });
            if brush != self.view.brush {
                self.view.brush = brush;
                self.view.clamp_rotation();
            }
            ui.separator();
            ui.label("Rotation");
            for rotation in 0..self.view.brush.rotation_count() {
                let selected = self.view.rotation == rotation;
                let text = RichText::new(format!(" {rotation} "))
                    .color(Color32::from_rgb(0x10, 0x12, 0x16))
                    .strong();
                let button = egui::Button::new(text)
                    .fill(rotation_color(rotation))
                    .stroke(if selected {
                        egui::Stroke::new(2.0, ui.visuals().strong_text_color())
                    } else {
                        egui::Stroke::NONE
                    });
                if ui.add(button).clicked() {
                    self.view.rotation = rotation;
                }
            }
            ui.separator();
            ui.label(
                RichText::new("left-click paints or cycles · drag paints · right-click erases")
                    .weak()
                    .small(),
            );
        });

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("north ↑   +X east →   +Z south ↓")
                    .weak()
                    .small(),
            );
            if let Some((x, z)) = self.view.hovered {
                ui.separator();
                ui.label(
                    RichText::new(format!("cursor  {x} {} {z}", self.view.layer))
                        .monospace()
                        .small(),
                );
            }
        });
        ui.add_space(2.0);
        let changed = egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| grid::show(ui, &mut self.config, &mut self.view))
            .inner;
        if changed {
            self.after_filter_change();
        }
    }

    fn rows_editor(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Apply rows").clicked() {
                let path = self.source_path();
                let text = self.rows_text.clone();
                match self.config.set_filter_text(&text, &path) {
                    Ok(()) => {
                        self.rows_error = None;
                        self.after_filter_change();
                        self.note("Filter rows applied.");
                    }
                    Err(error) => self.rows_error = Some(error),
                }
            }
            if ui.button("Revert").clicked() {
                self.rows_text = self.config.filter_text();
                self.rows_error = None;
            }
            if ui.button("Copy config to clipboard").clicked() {
                ui.ctx().copy_text(self.config.to_conf_text());
                self.note("Full config copied to the clipboard.");
            }
        });
        if let Some(error) = &self.rows_error {
            ui.colored_label(Color32::from_rgb(0xD8, 0x5A, 0x5A), error);
        }
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.rows_text)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(20)
                        .hint_text("x y z | variant [side|netherrack-<face>]"),
                );
            });
    }

    fn results_panel(&mut self, ui: &mut egui::Ui) {
        let running = self.scanning();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if running {
                let cancelling = self.scan.as_ref().is_some_and(|scan| scan.cancelling());
                if ui
                    .add_enabled(!cancelling, egui::Button::new("■  Stop"))
                    .clicked()
                {
                    if let Some(scan) = &self.scan {
                        scan.cancel();
                    }
                    self.note("Stopping after the current work item…");
                }
            } else if ui
                .add_enabled(self.summary.is_ok(), egui::Button::new("▶  Start scan"))
                .clicked()
            {
                self.start_scan();
            }

            if !self.progress.backend.is_empty() {
                ui.label(RichText::new(&self.progress.backend).weak());
            }
            ui.separator();
            ui.label(format!("{} match(es)", thousands(self.progress.matches)));
            if !self.matches.is_empty() {
                if ui.button("Copy all").clicked() {
                    let text = self
                        .matches
                        .iter()
                        .map(|found| format!("{} {} {}", found.x, found.y, found.z))
                        .collect::<Vec<_>>()
                        .join("\n");
                    ui.ctx().copy_text(text);
                    self.note("Match coordinates copied to the clipboard.");
                }
                if ui.button("Save matches…").clicked() {
                    self.save_matches_dialog();
                }
                if ui.button("Clear").clicked() {
                    self.matches.clear();
                }
            }
        });

        ui.add_space(2.0);
        let rate = self.progress.rate();
        let mut text = format!(
            "{}/{} work items · {:.1} M candidates/s",
            thousands(self.progress.completed as u64),
            thousands(self.progress.items as u64),
            rate / 1_000_000.0
        );
        if let Some(remaining) = self.progress.remaining() {
            text.push_str(&format!(" · about {} left", duration(remaining)));
        }
        ui.add(
            egui::ProgressBar::new(self.progress.fraction())
                .text(text)
                .corner_radius(3),
        );

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.results, Results::Matches, "Matches");
            ui.selectable_value(&mut self.results, Results::Log, "Log");
            ui.separator();
            ui.label(RichText::new(&self.status).weak().small());
        });
        ui.separator();
        match self.results {
            Results::Matches => self.matches_pane(ui),
            Results::Log => self.log_pane(ui),
        }
    }

    fn matches_pane(&mut self, ui: &mut egui::Ui) {
        if self.matches.is_empty() {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(8.0);
                    ui.label(RichText::new("No matches yet.").weak());
                });
            return;
        }
        let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
        let mut copy: Option<String> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show_rows(ui, row_height, self.matches.len(), |ui, range| {
                for index in range {
                    let found = self.matches[index];
                    let line = format!(
                        "{:>10} {:>5} {:>10}   dir {:>3}°   {} mismatch(es)",
                        found.x, found.y, found.z, found.direction, found.mismatches
                    );
                    if ui
                        .add(
                            egui::Label::new(RichText::new(line).monospace())
                                .sense(egui::Sense::click()),
                        )
                        .on_hover_text("Click to copy these coordinates")
                        .clicked()
                    {
                        copy = Some(format!("{} {} {}", found.x, found.y, found.z));
                    }
                }
            });
        if let Some(text) = copy {
            ui.ctx().copy_text(text.clone());
            self.note(format!("Copied {text}."));
        }
    }

    fn log_pane(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &self.log {
                    ui.label(RichText::new(line).monospace().small());
                }
            });
    }

    fn save_matches_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Text", &["txt"])
            .set_file_name("matches.txt")
            .save_file()
        else {
            return;
        };
        let text: String = self
            .matches
            .iter()
            .map(|found| {
                format!(
                    "Found with {} mismatch(es)! ({}, {}, {}), direction {}\n",
                    found.mismatches, found.x, found.y, found.z, found.direction
                )
            })
            .collect();
        match std::fs::write(&path, text) {
            Ok(()) => self.note(format!(
                "Wrote {} matches to {}.",
                self.matches.len(),
                path.display()
            )),
            Err(error) => self.note(format!("Could not write {}: {error}", path.display())),
        }
    }

    fn save_current(&mut self) {
        match self.path.clone() {
            Some(path) => self.save(&path),
            None => self.save_as_dialog(),
        }
    }

    fn new_document(&mut self) {
        let blank: Option<PathBuf> = None;
        self.reset_document(crate::model::EditableConfig::default(), blank);
        self.note("New config.");
    }
}
