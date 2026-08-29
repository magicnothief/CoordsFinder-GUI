//! The CoordsFinder GUI application: config editing, validation, and scanning.

use std::path::{Path, PathBuf};
use std::time::Instant;

use coordsfinder::config::ScanConfig;
use coordsfinder::filter::prepare_filters;
use coordsfinder::scan::make_plan;
use coordsfinder::types::Match;

use crate::grid::GridView;
use crate::history::History;
use crate::model::EditableConfig;
use crate::runner::{self, BackendChoice, ScanHandle, ScanRequest, Update};

/// Log lines kept in the log pane.
const LOG_LIMIT: usize = 500;
/// Placeholder path used when the document has not been saved yet, so parser
/// error messages still read sensibly.
const UNSAVED_NAME: &str = "(unsaved).conf";
/// Placeholder path used while parsing pasted text, for the same reason.
const PASTED_NAME: &str = "(pasted config)";

/// Which editor is shown in the central panel.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Editor {
    Grid,
    Rows,
}

/// Which pane is shown in the results panel.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Results {
    Matches,
    Log,
}

/// Everything derived from a valid config, cached until the config changes.
pub struct Summary {
    pub config: ScanConfig,
    pub constraints: usize,
    pub cpu_items: usize,
    pub gpu_items: usize,
    pub candidates: u64,
    pub saturated: bool,
    pub warning: Option<String>,
}

/// The paste-a-config dialog.
///
/// WebCoordsFinder offers a config on the clipboard as well as a file, so the
/// text is validated as it is pasted and the load is only offered once it
/// parses. `parsed` is recomputed only when the text changes.
pub struct PasteDialog {
    pub text: String,
    pub parsed: Result<ScanConfig, String>,
    /// The text `parsed` was produced from; `None` before the first check.
    checked: Option<String>,
    /// Cleared after the first frame, once the text area has been focused.
    pub focus: bool,
}

impl PasteDialog {
    pub fn new(text: String) -> Self {
        let mut dialog = Self {
            text,
            parsed: Err(String::new()),
            checked: None,
            focus: true,
        };
        dialog.refresh();
        dialog
    }

    /// Re-parses the pasted text if it changed since the last check.
    pub fn refresh(&mut self) {
        if self.checked.as_deref() == Some(self.text.as_str()) {
            return;
        }
        self.checked = Some(self.text.clone());
        self.parsed = if self.text.trim().is_empty() {
            Err("Paste a config here, or press Ctrl+V.".to_owned())
        } else {
            coordsfinder::config::parse(&self.text, PASTED_NAME)
        };
    }
}

/// Live state of a scan, kept separately so it survives across frames.
#[derive(Default)]
pub struct Progress {
    pub backend: String,
    pub items: usize,
    pub completed: usize,
    pub candidates: u64,
    pub total_candidates: u64,
    pub saturated: bool,
    pub matches: u64,
    pub started: Option<Instant>,
    pub elapsed: f64,
}

impl Progress {
    pub fn fraction(&self) -> f32 {
        if self.items == 0 {
            return 0.0;
        }
        (self.completed as f32 / self.items as f32).clamp(0.0, 1.0)
    }

    /// Candidates checked per second, from the live or final elapsed time.
    pub fn rate(&self) -> f64 {
        let elapsed = match self.started {
            Some(started) => started.elapsed().as_secs_f64(),
            None => self.elapsed,
        };
        if elapsed <= 0.0 {
            return 0.0;
        }
        self.candidates as f64 / elapsed
    }

    /// Seconds of work left, or `None` when it cannot be estimated yet.
    pub fn remaining(&self) -> Option<f64> {
        let rate = self.rate();
        if rate <= 0.0 || self.saturated || self.total_candidates <= self.candidates {
            return None;
        }
        Some((self.total_candidates - self.candidates) as f64 / rate)
    }
}

/// The application.
pub struct CoordsFinderApp {
    /// Kept so the scan thread can wake the UI from off the UI thread.
    ctx: egui::Context,

    pub config: EditableConfig,
    pub path: Option<PathBuf>,
    pub unsaved: bool,

    pub summary: Result<Summary, String>,
    /// Undo and redo for `config`.
    pub history: History,
    /// The document as of the last save or load, so undoing back to it clears
    /// the unsaved marker instead of leaving it stuck on.
    pub saved: EditableConfig,

    pub view: GridView,
    pub editor: Editor,
    pub rows_text: String,
    pub rows_error: Option<String>,
    pub paste: Option<PasteDialog>,

    pub backend: BackendChoice,
    pub threads: usize,
    pub output: Option<PathBuf>,

    pub scan: Option<ScanHandle>,
    pub progress: Progress,
    pub matches: Vec<Match>,
    pub log: Vec<String>,
    pub results: Results,
    pub status: String,
}

