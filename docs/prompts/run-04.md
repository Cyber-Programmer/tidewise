# Run 04 — green CI, security groundwork, M2: Storage + first test build (unattended)

You are working alone; nobody can answer questions until the run ends.
Read `CLAUDE.md` (and the `@docs/ENGINEERING.md` it imports), `docs/design/DESIGN-CHECK.md`, the last
entry of `docs/progress.md` and the open entries in `docs/questions.md` first.
CLAUDE.md rules apply, with these overrides **for this run only**:

- **No waiting for approval.** Write a 3–5 line plan per task into `docs/progress.md`, then do it.
- Several tasks per run is fine; finish one fully (red commit, green commit, checks, design check if it
  has a screen) before the next.
- **Pre-approved:** `cargo install cargo-fuzz`; dev-dependency `libfuzzer-sys` (in `fuzz/` only);
  `trash` (latest 5.x) if `cargo deny` accepts its licence tree. Nothing else new → `docs/questions.md`.
- When stuck on a decision only the human can make: question + recommended answer in
  `docs/questions.md`, take the safest reversible option, keep going.

## Hard limits (never)

- No `sudo`, `pkexec`, `su`, nothing needing root. Missing system tool → `docs/questions.md`, skip.
- No work on `crates/helper`, `crates/helper-protocol`, `packaging/polkit` (that's M3).
- No files changed outside this repository (cargo caches and `~/.cargo/bin` excepted).
- No `git push`, merge, rebase, force or history rewrites; no tags. Local commits only.
- No destructive commands on real user data. "Move to Trash" is tested only on temp dirs with
  `XDG_DATA_HOME` and `HOME` pointed at them; never trash a real file.
- Don't edit `docs/design/`.

## Dates and naming

Run `date '+%F %H:%M %Z'` at the start and use **that** date everywhere; never guess it.
Branch: `run/04-<YYYY-MM-DD>` from `main`. Commits in Conventional Commits style, prefixed with the task:
`test(core): R4-T2 drive totals (red)`, `feat(core): R4-T2 drive totals`.

## Tasks, in order (red commit first, then green — every behaviour change)

**R4-T0 — CI is green on GitHub's Ubuntu runner.** The last CI run on `main` (merge of run 3) failed in
`cargo test` (exit 101). Already diagnosed on a clean Ubuntu 24.04 machine — it's a real bug, not a flaky test:
- Only failing test: `scan::tests::the_background_scan_reports_progress_and_returns_the_result`
  (`crates/core/src/scan.rs:445`), `left: 204800, right: 208896` — off by exactly 4096 bytes.
- Cause: `total_bytes` includes the blocks of sub-**directories** (line ~205 adds `meta.blocks() * 512` for
  every entry under a top folder), but `progress.bytes` only adds **files** (line ~222). On this laptop
  `/tmp` is tmpfs, where directories use 0 blocks, so the two matched; on ext4 (CI) a directory uses 4096.
- Effect for users: the progress total and the final total disagree on ext4/btrfs, so a progress bar
  never quite reaches 100 % and "scanned X GB" differs from the result.
Fix: count directory blocks in progress too, so progress and total use one rule (and `du` agrees). First
make the test fail **on this laptop** too: create the temp dir on a real disk (e.g. under `target/`), not
`/tmp`, so tmpfs can't hide it; add a second test with nested folders. Then also check every other test
that uses `tempfile::tempdir()` for the same tmpfs blind spot, and use a disk-backed temp dir where block
counts matter. Never weaken an assertion to pass.
Also in this task:
- remove `screenshots-test/` (old spike screenshots committed by mistake);
- `actions/checkout@v4` warns that Node 20 is deprecated: move every action to its current major,
  pinned to a full commit SHA with the version in a comment (OpenSSF "Pinned-Dependencies");
- add `permissions: contents: read` at the top of the workflow (least privilege).
Done when you have shown, from the code and a local run, why CI failed and that the fix covers it. The human
checks GitHub after pushing.

**R4-T1 — Security groundwork.**
- `docs/security/threat-model.md`: assets, trust boundaries, data flows (app as user, helper as root,
  update check), STRIDE table per boundary with defence + the test that proves it (or "TODO M3").
- `SECURITY.md` at the root: how to report a vulnerability privately (GitHub private advisories), supported
  versions, what's in scope.
- `#![forbid(unsafe_code)]` in `crates/core` and `crates/adapters`; `#![deny(unsafe_code)]` in `crates/app`
  (Slint's generated code may need an allow; document any).
- `fuzz/` with `cargo fuzz` targets for: pacman `-Qi`, flatpak list, os-release, `statm`, `/proc/<pid>/stat`,
  `.desktop` parsing. Run each for 5 minutes (`-max_total_time=300`) with the nightly toolchain. Every
  crash becomes a fixture + regression unit test, then a fix. Record targets, runtime and results in
  `docs/security/fuzzing.md`. CI doesn't run fuzzing yet (note it as a follow-up).

**R4-T2 — Drive totals and access-time reliability (`core`).** Per mounted drive: size, used, free, using
`sysinfo::Disks` (already a dependency). Skip pseudo filesystems. Detect `noatime`/`relatime` from
`/proc/mounts` for the home folder's filesystem and report whether "not opened in 6 months" can use
access time or must fall back to modified time. Fixture tests for `/proc/mounts` lines.

**R4-T3 — Storage screen** to match `docs/design/png/Storage.png` and `Storage--downloads.png`: drive
switcher, drive bar with legend, folder map (treemap), drill-in with checkboxes, biggest files with trash
buttons, the Downloads hint. Wire `core::scan` (background thread, progress, cancel) in live mode; demo mode
uses the artboard's sample values. Run the design-check loop until both images are ≤ 10 %, scores into
`docs/design-check-report.md`. Accepted difference: biggest-file sizes stay on one line.

**R4-T4 — Move to Trash with Undo.** Freedesktop Trash spec, as the user, through the `trash` crate (or
your own small implementation if deny rejects it; say which). Undo restores the exact files. Toast with Undo
(artboard F shows it). Tests only in temp dirs: trash then restore round-trip, name clashes in Trash, a file
on another filesystem (expect a clear "can't move to Trash" error, never a silent delete), a symlink
(trash the link, not its target).

**R4-T5 — Home suggestions + health score from real data.** Rules exactly as on the "Why 82?" artboard
(`docs/design/screens/Health.dc.html`): drive fullness and big-memory suggestions are real now; junk total
stays a clearly labelled stub until M3. Unit test per rule (boundaries included).

**R4-T6 — AppImage build.** `packaging/appimage/build.sh` + `.desktop` file + icon (from
`docs/design/icons/logo.svg`), using `appimagetool` from `PATH` (the human installs it; if missing, write
the script and its test anyway and note it). Output `dist/Tidewise-<version>-x86_64.AppImage`. Test: the
AppImage starts with `--screenshot home --out …` and produces a 1360×960 PNG (headless, no FUSE needed:
`--appimage-extract-and-run`). Add a GitHub workflow `release.yml` that builds the AppImage on
`ubuntu-22.04` when a tag `v*` is pushed and attaches it plus a `.sha256` to a **draft pre-release**.
Signing comes later (the human holds the key).

**R4-T7 (stretch) — "Add to app menu".** Writes `~/.local/share/applications/io.github.tidewise.desktop`
and the icon; idempotent; removable. Tests with `XDG_DATA_HOME` pointed at a temp dir.

## Done means

- Every task committed with red → green commits, or written up in `docs/progress.md` as not reached.
- `docs/progress.md`: plans, per-task notes, any bent rule, and a summary at the end.
- Final checks on the final commit, all exit 0: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --all`, `cargo audit`, `cargo deny check`.
- Build last and end with the **Run it** block from CLAUDE.md's Definition of done (plus the AppImage
  path if R4-T6 produced one).
