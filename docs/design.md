# Design

How rusteezy works and why it's built this way.

## Goals

- **Fast.** A full grid of a few hundred columns redraws every frame with a lot of
  headroom above 120 fps on integrated GPUs.
- **Smooth.** Animation doesn't depend on the frame rate. Colors fade evenly.
- **Glow.** Light bleeds between characters. A terminal can't do this, and it's the
  reason rusteezy draws into its own window instead of a terminal.
- **The grid is the API.** You write characters and colors into cells. Everything
  else (3D, particles, effects) is built on top of the grid.

Non-goals: terminal output, a widget/UI toolkit, image-to-ASCII conversion.

## Approaches considered

1. **CPU scene → GPU character grid → bloom (chosen).** The CPU simulates and fills a
   grid of cells; the GPU draws the grid in one instanced call and post-processes it.
   It keeps the grid as the core primitive, it's easy to debug, and a 200×100 grid
   is tiny for a GPU. Trade-off: sub-cell motion only shows through color and glow.
2. **Render pixels on the GPU, then "ASCII-fy" in a final pass.** Smoother sub-cell
   detail and millions of particles, but the library becomes a set of shaders and
   characters become a filter. Rejected: harder to debug, and other code can't draw
   its own cells.
3. **Write to a real terminal.** No true blur, and the frame rate depends on the
   user's terminal emulator. Rejected.

## Frame pipeline

```
Scene::update  ×N (fixed step, 240 Hz)     ← simulation, particles
Scene::draw    ×1 per frame                ← fill the Grid (glyph, fg, bg, glow, depth)
   │
   ▼  pack cells (32 B each), one buffer upload
grid pass       one instanced draw, instance = cell → HDR target (Rgba16Float)
bright pass     soft-threshold the HDR image into bloom level 0 (half res)
downsample ×N   dual-filter (Kawase) blur chain, down to ~1/64 res
upsample ×N     3×3 tent, additively blended back up the chain
composite       scene + bloom × intensity → exposure → ACES tonemap → swapchain
```

- **Cells.** `fg.rgb + glow`, `bg.rgb`, glyph slot: 32 bytes. The fragment shader
  reads the cell by instance index from a storage buffer and fetches glyph coverage
  from the atlas with `textureLoad`, so glyphs stay pixel-exact.
- **Glow.** The glyph color is multiplied by `1 + glow × gain`, which pushes bright
  cells above 1.0 in the HDR target. The bloom bright-pass picks those up.
- **Atlas.** Printable ASCII plus a set of extra symbols are rasterized once with
  fontdue into one R8 texture. Missing glyphs come from fallback fonts. Cell size
  and baseline always come from the primary font.

## Timing

`FixedClock` accumulates real frame time and runs whole simulation steps;
`Frame::alpha` is the fraction between the last two steps, which particles use to
interpolate their positions. Record mode replaces real time with exactly `1/fps`
per frame, so recordings are reproducible.

## Depth

`Grid` carries a CPU-side depth buffer. `Grid::plot` is depth-tested, so a 3D scene
can draw its surfaces, then particles and decorations that pass correctly in front
of and behind them. The tree example ray-traces every cell against cones; the torus
example splats lit surface points.

## Measuring

`--bench` renders off-screen and waits for the GPU on every frame, which is the
most conservative measurement (CPU and GPU fully serialized). On macOS a window
can't present faster than the display refresh, so `--uncapped` alone doesn't show
headroom there.

## Future work

- Dirty-region uploads (only changed chunks of the cell buffer).
- GPU particle simulation for very large counts.
- A [ratatui](https://ratatui.rs) backend so existing TUI apps get glow for free.
- Snow that settles on the tree's branches (in tree space, so it rotates with it).
