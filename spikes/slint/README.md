# Slint spike

The Performance screen (`docs/design/screens/Performance.dc.html`, dark theme) built with Slint 1.18,
fed by `tidewise-core` (`LiveStats`, one sample per second).
Not part of the main workspace: it has its own `Cargo.lock` and `target/`.

## Build

```
cd spikes/slint
cargo build --release          # -> target/release/tidewise-spike-slint
cargo test --release           # unit tests for the chart-path and scale helpers
```

First build takes about 8 minutes on this machine (Slint + femtovg + winit + software renderer).

## Run

Both renderers are compiled in; pick one at run time with `SLINT_BACKEND`:

```
# FemtoVG (OpenGL)
SLINT_BACKEND=winit-femtovg ./target/release/tidewise-spike-slint

# Software renderer (no GPU needed)
SLINT_BACKEND=winit-software ./target/release/tidewise-spike-slint
```

Without the variable Slint chooses for you (FemtoVG when OpenGL works).

## What is in it
Sidebar, four metric cards with sparklines (click to switch the chart), a 60-second line chart
redrawn once per second, and a scrollable list of the 200 busiest processes.

## Known gaps (it is a spike)
- Geist / Geist Mono are not bundled; system sans and monospace stand in.
- Sidebar has no icons; "Show technical details" and the per-metric explanation text are not built.
- Sampling pauses while the window is hidden (`Window::is_visible`), but Slint gives no direct signal
  for "minimised", so a minimised window may keep sampling.
- Process names come from `/proc/<pid>/comm`, so they are cut at 15 characters (for example `tidewise-spike-`).
