# CoordsFinder GUI

A desktop front-end for [CoordsFinder](https://github.com/ALaggyDev/CoordsFinder),
the Minecraft texture-rotation coordinate cracker. Paint the pattern you see in
a screenshot onto a grid, watch the config validate as you go, and run the scan
in the window.

[![build](https://github.com/magicnothief/CoordsFinder-GUI/actions/workflows/build.yml/badge.svg)](https://github.com/magicnothief/CoordsFinder-GUI/actions/workflows/build.yml)
[![release](https://img.shields.io/github/v/release/magicnothief/CoordsFinder-GUI?include_prereleases&sort=semver)](https://github.com/magicnothief/CoordsFinder-GUI/releases/latest)
[![licence: MIT](https://img.shields.io/badge/licence-MIT-blue.svg)](./LICENSE)

![The CoordsFinder GUI window: settings on the left, a painted filter grid in the middle, and a finished scan showing one match](./docs/images/window.png)

> **This is a fork.** The search engine is
> [Laggy's](https://github.com/ALaggyDev) — the texture-rotation algorithms, the
> multithreaded CPU backend, the wgpu compute backend, all of it. This fork adds
> the window on top and changes nothing about how the search behaves. The GUI
> itself was developed by **Claude Opus 5**. See [NOTICE.md](./NOTICE.md).

## What it does

CoordsFinder recovers the coordinates a Minecraft screenshot was taken at, by
brute-forcing which world position would produce the exact pattern of texture
rotations visible on its blocks. The command-line tool is very fast. The awkward
part has always been *writing the config* — hand-counting block offsets into a
text file and hoping you did not transpose a digit.

This fork makes that part visual, and leaves everything else alone:

| | |
| --- | --- |
| **Paint the filter** | Click cells on a grid to set texture rotations, one Y layer at a time. `side` faces and all six netherrack faces are brushes. Full undo and redo. |
| **See it validate** | Every edit is re-checked by the real config parser. Block constraints, candidate count, and work items are shown before you commit to a scan. |
| **Get configs in easily** | Open a `.conf`, drop one on the window, or just press **Ctrl+V** — WebCoordsFinder puts its config on the clipboard, and it pastes straight in. |
| **Scan in the window** | Pick CPU or GPU, watch the live rate and estimated time, stop when you want, copy or save the matches. |

The original command-line tool is unchanged and ships alongside it.

## Download

Grab a build from the
[latest release](https://github.com/magicnothief/CoordsFinder-GUI/releases/latest).
Each platform gets two executables: `coordsfinder-gui`, the window, and
`coordsfinder`, the original command-line tool.

| Platform | Asset |
| --- | --- |
| Windows x86-64 | `coordsfinder-gui-windows-x86_64.exe` |
| Linux x86-64 | `coordsfinder-gui-linux-x86_64` |
| macOS Apple silicon | `coordsfinder-gui-macos-arm64` |

There is no installer and nothing to set up. On macOS and Linux you may need to
mark the download executable (`chmod +x`). Unsigned builds will also prompt
Windows SmartScreen and macOS Gatekeeper the first time you run them.

## Quick start

1. Mark up your screenshot in
   [WebCoordsFinder](https://github.com/ALaggyDev/WebCoordsFinder) and press its
   copy button — or just open one of the [`examples`](./examples) here.
2. Start `coordsfinder-gui` and press **Ctrl+V**. Check the summary it reports,
   then press **Load config**.
3. Set the X/Z ranges to the area worth searching and pick the directions. If
   you do not know which way the screenshot faces, tick all four.
4. Press **Start scan**.

Matches appear as they are found. Click one to copy its coordinates, or set an
output file first to have every match appended to it as well.

The [usage guide](./docs/usage-guide.md) walks a real search from screenshot to
coordinates, including what to do when you get no matches — or far too many.

## Requirements

The CPU backend needs nothing beyond the executable and works everywhere.

The optional GPU backend needs a driver exposing wgpu's `SHADER_INT64` feature:
Vulkan, DirectX 12 with DXC, or Metal 2.3+ on supported hardware. In the default
**Auto** mode the app uses the GPU when one qualifies and falls back to the CPU
when it does not, telling you which it picked.

For scale, on the machine this fork was developed on — Ryzen 5 5600 (12 threads)
and an RTX 3060 — the six-billion-candidate [`example.conf`](./example.conf)
search takes:

| Backend | Time |
| --- | --- |
| CPU, 12 threads | 4.7 s |
| GPU (Vulkan) | 0.13 s |

Upstream's benchmark, on much larger hardware and a much larger search, is in
[the config reference](./docs/config-reference.md#upstream-benchmark).

## Reading the grid

![Close-up of grid cells: plain coloured cells, cells with a bar, and cells with a red ring and a letter](./docs/images/grid-cells.png)

A painted cell shows its rotation as a **colour** and as the **digit** that goes
into the config. What a colour cannot say is which *face* a row is, so each
family carries one cue of its own:

| Cell | Meaning |
| --- | --- |
| Colour and digit only | Top or bottom face of an ordinary rotated block |
| A **bar** | A `side` face — bar at the top for `0`, at the bottom for `1` |
| A **red ring** and a letter | Netherrack — `U` `D` `N` `S` `E` `W` for its six faces |

The white outline marks the origin, the coordinate every offset is relative to.
Details in the [GUI reference](./docs/gui-guide.md#reading-a-cell).

## Documentation

| | |
| --- | --- |
| [Usage guide](./docs/usage-guide.md) | Screenshot to coordinates, end to end |
| [GUI reference](./docs/gui-guide.md) | Every panel, the grid's visual language, undo, pasting |
| [Config format](./docs/config-reference.md) | The `.conf` file, algorithms, directions, filter rows, CLI flags |
| [Development](./docs/development.md) | Building, testing, architecture, releasing, merging upstream |
| [Codebase guide](./docs/codebase-guide.md) | How the search engine itself works |

## Building from source

```sh
git clone https://github.com/magicnothief/CoordsFinder-GUI
cd CoordsFinder-GUI
cargo build --release
```

Rust 1.87 or newer. On Debian or Ubuntu the GUI also needs the system windowing
and file-dialog libraries:

```sh
sudo apt-get install libgtk-3-dev libxkbcommon-dev libwayland-dev \
  libgl1-mesa-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

To build only the command-line tool, and skip the GUI dependencies entirely:

```sh
cargo build --release -p coordsfinder
```

See [development.md](./docs/development.md) for the workspace layout, the test
suite, and how to merge upstream changes into the fork.

## Credits and licence

CoordsFinder is by **Laggy ([@ALaggyDev](https://github.com/ALaggyDev))**, and so
is everything that makes the search work. If you use this in a video or a
project, credit the original — upstream asks for it, and it is deserved.

- [CoordsFinder](https://github.com/ALaggyDev/CoordsFinder) — the upstream project
- [Laggy's video on texture rotation cracking](https://www.youtube.com/watch?v=gXTN9DD_Cp0)
- [WebCoordsFinder](https://github.com/ALaggyDev/WebCoordsFinder) — mark up a screenshot in the browser
- [Colab notebook](https://colab.research.google.com/drive/17qih1n6VpQx_77C2spIF-JOJp17y9Jt6?usp=sharing) — run a search on a free GPU

The GUI fork was developed by **Claude Opus 5** (Anthropic), directed and
reviewed by [@zselybence](https://github.com/magicnothief).

MIT, the same licence as upstream — see [LICENSE](./LICENSE). Attribution is set
out in [NOTICE.md](./NOTICE.md), and the licences of every bundled dependency,
including the fonts embedded in the GUI, are reproduced in
[THIRD-PARTY-NOTICES.md](./THIRD-PARTY-NOTICES.md).

Minecraft is a trademark of Mojang Studios. This project is not affiliated with,
endorsed by, or connected to Mojang Studios or Microsoft.
