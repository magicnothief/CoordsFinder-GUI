# Development

How this fork is laid out, how to build and test it, and how to keep it in step
with upstream.

## Layout

The repository is a Cargo workspace with two members:

```text
CoordsFinder-GUI/
├── Cargo.toml            workspace root
├── coordsfinder/         the upstream crate: engine + CLI
│   ├── src/lib.rs        config, filter, texture, scan, cpu, gpu
│   ├── src/main.rs       the command-line program
│   └── tests/            config fixtures
├── coordsfinder-gui/     this fork's addition
│   └── src/
│       ├── main.rs       window setup, optional config argument
│       ├── app.rs        application state, validation cache, scan lifecycle
│       ├── ui.rs         panels and widgets
│       ├── grid.rs       the click-to-paint grid
│       ├── model.rs      editable config, .conf writing, filter editing
│       ├── history.rs    undo/redo snapshots and edit-burst grouping
│       └── runner.rs     worker thread, progress channel, cancellation
├── docs/                 these pages
└── examples/             sample configs
```

`coordsfinder/` is upstream's code. Keeping it in its own directory, untouched,
is what makes merging upstream releases tractable — see
[Merging upstream](#merging-upstream).

## Building

```sh
cargo build --release              # both binaries
cargo build --release -p coordsfinder      # CLI only, no GUI dependencies
cargo run --release -p coordsfinder-gui -- ./example.conf
```

Rust 1.87 or newer. On Debian or Ubuntu the GUI needs system libraries for
windowing and file dialogs:

```sh
sudo apt-get install libgtk-3-dev libxkbcommon-dev libwayland-dev \
  libgl1-mesa-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

The GUI uses [eframe/egui](https://github.com/emilk/egui) with the `glow`
(OpenGL) renderer, so it does not pull a second copy of wgpu alongside the one
the search backend uses.

## Testing

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs all three on every push. Clippy is denied on warnings, so a lint failure
is a build failure.

The suite covers the engine (texture algorithms against reference vectors, the
netherrack face table, filter compilation, scan planning, CPU/GPU agreement) and
the GUI's own logic:

| Test | What it pins down |
| --- | --- |
| `model::round_trips_through_the_real_parser` | An edited document survives being written and re-parsed |
| `model::every_shipped_config_survives_a_save` | Every config in the repo is unchanged by a GUI save/reload |
| `model::no_two_badges_share_a_letter` | Grid badges stay unambiguous |
| `history::*` | Undo/redo ordering, burst grouping, redo invalidation, depth cap |
| `grid::fitting_uses_the_tighter_axis…` | The board fits the window without going illegible |

The GPU test skips itself when no compatible adapter is present, so it is safe
on CI runners.

### Checking the engine end to end

[`example.conf`](../example.conf) documents its own answer: `(1570, -45, -1236)`.
Both backends should find exactly it.

```sh
cargo run --release -p coordsfinder -- --backend cpu --output /tmp/cpu.txt example.conf
cargo run --release -p coordsfinder -- --backend gpu --output /tmp/gpu.txt example.conf
diff /tmp/cpu.txt /tmp/gpu.txt
```

`--validate` checks a config and prints the plan without scanning, which is the
quickest way to confirm a config parses:

```sh
cargo run --release -p coordsfinder -- --validate example.conf
```

## How the GUI is put together

The GUI is a consumer of the library and adds nothing to the search. Configs are
parsed by `coordsfinder::config`, filters compiled by `coordsfinder::filter`, and
scans run on `coordsfinder::cpu` or `coordsfinder::gpu`.

Two decisions are worth knowing before changing anything:

**The document is an `EditableConfig`, not a `ScanConfig`.** A document being
edited is allowed to be temporarily invalid — an empty filter, a backwards
range — which `ScanConfig` cannot represent. `EditableConfig::to_conf_text`
writes config text and `to_scan_config` parses it back through the real parser.
Validation, saving, and scanning therefore all run through one code path and
cannot disagree about what is valid. It also means the GUI needs no validation
logic of its own, and cannot drift from the CLI's.

**Scans run on a worker thread.** `runner.rs` turns the backends' `sink`,
`progress`, and `cancelled` callbacks into channel messages; the UI drains them
once per frame. The UI thread never blocks, and cancellation is an `AtomicBool`
the scan polls. Dropping the handle cancels and joins, so closing the window
cannot leave a scan running.

Undo is whole-document snapshots rather than inverse edit commands — see the
module docs in `history.rs` for why, and how edits are grouped into bursts so one
gesture is one undo step.

## The one change to upstream code

`config::load` was split into `load` (read a file, then parse) and a new public
`config::parse` (parse text already in memory). The GUI validates documents that
have never been written to disk, and needed a parse entry point that does not
require a file.

That is the entire diff to `coordsfinder/`. Everything else the fork adds lives
in `coordsfinder-gui/`.

## Merging upstream

The fork keeps upstream's crate in `coordsfinder/`, so upstream changes apply
with git's rename detection:

```sh
git remote add upstream https://github.com/ALaggyDev/CoordsFinder
git fetch upstream
git merge upstream/main
```

Conflicts should be confined to `coordsfinder/src/config.rs` (the `parse` split),
the workspace manifests, the workflows, and the README. If upstream changes the
config format or the `RotationKind` set, the GUI needs matching work in
`model.rs` (a new `Brush`) and `grid.rs` (how the new kind is drawn).

Run the full test suite after a merge. `every_shipped_config_survives_a_save`
catches format changes the GUI's writer has not kept up with.

## Releasing

Releases are built by CI, not locally, so all three platforms come from the same
commit.

1. Bump `version` in the workspace `Cargo.toml`, and run `cargo build` so
   `Cargo.lock` follows.
2. Commit, tag, and push:

   ```sh
   git tag -a v1.3.0 -m "v1.3.0"
   git push origin main --tags
   ```

3. [`release.yml`](../.github/workflows/release.yml) builds Windows, Linux, and
   macOS ARM, and opens a **draft** release with both binaries for each. Check
   the assets and publish it.

The workflow can also be run manually from the Actions tab to test it without
tagging; it only creates a release for tag pushes.

## Contributing

Upstream's contributing note applies here too, and is worth repeating: keep
changes minimal and scoped, avoid optimisations that make code unreadable, and
understand and describe your own change. If your change is to the search engine
rather than the GUI, it probably belongs
[upstream](https://github.com/ALaggyDev/CoordsFinder) where everyone benefits
from it.

Style is enforced mechanically: `cargo fmt` and clippy with `-D warnings`. Match
the surrounding code's comment density — comments here explain *why* a thing is
the way it is, and the ones that would only restate the code are left out.
