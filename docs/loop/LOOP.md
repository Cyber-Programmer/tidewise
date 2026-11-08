# Run 09 — unattended loop: keyboard, real Health and Settings features, release readiness

This file is the instruction for **every round** of a self-paced `/loop`. Each round does **one task**
from `docs/loop/queue.md`, end to end, then schedules the next round. Nobody can answer questions until
morning. State lives in files, not in your memory: after a context compaction or a resume, re-read this
file, `docs/loop/queue.md` and the end of `docs/loop/report.md` and carry on from there.

`CLAUDE.md` and `@docs/ENGINEERING.md` apply, with the overrides below **for this run only**.

## Overrides for this run

- **No waiting for approval.** Write a 3–5 line plan into `docs/progress.md`, then do it.
- **Several tasks per session** (one per round), each finished fully before the next.
- **Pre-approved by the human for this run** (nothing else new; anything else → `docs/questions.md`):
  - dependencies `serde` (with `derive`) and `serde_json`, if `cargo deny check` accepts them;
  - writing these locations **in app code** (tests always use temp dirs with the `XDG_*` variables and
    `HOME` pointed at them): `$XDG_CONFIG_HOME/tidewise/` (settings), `$XDG_STATE_HOME/tidewise/history/`
    (history records), `$XDG_CONFIG_HOME/autostart/` (startup overrides, `Hidden=true` copies only, with a
    backup), `$XDG_DATA_HOME/Trash/` (already used).
- **Fixing your own red-first slips is allowed** on this run's branch before anything is pushed: go back
  with `git reset --soft <commit before the task>` (keeps every file), then re-commit in the right order
  (test-only red commit, then code). Never `--hard`, never on `main`, never after a push.
- **Pre-approved for this run:** editing `.github/**` (workflows, Dependabot config, issue templates),
  `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`, and adding `tools/ci-local.sh`. Every
  third-party GitHub Action is pinned to a full commit SHA (as in `ci.yml` today); look up the SHA of the
  latest release with `gh api` only if `gh` is logged in, otherwise keep the existing pins and put the
  wanted new action in `docs/questions.md`.
- **The loop cannot see GitHub.** It cannot read Actions logs or push. "CI passes" is therefore always
  **Not verified** in the report; what it can show is that `tools/ci-local.sh` (same commands as the
  workflow, in the same order) exits 0, and that each workflow file parses (`python3 -c 'import yaml'`
  or `actionlint` if installed; say which).
- **Pre-approved for this run (nothing else new):**
  - dev-dependency `i-slint-backend-testing` at the same version as `slint`, only if `cargo deny check`
    accepts it, to send key presses in headless tests; if not accepted, test the controller logic in Rust
    and say what stayed unverified;
  - reading `/proc/pressure/*`, `/proc/meminfo`, `/sys/class/hwmon/*/temp*_input` and running
    `/usr/bin/systemd-analyze time` (absolute path, argument list, `LC_ALL=C`, read-only);
  - writing `$XDG_CONFIG_HOME/autostart/tidewise.desktop` **only** when the user turns "Start Tidewise when
    I log in" on, and removing that same file when they turn it off (tests: temp `XDG_CONFIG_HOME`);
  - the artboards' own sample numbers are not a reason to hard-code text: every number on screen comes from
    code that measured it or from the demo data.
- **Hand tests stay inside the repo.** Run 08 wrote a file outside the repository and left another in
  `/tmp`. Any command you run by hand that writes a file (`--out`, `--screenshot`, scratch files) writes
  under `target/` of this repository, nothing else. Check with `git status --short` and `ls /tmp` at the end
  of each round and report anything stray.
- When stuck on a decision only the human can make: question + recommended answer in
  `docs/questions.md`, take the safest reversible option, keep going.

## Hard limits (never)

