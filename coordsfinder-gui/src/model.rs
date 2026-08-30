//! Editable in-memory representation of a CoordsFinder search config.
//!
//! The GUI never hand-rolls validation. [`EditableConfig::to_conf_text`] writes
//! the same INI-like text the command-line tool reads, and every validation and
//! scan request goes back through [`coordsfinder::config::parse`]. What the user
//! sees in the editor, what gets saved, and what gets scanned are therefore
//! always the same document.

use std::fmt::Write as _;
use std::path::Path;

use coordsfinder::config::{IntRange, ScanConfig, ScanOrder, TileSize};
use coordsfinder::types::{Face, RotationInfo, RotationKind, TextureAlgorithm};

/// Every texture algorithm, in the order shown in the picker.
pub const ALGORITHMS: [TextureAlgorithm; 5] = [
    TextureAlgorithm::Vanilla1,
    TextureAlgorithm::Vanilla2,
    TextureAlgorithm::Vanilla3,
    TextureAlgorithm::Sodium1,
    TextureAlgorithm::Sodium2,
];

/// The four structure directions a filter can be scanned at.
pub const DIRECTIONS: [i32; 4] = [0, 90, 180, 270];

/// Filter offsets are stored as `i8`, which bounds the grid editor too.
pub const OFFSET_MIN: i32 = i8::MIN as i32;
/// Upper bound of a filter offset on any axis.
pub const OFFSET_MAX: i32 = i8::MAX as i32;

/// Which kind of texture observation the grid editor paints.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Brush {
    /// Top or bottom face of an ordinary rotated block; four states.
    FourWay,
    /// Side face of a mirrored block such as stone; two states.
    Side,
    /// One named world face of a netherrack block; four states.
    Netherrack(Face),
}

impl Brush {
    /// Every brush, in picker order.
    pub const ALL: [Self; 8] = [
        Self::FourWay,
        Self::Side,
        Self::Netherrack(Face::Up),
        Self::Netherrack(Face::Down),
        Self::Netherrack(Face::North),
        Self::Netherrack(Face::South),
        Self::Netherrack(Face::East),
        Self::Netherrack(Face::West),
    ];

    /// The matching [`RotationKind`] for filter rows.
    pub fn kind(self) -> RotationKind {
        match self {
            Self::FourWay => RotationKind::StandardFourWay,
            Self::Side => RotationKind::StandardSide,
            Self::Netherrack(face) => RotationKind::Netherrack(face),
        }
    }

    /// Number of distinct rotations this brush accepts.
    pub fn rotation_count(self) -> u8 {
        match self {
            Self::Side => 2,
            _ => 4,
        }
    }

    /// Name shown in the brush picker.
    pub fn label(self) -> &'static str {
        match self {
            Self::FourWay => "Top / bottom (4-way)",
            Self::Side => "Side face (2-way)",
            Self::Netherrack(face) => match face {
                Face::Up => "Netherrack up",
                Face::Down => "Netherrack down",
                Face::North => "Netherrack north",
                Face::South => "Netherrack south",
                Face::East => "Netherrack east",
                Face::West => "Netherrack west",
            },
        }
    }

    /// Single-character badge drawn on a painted cell.
    ///
    /// Only netherrack rows get one, and it is always a compass letter. A side
    /// row is marked by its bar instead: giving it a letter too would put an
    /// `S` for "side" next to an `S` for "south", which reads as ambiguous even
    /// though the two can never share a block.
    pub fn badge(self) -> &'static str {
        match self {
            Self::FourWay | Self::Side => "",
            Self::Netherrack(face) => face_badge(face),
        }
    }
}

/// The brush that produced an existing filter row.
pub fn brush_of(info: &RotationInfo) -> Brush {
    match info.kind {
        RotationKind::StandardFourWay => Brush::FourWay,
        RotationKind::StandardSide => Brush::Side,
        RotationKind::Netherrack(face) => Brush::Netherrack(face),
    }
}

/// Config-file spelling of a netherrack face marker.
pub fn face_name(face: Face) -> &'static str {
    match face {
        Face::Up => "up",
        Face::Down => "down",
        Face::North => "north",
        Face::South => "south",
        Face::East => "east",
        Face::West => "west",
    }
}

fn face_badge(face: Face) -> &'static str {
    match face {
        Face::Up => "U",
        Face::Down => "D",
        Face::North => "N",
        Face::South => "S",
        Face::East => "E",
        Face::West => "W",
    }
}