impl CoordsFinderApp {
    /// Builds the application, optionally opening a config given on the CLI.
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        cc.egui_ctx.style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        });
        let mut app = Self {
            ctx: cc.egui_ctx.clone(),
            config: EditableConfig::default(),
            path: None,
            unsaved: false,
            summary: Err(String::new()),
            history: History::new(&EditableConfig::default()),
            saved: EditableConfig::default(),
            view: GridView::default(),
            editor: Editor::Grid,
            rows_text: String::new(),
            rows_error: None,
            paste: None,
            backend: BackendChoice::default(),
            threads: std::thread::available_parallelism().map_or(1, |count| count.get()),
            output: None,
            scan: None,
            progress: Progress::default(),
            matches: Vec::new(),
            log: Vec::new(),
            results: Results::Matches,
            status: "Ready.".to_owned(),
        };
        match initial {
            Some(path) => app.open(&path),
            None => app.revalidate(),
        }
        app
    }

    /// Path used for parser error messages and for [`ScanConfig::source_path`].
    pub fn source_path(&self) -> PathBuf {
        self.path
            .clone()
            .unwrap_or_else(|| PathBuf::from(UNSAVED_NAME))
    }

    pub fn note(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.status = message.clone();
        self.log.push(message);
        if self.log.len() > LOG_LIMIT {
            let excess = self.log.len() - LOG_LIMIT;
            self.log.drain(..excess);
        }
    }

    /// Re-runs validation through the real config parser.
    pub fn revalidate(&mut self) {
        let path = self.source_path();
        self.summary = self.config.to_scan_config(&path).and_then(summarize);
    }

    /// Records edits for undo and revalidates when the document changed.
    ///
    /// Runs once per frame, after the panels. A burst of edits is left open
    /// while the pointer is held, so one gesture — a paint stroke, a drag on a
    /// range — becomes one undo step instead of dozens.
    pub fn sync_document(&mut self, ctx: &egui::Context) {
        let settled = !ctx.input(|input| input.pointer.any_down());
        if self.history.track(&self.config, settled) {
            self.unsaved = self.config != self.saved;
            self.revalidate();
        }
    }

    pub fn reset_document(&mut self, config: EditableConfig, path: Option<PathBuf>) {
        self.config = config;
        self.path = path;
        self.unsaved = false;
        self.saved = self.config.clone();
        self.history.reset(&self.config);
        self.rows_text = self.config.filter_text();
        self.rows_error = None;
        self.view.fit(&self.config);
        // Start on a layer that actually holds rows.
        if let Some(layer) = self.config.layers().first() {
            self.view.layer = *layer;
        }
        self.revalidate();
    }

    /// Steps back one edit.
    pub fn undo(&mut self) {
        let mut config = std::mem::take(&mut self.config);
        let stepped = self.history.undo(&mut config);
        self.config = config;
        self.after_history_step(stepped, "Undo", "Nothing to undo.");
    }

    /// Steps forward one undone edit.
    pub fn redo(&mut self) {
        let mut config = std::mem::take(&mut self.config);
        let stepped = self.history.redo(&mut config);
        self.config = config;
        self.after_history_step(stepped, "Redo", "Nothing to redo.");
    }

    fn after_history_step(&mut self, stepped: bool, action: &str, empty: &str) {
        if !stepped {
            self.note(empty);
            return;
        }
        // The rows editor holds its own copy of the filter text, so it has to
        // be refreshed or it would still show the state that was just undone.
        self.rows_text = self.config.filter_text();
        self.rows_error = None;
        // Undoing back to the last saved state is not an unsaved change.
        self.unsaved = self.config != self.saved;
        self.revalidate();
        self.note(format!(
            "{action}: {} filter row(s).",
            self.config.filter.len()
        ));
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn open(&mut self, path: &Path) {
        match coordsfinder::config::load(path) {
            Ok(config) => {
                let editable = EditableConfig::from_scan_config(&config);
                self.reset_document(editable, Some(path.to_owned()));
                self.note(format!(
                    "Opened {} ({} filter rows).",
                    path.display(),
                    self.config.filter.len()
                ));
            }
            Err(error) => self.note(format!("Could not open: {error}")),
        }
    }

    /// Loads config text that did not come from a file.
    ///
    /// The document keeps no path, so the next save asks where to put it rather
    /// than silently overwriting whatever was open before.
    pub fn load_config_text(&mut self, text: &str) {
        match coordsfinder::config::parse(text, PASTED_NAME) {
            Ok(config) => {
                let editable = EditableConfig::from_scan_config(&config);
                self.reset_document(editable, None);
                self.unsaved = true;
                self.note(format!(
                    "Loaded a pasted config ({} filter rows). Save it to keep it.",
                    self.config.filter.len()
                ));
            }
            Err(error) => self.note(format!("Could not load the pasted config: {error}")),
        }
    }

    /// Opens the paste dialog, pre-filled with `text`.
    pub fn open_paste_dialog(&mut self, text: String) {
        self.paste = Some(PasteDialog::new(text));
    }

    pub fn save(&mut self, path: &Path) {
        match std::fs::write(path, self.config.to_conf_text()) {
            Ok(()) => {
                self.path = Some(path.to_owned());
                self.unsaved = false;
                // The new baseline for the unsaved marker: undoing back to
                // here should read as saved again. History itself is kept, so
                // a save does not cost the user their undo steps.
                self.saved = self.config.clone();
                self.note(format!("Saved {}.", path.display()));
            }
            Err(error) => self.note(format!("Could not save {}: {error}", path.display())),
        }
    }

    pub fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("CoordsFinder config", &["conf"])
            .add_filter("All files", &["*"])
            .pick_file()
        {
            self.open(&path);
        }
    }

    pub fn save_as_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("CoordsFinder config", &["conf"])
            .set_file_name(
                self.path
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .map_or_else(
                        || "search.conf".to_owned(),
                        |name| name.to_string_lossy().into_owned(),
                    ),
            )
            .save_file()
        {
            self.save(&path);
        }
    }

    /// Marks the document dirty after the filter was edited.
    pub fn after_filter_change(&mut self) {
        self.unsaved = true;
    }

    pub fn scanning(&self) -> bool {
        self.scan.as_ref().is_some_and(|scan| !scan.finished())
    }

    pub fn start_scan(&mut self) {
        let Ok(summary) = &self.summary else {
            self.note("Cannot scan: the config is not valid.");
            return;
        };
        let request = ScanRequest {
            config: summary.config.clone(),
            backend: self.backend,
            threads: self.threads.max(1),
            output: self.output.clone(),
        };
        self.matches.clear();
        self.progress = Progress {
            started: Some(Instant::now()),
            backend: "starting…".to_owned(),
            ..Progress::default()
        };
        self.results = Results::Matches;
        self.note(format!(
            "Scan started ({} backend requested).",
            self.backend.label()
        ));
        let ctx = self.ctx.clone();
        self.scan = Some(runner::start(request, move || ctx.request_repaint()));
    }

    /// Drains scan updates into UI state.
    pub fn pump(&mut self) {
        let Some(scan) = self.scan.as_mut() else {
            return;
        };
        for update in scan.poll() {
            match update {
                Update::Log(line) => self.note(line),
                Update::Backend(name) => {
                    self.progress.backend = name.clone();
                    self.note(format!("Backend: {name}."));
                }
                Update::Plan {
                    items,
                    candidates,
                    saturated,
                } => {
                    self.progress.items = items;
                    self.progress.total_candidates = candidates;
                    self.progress.saturated = saturated;
                    self.note(format!(
                        "Plan: {} work items, {} candidates.",
                        thousands(items as u64),
                        if saturated {
                            format!("more than {}", thousands(candidates))
                        } else {
                            thousands(candidates)
                        }
                    ));
                }
                Update::Progress {
                    candidates,
                    completed,
                    matches,
                } => {
                    self.progress.candidates = candidates;
                    self.progress.completed = completed;
                    self.progress.matches = matches;
                }
                Update::Matches(found) => self.matches.extend(found),
                Update::Finished {
                    elapsed,
                    matches,
                    cancelled,
                    error,
                } => {
                    self.progress.started = None;
                    self.progress.elapsed = elapsed;
                    self.progress.matches = matches;
                    match error {
                        Some(error) => {
                            self.note(format!("Scan failed after {elapsed:.2} s: {error}"))
                        }
                        None if cancelled => self.note(format!(
                            "Scan cancelled after {elapsed:.2} s ({} match(es)).",
                            thousands(matches)
                        )),
                        None => self.note(format!(
                            "Scan finished in {elapsed:.2} s ({} match(es)).",
                            thousands(matches)
                        )),
                    }
                }
            }
        }
    }
}

