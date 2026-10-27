# Overnight run 1 — foundation + spike builds (unattended)

You are working alone overnight. Nobody can answer questions until morning.
Read `CLAUDE.md` first. Its rules still apply, with these overrides **for this run only**:

- **No waiting for approval.** Write a 3–5 line plan for each task into `docs/progress.md`, then do it.
- **Several tasks in one session** is fine: finish one task completely (tests green, committed) before
  starting the next.
- **Pre-approved dependencies:** `sysinfo`, `serde`, `serde_json`, `thiserror`, `anyhow`, `tempfile`,
  `pretty_assertions`, `slint` (+ `slint-build`), `tauri` v2 (+ `tauri-build`, `tauri-cli`).
  Installing `cargo-audit` and `cargo-deny` with `cargo install` is allowed. Anything else: don't add
  it; write it to `docs/questions.md` and continue with another task.
- **When stuck** on a decision only the human can make, write the question and your recommended answer
  to `docs/questions.md`, choose the safest option that keeps work reversible, and move on. Never stop
  just to ask.

## Hard limits (never, for any reason)

- No `sudo`, `pkexec`, `su`, or anything that needs root. If a system package is missing, record it in
  `docs/questions.md` and skip that task.
- No work on `crates/helper`, `crates/helper-protocol` or `packaging/polkit`.
- No deleting or changing files outside this repository (cargo and npm caches excepted).
- No `git push`, no force operations, no rewriting history. Local commits only.
- No destructive system commands at all (no cache cleaning, no package removal, no `systemctl`).
  Read-only commands like `pacman -Qi` are fine.

## Git

1. If this folder isn't a git repo yet: `git init`, commit the starter files on `main` as "Starter files".
2. Create and work on branch `overnight/2026-10-27`.
3. One commit per finished task, message: `<task id>: <what changed>`.

## Tasks, in order (test-driven: write the failing test first, show it failing, then the code)

**T1 — Workspace + CI.** Cargo workspace with `crates/core` and `crates/adapters` (empty `crates/app` is
fine). `LICENSE` (GPL-3.0-or-later text), README stub, `.gitignore`, `.github/workflows/ci.yml` running
fmt, clippy `-D warnings`, test, audit, deny. `deny.toml` that allows GPL-compatible licences.

**T2 — LiveStats in `core`.** API that samples once per call and returns CPU %, memory used/total, disk
read/write rate, network down/up rate, and processes (pid, name, CPU %, memory). Rates come from two
samples; put the rate maths in pure functions and unit-test them (zero interval, counter wrap, first
sample). One integration test that takes two real samples on this machine and checks values are sane.

**T3 — Distro detection in `core`.** Parse `/etc/os-release` into name, id, id_like, version; detect
which of pacman, apt, dnf, flatpak, snap exist (by absolute paths in `/usr/bin`) and the desktop from
`XDG_CURRENT_DESKTOP`. Fixtures for Arch, Manjaro, Ubuntu 22.04, Ubuntu 24.04, Debian 12, Mint, Fedora
(write these os-release fixtures from the official formats; mark them as hand-written in a comment).

**T4 — Read-only pacman adapter.** Capture real fixtures on this machine with `LC_ALL=C`:
`pacman -Qq`, `pacman -Qi` for 3 packages (one with optional deps, one with "Required By"), `pacman -Qdtq`.
Parse installed packages (name, version, installed size, required-by). Tests: normal, a package with
no "Required By", a non-English output sample (run once with `LC_ALL=de_DE.UTF-8` if that locale exists;
otherwise note it in questions), empty output.

**T5 — Read-only Flatpak and AppImage listing.** If `flatpak` exists, capture
`flatpak list --app --columns=application,name,size,installation` as a fixture and parse it; otherwise
write the parser from the documented format and mark the fixture hand-written. AppImage: find
`*.AppImage` files in `~/Applications`, `~/.local/bin`, `~/Downloads` (read-only), size and name.
Use a temp dir in tests, never the real home.

**T6 — Slint spike** in `spikes/slint/` using `crates/core`: sidebar, 4 metric cards, a 60-second line
chart updating each second, a scrollable process list (~200 rows), colours and sizes from
`docs/design/tokens.md`, layout from `docs/design/screens/Performance.dc.html`. Builds with
`cargo build --release`. Add run instructions for both the FemtoVG and software renderers to its README.

**T7 — Tauri spike** in `spikes/tauri/`, same screen, same data from `crates/core` via Tauri commands,
plain HTML/CSS/JS (no frontend framework). Builds with `cargo tauri build --no-bundle` or the closest
working equivalent. If a system library is missing (webkit2gtk-4.1 etc.), record it and skip.

**T8 — Measurement script** `spikes/measure.sh <command>`: launches the app, waits 60 s, sums `Pss`
from `/proc/<pid>/smaps_rollup` for the process **and all descendants**, samples CPU over 60 s, records
binary size, prints one CSV line. Test the summing logic with a fake `/proc` tree in `tests/fixtures`.
If a display is available, run it on both spikes (and Slint's software renderer) and put the numbers in
`docs/decisions/0001-ui-toolkit.md` marked **DRAFT — human decides**. Don't pick the toolkit yourself.

## When you finish (or can't continue)

- Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all` one last time
  and show the output.
- Append a morning summary to `docs/progress.md`: tasks done, tasks skipped and why, questions waiting
  in `docs/questions.md`, and exactly how to run each spike.
