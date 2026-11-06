# Run 08 queue

Take the first `[ ]` task each round. Marks: `[ ]` open, `[~]` in progress, `[x]` done, `[/]` partial,
`[!]` blocked. Each task names its WBS rows, the design it must match (`docs/design/screens/` +
`docs/design/png/`) and what "done" means. This queue wins over older prompts in `docs/prompts/`.
Run 07's queue and report are in `docs/loop/archive/run-07-*.md`.

## A. CI/CD first (the human saw the pipeline fail)

Known facts, from the human's GitHub Actions page on 2 Nov 2026 (CI workflow, 8 runs: #1, #2, #5 passed;
#3, #4, #6, #7, #8 failed; #8 is "Merge run 06"). Run 07 is not pushed yet, so GitHub still runs run 06's
code. Reproduced by the assistant on `main` @ a5bfc49 in an Ubuntu container: `cargo test --all` fails with
exactly one test, `scan::tests::cancelling_stops_the_scan_and_says_so` (`left: 4096, right: 0`, scan.rs
~line 486). Cause: on a real disk (ext4) the scanned root directory itself counts 4096 bytes; on tmpfs it
counts 0. The test hard-coded 0. Run 07 R7-Q01 changed this on `run/07-2026-11-02`. fmt, clippy and the
other 600+ tests passed in that container. The Actions logs themselves could not be read, so other
failures on GitHub (for example in the `supply-chain` job, or on #3/#4/#6/#7, which may have had other
causes) are not ruled out.

- [x] **Q01 Look at the pipeline before anything else.** (WBS 2.x CI row: find it by searching column B for
  "CI".) Steps: (1) check the cancel test on the branch: it must not depend on the file system. Run the
  plain `cargo test --all` **and** the same with `TMPDIR` on a different file system than the repo (tmpfs
  vs real disk; say which is which, `df -T`). (2) Write `tools/ci-local.sh`: the exact commands of
  `ci.yml` in order (`cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all --locked`, `cargo audit`, `cargo deny check`), stops at the first failure, prints each
  command's exit code. (3) Read both workflows line by line against what the repo needs now: system
  libraries Slint needs on ubuntu-22.04, `--locked` everywhere, caches, `permissions`, the AppImage steps
  in `release.yml`. Fix what is wrong; every change gets a reason in `docs/questions.md`. (4) Add to
  `ci.yml`: `concurrency` (cancel superseded runs on the same ref), `timeout-minutes` on every job, and a
  `release-build` job (`cargo build --release -p tidewise-app --locked`). Do **not** add any job that
  needs a secret, root on the runner for anything but installing packages, or the network beyond cargo
  and the pinned action downloads. (5) Add `.github/dependabot.yml` for `cargo` and `github-actions`
  (weekly). Done: `tools/ci-local.sh` exits 0 (show output), both workflow files parse, a test or script
  proves the cancel test passes on both file systems. **CI on GitHub: Not verified** (say so). If an
  Actions log is pasted by the human into `docs/ci-failure.log`, read it first and fix what it shows.
- [x] **Q02 Secret and supply-chain checks in CI.** A pinned secret scanner step only if it needs no new
  secret and can be pinned by SHA (otherwise a `grep`-based `tools/check-secrets.sh` with a test that it
  catches a fake key in a temp dir and ignores `tests/fixtures/`). `cargo deny` licence and advisory
  settings reviewed once more (`deny.toml`). `cargo audit` result recorded. Done: the script's tests, the
  workflow step, and exit codes in the report.
- [/] **Q03 Weekly fuzz smoke (optional job).** `.github/workflows/fuzz.yml`, `schedule` + `workflow_dispatch`
  only, nightly toolchain, 60 s per target from a loop over `fuzz/fuzz_targets`, uploads a crash artefact
  if any. Never on push. Done: the file parses and the target list is generated from the directory, not
  typed by hand. Not verifiable here: whether it runs on GitHub.

## B. README and project pages

