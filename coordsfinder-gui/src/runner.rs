//! Runs a scan on a worker thread and streams its progress to the UI.
//!
//! The scan backends take `sink`, `progress`, and `cancelled` closures, so the
//! runner only has to translate those callbacks into channel messages. The UI
//! thread never blocks on a scan; it drains [`ScanHandle::poll`] once per frame.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use coordsfinder::config::ScanConfig;
use coordsfinder::cpu::CpuScanner;
use coordsfinder::gpu::GpuScanner;
use coordsfinder::scan::make_plan;
use coordsfinder::types::Match;

/// Matches held for display. Beyond this the scan keeps counting and still
/// writes every match to the output file, but stops feeding the match table so
/// an over-broad filter cannot exhaust memory.
pub const DISPLAYED_MATCH_LIMIT: usize = 50_000;

/// Which backend the user asked for.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BackendChoice {
    /// Use the GPU when one is usable, otherwise fall back to the CPU.
    #[default]
    Auto,
    Cpu,
    Gpu,
}

impl BackendChoice {
    /// Every choice, in picker order.
    pub const ALL: [Self; 3] = [Self::Auto, Self::Cpu, Self::Gpu];

    /// Name shown in the backend picker.
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Cpu => "CPU",
            Self::Gpu => "GPU",
        }
    }
}

/// Everything a scan thread needs to run without touching UI state.
pub struct ScanRequest {
    pub config: ScanConfig,
    pub backend: BackendChoice,
    pub threads: usize,
    pub output: Option<PathBuf>,
}

/// One update from a running scan.
#[derive(Debug)]
pub enum Update {
    /// A human-readable status line for the log pane.
    Log(String),
    /// The backend that was actually chosen, once it is known.
    Backend(String),
    /// Plan totals, sent before the first tile is scanned.
    Plan {
        items: usize,
        candidates: u64,
        saturated: bool,
    },
    /// Cumulative candidates checked and work items completed.
    Progress {
        candidates: u64,
        completed: usize,
        matches: u64,
    },
    /// Newly found matches, capped by [`DISPLAYED_MATCH_LIMIT`].
    Matches(Vec<Match>),
    /// The scan ended, either by finishing, cancelling, or failing.
    Finished {
        elapsed: f64,
        matches: u64,
        cancelled: bool,
        error: Option<String>,
    },
}

/// Handle to a scan running on its own thread.
pub struct ScanHandle {
    updates: Receiver<Update>,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    finished: bool,
    /// When the handle was created, for reporting a worker that unwound.
    started: Instant,
    /// Latest match count seen, so a synthesized `Finished` does not report
    /// zero for a scan that had already found something.
    last_matches: u64,
}

