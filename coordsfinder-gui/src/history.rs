//! Undo and redo for the document being edited.
//!
//! History is kept as whole-document snapshots rather than as a list of edit
//! commands. A [`EditableConfig`] is a few kilobytes at worst — a 500-row filter
//! is about 4 KB — so snapshots cost little, and they cannot drift out of sync
//! with the editor the way hand-written inverse operations can. Every edit path
//! in the GUI is covered without any of them having to know history exists.
//!
//! Edits are grouped into bursts so one gesture is one undo step: painting a
//! stroke across twenty cells, or dragging a range value through fifty
//! intermediate numbers, should each undo as a single action. A burst stays open
//! while the pointer is held and closes on the frame after it is released.

use std::collections::VecDeque;

use crate::model::EditableConfig;

/// Undo steps kept. At a few kilobytes per snapshot this is a bounded cost, and
/// far more history than a filter edit realistically needs.
const DEPTH: usize = 200;

/// Undo and redo stacks for one document.
pub struct History {
    undo: VecDeque<EditableConfig>,
    redo: Vec<EditableConfig>,
    /// The document as it was before the burst of edits in progress, held back
    /// until the burst closes so the whole gesture becomes one step.
    pending: Option<EditableConfig>,
    /// The document as of the last frame, for detecting edits.
    last: EditableConfig,
}

impl History {
    /// Starts an empty history for `config`.
    pub fn new(config: &EditableConfig) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            pending: None,
            last: config.clone(),
        }
    }

    /// Throws the history away and starts again from `config`.
    ///
    /// Used when a different document is loaded: undoing across an Open into
    /// the previous file's edits would be more surprising than helpful.
    pub fn reset(&mut self, config: &EditableConfig) {
        self.undo.clear();
        self.redo.clear();
        self.pending = None;
        self.last = config.clone();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.pending.is_some()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Notices edits and closes finished bursts. Returns whether the document
    /// changed since the last call.
    ///
    /// `settled` should be false while the pointer is held, so a drag is not
    /// chopped into one undo step per frame.
    pub fn track(&mut self, config: &EditableConfig, settled: bool) -> bool {
        if *config != self.last {
            if self.pending.is_none() {
                self.pending = Some(self.last.clone());
                // A fresh edit is a new branch: whatever was undone is gone.
                self.redo.clear();
            }
            self.last = config.clone();
            return true;
        }
        if settled {
            self.close_burst();
        }
        false
    }

    /// Steps back one edit. Returns whether anything was undone.
    pub fn undo(&mut self, config: &mut EditableConfig) -> bool {
        // An edit still in progress is undone as a unit, not left half-open.
        self.close_burst();
        let Some(previous) = self.undo.pop_back() else {
            return false;
        };
        self.redo.push(std::mem::replace(config, previous));
        self.last = config.clone();
        true
    }

    /// Steps forward one undone edit. Returns whether anything was redone.
    pub fn redo(&mut self, config: &mut EditableConfig) -> bool {
        self.close_burst();
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.push_undo(std::mem::replace(config, next));
        self.last = config.clone();
        true
    }

    /// Commits the burst in progress, if there is one.
    fn close_burst(&mut self) {
        if let Some(before) = self.pending.take() {
            self.push_undo(before);
        }
    }

    fn push_undo(&mut self, snapshot: EditableConfig) {
        self.undo.push_back(snapshot);
        if self.undo.len() > DEPTH {
            self.undo.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Brush;

    fn with_rows(rows: &[(i8, u8)]) -> EditableConfig {
        let mut config = EditableConfig::default();
        for (x, rotation) in rows {
            config.paint(*x, 0, 0, Brush::FourWay, *rotation);
        }
        config
    }

    /// Drives a burst the way a frame loop does: edits arrive while the pointer
    /// is down, and the burst closes on a settled frame with no change.
    fn burst(history: &mut History, config: &EditableConfig) {
        history.track(config, false);
        history.track(config, true);
    }

    #[test]
    fn undo_and_redo_walk_the_edits() {
        let first = with_rows(&[(0, 1)]);
        let mut config = first.clone();
        let mut history = History::new(&config);
        assert!(!history.can_undo());

        config = with_rows(&[(0, 1), (1, 2)]);
        burst(&mut history, &config);
        let second = config.clone();
        config = with_rows(&[(0, 1), (1, 2), (2, 3)]);
        burst(&mut history, &config);
        let third = config.clone();

        assert!(history.undo(&mut config));
        assert_eq!(config, second);
        assert!(history.undo(&mut config));
        assert_eq!(config, first);
        assert!(!history.undo(&mut config), "history should be exhausted");

        assert!(history.redo(&mut config));
        assert_eq!(config, second);
        assert!(history.redo(&mut config));
        assert_eq!(config, third);
        assert!(!history.redo(&mut config));
    }

    #[test]
    fn a_held_pointer_keeps_one_gesture_as_one_step() {
        let start = with_rows(&[]);
        let mut config = start.clone();
        let mut history = History::new(&config);

        // A stroke painting three cells without releasing the button.
        for x in 0..3 {
            config.paint(x, 0, 0, Brush::FourWay, 1);
            history.track(&config, false);
            // Frames in the middle of the stroke where nothing moved.
            history.track(&config, false);
        }
        history.track(&config, true);

        assert!(history.undo(&mut config));
        assert_eq!(config, start, "the whole stroke should undo at once");
    }

    #[test]
    fn editing_after_an_undo_drops_the_redone_future() {
        let mut config = with_rows(&[]);
        let mut history = History::new(&config);
        config = with_rows(&[(0, 1)]);
        burst(&mut history, &config);

        assert!(history.undo(&mut config));
        assert!(history.can_redo());

        config = with_rows(&[(5, 3)]);
        history.track(&config, false);
        assert!(!history.can_redo(), "a new edit replaces the undone branch");
    }

    #[test]
    fn an_unfinished_burst_is_still_undoable() {
        let start = with_rows(&[]);
        let mut config = start.clone();
        let mut history = History::new(&config);

        // Mid-drag: the burst has not been closed by a settled frame yet.
        config = with_rows(&[(0, 2)]);
        history.track(&config, false);
        assert!(history.can_undo());
        assert!(history.undo(&mut config));
        assert_eq!(config, start);
    }

    #[test]
    fn history_is_bounded() {
        let mut config = EditableConfig::default();
        let mut history = History::new(&config);
        for step in 0..(DEPTH + 40) {
            config.error_tolerance = step as i32 % 7;
            history.track(&config, false);
            history.track(&config, true);
        }
        assert_eq!(history.undo.len(), DEPTH);
    }

    #[test]
    fn reset_forgets_everything() {
        let mut config = with_rows(&[]);
        let mut history = History::new(&config);
        config = with_rows(&[(0, 1)]);
        burst(&mut history, &config);
        assert!(history.can_undo());

        history.reset(&config);
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }
}