- No `sudo`, `pkexec`, `su`, nothing needing root, no password prompts.
- **No work on `crates/helper`, `crates/helper-protocol`, `packaging/polkit`.** Anything that needs root
  (package cache, journal, system packages, system Flatpaks, snaps, system services) gets the real dry-run
  / listing and the screen, but its action button shows the "needs admin access (coming soon)" state.
- Never remove, uninstall, stop, disable, kill or trash anything that is real on this machine: not with a
  command you run, not from a test. Destructive code is tested only on temp dirs or on processes the test
  itself started. Read-only commands (`pacman -Qi`, `flatpak list`, `systemctl list-units`, `journalctl
  --disk-usage`, …) are fine for fixtures; save their output under `tests/fixtures/`.
- No `git push`, merge, rebase, tags, `reset --hard`, `clean`. No files changed outside this repository
  (cargo caches, `~/.cargo/bin` and the WBS sheet excepted).
- Don't edit `docs/design/`. No new colours, shadows or gradients (CLAUDE.md design rules).

## Honesty rules (the human reads the report first)

- **Never write "done", "works" or "passes" without the command and its exit code in the report.**
- Anything you didn't run, couldn't run, skipped, stubbed, or only looked at in a picture is listed under
  **Not verified** with the reason. Every error you hit, even ones you fixed, goes under **Errors**.