/// Renders one filter row exactly as the config parser expects it.
pub fn row_text(info: &RotationInfo) -> String {
    let marker = match info.kind {
        RotationKind::StandardFourWay => String::new(),
        RotationKind::StandardSide => " side".to_owned(),
        RotationKind::Netherrack(face) => format!(" netherrack-{}", face_name(face)),
    };
    format!(
        "{} {} {} | {}{marker}",
        info.x, info.y, info.z, info.rotation
    )
}

/// A settings-and-filter document being edited in the GUI.
///
/// Ranges use the same half-open `[start, end)` bounds as the config file.
#[derive(Clone, Debug, PartialEq)]
pub struct EditableConfig {
    pub algorithm: TextureAlgorithm,
    pub scan_order: ScanOrder,
    /// One flag per entry of [`DIRECTIONS`].
    pub directions: [bool; 4],
    pub x_range: IntRange,
    pub y_range: IntRange,
    pub z_range: IntRange,
    pub error_tolerance: i32,
    pub cpu_tile_size: TileSize,
    pub gpu_tile_size: TileSize,
    pub verbose: bool,
    pub filter: Vec<RotationInfo>,
}

impl Default for EditableConfig {
    fn default() -> Self {
        // A new document starts on the engine's own tile sizes rather than a
        // copy of them, so an upstream change to the defaults arrives here too.
        let engine = ScanConfig::default();
        Self {
            algorithm: TextureAlgorithm::Vanilla3,
            scan_order: ScanOrder::Spiral,
            directions: [true, false, false, false],
            x_range: IntRange {
                start: -5_000,
                end: 5_000,
            },
            y_range: IntRange { start: -60, end: 0 },
            z_range: IntRange {
                start: -5_000,
                end: 5_000,
            },
            error_tolerance: 0,
            cpu_tile_size: engine.cpu_tile_size,
            gpu_tile_size: engine.gpu_tile_size,
            verbose: false,
            filter: Vec::new(),
        }
    }
}

impl EditableConfig {
    /// Rebuilds the editable form from a parsed config.
    pub fn from_scan_config(config: &ScanConfig) -> Self {
        let mut directions = [false; 4];
        for (slot, value) in directions.iter_mut().zip(DIRECTIONS) {
            *slot = config.directions.contains(&value);
        }
        Self {
            algorithm: config.algorithm,
            scan_order: config.scan_order,
            directions,
            x_range: config.x_range,
            y_range: config.y_range,
            z_range: config.z_range,
            error_tolerance: config.error_tolerance,
            cpu_tile_size: config.cpu_tile_size,
            gpu_tile_size: config.gpu_tile_size,
            verbose: config.verbose,
            filter: config.filter.clone(),
        }
    }

    /// Writes the config-file text for this document.
    pub fn to_conf_text(&self) -> String {
        let mut text = String::with_capacity(256 + self.filter.len() * 24);
        text.push_str("# Written by CoordsFinder GUI.\n\n");
        let _ = writeln!(text, "algorithm = {}", self.algorithm);
        let _ = writeln!(
            text,
            "scanOrder = {}",
            match self.scan_order {
                ScanOrder::Linear => "linear",
                ScanOrder::Spiral => "spiral",
            }
        );
        let selected: Vec<String> = self
            .selected_directions()
            .iter()
            .map(i32::to_string)
            .collect();
        let _ = writeln!(text, "directions = [{}]\n", selected.join(", "));
        let _ = writeln!(
            text,
            "xRange = ({}, {})",
            self.x_range.start, self.x_range.end
        );
        let _ = writeln!(
            text,
            "yRange = ({}, {})",
            self.y_range.start, self.y_range.end
        );
        let _ = writeln!(
            text,
            "zRange = ({}, {})\n",
            self.z_range.start, self.z_range.end
        );
        let _ = writeln!(text, "errorTolerance = {}\n", self.error_tolerance);
        let _ = writeln!(
            text,
            "cpuTileSize = ({}, {})",
            self.cpu_tile_size.x, self.cpu_tile_size.z
        );
        let _ = writeln!(
            text,
            "gpuTileSize = ({}, {})",
            self.gpu_tile_size.x, self.gpu_tile_size.z
        );
        let _ = writeln!(text, "verbose = {}\n", self.verbose);
        text.push_str("[filter]\n# x y z | variant [side|netherrack-<face>]\n");
        text.push_str(&self.filter_text());
        text
    }

