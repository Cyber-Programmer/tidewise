# Phase 0 — UI toolkit spike (≈10 hours, ~5 sessions)

## Context
Tidewise must be light on memory: it reports other apps' resource use, so its own footprint matters.
We are choosing between **Slint** (Rust-native UI) and **Tauri** (Rust + system web view). We build the
same screen in both and measure. Read `CLAUDE.md` first. Design reference: `docs/design/screens/Performance.dc.html`
and `docs/design/tokens.md`.

## Goal
Two throwaway prototypes of the Performance screen sharing one real `crates/core`, plus a written
decision with numbers in `docs/decisions/0001-ui-toolkit.md`.

## Sessions (one per sitting; stop after each)
1. **Workspace + core.** Create the Cargo workspace and `crates/core` with a `LiveStats` API: CPU %,
   memory used/total, disk read/write, network down/up, and a process list (name, CPU %, memory),
   sampled once per second. Unit tests for the maths (rates from two samples). Propose the crate for
   reading stats (e.g. `sysinfo`) and wait for OK.
2. **Slint prototype** in `spikes/slint/`: sidebar, 4 metric cards, a 60-second line chart, a scrollable
   ~200-row process list, styled with the tokens. Timebox: 6 hours total across sessions 2–3.
3. **Slint, finish + measure** with both the FemtoVG and the software renderer.
4. **Tauri prototype** in `spikes/tauri/` with the same screen (plain HTML/CSS/JS or a small framework;
   propose which). Same timebox.
5. **Measure + decide.** Write `spikes/measure.sh` that launches each build, waits 60 s, then sums `Pss`
   from `/proc/<pid>/smaps_rollup` across the app **and every child process** (Tauri starts WebKit
   helpers). Also record idle CPU over 60 s, time to first window (3 cold runs), AppImage/binary size.
   Fill the table in the decision note.

## Decision rule (already agreed)
- Slint if its PSS ≤ 60% of Tauri's **and** the design was matched within the 6-hour timebox.
- Tauri if Slint could not match the design in 6 hours, or the gap is under 40 MB.

## Acceptance
- `crates/core` has tests and passes clippy; it will be kept.
- `docs/decisions/0001-ui-toolkit.md` lists: machine, distro, GPU, X11/Wayland, every number, effort
  hours, glitches seen, and the choice.
- Spikes stay in `spikes/` and are deleted after the decision is merged.

## Don't
- Don't polish either prototype beyond what the measurement needs.
- Don't add any write/delete capability. This phase is read-only.