impl ScanHandle {
    /// Asks the scan to stop at its next cancellation check.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Whether cancellation has been requested.
    pub fn cancelling(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Whether a [`Update::Finished`] has already been drained.
    pub fn finished(&self) -> bool {
        self.finished
    }

    /// Drains every update that has arrived since the last call.
    pub fn poll(&mut self) -> Vec<Update> {
        let mut drained = Vec::new();
        loop {
            match self.updates.try_recv() {
                Ok(update) => {
                    match update {
                        Update::Finished { .. } => self.finished = true,
                        Update::Progress { matches, .. } => self.last_matches = matches,
                        _ => {}
                    }
                    drained.push(update);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // The worker always sends `Finished` before it returns, so a
                    // closed channel without one means it unwound instead. wgpu
                    // panics this way after a display driver reset, and a release
                    // build has no console to print the panic to, so the scan
                    // would otherwise just stop with nothing said.
                    if !self.finished {
                        drained.push(Update::Finished {
                            elapsed: self.started.elapsed().as_secs_f64(),
                            matches: self.last_matches,
                            cancelled: false,
                            error: Some(
                                concat!(
                                    "the scan thread stopped unexpectedly. On the GPU ",
                                    "backend this is usually a display driver reset ",
                                    "(Windows TDR); lower the GPU tile size under ",
                                    "Advanced and try again.",
                                )
                                .to_owned(),
                            ),
                        });
                    }
                    self.finished = true;
                    break;
                }
            }
        }
        drained
    }
}

impl Drop for ScanHandle {
    fn drop(&mut self) {
        // Closing the window must not leave a scan running on a detached
        // thread; cancel and wait for the backend to unwind.
        self.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Collects matches, writes them to the optional output file, and forwards a
/// bounded number of them to the UI.
struct Sink {
    updates: Sender<Update>,
    repaint: Box<dyn Fn() + Send>,
    output: Option<std::fs::File>,
    total: Arc<AtomicU64>,
    forwarded: usize,
    write_error: Option<String>,
}

impl Sink {
    fn accept(&mut self, matches: &[Match]) {
        self.total
            .fetch_add(matches.len() as u64, Ordering::Relaxed);
        if let Some(file) = &mut self.output {
            for found in matches {
                let line = format!(
                    "Found with {} mismatch(es)! ({}, {}, {}), direction {}",
                    found.mismatches, found.x, found.y, found.z, found.direction
                );
                if let Err(error) = writeln!(file, "{line}") {
                    // Report once and keep scanning; losing the file copy must
                    // not throw away matches already on screen.
                    if self.write_error.is_none() {
                        self.write_error = Some(error.to_string());
                        let _ = self
                            .updates
                            .send(Update::Log(format!("Output file write failed: {error}")));
                    }
                    break;
                }
            }
            let _ = file.flush();
        }
        if self.forwarded < DISPLAYED_MATCH_LIMIT {
            let room = DISPLAYED_MATCH_LIMIT - self.forwarded;
            let batch: Vec<Match> = matches.iter().copied().take(room).collect();
            self.forwarded += batch.len();
            let _ = self.updates.send(Update::Matches(batch));
            if self.forwarded >= DISPLAYED_MATCH_LIMIT {
                let _ = self.updates.send(Update::Log(format!(
                    "Match list capped at {DISPLAYED_MATCH_LIMIT}; \
                     later matches are still counted and still written to the output file."
                )));
            }
            (self.repaint)();
        }
    }
}

/// Starts a scan and returns a handle to it.
///
/// `repaint` is called whenever an update is queued so the UI wakes up promptly
/// instead of waiting for the next idle frame.
pub fn start(request: ScanRequest, repaint: impl Fn() + Send + Clone + 'static) -> ScanHandle {
    let (sender, updates) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let thread_cancel = Arc::clone(&cancel);
    let worker = thread::spawn(move || {
        let started = Instant::now();
        let total = run(request, &sender, &thread_cancel, repaint.clone());
        let (matches, error) = match total {
            Ok(matches) => (matches, None),
            Err((matches, error)) => (matches, Some(error)),
        };
        let _ = sender.send(Update::Finished {
            elapsed: started.elapsed().as_secs_f64(),
            matches,
            cancelled: thread_cancel.load(Ordering::Relaxed),
            error,
        });
        repaint();
    });
    ScanHandle {
        updates,
        cancel,
        worker: Some(worker),
        finished: false,
        started: Instant::now(),
        last_matches: 0,
    }
}

/// Runs one scan, returning the match count or the count plus a failure.
fn run(
    request: ScanRequest,
    updates: &Sender<Update>,
    cancel: &AtomicBool,
    repaint: impl Fn() + Send + Clone + 'static,
) -> Result<u64, (u64, String)> {
    let ScanRequest {
        config,
        backend,
        threads,
        output,
    } = request;

    let output = match output {
        Some(path) => match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => {
                let _ = updates.send(Update::Log(format!(
                    "Appending matches to {}.",
                    path.display()
                )));
                Some(file)
            }
            Err(error) => {
                return Err((
                    0,
                    format!("could not open output file {}: {error}", path.display()),
                ));
            }
        },
        None => None,
    };

    // Selecting the scanner can take a moment on the GPU path, so it happens
    // here on the worker thread rather than while building the request.
    let scanner = match backend {
        BackendChoice::Cpu => CpuScanner::new(threads).map(Selected::Cpu),
        BackendChoice::Gpu => {
            GpuScanner::new(&config).map(|scanner| Selected::Gpu(Box::new(scanner)))
        }
        BackendChoice::Auto => match GpuScanner::new(&config) {
            Ok(scanner) => Ok(Selected::Gpu(Box::new(scanner))),
            Err(reason) => {
                let _ = updates.send(Update::Log(format!(
                    "GPU unavailable ({reason}); falling back to CPU."
                )));
                CpuScanner::new(threads).map(Selected::Cpu)
            }
        },
    };
    let scanner = scanner.map_err(|error| (0, error))?;

    let tile_size = match &scanner {
        Selected::Cpu(scanner) => {
            let _ = updates.send(Update::Backend(format!(
                "CPU ({} threads)",
                scanner.threads()
            )));
            config.cpu_tile_size
        }
        Selected::Gpu(scanner) => {
            let _ = updates.send(Update::Backend(format!(
                "wgpu/{:?} ({})",
                scanner.adapter_backend(),
                scanner.adapter_name()
            )));
            config.gpu_tile_size
        }
    };
    repaint();

    let plan = make_plan(&config, tile_size).map_err(|error| (0, error))?;
    let _ = updates.send(Update::Plan {
        items: plan.total_items(),
        candidates: plan.total_candidates,
        saturated: plan.total_candidates_saturated,
    });
    repaint();

    // The match total is shared because the progress callback reports it while
    // the sink callback is the one advancing it.
    let total = Arc::new(AtomicU64::new(0));
    let mut sink = Sink {
        updates: updates.clone(),
        repaint: Box::new(repaint.clone()),
        output,
        total: Arc::clone(&total),
        forwarded: 0,
        write_error: None,
    };

    let result = {
        let progress_updates = updates.clone();
        let progress_repaint = repaint.clone();
        let progress_total = Arc::clone(&total);
        let mut progress = move |candidates: u64, completed: usize| {
            let _ = progress_updates.send(Update::Progress {
                candidates,
                completed,
                matches: progress_total.load(Ordering::Relaxed),
            });
            progress_repaint();
        };
        let mut sink_fn = |found: &[Match]| sink.accept(found);
        let cancelled = || cancel.load(Ordering::Relaxed);
        match &scanner {
            Selected::Cpu(scanner) => {
                scanner.scan(&config, &plan, &mut sink_fn, &mut progress, cancelled)
            }
            Selected::Gpu(scanner) => {
                scanner.scan(&config, &plan, &mut sink_fn, &mut progress, cancelled)
            }
        }
    };
    let found = total.load(Ordering::Relaxed);
    match result {
        Ok(()) => Ok(found),
        Err(error) => Err((found, error)),
    }
}

/// The scanner chosen for one run.
enum Selected {
    Cpu(CpuScanner),
    Gpu(Box<GpuScanner>),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A handle around a bare channel, standing in for a running scan.
    fn handle(updates: Receiver<Update>) -> ScanHandle {
        ScanHandle {
            updates,
            cancel: Arc::new(AtomicBool::new(false)),
            worker: None,
            finished: false,
            started: Instant::now(),
            last_matches: 0,
        }
    }

