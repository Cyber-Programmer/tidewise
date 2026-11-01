# Run 06 queue

Take the first `[ ]` task each round. Marks: `[ ]` open, `[~]` in progress, `[x]` done, `[/]` partial,
`[!]` blocked. Each task names its WBS rows, the design it must match (`docs/design/screens/` +
`docs/design/png/`) and what "done" means. Read the matching milestone prompt in `docs/prompts/` for
detail; this queue wins where they differ (no root, no helper).

## A. Leftovers (small, do first)

- [x] **Q01 Slint `viewport-*` → `content-*`** (WBS 4.23). Refactor only, no behaviour change: no red
  commit needed, but every test and the design check must stay where they were. Done: no deprecation
  warnings in `cargo build`, design scores unchanged (±0.5 %).
- [x] **Q02 AppImage size and start time** (WBS 2.8, 4.14). Build with `./packaging/appimage/build.sh`
  if `appimagetool` is on PATH (else Blocked for this part). Measure size and time to first window
  (`--screenshot` start-to-file is fine; say which). Record both in `docs/decisions/0001-ui-toolkit.md`
  follow-ups and propose an AppImage size budget in `docs/questions.md` (don't set it yourself). WBS 4.14:
  record how the existing `dist/` AppImage relates (rebuilt → Done with size/hash).
- [x] **Q03 core has no network dependency** (WBS 5.9). Test that fails if `tidewise-core` or
  `tidewise-adapters` gains a networking crate in its dependency tree (`cargo metadata`, a deny list of
  crate names such as reqwest, hyper, ureq, curl, isahc, attohttpc, rustls, native-tls). Done: red
  (prove the check fails on a small fixture workspace that has such a dependency, not by adding one to
  the real crates), green, threat model row updated.
- [ ] **Q04 Tester notes** (WBS 4.20). `docs/test-build-notes.md`: what testers should try, known gaps,
  how to report (GitHub issues, what to attach, `--version`). Docs only.

## B. M3 user side — Clean up without root

- [ ] **Q05 Junk scanner** (WBS 6.5). As the user, read-only: app caches under `~/.cache` (skip caches of
  running apps; list what's skipped and why), Trash size, package cache size (pacman/apt/dnf dirs, read
  only), journal size (`journalctl --disk-usage`), unused Flatpak runtimes **computed from
  `flatpak list` output** (never run `flatpak uninstall`). Output = the exact confirm list (dry-run).
  Fixture tests per source + a temp-dir tree; fuzz target if it parses outside text.
- [ ] **Q06 Clean up screen + flow C, user-owned items only** (WBS 6.6, design `Cleaner`,
  `CleanFlow`). Scan → review → confirm → progress with "stop after this step" → done report →
  partial/skipped. User caches and Trash really cleaned (golden rule 4: caches removed directly only
  after the confirm screen; tests on temp dirs only). Root items show the needs-admin state. Writes a
  History record (Q16 format; if Q16 isn't done yet, define the record type here). Design check ≤ 10 %.
  WBS 6.6 → In Progress or Needs Verification, never Done (root part missing).
- [ ] **Q07 Home junk score real** (WBS 4.11 note). Feed Q05's totals into the health score; remove the
  stub label. Unit tests at the rule boundaries.

## C. M4 user side — Apps

- [ ] **Q08 `AppSource` trait** (WBS 7.1.1): list, size, removal plan (dry-run), with the existing
  pacman/flatpak/appimage code moved onto it.
- [ ] **Q09 pacman adapter finished** (WBS 7.1.2): removal plan from **saved** `pacman -Rns --print`
  output (capture fixtures only if the command runs without root and without changing anything; you
  may not run `pacman -R*` yourself, so if no fixture can be made, write the parser against
  documented output and mark the fixture hand-written).
- [ ] **Q10 apt / dpkg adapter** (WBS 7.1.3). Parsers + fixtures (hand-written where this Arch machine
  can't produce real output; say so in the fixture README and the WBS note), non-English and broken
  cases, fuzz target.
- [ ] **Q11 dnf / rpm adapter** (WBS 7.1.4). Same rules as Q10.
- [ ] **Q12 snap adapter** (WBS 7.1.6). Same rules as Q10; disabled revisions = old copies.
- [ ] **Q13 Flatpak runner + AppImage menu entries; packages → apps via .desktop** (WBS 7.1.5, 7.1.7,
  7.1.8).
- [ ] **Q14 Apps screen, view + uninstall panel up to confirm** (WBS 7.4, design `Apps`,
  `UninstallFlow`). One list across sources, search, filters, sizes, blocked/busy states (WBS 7.2 logic
  with fixtures). Real removal only for AppImages (to Trash, with its .desktop and icon) and **user**
  Flatpaks (`flatpak uninstall --user` behind the confirm screen; tested with a fake runner, never run for
  real). Packages, system Flatpaks, snaps: needs-admin state. Design check ≤ 10 %.
- [ ] **Q15 End task** (WBS 7.5, design `EndTask`). SIGTERM to the process group via `/usr/bin/kill` by
  absolute path (no shell, no `unsafe`), wait 5 s, offer Force quit; protected processes refused in code;
  memory-freed toast. Tests only against processes the test started.

## D. M5 / M6 user side — remaining screens

- [ ] **Q16 History records + History screen** (WBS 8.4, design `History`). JSON per action in
  `$XDG_STATE_HOME/tidewise/history/`; Storage Trash/Undo and Q06 write records; list by day, Undo where
  possible, "Copy for a help forum". Threat model 'R' row updated.
- [ ] **Q17 Startup apps** (WBS 8.1, design `Startup`). XDG autostart; turning off writes a user
  `Hidden=true` copy with a backup; Undo toast. `.desktop` parser fixtures + fuzz target extended.
  Impact numbers only if they can be measured honestly; otherwise leave them out and say so.
- [ ] **Q18 Services list, read-only** (WBS 8.2, design `Services`). Machine-readable `systemctl`
  output (fixtures from read-only commands), failed units, curated "worth a look" list, protected units
  refused in code. Start/stop/enable buttons show needs-admin (WBS 8.3 is helper work: not this run).
- [ ] **Q19 Settings screen** (WBS 9.5, design `Settings`). Settings stored in
  `$XDG_CONFIG_HOME/tidewise/`. Theme dark/light switch (WBS 9.4) without "Follow system" if that needs
  a new dependency (→ questions). Update-check toggle stored but no network code (WBS 9.6 has an open
  conflict).
- [ ] **Q20 Health page** (WBS 9.2, design `Health`). Each rule, its points, the check and a fix
  button that navigates to the right screen.
- [ ] **Q21 Search, Ctrl K** (WBS 9.1, design `Search`). Index over apps, processes, actions, settings;
  home files only if it stays within the memory budget (measure). Ranking unit tests.
- [ ] **Q22 Empty / loading / error states pass** (WBS 9.7, design `States`) for every built screen.

## E. Last (long-running, only if time is left)

- [ ] **Q23 Fuzz 10 minutes per target** (WBS 5.5), every target in `fuzz/`, results in
  `docs/security/fuzzing.md`; crashes → fixture + regression test + fix.
- [ ] **Q24 Budgets on every new screen**: `tools/measure.sh` on each page built this run (≤ 40 MB PSS,
  < 2 % idle CPU); fix anything over.