    /// Writes just the `[filter]` rows, one per line.
    pub fn filter_text(&self) -> String {
        let mut text = String::with_capacity(self.filter.len() * 24);
        for info in &self.filter {
            text.push_str(&row_text(info));
            text.push('\n');
        }
        text
    }

    /// Replaces the filter rows by re-parsing them through the real parser.
    ///
    /// The settings of this document are prepended so the rows are validated in
    /// the context they will actually be scanned in.
    pub fn set_filter_text(&mut self, rows: &str, source_path: &Path) -> Result<(), String> {
        let mut probe = self.clone();
        probe.filter.clear();
        if probe.selected_directions().is_empty() {
            // An empty direction list is its own error, reported by the Search
            // panel. Substituting one here keeps this call reporting problems
            // with the rows the user is actually editing.
            probe.directions[0] = true;
        }
        let mut text = probe.to_conf_text();
        text.push_str(rows);
        let parsed = coordsfinder::config::parse(&text, source_path)?;
        self.filter = parsed.filter;
        Ok(())
    }

    /// The directions that are ticked, in ascending order.
    pub fn selected_directions(&self) -> Vec<i32> {
        DIRECTIONS
            .iter()
            .zip(self.directions)
            .filter(|(_, enabled)| *enabled)
            .map(|(direction, _)| *direction)
            .collect()
    }

    /// Validates the document and returns the config a scan would run.
    pub fn to_scan_config(&self, source_path: &Path) -> Result<ScanConfig, String> {
        if self.selected_directions().is_empty() {
            return Err("select at least one direction".to_owned());
        }
        coordsfinder::config::parse(&self.to_conf_text(), source_path)
    }

    /// Index of the row at `(x, y, z)` painted with `brush`, if any.
    pub fn index_at(&self, x: i8, y: i8, z: i8, brush: Brush) -> Option<usize> {
        let kind = brush.kind();
        self.filter
            .iter()
            .position(|info| info.x == x && info.y == y && info.z == z && info.kind == kind)
    }

    /// Paints one observation, replacing any earlier row of the same brush.
    ///
    /// A single Minecraft block cannot use both model selectors, so painting a
    /// netherrack face clears ordinary rows at that offset and vice versa. The
    /// config parser rejects that mix, and catching it here keeps the editor
    /// from producing a document that cannot validate.
    pub fn paint(&mut self, x: i8, y: i8, z: i8, brush: Brush, rotation: u8) {
        let netherrack = matches!(brush, Brush::Netherrack(_));
        self.filter.retain(|info| {
            if info.x != x || info.y != y || info.z != z {
                return true;
            }
            matches!(info.kind, RotationKind::Netherrack(_)) == netherrack
        });
        let row = match brush {
            Brush::FourWay => RotationInfo::new(x, y, z, rotation, false),
            Brush::Side => RotationInfo::new(x, y, z, rotation, true),
            Brush::Netherrack(face) => RotationInfo::netherrack(x, y, z, rotation, face),
        };
        match self.index_at(x, y, z, brush) {
            Some(index) => self.filter[index] = row,
            None => self.filter.push(row),
        }
    }

    /// Removes every row at `(x, y, z)`. Returns whether anything was removed.
    pub fn erase(&mut self, x: i8, y: i8, z: i8) -> bool {
        let before = self.filter.len();
        self.filter
            .retain(|info| info.x != x || info.y != y || info.z != z);
        before != self.filter.len()
    }

    /// Inclusive X/Z bounds covering every filter row, or `None` when empty.
    pub fn extent(&self) -> Option<(i32, i32, i32, i32)> {
        let mut rows = self.filter.iter();
        let first = rows.next()?;
        let mut bounds = (
            i32::from(first.x),
            i32::from(first.x),
            i32::from(first.z),
            i32::from(first.z),
        );
        for info in rows {
            bounds.0 = bounds.0.min(i32::from(info.x));
            bounds.1 = bounds.1.max(i32::from(info.x));
            bounds.2 = bounds.2.min(i32::from(info.z));
            bounds.3 = bounds.3.max(i32::from(info.z));
        }
        Some(bounds)
    }