    #[test]
    fn a_worker_that_unwinds_is_reported_as_a_failure() {
        let (sender, receiver) = channel();
        let mut scan = handle(receiver);
        sender
            .send(Update::Progress {
                candidates: 100,
                completed: 1,
                matches: 3,
            })
            .unwrap();
        // No `Finished`: the worker panicked, as wgpu does after a driver reset.
        drop(sender);

        let drained = scan.poll();
        assert!(scan.finished());
        let Some(Update::Finished {
            matches,
            cancelled,
            error,
            ..
        }) = drained.last()
        else {
            panic!("expected a synthesized Finished, got {drained:?}");
        };
        assert_eq!(
            *matches, 3,
            "the last known count should survive the failure"
        );
        assert!(!*cancelled);
        assert!(
            error
                .as_deref()
                .is_some_and(|error| error.contains("stopped unexpectedly"))
        );
    }

    #[test]
    fn a_clean_finish_is_not_reported_twice() {
        let (sender, receiver) = channel();
        let mut scan = handle(receiver);
        sender
            .send(Update::Finished {
                elapsed: 1.0,
                matches: 2,
                cancelled: false,
                error: None,
            })
            .unwrap();
        drop(sender);

        let drained = scan.poll();
        assert_eq!(drained.len(), 1, "the worker's own Finished, and only it");
        assert!(scan.finished());
        assert!(scan.poll().is_empty());
    }
}
