# Run 09 queue

Take the first `[ ]` task each round. Marks: `[ ]` open, `[~]` in progress, `[x]` done, `[/]` partial,
`[!]` blocked. Run 08's queue and report: `docs/loop/archive/run-08-*.md`.

## Why this queue looks the way it does

Runs 07 and 08 built screens and then **measured** them: no design target (≤ 10 %) is met (Settings
70.3 %, Search 50.2 %, Health 33.7 %, History 32.8 %, End dialog 26.1 %, Startup 18.7 %, Services 17.1 %).
Most of the gap is **features the artboards show and the app does not have** (see `docs/design-proposals.md`),
not layout bugs. So this run builds the real, local, no-root features behind those differences, test-first,
instead of tuning pictures. Things that need the admin helper, a network, or a decision from the human stay
out. Nothing here is blocked on the human's answers: where a decision is needed, take the recommended
option, write it into `docs/questions.md`, keep going.

## A. Start with facts

- [ ] **Q01 CI log, if there is a newer one.** If `docs/ci-failure.log` is newer than the last report entry
  of run 08 (compare `ls -l --time-style=full-iso`), read it, find the failing step and fix it test-first
  (a red `tools/test-ci.sh` check where it can be expressed). If it is not newer, mark this task `[x]`
  "skipped: no new log" and say so. Run 08 found the real cause of the earlier red run: `cargo audit`
  (RUSTSEC-2026-0433, pulldown-cmark) — not the file-system test. GitHub's status for the run 07/08 pushes
  is **Not verified** unless a log says otherwise.
- [ ] **Q02 Finish what run 08 skipped: R8-Q13 and R8-Q14.** Wire the half-wired states: the "needs admin
  access" states that are missing (Settings offline, update check), the Performance "sensor not available"
  card, the storage.png footer that is cut off. Then the questions triage table at the top of
  `docs/questions.md` (ID, one-line question, recommended answer, what breaks if the human says no,
  status; sorted by how much depends on it; answer nothing for the human, delete nothing).

## B. Keyboard and accessibility (WBS 9.7, the biggest honest gap)

- [ ] **Q03 Keyboard in the three dialogs and in Ctrl K.** Run 08 found that the Clean up, Uninstall and End
  dialogs appear to have no keyboard support at all. Tests first (controller tests in Rust; headless key
  presses if the pre-approved testing backend is accepted): Esc closes and focus returns to the control
  that opened it; Enter activates the default (safe) button, never the destructive one; Tab stays inside the
  open dialog; a dialog that is working (progress) ignores Esc except "Stop after this step". Ctrl K box:
  ↑ ↓ move, Enter opens, Esc closes, typing keeps focus. Also visible focus rings using existing tokens.
  Done: red commit, green commit, the tests named in the report, and the list of what a person must still
  press by hand.

## C. Health: the checks the artboard shows (WBS 9.2)

- [ ] **Q04 Real Health checks.** Memory pressure (`/proc/pressure/memory`, falling back to `MemAvailable`
  when PSI is off), start-up time (`/usr/bin/systemd-analyze time`; "Not measured" when it is not systemd),
  temperatures (`/sys/class/hwmon`; "No sensors" when none). Each: parser with fixtures from the real files
  on this machine (read-only), unusual and missing-file cases, a rule with points (take the artboard's
  point values as the recommended answer and list them in `docs/questions.md`), the card state, and the
  Home score change documented in `CHANGELOG.md`. The "updates waiting" row stays "Not measured": counting
  updates needs a synced package database (state it on the card). Fuzz target for the new parsers.
  Done: tests, plain `cargo test --all` exit 0, `tools/design_check.sh health` score before and after.

## D. Settings: the groups the artboard shows (WBS 9.5)

- [ ] **Q05 Behaviour group.** "Size units" (GB 1000 / GiB 1024) applied by the one size formatter
  everywhere, boundary-tested; "technical details everywhere" as a global switch the screens honour; "Start
  Tidewise when I log in" (the pre-approved autostart file, create/remove, temp-dir tests, never touches
  other autostart entries); "Text size" only if Slint scaling can do it without new colours or layouts that
  break (else a `docs/questions.md` entry). "Notify me about problems": store the choice only if something
  reads it; otherwise leave it out and say so. Accent colour: not built (needs new colours).
- [ ] **Q06 "Never touch these folders".** A list in settings that every scanner (cleaner, storage
  selection, uninstall data removal) must respect: path rules (prefix match on whole components, symlinks
  resolved, `..` rejected), tests that a listed folder is never offered, never ticked, never removed, even
  when an explicit confirm contains it. Add/remove UI with the folder picker replaced by a typed path
  (no portal dependency). This is a safety feature: test it hardest.
- [ ] **Q07 Settings index and About.** The left section index (scrolls to a section), About with version,
  licence, "Copy system info" (distro, kernel, Tidewise version, renderer; no paths, no usernames, tested),
  and "Source code" / "Report a problem" shown as links to copy, not opened (no network code in Tidewise).
  The "Cleaning" items that need the helper or a notifier ("keep system logs for", "remind me to clean up",
  "package versions to keep") stay out: list them in `docs/design-proposals.md` with the reason.

## E. Release readiness (no tag, no publishing: the human does that)

- [ ] **Q08 AppImage smoke test and RELEASING.md.** `tools/appimage-smoke.sh`: build or take `dist/`,
  run it with `--appimage-extract-and-run`-style extraction in a temp dir under `target/`, check `--version`
  prints the Cargo version, `--screenshot home --demo` exits 0 and produces a non-blank PNG, and that nothing
  was written outside the temp dir and `XDG_*` dirs given to it. Test the script against a fake AppImage
  that fails each check. `docs/RELEASING.md`: the exact human steps (merge, green CI, `tools/ci-local.sh`,
  smoke test on a second distro, tag `v0.1.0`, read the draft release, verify the `.sha256`, publish),
  what the loop never does, and what is still missing (signing, an AppImageUpdate story). `CHANGELOG.md`
  gets a `0.1.0` section **drafted** under `Unreleased`, claims limited to what exists.
- [ ] **Q09 Budgets again.** `tools/measure.sh` on Health, Settings and the open Ctrl K box (run 08 left it
  unmeasured), 60 s window, look at each page first; fix anything over ≤ 40 MB PSS / < 2 % idle CPU.
  Update `docs/budgets.md`.

## F. End of run

- [ ] **Q10 Design re-measure and honest sorting.** Re-run `tools/design_check.sh` for every screen with a
  script entry (render at each artboard's own height; run 07's Settings number was a wrong crop). Update
  the scores in `docs/design-proposals.md` and `docs/design-check-report.md`. For each remaining gap: real
  bug, not-built feature, or checker limit, with the measured evidence. No tuning of copy or colours.
- [ ] **Q11 Questions table and human checklist.** Refresh the table from Q02 with this run's decisions;
  update `docs/loop/human-checklist.md` (what to press, what to run, what to compare) and
  `docs/loop/RESUME.md` for run 10.
