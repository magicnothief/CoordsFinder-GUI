# CoordsFinder GUI guide

CoordsFinder GUI is a desktop front-end for the same search engine the
command-line tool uses. It edits a search config, validates it live, draws the
filter as a grid you can paint, and runs the scan in the window.

It does not reimplement anything. Configs are parsed by
`coordsfinder::config`, filters are compiled by `coordsfinder::filter`, and
scans run on `coordsfinder::cpu` or `coordsfinder::gpu`. A config saved from the
GUI is an ordinary `.conf` file, and a config written by hand or exported from
WebCoordsFinder opens in the GUI unchanged.

## Running it

```sh
cargo run --release -p coordsfinder-gui
cargo run --release -p coordsfinder-gui -- ./example.conf
```

The optional argument is a config to open at start, so the executable also works
as the "open with" target for `.conf` files. A config can also be dropped onto
the window.

## Window layout

```text
+-- menu ------------------------------------------------------------+
| File   Filter        example.conf  • unsaved                       |
+-- settings ----------+-- filter editor ----------------------------+
| Search               | [ Grid ] [ Rows ]                           |
|   Algorithm          |                                             |
|   Scan order         | Y layer  used: -1 0    Zoom   Auto-fit      |
|   Directions         | Paint [Top / bottom (4-way)]  Rotation 0123 |
|   Ranges X / Y / Z   |                                             |
|   Error tolerance    |   north up, +X east right, +Z south down    |
|   Advanced           |     +---+---+---+---+---+                   |
|                      |     | 3 | 0 | 2 | 1 | 0 |                   |
| Run                  |     +---+---+---+---+---+                   |
|   Backend            |     | 1 | 3 |[3]| 0 | 1 |   [ ] = origin    |
|   CPU threads        |     +---+---+---+---+---+                   |
|   Output file        |                                             |
|                      |                                             |
| Summary              |                                             |
|   Filter rows        |                                             |
|   Block constraints  |                                             |
|   Candidates         |                                             |
|   Work items         |                                             |
+----------------------+---------------------------------------------+
| [ Start scan ]  wgpu/Vulkan (RTX 5080)   3 match(es)  Copy  Save    |
| [========================        ] 41/64 items, 155000 M cand/s     |
| [ Matches ] [ Log ]                                                 |
|   1570  -45  -1236   dir 0°   0 mismatch(es)                        |
+---------------------------------------------------------------------+
```

## Settings

Every setting maps to one config key, with the same meaning and the same
half-open `[start, end)` ranges described in the [README](../README.md).

| Control | Config key |
| --- | --- |
| Algorithm | `algorithm` |
| Scan order | `scanOrder` |
| Directions | `directions` |
| Ranges X / Y / Z | `xRange`, `yRange`, `zRange` |
| Error tolerance | `errorTolerance` |
| Advanced → CPU tile, GPU tile, Verbose | `cpuTileSize`, `gpuTileSize`, `verbose` |

Backend, CPU threads, and the output file are run options rather than config
settings; they match the `--backend`, `--threads`, and `--output` flags.

## Summary and validation

The Summary block re-validates after every edit by writing the document out as
config text and parsing it back with the real parser. It reports:

- **Filter rows** — configured observations.
- **Block constraints** — rows after same-block rows are merged, which is what
  the scanner actually checks.
- **Candidates** — coordinates the plan will test.
- **Work items** — tiles for the CPU and GPU tile sizes.

If the document is not valid, the block shows the parser's own error message and
the Start button stays disabled. Forced-error warnings from filter preparation
appear here too, in amber.

## The grid editor

The grid shows one Y layer in Minecraft's top-down map orientation: `+X` runs
right (east), `+Z` runs down (south), so north is up. The origin — the candidate
coordinate the offsets are relative to — is the outlined cell at `0, 0`.

Each painted cell shows the rotation digit that will be written to the config,
coloured by rotation, with a corner badge for anything that is not a plain
four-way face:

| Badge | Meaning |
| --- | --- |
| *(none)* | Top or bottom face of an ordinary rotated block |
| `S` | `side` — side face of a mirrored block |
| `U` `D` `N` `So` `E` `W` | `netherrack-up`, `-down`, `-north`, `-south`, `-east`, `-west` |

`+n` in the lower-left corner means the block carries more rows than the one
shown; hover it to see them all. A faint dot marks a cell that is empty on this
layer but painted on another.

Editing:

- **Left-click** paints the selected brush and rotation. Clicking a cell that
  already holds exactly that advances the rotation, so repeated clicks cycle
  `0 → 1 → 2 → 3`.
- **Drag** paints without cycling.
- **Right-click** erases every row at that cell.
- **Y layer** switches layers; the numbers next to it are the layers already in
  use.
- **Auto-fit** keeps three empty cells around the painted area, so there is
  always somewhere to extend into. Turn it off to keep the view still.

One Minecraft block cannot use both model selectors, so painting a netherrack
face over ordinary rows — or the reverse — clears the rows it cannot coexist
with. This is the same rule the config parser enforces; the editor applies it up
front rather than letting it surface as a validation error.

## The rows editor

The **Rows** tab is the same filter as text, one `x y z | variant [marker]` row
per line. **Apply rows** parses the text with the settings currently in the
Search panel and replaces the filter; a bad row is reported without changing
anything. **Copy config to clipboard** copies the whole document, which is handy
for pasting into an issue or a Colab notebook.

Switching to the tab refreshes the text from the grid, so the two views never
disagree.

## Running a scan

Start disables config editing and runs the scan on a worker thread; the window
stays responsive. The progress bar shows completed work items, the live
candidate rate, and a rough time remaining. Stop asks the scan to stop — the CPU
backend checks between X/Z columns, the GPU backend after its current tile, so a
large tile can take a moment to wind down.

Matches stream into the Matches tab as they are found. Click a row to copy its
coordinates, or use Copy all / Save matches. Setting an output file first
appends every match to it as the CLI's `--output` does, which is the safer
choice for a long scan: the match list in the window stops growing after 50,000
entries, while the file keeps every one.

Closing the window cancels and waits for a running scan rather than leaving it
on a detached thread.

## Source layout

| File | What it does |
| --- | --- |
| `coordsfinder-gui/src/main.rs` | Window setup and the optional config argument |
| `coordsfinder-gui/src/app.rs` | Application state, validation cache, scan lifecycle |
| `coordsfinder-gui/src/ui.rs` | Panels and widgets |
| `coordsfinder-gui/src/grid.rs` | The click-to-paint grid |
| `coordsfinder-gui/src/model.rs` | Editable config, `.conf` writing, filter editing |
| `coordsfinder-gui/src/runner.rs` | Worker thread, progress channel, cancellation |

The GUI holds an `EditableConfig` rather than a `ScanConfig`, because a document
being edited is allowed to be temporarily invalid. `EditableConfig::to_conf_text`
writes config text and `to_scan_config` parses it back, so validation, saving,
and scanning all go through one code path and cannot drift apart.
