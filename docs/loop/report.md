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
