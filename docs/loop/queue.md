# Run 07 queue

Take the first `[ ]` task each round. Marks: `[ ]` open, `[~]` in progress, `[x]` done,
`[/]` partial **by design** (the rest needs the admin helper, a person, or a real other distro: say which),
`[!]` blocked or broken (a bug, a failing check, an error you could not fix). Each task names its WBS
row(s), the design it must match (`docs/design/screens/` + `docs/design/png/`) and what "done" means.
Read the matching milestone prompt in `docs/prompts/` for detail; this queue wins where they differ.
Run 06's queue and report are in `docs/loop/archive/`.

## A. Make the checks honest again (first)

- [x] **Q01 Plain `cargo test --all` must exit 0, on any file system.** Run 06 could not get it green:
  (a) `/tmp` filled up with AppImage unpackings (the tests are fixed to clean up; verify with
  `ls -d /tmp/appimage_extracted_* | wc -l` before and after a full run, and say so; the human cleared
  `/tmp` on 2 Nov 06:05 and plain `cargo test --all` then exited 0 with 512 passed, 3 ignored); (b)
  `core::scan::tests::cancelling_stops_the_scan_and_says_so` expects 0 bytes for an empty folder, which
  is only true on tmpfs. Fix (b) red-first so the expectation comes from the file system itself (the
  folder's own blocks), **without weakening it**. Then run the plain command and also with
  `TMPDIR=$PWD/target/tmp`; both must exit 0 and both exit codes go in the report. If `/tmp` is still
  full when you start, say so, don't try to delete in `/tmp`, and use the `TMPDIR` form for every run.
  Add a WBS row 5.13 under workstream 5 for this.
- [x] **Q02 Put every decision where the human will see it.** Run 06 left decisions only in report
  entries (Q07 junk threshold and 5-minute rescan, Q08 `PackageSource` removed and the plain-name rule,
  Q10 Blocked::Protected/Busy, Q13 one CommandRunner, Q14 "kept" row, Q15 group-signal rule). Add each to
  `docs/questions.md` under "Run 06 — decisions taken before they were listed here" with a recommended
  answer. Docs only.

- [/] **Q18 Clean up must tell the truth when something fails** (WBS 6.6, 6.5). Evidence: the human's
  real runs on 2 Nov, History records `1793580727-clean_up.json`, `1793581322-…`, `1793581699-…`,
  `1793581707-…` in `~/.local/state/tidewise/history/`. In **all four** a folder named `dotslash` could
  not be removed ("Permission denied (os error 13)"), yet each step has `"status": "done"`, the title says
  "Cleaned 19.5 GB", the note shows only the folder's bare name, the record doesn't list what failed, and
  `freed_bytes` is the sum of scan-time sizes. Red-first, tests on temp dirs only:
  (1) any failure inside a step makes it `partial`, not `done`; the end dialog and the record title say
  what is left ("Cleaned 19.5 GB · 1 folder left");
  (2) record `version` 2: per item `{path, bytes, result, reason}`; v1 records still read;
  (3) the reason is plain and names the full path ("read-only folder: ~/.cache/dotslash");
  (4) freed bytes = what actually disappeared (re-measure a failed item after the attempt);
  (5) read-only folders inside a cache the user owns (some tools create 0555 folders): make them
  writable before removing, only for folders owned by the user, inside `~/.cache`, never following
  symlinks, refused otherwise. Put this decision in `docs/questions.md` with the alternative (leave them
  and say why). Rows: 6.6 stays `Needs Verification` until a person has seen the new end dialog.
- [/] **Q19 Split quick-to-rebuild caches from caches that take a long download** (WBS 6.5, 6.6).
  Evidence: the same first record removed 17.9 GB of "App caches", all ticked by default, including
  `~/.cache/huggingface` (ML models), `ms-playwright*` (browsers), `uv`, `pip`, `paru`, `yay`,
  `Docker Desktop`, `codex-runtimes`, `chrome-devtools-mcp`, `eas-cli`, `maestro`, `spotify`. They are
  caches, but several cost minutes and gigabytes of bandwidth to rebuild, and the screen gave no hint.
  Red-first: a table-driven classifier (name → `Rebuilds itself` | `Takes a while to download again`);
  thumbnails, shader and font caches, browser caches, KDE/GNOME caches, appstream, crash metadata are the
  first kind; ML-model, browser-binary, language-package, build-tool, container, game-launcher and
  streaming caches are the second; **any unknown cache over 2 GB is the second kind** (cautious default).
  The second kind is its own group in Clean up: **not ticked by default**, each item listed with its size
  and "will be downloaded again". This adds a group to the design (7 instead of 6): record it in
  `docs/questions.md`, keep demo mode as the artboard shows it, design check Cleaner ≤ 10 %.
  Keep skipping caches of running apps.

## B. Finish the partials that don't need the helper

- [/] **Q03 dpkg / rpm owner lookups** (WBS 7.1.8, 7.1.3, 7.1.4). `dpkg -S` and `rpm -qf` runners through
  `CommandRunner`, so apt and dnf packages appear in the Apps list as apps (visible launchers only).
  Hand-written fixtures are fine where the tool is missing; say so in the fixture README and the WBS note.
  Rows stay `Needs Verification` unless a real capture exists.
- [/] **Q04 "Also delete my settings" in the uninstall panel** (WBS 7.4). Offered only where Tidewise
  knows the folder: a user Flatpak's `~/.var/app/<id>`. Never guessed for AppImages or packages. Goes to
  the Trash through `core::trash` (golden rule 4). Tests on temp dirs with `HOME` pointed there.
- [/] **Q05 Orphans group in Clean up** (WBS 6.6, 6.5). pacman orphans via `pacman -Qdtq` (read-only,
  real capture for the fixture); apt `apt-get -s autoremove` and dnf `dnf autoremove --assumeno` parsers
  from hand-written fixtures. Shown as the design's 6th group, "needs admin access (coming soon)".
- [/] **Q06 Processes: the stub Force quit button and the End dialog layout** (WBS 7.5, 3.4). Wire the
  Processes details card's Force quit to the End task flow (or remove it if the design allows; say which).
  For the End task confirm dialog (24.3 % in run 06): fix layout differences that are not caused by the
  missing app-specific sentence (wrapping, button position). **Do not add copy that isn't true for every
  app, and do not tune text to reach the number.** Report the final score and the exact remaining cause.

## C. Remaining screens (user side; admin parts show "needs admin access (coming soon)")

- [/] **Q07 History records + History screen** (WBS 8.4, design `History`). Records already exist for
  Clean up; make Storage Trash/Undo, uninstall and End task write them too. List by day, what changed,
  Undo where it is possible without admin (Trash restore), "Copy for a help forum". Threat model 'R' row
  updated. Records in `$XDG_STATE_HOME/tidewise/history/`.
- [/] **Q08 Startup apps** (WBS 8.1, design `Startup`). XDG autostart; turning off writes a user
  `Hidden=true` copy with a backup; Undo toast. `.desktop` parser fixtures; extend the desktop fuzz
  target. Impact numbers only if they can be measured honestly; otherwise leave them out and say so.
- [/] **Q09 Services list, read-only** (WBS 8.2, design `Services`). Machine-readable `systemctl` output
  (fixtures from read-only commands such as `systemctl list-units --output=json`), failed units, curated
  "worth a look" list, protected units refused in code. Start/stop/enable show needs-admin.
- [ ] **Q10 Settings screen** (WBS 9.5, 9.4, design `Settings`). Stored in `$XDG_CONFIG_HOME/tidewise/`.
  Dark/light switch; "Follow system" only if it needs no new dependency (else `docs/questions.md`).
  Update-check toggle is stored but there is no network code (WBS 9.6 has an open conflict).
- [ ] **Q11 Health page** (WBS 9.2, design `Health`). Each rule, its points, the check, and a fix button
  that goes to the right screen.
- [ ] **Q12 Search, Ctrl K** (WBS 9.1, design `Search`). Index over apps, processes, actions, settings;
  home files only if it stays inside the memory budget (measure). Ranking unit tests.
- [ ] **Q13 Empty / loading / error states pass** (WBS 9.7, design `States`) for every built screen.

## D. Cheap docs

- [ ] **Q14 CHANGELOG.md and README screenshots** (WBS 9.11). Keep a Changelog style, an `Unreleased`
  section listing what exists today (and nothing that doesn't). README screenshots from `--screenshot`
  of the built screens, stored under `docs/screenshots/`.
- [ ] **Q15 Pen-test checklist** (WBS 5.10). `docs/security/pentest-checklist.md`: what a human tester
  does on a VM against the AppImage, the helper (when it exists) and the polkit policy. The human runs
  it; mark the WBS row `In Progress` (document written, test not run), never `Done`.

## E. Last (time-boxed)

- [ ] **Q16 Budgets on every page built since run 05** (WBS 3.8 note): `tools/measure.sh` on Clean up,
  Apps (with the list loaded), Home with the junk rescan, History, Startup, Services, Settings (≤ 40 MB
  PSS, < 2 % idle CPU). Over budget → fix it, or report it as a bug with the numbers.
- [ ] **Q17 Fuzz every target 10 minutes** (WBS 5.5). Targets in `fuzz/`; `-max_total_time=600` each.
  Results in `docs/security/fuzzing.md`; a crash becomes a fixture + regression test + fix. Don't start a
  target that cannot finish before the stop time.