/// Formats an integer with thin thousands separators.
pub fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Formats a duration in seconds as a compact `1h 02m 03s`.
pub fn duration(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "—".to_owned();
    }
    let total = seconds.round() as u64;
    let (hours, minutes, secs) = (total / 3600, (total / 60) % 60, total % 60);
    if hours > 0 {
        format!("{hours}h {minutes:02}m {secs:02}s")
    } else if minutes > 0 {
        format!("{minutes}m {secs:02}s")
    } else {
        format!("{secs}s")
    }
}

/// Builds the derived facts the summary panel shows.
fn summarize(config: ScanConfig) -> Result<Summary, String> {
    let prepared = prepare_filters(
        &config.filter,
        config.algorithm,
        &config.directions,
        config.error_tolerance,
    )?;
    let warning = prepared.warning();
    let first = &prepared.directions[0];
    let constraints = first.constraints.len() + first.forced_errors as usize;
    let (cpu_items, gpu_items, candidates, saturated) = {
        let cpu = make_plan(&config, config.cpu_tile_size)?;
        let gpu = make_plan(&config, config.gpu_tile_size)?;
        (
            cpu.total_items(),
            gpu.total_items(),
            cpu.total_candidates,
            cpu.total_candidates_saturated,
        )
    };
    Ok(Summary {
        config,
        constraints,
        cpu_items,
        gpu_items,
        candidates,
        saturated,
        warning,
    })
}
