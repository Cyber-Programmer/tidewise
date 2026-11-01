# Run 06 report

Written by the loop, one entry per round (template in `docs/loop/LOOP.md`). Read this first in the
morning: **Summary** at the end, then **Not verified** and **Errors** in each entry.

## Preflight

Run started 2026-11-01 22:46 PKT on branch `run/06-2026-11-01` (from `main` @ d73cf3e; `git checkout -b` → exit 0).

| Check | Result |
| --- | --- |
| `cargo --version` | cargo 1.98.0 (797e8a9bc 2026-08-05), exit 0 |
| `cargo +nightly --version` | cargo 1.101.0-nightly (cad07cf8e 2026-10-23), exit 0 |
| `cargo fuzz --version` | cargo-fuzz 0.13.2, exit 0 |
| `command -v appimagetool` | /home/bahauddin/.local/bin/appimagetool, exit 0 |
| WBS sheet read (`Overview!B1:B2`) | readable; B1 "Tidewise — Work Breakdown Structure" |
| Baseline `cargo test --all` (starting commit d73cf3e) | exit 0; 0 failed, 2 ignored (existing `#[ignore]`s), largest suites 155 / 95 / 20 / 17 passed |

## R6-Q01 Slint viewport-* → content-* — Done   (2026-11-01, exact time not recorded; before 23:36 PKT)
WBS: 4.23 → Done (row 50; G, H, I, L, M, N written, `update_values` status success)
Commits: none (red; refactor only, no red commit by the queue's own rule), 20afd5b
Evidence:
- `grep viewport- crates` found 18 uses, all in `crates/app/ui/storage.slint`; replaced with `content-*`.
- `cargo build -p tidewise-app` after `touch` of the file, count of lines containing "deprecat": before 20, after 0 (build exit 0).
- `cargo fmt --check` → exit 0; `cargo clippy --all-targets -- -D warnings` → exit 0; `cargo test --all` → exit 0 (307 passed, 0 failed, 2 ignored; the same 307 passed as the baseline).
- Design check, same commands before (stashed) and after: `Storage` 3.0 % → 3.0 %; `Storage--downloads` 9.0 % → 9.0 %. Pictures: `target/design-check/{old,new}-Storage.png`, `{old,new}-Storage--downloads.png`.
Not verified: I did not look at the screenshots or scroll the Storage page by hand (identical scores only show the static pictures agree); scroll thumb and keyboard scrolling behaviour at runtime were not exercised beyond the existing tests.
Errors: none
Decisions taken without the human: none

## R6-Q02 AppImage size and start time — Done   (2026-11-01, exact time not recorded; before 23:36 PKT)
WBS: 2.8 → In Progress (size budget not set by design; `G15:I15`, `L15:N15` written, status success); 4.14 → Done (`G41:I41`, `L41:N41` written, status success)
Commits: none for code (docs only); docs commit `docs(loop): R6-Q02 report` (see `git log`)
Evidence:
- `./packaging/appimage/build.sh` → exit 0, 260 s wall; `dist/Tidewise-0.1.0-x86_64.AppImage` = 10,467,832 bytes, sha256 `5f9117af73026d0a6711261910e3ad13196e36ce736d7d13b7fad7e2d7911be4`. The previous 28 Oct file (10,471,928 bytes, md5 `f32e03dc2f365ce10989b498a2a010d9`) was overwritten.
- Start time = `--screenshot home --out <file>` from process start to exit (headless render, not a visible window): AppImage 0.187 / 0.170 / 0.171 / 0.171 / 0.173 s (5 runs, all exit 0); `target/release/tidewise` 0.083 / 0.072 / 0.075 s (3 runs, all exit 0).
- Written to `docs/decisions/0001-ui-toolkit.md` (Follow-up measurements) and a budget proposal (≤ 25 MB, not set) in `docs/questions.md`.
- `cargo fmt --check` → 0; `cargo clippy --all-targets -- -D warnings` → 0; `cargo test --all` → 0 (307 passed, 0 failed, 2 ignored).
Not verified: time to a real window on screen (no compositor measurement); the AppImage was not run on Ubuntu 22.04 or any other distro and bundles no shared libraries; how the 28 Oct AppImage was built is unknowable now; picture h1.png was not looked at (`target/q02/h*.png`, 220,377 bytes each).
Errors: (1) first measuring command failed: `ls --time-style` rejected (ls is aliased to `lsd`) and `/usr/bin/time` does not exist, so the build did not run on that attempt; re-run succeeded. (2) first timing loop printed 0.000 s because `zsh/datetime` was not loaded; re-ran with it loaded, the numbers above are from the second run. (3) I wrote one temp file (`old.md5`) to `/tmp/claude-1000/` instead of the scratchpad; outside the repo, harmless.
Decisions taken without the human: none (the size budget is only proposed → docs/questions.md "Run 06 — Q02").

## R6-Q03 core has no network dependency — Done   (2026-11-01, exact time not recorded; before 23:36 PKT)
WBS: 5.9 → Done (row 60; written below, status success)
Commits: 06102ae (red), a73a8c7
Evidence:
- Red: `cargo test -p tidewise-core --test no_network_deps` → exit 101; failures `fixture_with_a_transitive_networking_crate_is_caught` (left `[]`, right `["reqwest"]`) and `a_package_that_does_not_exist_is_an_error_not_a_pass`. The other 3 tests passed vacuously against the stub.
- Green: same command → exit 0, 5 passed (includes `tidewise_core_has_no_networking_dependency`, `tidewise_adapters_has_no_networking_dependency`). Real tree is not empty (`cargo tree -p tidewise-core` has 12 distinct lines; the test also asserts the first line is the package).
- `cargo fmt --check` → 0; `cargo clippy --all-targets -- -D warnings` → 0; `cargo test --all` → 0 (312 passed, 0 failed, 2 ignored).
- Threat model row (`docs/security/threat-model.md:97`) updated, with limits stated.
Not verified: that the check catches a networking crate added to the real crates (queue said to prove it on a fixture only, not by editing the real crates); names only, so `std::net` or raw sockets are not caught; dev-dependencies are deliberately not checked; `cargo metadata` was not used (queue named it) — `cargo tree` instead, same dependency data, no new dependency.
Errors: none
Decisions taken without the human: none

## R6-Q04 Tester notes — Done   (2026-11-01, exact time not recorded; before 23:36 PKT)
WBS: 4.20 → Done (row 47; sheet write status success)
Commits: none for code (docs only); docs commit `docs(loop): R6-Q04 …` follows this entry in `git log`
Evidence: `docs/test-build-notes.md` written (what to try, known gaps, how to report, what to attach). Facts checked against `crates/app/ui/main.slint` (pages: only Home, Performance, Processes, Storage are built; others show "Coming in a later version"), `processes.slint:315` (End task does nothing), `cli.rs` (no `--version`/`--help`; `./target/release/tidewise --help` → `Error: "unknown option --help"`), `SECURITY.md`, `Cargo.toml` repository URL. `cargo fmt --check` → 0; `cargo clippy --all-targets -- -D warnings` → 0; `cargo test --all` → 0 (312 passed, 0 failed, 2 ignored).
Not verified: I did not run the app to try the listed steps myself (keyboard keys list is copied from the run 5 progress entry, Trash/Undo behaviour from `storage_live.rs`); the issues URL is the `repository` value in `Cargo.toml` and was not opened; I did not check that GitHub private vulnerability reporting is enabled.
Errors: none
Decisions taken without the human: none

## R6-Q05 Junk scanner — Done   (2026-11-01, exact time not recorded; before 23:36 PKT)
WBS: 6.5 → Done (row 69; sheet write status success)
Commits: a492bd8 (red), 0595354
Evidence:
- Red: `cargo test -p tidewise-adapters` → exit 101, 18 failed against stubs (e.g. `junk::tests::caches_are_listed_and_caches_of_running_apps_are_skipped_with_a_reason`, `journal::tests::real_output_is_in_binary_units`, `flatpak::tests::real_machine_has_nothing_unused`).
- Green: `cargo test -p tidewise-adapters` → exit 0, 42 passed (+ the real-command test). Sources covered by fixture tests: app caches (running → skipped with reason, symlink → skipped, 15-char process name, short-name false positive), Trash, package cache (needs admin, missing dir absent), journal (real, German/comma, `0B`, broken, empty, absurd size), Flatpak runtimes (real, hand-written, broken, cross-installation use), totals, "scanning changes nothing".
- `cargo fmt --check` → 0; `cargo clippy --all-targets -- -D warnings` → 0 (after one fix, see Errors); `cargo test --all` → 0 (334 passed, 0 failed, 3 ignored — the 3rd is the new opt-in `junk_live`).
- Real machine, read-only (`cargo test -p tidewise-adapters --test junk_live -- --ignored --nocapture` → exit 0): journal 3,006,477,107 B (matches `journalctl --disk-usage` 2.8G), pacman cache 1,865,043,968 B (`du -sh` said 1.8G), Trash 1,576,296,448 B, 8 running-app caches skipped (claude, kitty, plasmashell …), user 20,304,068,608 B / admin 4,871,521,075 B.
- Fuzz: `cargo +nightly fuzz run journal_usage -- -max_total_time=20` → exit 0, 696,769 runs; `flatpak_list` (now also the runtime parsers) 20 s → exit 0, 301,318 runs. Logged in `docs/security/fuzzing.md`.
Not verified:
- The unused Flatpak runtime list is computed from `flatpak list` only and is NOT compared with `flatpak uninstall --unused` (not allowed this run); on this machine it found none. The rule is cautious (only `.Platform`/`.Sdk`, never extensions or `org.freedesktop.*`), so it can miss unused runtimes but should not offer used ones — untested against a machine that has unused runtimes.
- Hand-written fixtures: unused runtimes, German/zero/broken journal text (README in `tests/fixtures/junk/`). apt and dnf cache folders are only checked as "missing → absent"; not run on those distros.
- The running-app match is a name heuristic (equal / ≥5-char prefix / `mozilla`→firefox); it can wrongly skip or (for apps with unrelated process and cache names) wrongly include a cache. Sizes of `~/.cache` items were not compared with `du`.
- Fuzzing was 20 s per target, not the 10 minutes ENGINEERING.md asks for (Q23).
- Nothing yet uses the report: no screen, no Trash/cache removal (Q06/Q07).
Errors: (1) first `cargo clippy` after the green code → exit 101, `unnecessary_sort_by` in `junk.rs`; fixed with `sort_by_key(Reverse)`, rerun exit 0. (2) My first edit of `flatpak.rs` deleted `parse_list` (slicing slip); caught by the compiler (E0425), restored from `HEAD`, tests then green. (3) One test fixture (`flatpak-app-runtimes-handwritten.txt`) was wrong for the test I wrote (Tiny used the freedesktop base); I corrected the fixture in the green commit rather than the assertion, noted in the commit message. (4) Wrote one temp file to `/tmp/claude-1000/…/scratchpad` only (fine).
Decisions taken without the human: `adapters` now depends on `tidewise-core` (a path dependency inside the workspace, not an external crate; `core` still has no networking crate — Q03 test green). Not in `docs/questions.md` as a blocker; recommended to keep.

## R6-Q06 Clean up screen + flow C, user-owned items — Partial   (2026-11-01 23:36 PKT)
WBS: 6.6 → In Progress (row 70; G, H, L, M, N written; never Done, root part missing). Also 8.4 note only in questions.md (record type defined here).
Commits (red → green): 55b30b4 → 1282a84 (core::history), 618f099 → c6845ea (adapters::cleanup), e2a5f34 → 5e359aa (view-model), 6d35743 → 5b07f43 (end-of-run texts + Slint screen/dialog + controller + demo states + screenshot tests), plus fix 37e9ea6 (cargo deny).
Evidence:
- Red runs: `cargo test -p tidewise-core history` → 7 failed; `-p tidewise-adapters cleanup` → 13 failed; `-p tidewise-app cleaner` → 8 failed, then 6 failed. Green: history 9 passed; adapters 56 passed; app `cleaner` 14 passed.
- Final on 5b07f43: `cargo fmt --check` → 0; `cargo clippy --all-targets -- -D warnings` → 0; `cargo test --all` → 0 (373 passed, 0 failed, 3 ignored); `cargo deny check` → 0; `cargo audit` → 0 (5 allowed warnings, the ones in deny.toml).
- What the tests prove (temp dirs only, nothing on this machine removed): only confirmed cache folders are removed (`app_caches_are_removed_and_nothing_else`); a cache swapped for a symlink after the scan is refused and its target survives; a cache whose app started is left alone; paths outside the cache folder, `..`, the cache root, `/` and empty are refused; the Trash is emptied but its folders stay and a non-Trash folder is refused; admin and Flatpak steps change nothing; "stop after this step" skips the rest; the History record lists removed paths and skipped steps; ids that could leave the folder are refused; records are written atomically and never overwritten.
- Design check: `Cleaner` 8.8 % (≤ 10 %) — `python3 tools/design_diff.py docs/design/png/Cleaner.png target/design-check/Cleaner.png`. `CleanFlow` whole image **58.0 % (fails)**: the artboard's backdrop is grey placeholder blocks and a "Prototype step" bar, not the real screen; dialog-only crop (400,100)–(960,560) of both images: **5.2 %**. Pictures: `target/design-check/Cleaner.png`, `Cleaner-side.png`, `CleanFlow.png`, `CleanFlow-dialog-side.png`, `Clean-done.png`, `Clean-details.png`, `Clean-live.png`, `Clean-cleaning.png`, `Clean-skipped.png`, `Clean-empty.png`. I looked at `Cleaner`, `CleanFlow`, `Clean-done`, `Clean-details`, `Clean-live`; I did not open `Clean-cleaning`, `Clean-skipped`, `Clean-empty` (the screenshot test only proves they render at 1360×960 and differ from the plain screen).
- Live, read-only scan on this machine: `--screenshot clean-up --state live` → exit 0: 23.6 GB selected of 25.2 GB found, 4 groups, no removal.
Not verified:
- **Nobody clicked it.** The buttons (tick, Clean, Cancel, Clean N GB, Stop after this step, Back to Home, Done) are wired but I never pressed them in a real window; pressing Clean on this machine would delete real caches, which this run forbids. The worker thread, progress updates, the end-of-run dialog and History writing in the live app are therefore only covered by the engine tests and pictures of demo states.
- The History file was written only in tests on temp dirs; not checked against the real `~/.local/state/tidewise/history/`.
- Missing parts of the queue item: the password step, the admin groups (package downloads, system logs), Flatpak removal, and the design's 6th group (orphans) — see `docs/questions.md` "Run 06 — Q06".
- Memory/CPU budget of the Clean up page not measured (Q24). Scan time for a 25 GB cache is not measured either (the scan runs off the UI thread).
- Keyboard/Tab behaviour and 4.5:1 contrast of the new screen were not checked; the new shades come from the artboard.
Errors: (0) The times I wrote in the headings of R6-Q01–Q05 and a first one for Q06 were guesses, some later than the real clock (23:36 PKT); corrected to what is known. (1) `cargo deny check` exit 2 (wildcard path dependency from Q05), found while adding serde in this task, fixed in 37e9ea6. (2) A `sed` meant to tighten a test assertion did not match (rustfmt had reformatted the line), so one red-commit assertion was loose; fixed in the green commit and said in its message. (3) Clippy `items after a test module` in `demo.rs`/`view.rs` after I appended functions; moved above the tests, rerun exit 0. (4) First screenshots: header switch not right-aligned and dialog 22 px short (wrapped `Text` preferred height); fixed with widths, re-measured. (5) The Slint screen and the screenshot tests were written after/together with the code, not red first.
Decisions taken without the human: yes — 8 points in `docs/questions.md` ("Run 06 — Q06"): new theme shades, no orphans group, no password step, Flatpak parts not removed, freed bytes measured at scan time, History text not a link, demo not wired, Q05 slip.

## R6-Q07 Home junk score real — Done   (2026-11-01 23:42 PKT)
WBS: 4.11 → stays Done, evidence and note updated (row 38, sheet write status success below)
Commits: 398b62f (red), 928bea8 (red), c377df6
Evidence:
- Red: `cargo test -p tidewise-app --bin tidewise home::` → 9 failed / 8 passed; `live::` → 2 failed (`the_first_junk_scan_starts_at_once…`, `later_scans_wait_for_home_and_five_minutes`).
- Green: `home::` 17 passed; `live::` 14 passed. Rule boundaries: `health::junk_points` tests already existed (2.499 GB → 0, 2.5 GB → 1, 20 GB → 8, u64::MAX → 8); new tests at the Home layer: 0.99 GB no suggestion, 1.0 GB suggestion, 2.5 GB −1, 12.6 GB → 95, 500 GB → 92, stacked with the drive (−8 −8), junk first then drive, headline counts, `None` = nothing claimed.
- `cargo fmt --check` → 0; `cargo clippy --all-targets -- -D warnings` → 0; `cargo test --all` → 0 (382 passed, 0 failed, 3 ignored); `cargo deny check` → 0. (`cargo audit` not run this round.)
- Design: `--screenshot home` vs `docs/design/png/Main.png` → 1.3 % before (5b07f43) and 1.3 % after (demo data, unaffected by the change).
Not verified: the live Home screen was never looked at (the demo picture does not use `health_scan`); I did not open the real app. On this machine the junk probe found 25.2 GB, so the score should show −8 for junk, but that is computed by the unit tests, not seen. CPU/memory of the extra scan (every 5 min while Home is open, and once at start) is not measured (Q24). The score counts everything the scanner lists, including the Trash and admin-only items, as the artboard's 12.6 GB does; whether Trash should count is a product question. The note under the ring says "Junk … not part of this score yet" until the first scan ends.
Errors: first `cargo test … health_scan home::` was a bad command (cargo takes one filter); rerun correctly. A format string I wrote first read "Only services and updates"; the new test caught it before commit.
Decisions taken without the human: junk counts from all groups found (user + admin); suggestion threshold 1 GB; rescan every 5 min on Home. Not added to docs/questions.md yet — recommended to keep.

## R6-Q08 AppSource trait — Partial   (2026-11-01 23:49 PKT)
WBS: 7.1.1 → stays In Progress (row 76; `L76:N76` written below); 7.1 untouched (roll-up)
Commits: c0f9897 (red), 909bb7c
Evidence:
- Red: `cargo test -p tidewise-adapters` → exit 101, 16 failed / 60 passed (hostile-name table, empty listing, trait-object and default-method tests passed vacuously against stubs).
- Green: same command → 76 passed in the lib + the real-machine tests exit 0. New tests: merge sorted ignoring case; a failing source is named while others still list; trait objects work; `size` default; `remove` = NotYet; pacman entries from real fixtures (vlc, bash), plan with `pacman -Rns -- name`, blocked with the names that need it, non-`-Qi` text is an error not an empty list; Flatpak entries from real/hand-written/German fixtures, user vs system plan, broken fixture is an error; AppImage entries, plan = move to Trash, plans refused for any path that is not a currently listed AppImage (`../`, `/etc/passwd`, empty); hostile ids (`--force`, spaces, `../`, newline, NUL, `$(id)`, wildcards, look-alike unicode, 300 characters) refused.
- `cargo fmt --check` → 0; `cargo clippy --all-targets -- -D warnings` → 0; `cargo test --all` → 0 (403 passed, 0 failed, 3 ignored); `cargo deny check` → 0. (`cargo audit` not run this round.)
- Real machine, read-only: `real_machine_lists_apps_from_every_source_and_plans_without_changing_anything` → ok: no source errors, `bash` is Pacman and its plan is blocked.
Not verified:
- **Missing from the WBS wording:** `remove` is not implemented for any source (AppImage and user Flatpak removal belong to Q14, the rest to the helper), hence Partial.
- apt, dnf and Snap are not on the trait yet (Q10–Q12); pacman's plan is the package only, not the full `pacman -Rns --print` list (Q09); no protected-package list yet (golden rule 7: required before any removal exists; Q14 logic) — nothing removable exists now.
- No fuzz target for the new code (it parses nothing new: it reuses `parse_qi`/`parse_list`); the AppImage plan check reads the real folders.
- Flatpak `system()` against the real flatpak ran only through the real-machine test (no apps installed there: one app, system).
Errors: my first replacement script aborted halfway because `cargo fmt` had reformatted the stubs it searched for; the first half (source.rs) was applied, the rest redone by replacing whole sections. No failed commands left in the commits.
Decisions taken without the human: `PackageSource` (trait from M1) was deleted instead of extended, `Pacman::installed()` stays as a method; `is_plain_name` allows ASCII letters, digits and `. _ + - @ :`, no leading `-`, at most 255 characters — to be reviewed together with the helper's own name pattern later (the helper is not touched in this run). Not in docs/questions.md yet.

## R6-Q09 pacman adapter finished (removal plan from saved output) — Partial   (2026-11-01 23:56 PKT)
WBS: 7.1.2 → stays In Progress (row 77; `L77:N77` written below)
Commits: 67810ae (red), 57d66ad
Evidence:
- Red: `cargo test -p tidewise-adapters pacman::` → exit 101, 5 failed / 18 passed (the refuse-garbage, success/failure-swap, not-found/hostile-id and wrong-first-target tests passed vacuously against the stub).
- Green: same command → 23 passed. Cases: dashed names (`lib32-glibc`, `foo-bar-baz-1:2.0.1-10`) split at the last two dashes; blocked removal names each requirer once; not found; a German imitation and garbage are refused (`Unrecognized`) rather than guessed; success text read as failure and the reverse are refused; plan lists all dependencies with sizes only for the asked-for package; blocked plan; a plan for a different first target is refused; hostile id refused.
- `cargo fmt --check` → 0; `cargo clippy --all-targets -- -D warnings` → 0; `cargo test --all` → 0 (412 passed, 0 failed, 3 ignored); `cargo deny check` → 0; `cargo audit` → 0 (5 allowed warnings).
- Fuzz: `cargo +nightly fuzz run pacman_rns_print -- -max_total_time=20` → exit 0, 851,104 runs (logged in docs/security/fuzzing.md; 20 s, not the 10 minutes).
Not verified:
- **Every fixture is hand-written** (`tests/fixtures/pacman/Rns-print-*-handwritten.txt`, README says so): I may not run `pacman -R*`, so the exact stdout format of `--print` (assumed: `name-version-release` per line) and the exact refusal text (`:: removing X breaks dependency 'X' required by Y`) are from memory of pacman's documented behaviour, not captured. If the real format differs the parser answers `Unrecognized` (safe), not a wrong plan.
- No live runner: nothing runs `pacman -Rns --print` and `PacmanApps::removal_plan` still returns the cautious estimate; wiring needs a decision whether the app may run that command as the user (and a real capture first).
- Epoch and `~` in versions are accepted by pattern, not tested against real packages other than the handwritten ones.
Errors: none.
Decisions taken without the human: parser refuses non-C-locale text instead of trying to translate it; the dry run is only trusted when its first target is the app asked about. Not in docs/questions.md.

## R6-Q10 apt / dpkg adapter — Done   (2026-11-02 00:02 PKT)
WBS: 7.1.3 → Needs Verification (row 78; the code cannot be checked against real apt on this Arch machine; `G78:I78`, `L78:N78` written below)
Commits: 5e240e5 (red), 77fdce5
Evidence:
- Red: `cargo test -p tidewise-adapters apt::` → exit 101, 11 failed / 4 passed (garbage-refusal, empty, removed-with-settings and hostile-Remv tests passed vacuously against the stub).
- Green: same command → 15 passed. Cases: sizes KiB → bytes; `deinstall ok config-files` left out; `hold ok installed` kept; empty size → None; multi-arch `libc6:amd64`; bad size / too few / too many columns → `BadLine` with the line number; dry run: removes list in order, essential warning flagged, unknown package, not installed, lock held; German imitation, garbage, empty and headerless text refused; hostile name in a `Remv` line refused; plan with all packages, blocked as Protected or Busy, wrong first package refused, hostile ids refused.
- `cargo fmt --check` → 0; `cargo clippy --all-targets -- -D warnings` → 0; `cargo test --all` → 0 (427 passed, 0 failed, 3 ignored); `cargo deny check` → 0; `cargo audit` → 0 (5 allowed warnings).
- Fuzz: `cargo +nightly fuzz run apt_parsers -- -max_total_time=20` → exit 0, 509,694 runs (docs/security/fuzzing.md; 20 s only).
Not verified:
- **Every fixture is hand-written** (`tests/fixtures/apt/README.md`); there is no `dpkg-query` or `apt-get` here. The exact wording of the essential warning, the `Remv` lines and the lock error are from memory of apt's documented behaviour; a different real wording makes the parser say `Unrecognized` (safe) rather than guess, but that also means it may refuse real output. German/other-language handling is "refuse", by design: not tested against a real localized run.
- `AptApps::system()` was never run (dpkg-query does not exist here); it is only reached when `/usr/bin/dpkg-query` exists.
- Nothing runs `apt-get -s remove`; the trait's `removal_plan` is the cautious estimate (the package alone). No dependency listing (`Required By`) for apt packages.
- Mapping packages to apps via `.desktop` files is Q13; the list here is every installed package.
Errors: none.
Decisions taken without the human: added two variants to `source::Blocked` (`Protected`, `Busy`); dry-run text is only trusted when it carries the English `Reading package lists...` line (or the English lock error). Not in docs/questions.md.

## R6-Q11 dnf / rpm adapter — Done   (2026-11-02 00:14 PKT)
WBS: 7.1.4 → Needs Verification (row 79; fixtures hand-written; `G79:I79`, `L79:N79` written below)
Commits: 459a471 (red), fbe0306
Evidence:
- Red: `cargo test -p tidewise-adapters dnf::` → exit 101, 9 failed / 2 passed (empty rpm output and garbage-refusal passed vacuously against stubs).
- Green: same command → 11 passed: rpm sizes in bytes with two kernels added up and `gpg-pubkey` skipped; bad lines (1 or 3 columns, bad size, hostile name, overflow) → `BadLine` with the line number; dry-run table with a wrapped long name, protected packages, no match, lock; German imitation, garbage, empty and header-only text refused; plans (all packages, Protected, Busy, wrong first package, unknown name, hostile ids) and the estimate plan.
- Test change in the green commit: `parse_dry_run` takes only the text (dnf `--assumeno` exits non-zero even for a good plan, so a success flag would mislead); the red tests had passed a flag.
- Fuzz: `cargo +nightly fuzz run dnf_parsers -- -max_total_time=20` → exit 0, 541,726 runs (20 s only; docs/security/fuzzing.md).
- `cargo fmt --check` → 0; `cargo clippy --all-targets -- -D warnings` → 0. Tests: `TMPDIR=$PWD/target/tmp cargo test -p tidewise-app` → exit 0 (211 passed, 0 failed, 1 ignored) and `cargo test --workspace --exclude tidewise-app` → exit 0 (227 passed, 0 failed, 2 ignored) = 438 passed. `cargo deny check` → 0 and `cargo audit` → 0 (run earlier in the round, before the test-only change).
Not verified:
- **`cargo test --all` with the default `/tmp` did NOT pass** this round: see Errors. I ran it split (above) instead; the plain command will pass again once /tmp is cleaned.
- Every fixture is hand-written (`tests/fixtures/dnf/README.md`); no `rpm`/`dnf` here. dnf 4 table layout only; dnf 5 prints differently and is refused (or mis-handled) — untested. The protected-package and lock wording are from memory.
- `DnfApps::system()` never run; nothing runs `dnf remove --assumeno`; the trait plan is the package-only estimate.
- `cargo deny`/`audit` were not re-run after the test-file edits (no dependency change since).
Errors:
- **/tmp full:** 2 tests in `crates/app/tests/appimage.rs` failed with "Disk quota exceeded" (exit 101 on `cargo test --all`). Cause: my own earlier test runs left 14 × 407 MB `appimage_extracted_*` folders in /tmp (appimagetool and the AppImage's `--appimage-extract-and-run` unpack to $TMPDIR and keep the copy). I tried to delete them with one command; the permission check refused ("Shared Scratch Sweep"); nothing was deleted and I did not retry. I fixed the test so both unpack into the test's own temp folder (verified: leftover count 0 after a run with TMPDIR=target/tmp), and asked you to delete the 14 folders (docs/questions.md).
- Running all tests with TMPDIR on the repo disk made `core::scan::tests::cancelling_stops_the_scan_and_says_so` fail (it expects 0 bytes for an empty folder, true on tmpfs only). Passes on /tmp (6/6 repeated runs); not changed; noted in questions.md. Not flaky, file-system dependent.
- One python edit of the test patched the wrong AppImage run first; caught by the leftover check, fixed.
Decisions taken without the human: `parse_dry_run` ignores the exit status (see above); dnf5 explicitly out of scope. In docs/questions.md only the /tmp item.

## R6-Q12 Snap adapter — Done   (2026-11-02 00:21 PKT)
WBS: 7.1.6 → Needs Verification (row 81; fixtures hand-written; `G81:I81`, `L81:N81` written below)
Commits: afe2da7 (red), 543d68a
Evidence:
- Red: `cargo test -p tidewise-adapters snap::` → exit 101, 11 failed / 0 passed.
- Green: same command → 11 passed: every revision with notes (`-` = none, `x1` local install, `base,disabled`); disabled = old copies (core20 1974, firefox 3547); base snaps and snapd are "system"; nothing installed → empty list; German header, headerless text, short row, hostile name, uppercase, `/` and 7 columns refused with the line number; one entry per snap with the active revision; sizes from `<name>_<rev>.snap` in a temp folder; plan lists all revisions with old copies marked; `core20` and `snapd` protected; hostile ids and unknown snaps give no plan.
- Fuzz: `cargo +nightly fuzz run snap_list -- -max_total_time=20` → exit 0, 863,864 runs (20 s only).
- `cargo fmt --check` → 0; `cargo clippy --all-targets -- -D warnings` → 0; `cargo deny check` → 0; `cargo audit` → 0 (5 allowed warnings). Tests split because of the full /tmp: `TMPDIR=$PWD/target/tmp cargo test -p tidewise-app` → 0 (211 passed, 1 ignored); `cargo test --workspace --exclude tidewise-app` → 0 (238 passed, 2 ignored) = 449 passed, 0 failed.
- The leak fix works: `ls /tmp | grep -c appimage_extracted` stayed 14 across this round's test runs; `target/tmp` was empty after them.
Not verified:
- **Plain `cargo test --all` → exit 101** again this round: the same 2 AppImage tests ("Disk quota exceeded") because /tmp is still 5.5 GB full of last round's leftovers (see docs/questions.md, needs your `rm`). Not a code failure, but the plain command has not exited 0 since Q10.
- All fixtures are hand-written (`tests/fixtures/snap/README.md`): no `snap` here. Column layout, the `disabled`/`base`/`snapd` notes and `x1` revisions are from memory of `snap list --all`; whether `snap remove` takes `--` is unknown, so the shown command has none (names are validated instead).
- `SnapApps::system()` never run; sizes from `/var/lib/snapd/snaps` never read for real.
- Nothing yet turns old copies into a Clean up group (`old_revisions` exists; the queue's cleaner task is done).
Errors: none new (the /tmp problem above is last round's).
Decisions taken without the human: base snaps and snapd are shown as protected (Blocked::Protected) even though snap itself would refuse only when another snap needs them. Not in docs/questions.md.

## R6-Q13 Flatpak runner, AppImage menu entries, packages → apps via .desktop — Partial   (2026-11-02 00:29 PKT)
WBS: 7.1.5 → In Progress (row 80), 7.1.7 → In Progress (row 82), 7.1.8 → In Progress (row 83); `G/H/L:N` written below
Commits: 79e21f6 (red), cbdefc9 (red), bf0580d
Evidence:
- Red: `cargo test -p tidewise-adapters` → exit 101, 16 failed, then 2 failed after the first green step.
- Green: `cargo test -p tidewise-adapters` → 143 passed (+ real-machine tests). New: `SystemRunner` passes `"a b" "$(id)" "; x"` as plain arguments, forces `LC_ALL=C`, refuses relative programs; `uninstall_user` sends exactly `/usr/bin/flatpak uninstall --user --noninteractive -y <id>` through a fake runner and never calls it for system apps, other sources or hostile ids; `.desktop` parsing from the real Firefox/Code fixtures, broken file, wrong type, no name, hidden entries, other groups ignored; `Exec=` quoting and `env VAR=x`; real `pacman -Qo` output (fixture captured read-only) and hand-written `dpkg -S`; packages → apps keeps only packages that own a visible launcher and names them like it; AppImage launchers found by exact `Exec=`, icons only if inside the icon folders (absolute path outside, `..` and path-like names refused), plan lists launcher and icon with sizes; `pacman_owners` asks once with `-Qo -- files`, ignores relative/newline paths.
- Real machine, read-only: `cargo test -p tidewise-adapters --test pacman_real -- --nocapture` → exit 0 (3 passed): "1484 packages -> 83 apps", `bash` and `glibc` not among them.
- Fuzz: `cargo +nightly fuzz run desktop_files -- -max_total_time=20` → exit 0, 447,703 runs (20 s only).
- `cargo fmt --check` → 0; `cargo clippy --all-targets -- -D warnings` → 0 (after fixing 2 `cloned_ref_to_slice_refs` in my tests); `cargo deny check` → 0; `cargo audit` → 0. Tests split (full /tmp, 14 leftovers still there): `TMPDIR=$PWD/target/tmp cargo test -p tidewise-app` → 0 (211 passed); `cargo test --workspace --exclude tidewise-app` → 0 (260 passed) = 471 passed. Run before the last clippy-only edit; the adapters crate was re-run after it (143 passed).
Not verified:
- **Plain `cargo test --all` was not run to green** this round (same full-/tmp cause as Q11/Q12; needs your `rm -rf /tmp/appimage_extracted_*`).
- No real `flatpak uninstall` ever ran (by rule); the exact flags (`--noninteractive -y`, no `--`) are from memory of flatpak's CLI. System Flatpaks cannot be removed here (helper).
- The `dpkg -S` fixtures and the AppImage launcher fixtures are hand-written; no runner for `dpkg -S` or `rpm -qf` exists yet, so Debian and Fedora cannot map packages to apps. Mapping uses `/usr/share/applications` only (not Flatpak/Snap exports or `~/.local/share/applications`).
- AppImage removal itself (moving the file, launcher and icon to the Trash) is not implemented here (Q14); `with_menu` is not yet called by the app.
- `apps_from_packages` takes the first visible launcher as the app's name; apps with several launchers (or libraries that ship a visible launcher, e.g. some Qt tools) show up as apps.
Errors: clippy `cloned_ref_to_slice_refs` ×2 in new tests (fixed, exit 0). No other failures.
Decisions taken without the human: one `CommandRunner` trait in the adapters crate (the helper is not touched); `uninstall_user` omits `--` before the id because it validates the id instead. Not in docs/questions.md.

## R6-Q14 Apps screen, view + uninstall panel up to confirm — Partial   (2026-11-02 00:52 PKT)
WBS: 7.4 → In Progress (7.2 logic partly: blocked/busy reasons; row 7.4 and 7.2 written below)
Commits: 4e2a7d2 (red), 598b983; 9b6859d (red), f52fe80; ea202f6 (red), 760f22f; 8f7015d (red) with the `screen()` green; UI+controller 5fabe10
Evidence:
- Red runs: `cargo test -p tidewise-adapters uninstall::` → 6 failed; `-p tidewise-app --bin tidewise apps::` → 10, then 2, then 2 failed.
- Green: uninstall 7 passed (AppImage + launcher + icon moved to a temp Trash; list changed since confirm → nothing moves; path outside home refused; blocked never runs; half-way failure reports what went; packages/system Flatpaks → NeedsAdmin with the runner never called; user Flatpak through a fake runner); view-model `apps::` 14 passed.
- Final: `cargo fmt --check` → 0; `cargo clippy --all-targets -- -D warnings` → 0; `cargo deny check` → 0; `cargo audit` → 0. Tests split (/tmp still full, 14 leftovers): `TMPDIR=$PWD/target/tmp cargo test -p tidewise-app` → 0 (227 passed, 1 ignored); `cargo test --workspace --exclude tidewise-app` → 0 (267 passed, 2 ignored) = 494 passed, 0 failed.
- Design check (demo data): `Apps` **3.9 %** (≤ 10 %); `UninstallFlow` whole image 60.1 % (the artboard's backdrop is placeholder blocks + a prototype bar, as with CleanFlow), dialog-only crop (400,100)–(960,480): **7.8 %**. Pictures: `target/design-check/Apps.png`, `UninstallFlow.png`, `UF-side.png`, `Apps-{blocked,done,details,removing,admin,busy,failed,search,scanning,empty,error}.png`, `Apps-live.png`. I looked at Apps, UF-side, AP-pair, Apps-blocked and Apps-live; not at the others (the screenshot test proves they render at 1360×960 and differ from the plain screen).
- Live read-only run: `--screenshot apps --state live` → exit 0 in 3 s: "84 apps from 2 sources" (83 pacman apps, 1 Flatpak); 1.7 GB top entry; button "Needs admin access (coming soon)".
Not verified:
- **Nobody clicked it.** No uninstall was pressed in a real window. Removing a real AppImage or user Flatpak here is forbidden by the run rules; the real Trash move and the real `flatpak uninstall` path (`apps_live.rs`) are only covered by the engine tests on temp dirs and a fake runner. The worker thread, History write for an uninstall and the rescan after "Back to Apps" never ran.
- **Missing from the queue item:** system packages, system Flatpaks and Snaps only show "needs admin" (by rule); apt and dnf packages are not listed at all (no `dpkg -S` / `rpm -qf` runner, so no package→app mapping on Debian/Fedora); no "Also delete my settings & data" checkbox (the row says "kept"; settings paths per source are not built); the "Shared parts only it uses" size is only known for the Flatpak demo; the pacman plan is the package itself (the exact `-Rns --print` list is not wired, Q09); the "busy" state comes only from the engine's `Blocked::Busy`, nothing detects a held lock yet (Q09/apt/dnf runners do not run dry runs here).
- Demo numbers: the artboard's chips say 143/112/24/5/2; the demo has 8 apps, so the chip counts, subtitle and footer differ from the picture; one-line descriptions only exist in the demo (real rows show the version).
- Search typing, filter chip clicks, row clicks and the details toggle were not exercised by anyone; keyboard reach and contrast not checked.
Errors: (1) `cargo fmt` reformatted lines my later string-replace scripts searched for, so two edits failed to apply (caught: a failing assertion and a failing test); redone with exact text. (2) First screenshots of the end-of-uninstall dialogs showed the confirm text because the panel overwrote them; fixed by setting them after the panel (seen in Apps-blocked.png). (3) Slint refused `y` inside a layout; restructured. (4) The `rotation-angle` deprecation warning from my new file, fixed to `transform-rotation`. (5) Slint screen and controller written after the view-model tests, screenshot tests after the screen (not red first).
Decisions taken without the human: adapters get an `uninstall` module that moves AppImage + launcher + icon to the Trash and removes only user Flatpaks; Settings row says "kept" instead of a checkbox; hash-picked avatar colours from existing data tokens (no new colours). Not in docs/questions.md yet.