- A task is **Done** only when its acceptance in `queue.md` is met with evidence. Otherwise it is
  **Blocked** (with reason) or **Partial** (what exists, what's missing). Never stretch a definition.
- **Every decision you take without the human goes into `docs/questions.md` with a recommended answer,
  not only into the report.** A round isn't finished until that is done.
- Tests must clean up what they create. Never leave large files in `/tmp`; check after a full test run.
  You may not delete things in `/tmp` yourself: if it is full, report it and use `TMPDIR=$PWD/target/tmp`.
- If the plain `cargo test --all` doesn't exit 0, the task is not Done, whatever else passed. Say which
  command failed and why.
- Never weaken, skip, `#[ignore]` or delete a failing test to get green. A test that can't run here is
  reported as not run, not as passing.

## Start of the run (first round only)

1. `date '+%F %H:%M %Z'`; use that date everywhere. Run 08 lives on branch `run/08-2026-11-06`. If `main`
   already contains it (`git merge-base --is-ancestor run/08-2026-11-06 main` exits 0), create
   `run/09-<YYYY-MM-DD>` from `main`; if not, create it from the tip of `run/08-2026-11-06` and say so in
   the report.
2. **Stop time:** the first 08:00 local time after the run started (unless the `/loop` prompt names
   another). Write it as a full date and time in the report.
3. Preflight, each with its result in `docs/loop/report.md` under "Preflight": `cargo --version`,
   `cargo +nightly --version`, `cargo fuzz --version`, `command -v appimagetool`, `df -h /tmp`,
   `ls -d /tmp/appimage_extracted_* | wc -l`, and a **read of the WBS sheet** (`Overview!B1:B2`). If the
   sheet can't be read, say so and write every change you would have made to `docs/loop/wbs-pending.csv`
   (WBS ID, column, old, new).
4. Baseline: plain `cargo test --all` and its exit code (and the `TMPDIR` form if plain fails).

## Each round

1. Read `docs/loop/queue.md`; take the first task marked `[ ]`. Mark it `[~]` (in progress) and set its
   WBS row(s) to **In Progress** with today's start date if empty.
2. Plan (3–5 lines in `docs/progress.md`), then red → green → refactor with **separate commits**
   (`test(scope): R9-<id> … (red)`, then `feat|fix|refactor(scope): R9-<id> …`). Run the failing test and
   paste the failure line into the report before writing code.
3. Task checks: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all`.
   A screen task also runs its design check (≤ 10 % per reference image) and you **look** at the
   screenshot; write the picture paths in the report.
   Dialog artboards (CleanFlow, UninstallFlow, EndTask) have a placeholder backdrop, so they are checked
   on a crop of the dialog (see `docs/design/DESIGN-CHECK.md`). Don't tune copy or layout just to reach
   the number: report the score and the real cause of any remaining difference.
4. Update `docs/loop/queue.md` (`[x]` done, `[!]` blocked, `[/]` partial), append the round to
   `docs/loop/report.md` (template below), update the WBS sheet (rules below), commit
   (`docs(loop): R9-<id> report`).
5. **Time:** at the start of every round run `date '+%F %H:%M'`. At or after the stop time don't start a
   new task: go to "End of the run".
6. **Stop conditions** — stop the loop (no further round) and go to "End of the run" if: the queue has no
   `[ ]` left; or **3 tasks in a row ended `[!]`** (blocked or broken: a bug, a failing check, an error you
   couldn't fix). A task that ends `[/]` because the rest needs the admin helper, a person or a real other
   distro does **not** count toward the 3, as long as everything that can be done here is done and
   reported. Also stop if the build or test suite is broken and two attempts didn't fix it (don't commit
   broken code: `git stash push -m "R9-<id> broken"` so HEAD stays the last green commit, and report the
   stash name). Otherwise schedule the next round in about 1 minute.

### Report entry template (`docs/loop/report.md`)

```
## R9-<id> <title> — Done | Partial | Blocked   (<date time>)
WBS: <ids> → <new status>
Commits: <red sha> (red), <green sha>
Evidence: <command> → exit <n>; <test names>; design check <score>; pictures <paths>
Not verified: <what and why, or "nothing">
Errors: <every error hit, fixed or not, or "none">
Decisions taken without the human: <→ docs/questions.md entry, or "none">
```

## WBS sheet rules

Sheet: https://docs.google.com/spreadsheets/d/1gRI9jVQAm3w0y9UL2aFld0kT-kqiUSD9oMUV5yTX3Gc
(spreadsheet ID `1gRI9jVQAm3w0y9UL2aFld0kT-kqiUSD9oMUV5yTX3Gc`, tab **WBS**, columns A–O).

- Find rows by **WBS ID in column A** (read A:A first); never assume a row number.
- Status (G) only from the dropdown: `To Do`, `In Progress`, `Done`, `Needs Verification`. `Done` needs
  evidence in L (commits + tests/command). Partial → keep `In Progress`, say what's missing in N.
  Blocked → keep its status, write the blocker in N. Something built but not checkable here (e.g. needs
  a person or another distro) → `Needs Verification`.
- Dates (H start, I end) as `1 Nov 2026`, only when they happened. Never invent an owner (J).
- L = evidence (commit SHAs, test names, scores), M = source files/docs, N = notes/blockers. Replace
  outdated text in L/M/N instead of piling on.
- New work that has no row: insert a row directly below the last row of its workstream, WBS ID = next
  free number under the parent, Level `Task`, Workstream text copied exactly from the parent.
- **Never touch column O (Roll-up), the Overview tab's formulas, or the Sources & Verification tab.** At
  the end of the run update only `Overview!B2` to
  `Evidence snapshot: branch run/09-<date> @ <sha> (<date time>), not merged yet.`
- Never mark a human-owned row (J says Human) as Done.

## End of the run

1. Final checks on the final commit, all must exit 0 and be shown: `cargo fmt --check`,
   `cargo clippy --all-targets -- -D warnings`, `cargo test --all`, `cargo audit`, `cargo deny check`.
2. Build last: `cargo build --release -p tidewise-app`; if `appimagetool` is on PATH also
   `./packaging/appimage/build.sh` and record the AppImage size.
3. `docs/loop/report.md` ends with **Summary**: tasks Done / Partial / Blocked / not started (by ID),
   every Not verified item in one list, every error in one list, new questions in `docs/questions.md`,
   and what the human should click through by hand. `docs/progress.md` gets the usual entry.
4. Update `Overview!B2` (or the pending CSV), commit, then end your last message with the summary and
   the **Run it** block from CLAUDE.md (plus the AppImage path if built).
