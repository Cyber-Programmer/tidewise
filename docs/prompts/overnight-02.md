# Overnight run 2 — M1: real app shell in Slint, matched to the design (unattended)

You are working alone overnight. Nobody can answer questions until morning.
Read `CLAUDE.md` and `docs/design/DESIGN-CHECK.md` first. CLAUDE.md rules apply, with these overrides
**for this run only**:

- **No waiting for approval.** Write a 3–5 line plan per task into `docs/progress.md`, then do it.
- Several tasks in one session is fine; finish one fully (tests green, design check done, committed)
  before the next.
- **Pre-approved dependencies:** `slint`, `slint-build`, `image` (`default-features = false`,
  features `["png"]`), plus everything already in `Cargo.lock`. Anything else → `docs/questions.md`.
- When stuck on a decision only the human can make: write the question + your recommended answer to
  `docs/questions.md`, take the safest reversible option, keep going.

## Hard limits (never)

- No `sudo`, `pkexec`, `su`, nothing needing root. Missing system package → `docs/questions.md`, skip.
- No work on `crates/helper`, `crates/helper-protocol`, `packaging/polkit`.
- No files changed outside this repository (cargo caches excepted).
- No `git push`, merge, rebase, force or history rewrites. Local commits only.
- No destructive system commands. Read-only commands are fine.
- Don't edit anything in `docs/design/` (it's the target, not your work).

## Git

Branch `overnight/2026-10-28` from `main`. One commit per finished task: `M1-Tn: <what changed>`.

## Decision already made

UI toolkit: **Slint** (`docs/decisions/0001-ui-toolkit.md`). Software renderer is the default; FemtoVG is
used when `TIDEWISE_RENDERER=gpu` is set. The Slint spike is your starting point; Tauri is gone.

## Tasks, in order (test-driven: failing test first, every time)

**M1-T0 — Promote the spike.** Move `spikes/slint` code into `crates/app` (binary `tidewise`) inside the
workspace so CI, clippy, audit and deny cover it. Move `spikes/measure.sh` and its test to
`tools/measure.sh`. Delete `spikes/`. CI must still pass.

**M1-T1 — Theme, fonts, icons.** `ui/theme.slint` with a `Theme` global holding every token from
`docs/design/tokens.md` (dark values now; leave a light set stubbed). Import the Geist and Geist Mono TTFs
from `assets/fonts/` in the `.slint` files and set Geist as the window's default font; all numbers use
Geist Mono. Load icons from `docs/design/icons/` (copy them to `crates/app/ui/icons/`). Test: a unit test
that parses `tokens.md` and checks every colour exists in `theme.slint` with the same hex.

**M1-T2 — Demo data + headless screenshots.** Add `--demo` (sample data instead of live stats) and
`--screenshot <screen> [--state <variant>] --out <file.png>`: render at exactly 1360×960 with the
software renderer and no window (Slint `MinimalSoftwareWindow` / `take_snapshot`), then exit. Demo values
are copied from each artboard's script block in `docs/design/screens/*.dc.html` (for Performance, port the
`series()` function so the chart has the same shape). Tests: screenshot output is 1360×960; two runs give
identical bytes.

**M1-T3 — Performance screen** to match `png/Performance.png` and its variants `--details`, `--memory`,
`--memory-details`, `--network`, `--disk`: metric tabs, big chart with area fill, headline in plain words,
explanation box, "Using the most right now" top 3 with bars, "Show technical details" switch with the
facts grid (and per-thread bars on CPU), green "Live" dot. Run the design-check loop for all six images.

**M1-T4 — Processes screen** to match `png/Processes.png` and variants `--details`, `--apps`,
`--protected-selected`: grouped by app, filter tabs, search box, selected-process bar, lock icon and no End
button for protected ones (End task itself is M4: the button does nothing yet). Live mode: display names
from `/proc/<pid>/cmdline` / the `.desktop` name, not the 15-character `comm`. Unit tests for grouping
and naming from fixtures. Design-check all four images.

**M1-T5 — Home + shell** to match `png/Main.png`: sidebar with logo, icons, active state, distro card from
real `distro` detection (demo mode shows the design's text), health ring, suggestion rows, "Right now"
tiles, apps-by-source strip. Suggestions and health score are demo-only for now; in live mode show the
tiles and hide suggestions behind a "Scan coming soon" empty state. Navigation between Home, Performance
and Processes works; other sidebar items show a simple "Coming in a later version" page.

**M1-T6 — Live-mode polish.** Pause sampling when the window is hidden or minimised (find what Slint
offers; if nothing, note it in questions). Measure idle memory and CPU with `tools/measure.sh` in live mode,
both renderers, and add the numbers to `docs/progress.md`.

## Done means (for each screen task)

- LAYOUT MISMATCH ≤ 10 % for every reference image of that screen, scores in
  `docs/design-check-report.md` (before → after), plus the list of remaining accepted differences.
- Tests green: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all`.

## When you finish (or can't continue)

Append a morning summary to `docs/progress.md`: tasks done/skipped and why, the score table, questions
waiting, how to run the app (`cargo run -p tidewise-app`, `--demo`, `TIDEWISE_RENDERER=gpu`).