- [/] **Q04 Update the GitHub README** (WBS 9.11). Today it is 24 lines and partly stale. It must be true
  of the code on this branch, and every claim checked against the source or a command you ran. Contents:
  what Tidewise is and who it is for; a **status table** (page → Built / Partial / Not built, one line
  each, taken from run 07's Summary, not from hopes); features with the screenshots already in
  `docs/screenshots/` (re-take them with `--screenshot` if the UI changed since; look at them); install:
  build from source (Rust version from `Cargo.toml` `rust-version` or CI, system libraries needed, on
  Ubuntu 22.04 and Arch) and **AppImage: "no release yet"** unless a tag exists; run flags (`--demo`,
  `--screenshot`, `TIDEWISE_RENDERER=gpu`); what needs admin access (not built yet) and what the app
  never does (no network code, no telemetry, nothing deleted without a confirm screen, Trash first);
  security (link `docs/security/`, how to report: `SECURITY.md`); contributing (link); a CI badge for
  `.github/workflows/ci.yml` on `main`; licence. No claims of speed, size or distros you did not measure:
  use the numbers in `docs/budgets.md` with their conditions. Done: each command in the README was run
  (exit codes in the report), each link target exists (check with a script), `CHANGELOG.md` agrees.
- [x] **Q05 CONTRIBUTING.md, SECURITY.md, issue templates.** `CONTRIBUTING.md`: build, `tools/ci-local.sh`,
  the red-then-green rule, design rules, the conventional-commit style used in the log, what needs
  discussion first. `SECURITY.md`: private reporting (GitHub private vulnerability reporting; do not invent
  an email address), supported versions "none released yet", what is in scope. `.github/ISSUE_TEMPLATE/`
  bug report (distro, version from `--version`, what you clicked, History JSON if relevant, no personal
  paths) and feature request. Done: files exist, links resolve, no invented contact details.

## C. Real bugs the pen-test checklist and run 07 hinted at

- [x] **Q06 Pen-test A16 and A21 as tests** (WBS 5.10). A16: End task must refuse PID 1 (and other protected
  processes) in code. A21: `--out` must refuse paths outside a sensible location (`/etc/passwd` and
  anything not writable by the user; decide, record in `docs/questions.md`). Write the tests first (red),
  then fix. Temp dirs and processes the test started only. Update the checklist file with the result.
  Done: red commit, green commit, plain `cargo test --all` exit 0.
- [ ] **Q07 Keyboard pass and contrast** (WBS 9.7). Contrast of every text/background colour pair in
  `tokens` ≥ 4.5:1 (≥ 3:1 for large text and icons) as a unit test that reads the real token values; list
  failures, fix by choosing an existing token (no new colours without a `docs/questions.md` entry). Tab
  order and Enter/Esc for dialogs: as far as the headless test can show it; what cannot be shown goes
  under Not verified. Done: the test, its output, the list.
- [ ] **Q08 Home files search: decide with numbers** (WBS 9.1). Measure a filename index of `$HOME` in a temp
  tree of 200 000 files (PSS and build time). If within the 40 MB budget, build it with tests; if not,
  write the numbers into `docs/questions.md` and keep "Files are not searched yet". Never index outside
  `$HOME`; follow `.gitignore`-like hidden-dir skipping rules you document.
- [ ] **Q09 Budgets for the pages run 07 did not measure** (WBS 3.8): Performance, Processes, Storage, the
  open Ctrl K box, and the start-up peak. `tools/measure.sh`, 60 s window, look at the page before
  measuring. Fix anything over (≤ 40 MB PSS, < 2 % idle CPU).

## D. Design misses (decide, don't tune)

Run 07 scores above the 10 % target: End dialog 26.1 %, History 32.8 %, Startup apps 18.7 %, Services
17.1 %, Settings 86.3 %, Health 33.7 %, Search 42.4 %. For each: open the diff image and the artboard side
by side and sort every difference into (a) a real bug in our layout/copy: fix it test-first; (b) the
artboard shows something we deliberately don't build (for example the Search "FILES" group): add it to
`docs/design/DESIGN-CHECK.md` as an accepted difference **proposed** by the loop and listed in
`docs/questions.md` for the human to approve (you may not edit `docs/design/`: put the proposal in
`docs/design-proposals.md`); (c) a limit of the checker (backdrop, font rasteriser): say so with the
evidence. Never reach a number by tuning copy or colours. Settings 86.3 % is almost certainly a real
layout bug or a wrong crop: find which.

- [ ] **Q10 Settings (86.3 %) and Health (33.7 %).** One round each if needed; report the new scores.
- [ ] **Q11 History (32.8 %), Startup apps (18.7 %), Services (17.1 %).**
- [ ] **Q12 End dialog (26.1 %) and Search (42.4 %).** Search: the missing "FILES" group depends on Q08.

## E. Leftovers from run 07's Partial list

- [ ] **Q13 Wire what was only half-wired.** Read run 07's Summary and each Partial entry in
  `docs/loop/archive/run-07-report.md`; list in `docs/progress.md` what is stub, what is wired, what is
  untested. Fix test-first whatever can be done without root: the "needs admin" states that are missing
  (Settings offline, update check), the Performance "sensor not available" state, the storage.png footer
  that is cut off, "Follow system" theme where it can be tested.
- [ ] **Q14 Questions triage.** `docs/questions.md` is long. Add a table at the top: ID, one-line question,
  recommended answer, what breaks if the human says no, status. Sorted by how much work depends on the
  answer. Do not answer anything on the human's behalf and do not delete entries.
- [ ] **Q15 Human checklist.** `docs/loop/human-checklist.md` already exists (written before the run).
  Update it with anything new from this run: what to click, what to run, what to compare. Keep it to one
  screen per page.