    /// Every Y layer that holds at least one row, ascending.
    pub fn layers(&self) -> Vec<i8> {
        let mut layers: Vec<i8> = self.filter.iter().map(|info| info.y).collect();
        layers.sort_unstable();
        layers.dedup();
        layers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample() -> EditableConfig {
        let mut config = EditableConfig {
            directions: [true, false, true, false],
            ..EditableConfig::default()
        };
        config.paint(-6, 0, 0, Brush::FourWay, 3);
        config.paint(-5, 0, -1, Brush::Side, 1);
        config.paint(0, 1, 0, Brush::Netherrack(Face::North), 2);
        config
    }

    #[test]
    fn round_trips_through_the_real_parser() {
        let config = sample();
        let path = PathBuf::from("memory.conf");
        let parsed = config.to_scan_config(&path).unwrap();
        assert_eq!(parsed.directions, vec![0, 180]);
        assert_eq!(parsed.filter, config.filter);
        assert_eq!(EditableConfig::from_scan_config(&parsed), config);
    }

    #[test]
    fn painting_replaces_the_same_brush_and_clears_mixed_selectors() {
        let mut config = EditableConfig::default();
        config.paint(1, 0, 1, Brush::FourWay, 2);
        config.paint(1, 0, 1, Brush::Side, 1);
        config.paint(1, 0, 1, Brush::FourWay, 0);
        assert_eq!(config.filter.len(), 2);
        assert_eq!(config.filter[0].rotation, 0);

        // A netherrack row cannot share a block with ordinary rows.
        config.paint(1, 0, 1, Brush::Netherrack(Face::Up), 3);
        assert_eq!(
            config.filter,
            vec![RotationInfo::netherrack(1, 0, 1, 3, Face::Up)]
        );
    }

    #[test]
    fn rejects_filter_text_that_the_parser_rejects() {
        let mut config = sample();
        let path = PathBuf::from("memory.conf");
        assert!(config.set_filter_text("0 0 0 | 9\n", &path).is_err());
        config.set_filter_text("2 0 3 | 1 side\n", &path).unwrap();
        assert_eq!(config.filter, vec![RotationInfo::new(2, 0, 3, 1, true)]);
    }

    /// Saving must not change what a config means. Every shipped config is
    /// loaded, written back out by the GUI, and re-parsed; the parsed result
    /// has to be identical.
    #[test]
    fn every_shipped_config_survives_a_save() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut checked = 0;
        let mut candidates = vec![root.join("example.conf")];
        for directory in ["examples", "coordsfinder/tests"] {
            let entries = std::fs::read_dir(root.join(directory)).unwrap();
            candidates.extend(
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.extension().is_some_and(|kind| kind == "conf")),
            );
        }
        for path in candidates {
            // The invalid_* fixtures exist to fail parsing; skip those.
            let Ok(original) = coordsfinder::config::load(&path) else {
                continue;
            };
            let editable = EditableConfig::from_scan_config(&original);
            let written = editable.to_conf_text();
            let reparsed = coordsfinder::config::parse(&written, &path).unwrap_or_else(|error| {
                panic!("{} did not survive saving: {error}", path.display())
            });
            assert_eq!(
                reparsed.filter,
                original.filter,
                "filter changed for {}",
                path.display()
            );
            assert_eq!(reparsed.directions, original.directions);
            assert_eq!(reparsed.algorithm, original.algorithm);
            assert_eq!(reparsed.scan_order, original.scan_order);
            assert_eq!(reparsed.x_range, original.x_range);
            assert_eq!(reparsed.y_range, original.y_range);
            assert_eq!(reparsed.z_range, original.z_range);
            assert_eq!(reparsed.error_tolerance, original.error_tolerance);
            assert_eq!(reparsed.cpu_tile_size, original.cpu_tile_size);
            assert_eq!(reparsed.gpu_tile_size, original.gpu_tile_size);
            assert_eq!(reparsed.verbose, original.verbose);
            checked += 1;
        }
        assert!(checked >= 5, "expected several configs, checked {checked}");
    }

    #[test]
    fn no_two_badges_share_a_letter() {
        // A side row is marked by its bar rather than a letter, which is what
        // keeps an "S" for side from sitting next to an "S" for south.
        assert_eq!(Brush::Side.badge(), "");
        assert_eq!(Brush::FourWay.badge(), "");
        let letters: Vec<&str> = Brush::ALL
            .iter()
            .map(|brush| brush.badge())
            .filter(|badge| !badge.is_empty())
            .collect();
        let mut unique = letters.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(letters.len(), 6, "every netherrack face needs a badge");
        assert_eq!(unique.len(), letters.len(), "badges collide: {letters:?}");
    }

    #[test]
    fn empty_direction_selection_is_reported() {
        let config = EditableConfig {
            directions: [false; 4],
            ..sample()
        };
        assert!(
            config
                .to_scan_config(&PathBuf::from("memory.conf"))
                .is_err()
        );
    }
}
