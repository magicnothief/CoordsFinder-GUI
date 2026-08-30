# Usage guide

This walks a whole search: from a Minecraft screenshot to a coordinate, using
the GUI. If you want the reference for a particular panel instead, that is the
[GUI reference](./gui-guide.md); for the file format, the
[config format reference](./config-reference.md).

## What this actually does

Minecraft picks the rotation of many block textures from the block's world
coordinates. Grass, dirt, stone, sand, concrete powder, netherrack and others
all get a rotation that looks random but is a pure function of *where the block
is*. So a screenshot showing a patch of such blocks encodes its own position.

CoordsFinder takes the pattern of rotations you read off a screenshot and tries
every coordinate in a range until it finds one that would produce exactly that
pattern. A dozen or so blocks is usually enough to pin down a single answer
across millions of square blocks.

Two consequences worth knowing before you start:

- **You need enough blocks.** Each four-state block contributes 2 bits. A search
  space of 10,000 × 10,000 × 60 is about 36 bits, so you want roughly 18–20
  four-state blocks before a single answer is likely. Fewer, and you will get
  many matches.
- **You need the rotations to be right.** One misread block with tolerance `0`
  means no match at all.

## Step 1 — read the screenshot

Use [WebCoordsFinder](https://github.com/ALaggyDev/WebCoordsFinder). It lets you
upload the screenshot, drag a grid over the blocks, and click each cell to mark
the rotation you see. That is far easier than eyeballing offsets, and it is what
the tool exists for.

When you are done, use its **copy config** button, or download the `.conf`.

You can also write a config by hand or build the filter directly in this GUI,
but reading rotations off a flat screenshot by eye is the hard part, and
WebCoordsFinder is built for it.

> **A note on platforms.** Only the Windows build has actually been run. The
> Linux and macOS binaries compile and pass the test suite in CI, but the window
> itself is untested there. See the
> [README](../README.md#download).

## Step 2 — get it into the window

![The paste dialog, showing a pasted config validated as Vanilla-3 with 43 filter rows](./images/paste-config.png)

Start `coordsfinder-gui` and press **Ctrl+V**. The paste dialog opens with the
clipboard already in it and tells you what it found — algorithm, filter row
count, directions, ranges, tolerance — or shows the parser's error if the text
is not a valid config. Press **Load config**.

If you have a file instead, use **File → Open…** (Ctrl+O), drag the `.conf` onto
the window, or pass it on the command line:

```sh
coordsfinder-gui ./my-search.conf
```

## Step 3 — check what you loaded

Look at the **Summary** panel on the left before scanning anything:

- **Filter rows** — how many observations you marked.
- **Block constraints** — those rows after ones on the same block are merged.
  This is what the scanner actually checks, and it is the number that matters
  for whether your search is specific enough.
- **Candidates** — how many coordinates will be tested.
- **Work items** — how the search splits into tiles.

If the config is invalid the summary shows the parser's error instead, and the
Start button stays disabled.

The grid shows what you loaded. Scan it against your screenshot — a transposed
row or a misread rotation is much easier to spot as a picture than as a list of
numbers.

## Step 4 — set the search area

| Setting | What to put in it |
| --- | --- |
| **X / Z range** | The area worth searching. Ranges are half-open: `(-5000, 5000)` covers −5000 up to 4999. |
| **Y range** | The layers the blocks could be at. If you know the Y from the screenshot's F3 or from context, use a tight range — it is a direct multiplier on the work. |
| **Directions** | Which way the screenshot faces. Tick all four if you do not know; it costs 4× the work but a wrong guess costs you the answer. |
| **Error tolerance** | Blocks allowed to mismatch. Start at `0`. |

Candidates scale with X × Z × Y × directions, so the summary's candidate count
is the honest measure of how long this will take.

## Step 5 — scan

Pick a backend — **Auto** uses the GPU when one qualifies and otherwise the CPU,
and tells you which it chose. Set an **output file** if the run is long or you
expect many matches; matches are appended to it as they are found, so nothing is
lost if you close the window.

Press **Start scan**. The progress bar shows work items done, the live candidate
rate, and a rough time remaining. **Stop** ends it — the CPU backend stops
between X/Z columns, the GPU backend after its current tile, so a large tile can
take a moment to wind down.

Matches stream into the list as they are found:

```text
      1570    -45      -1236   dir   0°   0 mismatch(es)
```

Click a row to copy its coordinates.

## When it does not work

### No matches at all

Almost always a misread rotation, or the wrong algorithm or direction.

1. **Check the algorithm.** It must match the Minecraft version the screenshot
   came from — `Vanilla-3` for 1.21.2+, `Vanilla-2` for 1.13–1.21.1,
   `Vanilla-1` for 1.12.2 and earlier, and the `Sodium-*` modes for older Sodium
   versions. The full table is in the
   [config reference](./config-reference.md#algorithm).
2. **Tick all four directions.** If the facing was wrong, nothing will match.
3. **Widen the ranges.** The answer may simply be outside the box, including the
   Y range.
4. **Raise error tolerance to 1 or 2.** This lets a single misread block through.
   It is much slower — the cost climbs steeply, and above `3` it is rarely worth
   it — but it will find an answer a single mistake was hiding.

### Far too many matches

The filter is not specific enough for the area you are searching. Mark more
blocks in WebCoordsFinder, or narrow the X/Z range if you have any idea where to
look. Every extra four-state block quarters the number of false matches.

### The GPU is not being used

The status line says which backend was chosen and, if the GPU was rejected, why.
The usual reason is a driver without wgpu's `SHADER_INT64` feature. The CPU
backend gives the same answers, just slower.

### A tile found too many matches

The GPU backend reports this when one tile produced more than 262,144 matches
and asks for more filters. Shrinking the GPU tile would get past the error, but
not usefully — a filter that loose has hundreds of thousands of answers. Mark
more blocks instead.

### The scan stopped on its own

If the log says the scan thread stopped unexpectedly, the GPU backend was very
likely taken down by a display driver reset: Windows resets the driver when a
single work item runs too long. Lower **GPU tile** under **Advanced** so each
item finishes sooner — a nonzero error tolerance makes this much likelier — or
run the scan on the CPU backend. The
[config reference](./config-reference.md#why-did-windows-say-that-the-display-driver-stopped-responding-or-why-did-wgpu-panic-in-buffermap_async)
has the details.

## Building a filter by hand

You do not have to start from WebCoordsFinder. The grid is a full editor:

![The rows editor, showing the filter as editable text](./images/rows-editor.png)

- **Left-click** paints the selected brush and rotation; clicking a cell that
  already holds it cycles `0 → 1 → 2 → 3`.
- **Drag** paints a stroke, which undoes as a single action.
- **Right-click** erases.
- **Ctrl+Z** and **Ctrl+Y** step through your edits.
- The **Rows** tab is the same filter as text, if you would rather type it.

Set the brush to `Side face` for the sides of mirrored blocks like stone and
deepslate, and to one of the netherrack faces for netherrack. Which blocks have
texture rotations at all is listed in the
[config reference](./config-reference.md#which-blocks-have-texture-rotations).

Save with **Ctrl+S**. The result is an ordinary `.conf` that the command-line
tool reads, so you can build the config in the window and run the search on
another machine:

```sh
coordsfinder --backend gpu --output matches.txt ./my-search.conf
```
