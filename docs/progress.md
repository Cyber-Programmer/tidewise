# Progress log

Newest entry at the bottom. One entry per session, written by Claude Code at the end of a task.

## 2026-10-27 — Planning
Done: Design (21 artboards), build plan, CLAUDE.md, phase prompts.
Next: Phase 0, session 1 — workspace + `crates/core` LiveStats.
Blockers: none.

## 2026-10-27 — Overnight run 1 (plans)
Plan T1: cargo workspace (core, adapters, app), LICENSE, README, .gitignore, deny.toml, CI workflow; verify fmt/clippy/test.
Plan T2: `core::stats` — pure rate functions tested first (zero interval, wrap, first sample), sysinfo sampler, one real two-sample test.
Plan T3: `core::distro` — os-release parser from hand-written fixtures, tool detection by absolute path, desktop from env.
Plan T4: `adapters::pacman` — capture real fixtures with LC_ALL=C, parse -Qi/-Qq/-Qdtq, tests for normal/no-required-by/German/empty.
Plan T5: `adapters::flatpak` + `adapters::appimage` — parse list output, scan dirs via tempdir tests.
Plan T6–T8: Slint spike, Tauri spike, measure.sh with fake /proc fixture; numbers into ADR as DRAFT.

## 2026-10-27 — Overnight run 1: T1–T8
Done: T1 workspace, CI, licence · T2 `LiveStats` · T3 distro detection · T4 pacman adapter · T5 Flatpak + AppImage · T6 Slint spike · T7 Tauri spike · T8 `measure.sh` + draft numbers. All on branch `overnight/2026-10-27`, one commit per task (plus a rustfmt fix after T8). Nothing pushed.
Next: you decide the toolkit in `docs/decisions/0001-ui-toolkit.md` (DRAFT, numbers only), read `docs/questions.md`, review the branch, then pick the next milestone.
Blockers: none. No task was skipped.

### Morning summary

**Final checks (this machine, after the last commit):** `cargo fmt --check` exit 0 · `cargo clippy --all-targets -- -D warnings` exit 0 · `cargo test --all` exit 0 (44 tests) · `cargo audit` exit 0 · `cargo deny check` exit 0 (it warns about two `syn` versions, which is allowed).

**What exists**
- `crates/core`: `rates` (pure rate maths), `diskstats`, `stats::LiveStats`, `distro`. Tests include one real two-sample run.
- `crates/adapters`: `PackageSource` trait, `pacman` (parser + read-only runner), `flatpak` (parser only), `appimage` (folder scan).
- `tests/fixtures`: real pacman and flatpak captures from this Arch machine; hand-written os-release, German pacman, extra flatpak and `/proc` tree fixtures, each marked hand-written in a README or comment.
- `spikes/measure.sh` with `tests/measure/test_measure.sh` (run by `cargo test` through `crates/core/tests/measure_script.rs`).

**Where I did not follow the process exactly**
- T2 `diskstats` parser: tests and code were written together, so there is no separate red run for it. The other T2 pieces, and T3–T5, T8, went red first.
- T1 was committed before `cargo audit` and `cargo deny` were installed (the install stalled twice on downloads); both were run and passed afterwards.
- My first final `cargo fmt --check` failed on a file I added after the last format; fixed in its own commit.
- The spikes are excluded from the main workspace (`exclude` in `Cargo.toml`), so CI, clippy, audit and deny do not cover them. Intentional, they are throwaway.
- No `tauri-cli`: the Tauri spike builds with `cargo build --release --features custom-protocol`.
- No Flatpak command runner yet, only the parser. No effort in human hours or time-to-first-window in the ADR.
- Added dependencies (all pre-approved): `sysinfo`, `thiserror`, `tempfile`, `pretty_assertions`; spikes: `slint`, `slint-build`, `tauri`, `tauri-build`, `serde`.

**Questions waiting:** `docs/questions.md` has one entry (no German locale for a real non-English pacman capture).

**How to run each spike** (build first, about 8 min for Slint and 5 min for Tauri on a clean checkout)
```
# Slint, FemtoVG
cd spikes/slint && cargo build --release
SLINT_BACKEND=winit-femtovg   ./target/release/tidewise-spike-slint
# Slint, software renderer
SLINT_BACKEND=winit-software  ./target/release/tidewise-spike-slint

# Tauri (needs webkit2gtk-4.1 and gtk3)
cd spikes/tauri && cargo build --release --features custom-protocol
./target/release/tidewise-spike-tauri

# Measure (about 2 minutes per run)
spikes/measure.sh --header /bin/true >/dev/null 2>&1   # prints the CSV header only with no command
MEASURE_LABEL=tauri spikes/measure.sh "$PWD/spikes/tauri/target/release/tidewise-spike-tauri"
MEASURE_LABEL=slint-software SLINT_BACKEND=winit-software spikes/measure.sh "$PWD/spikes/slint/target/release/tidewise-spike-slint"
```
Numbers from one run: Slint FemtoVG 58 MB, Slint software 23 MB, Tauri 313 MB (3 processes), see the ADR for caveats.
