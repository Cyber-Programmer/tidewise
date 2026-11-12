# Progress log

Newest entry at the bottom. One entry per task: `## YYYY-MM-DD — <WBS id> <task>`, then Done / Next / Blockers /
Rules bent. Read the last entry before starting work.

## 2026-11-08 — 1.1–1.3 Plan v2
Done: Plan v2 approved (Arch-only rebuild, AUR packaging, repo-only installer, Rust + Slint, v0.1 after M4,
Update system in scope). Rule files rewritten. WBS sheet archived and rebuilt.
Next: reset the repo.
Blockers: none.
Rules bent: none.

## 2026-11-09 — 2.1 Reset repo history
Done: old `main` (fbd8a33) saved as annotated tag `archive/old-v1`; new orphan branch with root commit
f87dc05 (README + `.gitignore`); `main` replaced with `--force-with-lease=main:fbd8a33`; dependabot branch
deleted; `git branch -r` shows only `origin/main`; no releases on GitHub.
Next: 2.2 workspace.
Blockers: none.
Rules bent: none.

## 2026-11-09 — 2.2 Cargo workspace skeleton
Done: workspace with `tidewise-core`, `tidewise-pacman`, `tidewise-app` (binary `tidewise`), edition 2024,
resolver 3, shared `[workspace.package]` (b0600bc). First TDD cycle: `core::tests::app_name_is_tidewise` shown
failing (`left: ""`, fe542f7) then passing (9a4a1d4). Cargo's sample code removed from pacman (dcd14b3).
fmt / clippy -D warnings / test --all clean. Toolchain: rustc 1.99.1.
Lessons: `cargo new` made a nested `.git` in `crates/core` (rust-lang/cargo#11946) → use `--vcs none`.
Typos `[worksapce]` / `respository` caused "workspace has no members" → treat Cargo warnings as errors.
Next: 2.3 window.
Blockers: none.
Rules bent: none.

## 2026-11-09 — 2.3 Empty Slint window
Done: Slint 1.18.1 added without default features (no system tray; `cargo tree -i ksni` finds nothing),
slint-build as build-dependency, tidewise-core as path dependency (6b541ad). `build.rs` + `ui/main.slint` +
`main.rs`: window titled "Tidewise" showing `tidewise_core::app_name()` (35390a9). Opened with FemtoVG and with
`SLINT_BACKEND=winit-software`; no terminal errors (human screenshots). fmt / clippy / test clean.
Notes: window renders dark because Slint follows the desktop theme; light-first theme comes in 2.7.
Next: 2.5 repo docs, then 2.4 CI.
Blockers: none.
Rules bent: no automated test for the window itself (a real window can't open in `cargo test`); checked
manually. The text it shows comes from a tested function.

## 2026-11-09 — 2.5 Repo docs
Done: `CLAUDE.md`, `docs/PLAN.md`, `docs/ENGINEERING.md`, this log, decision records 0001 and 0002 written into
the repo from the claude.ai project copies, plus "Known traps". D10: repo docs are now the source of truth.
Next: 2.4 CI on GitHub Actions, learned through a pull request.
Blockers: none.
Rules bent: none (docs only, no behaviour change).

## 2026-11-09 — 2.4 CI on GitHub Actions
Done: `.github/workflows/ci.yml` (Ubuntu 26.04; job "check": fmt, clippy -D warnings, test, all `--locked`;
job "deps": cargo-deny) and `deny.toml`. Actions pinned to commit SHAs; token is read-only. Licence allow-list
generated from Cargo.lock (428 Linux crates, 8 licences). Decision: cargo-deny only (covers the RustSec database
that cargo-audit uses). First PR: both jobs green; merged with "Rebase and merge" as 3c6085f.
Reviewed advisories: 5 "unmaintained" (none vulnerable), all via Slint 1.18.1 — RUSTSEC-2026-0370/0371/0372
(zbus split crates, accessibility), 0192 (ttf-parser, Wayland title bar), 0441 (pin-utils, build-time). Ignored with
reasons in deny.toml; remove each when cargo-deny reports it unused.
Lessons: merge on GitHub before deleting branches / pulling; squash-merge would erase red/green commits.
Next: optional branch protection (required checks), then 2.6 licence/README/SECURITY.md.
Blockers: none.
Rules bent: none.

## 2026-11-09 — 2.6 Licence, README, SECURITY.md
Done: `LICENSE` (official GPL-3.0 text, added by the human from an official copy), `README.md` (what it is, honest
status, build steps, how the project works, GPL-3.0-or-later notice, Slint credit), `SECURITY.md` (GitHub private
vulnerability reporting, 7/14-day response times, scope). Ruleset "protect main" created (required checks,
no force pushes); SOP added to CLAUDE.md: every change via branch + PR, Rebase and merge.
Next: 2.7 design tokens (light-first theme) — last task of M0.
Blockers: none.
Rules bent: none (docs only).

## 2026-11-11 — 2.7 Design tokens (light-first)
Done: approved Hybrid token board transcribed to `docs/design/tokens.md` (21 colours light + dark, rules, type,
shape, 16 contrast pairs) and `crates/app/ui/theme.slint` (`Theme` global, light). IBM Plex Sans 400/500/600 +
Mono 400 embedded from IBM's official releases with `OFL.txt` (both licence files compared identical; `fc-scan`
shows family "IBM Plex Sans"/"IBM Plex Mono"). Main window uses tokens, Plex, forced light (human checked: light
background, dark text). `tidewise_core::contrast` (WCAG ratio, strict hex parsing incl. multi-byte and "+" inputs)
and `crates/app/tests/theme_contrast.rs` (reads theme.slint, all 16 pairs >= 4.5:1, exactly 21 colours).
Decision record 0003.
Next: close M0, then M1 (Apps list, read-only).
Blockers: none.
Rules bent: cycle 1 (contrast) red was run and shown but not committed separately; the `#+F0000` test was added
in the green step after finding that `from_str_radix` accepts a leading "+".

## 2026-11-11 — 3.2 pacman fixtures
Done: real `LC_ALL=C pacman -Qi` output for 7 official-repo packages (base, zsh, linux-api-headers, tzdata, bc,
baobab, qt6-imageformats), one multi-record file, a short `-Qq` list filtered to those names, and a hand-made
truncated copy of zsh's record as the broken case, all in `tests/fixtures/pacman/` with a README (command,
pacman 7.1.0 / libalpm 16.0.1, date, why each file is there). git swapped for zsh at the human's request so no
AUR package names appear; every name in `Required By` / `Optional For` / `Optional Deps` checked against
`pacman -Slq`. Found: output to a pipe isn't line-wrapped (base's `Depends On` is one 261-char line).
Next: 3.3 parser for `pacman -Qi` / `-Qq` using these fixtures.
Blockers: none.
Rules bent: no red/green commits (data and docs only, no behaviour change).

## 2026-11-11 — 3.3 Parse `pacman -Qi` / `-Qq`
Done: `tidewise-pacman` now has `parse_qi` (name, version, description, installed size in bytes, install reason)
and `parse_qq`, as pure functions on `&str`. Records split at blank lines (last one needs no blank line), indented
continuation lines join the previous field, lines split at the first `:` only. Errors are all-or-nothing and name
the package and field (`ParseError`, hand-written, no new dependencies); `qi-broken-truncated.txt` gives
`missing field "Installed Size"` for `zsh`. Sizes use integer maths only (`B/KiB/MiB/GiB`, 1024-based, rounded to
the nearest byte, so they are what pacman printed, not exact). `is_valid_package_name` is an allow-list from
`man PKGBUILD` (`pkgname`): ASCII letters/digits and `@ . _ + -`, not starting with `-` or `.`; used by `-Qq` and
for `Name` in `-Qi`. clippy denies `unwrap`, `expect`, indexing and `panic!` in library code. 32 tests.
Next: 3.4 (run `pacman -Qi`/`-Qq` through `std::process::Command`, `LC_ALL=C`) and the Apps list view model.
Blockers: none.
Rules bent: red commits use a stub that compiles (so tests fail on an assertion). In cycles 3, 5, 7, 8 and 9
some new tests passed on first run (they pin behaviour or are a safety net); they are listed in the commit
messages, not hidden. One green commit (continuation lines) was committed with a compile error because the
test run was not gating the commit; I fixed and amended it locally before anything was pushed.
Not parsed yet (not needed for 3.3): `Depends On`, `Optional Deps`, `Required By`, other fields.

## 2026-11-11 — 3.7 Run `pacman -Qi` safely
Done: new `crates/pacman/src/run.rs` with one public entry point, `installed_packages() -> Result<Vec<InstalledPackage>,
PacmanError>`. It runs `/usr/bin/pacman -Qi` (no names = every package; 1490 records here, equal to the `-Qq`
count) with `Command` by absolute path, args as a list, `env_clear()` then `LC_ALL=C`, stdin null. Checked first
that an empty environment gives identical output (`env -i`). The work is split: `run` (start the process;
missing program -> `NotFound`, other start failure -> `Spawn`) and `packages_from_output(&Output)` (exit check,
`from_utf8_lossy`, `parse_qi`), unit-tested with fake `Output` values. `PacmanError` = `NotFound`, `Spawn`,
`Failed {code, stderr}` (stderr kept; `code: None` if killed by a signal), `Parse(ParseError)` (wrapped); hand-written
Display/source, no new dependencies. Invalid UTF-8 becomes U+FFFD but a bad `Name` is still rejected (both tested;
checked the tests fail if decoding is made strict). One `#[ignore]` test runs the real pacman
(`cargo test -p tidewise-pacman -- --ignored`) and cross-checks the count against `-Qq`.
Next: 3.4 Apps list view model using `installed_packages()`. Follow-up: **no timeout** on the pacman process
yet (a stuck pacman blocks the caller; also the call must not run on the UI thread); `/usr/bin/pacman` is
hard-coded, fine for Arch. `-Qq` has no runner yet (not needed).
Blockers: none.
Rules bent: cycles 4 and 5 tests (parse wrap, UTF-8) passed on first run because earlier code already did it;
committed as "pin" tests and mutation-checked. Red commits use a stub that compiles; some red failures are
"expected an error, got Ok" panics from a test helper. WBS sheet not updated (asked to skip). Intermediate green
commit 2 has a dead-code warning until `installed_packages` landed; the final commit is warning-free.

## 2026-11-11 — 3.8 Parse `Required By`
Done: `InstalledPackage` has `required_by: Vec<String>`. `parse_required_by` (qi.rs) splits the value on any
whitespace (pacman's double spaces and `\n` from joined continuation lines alike); `None` or a missing line gives
an empty list; every name goes through `is_valid_package_name`, and the first bad one is
`BadValue { package, field: "Required By", value: <that name> }` (all-or-nothing). Fixture coverage: zsh = one
name, qt6-imageformats = two names, base/bc/baobab = `None`, multi = per-record lists, broken-truncated still
fails on `Installed Size`. Inline test: 7 names over 3 lines. 38 tests in the crate.
Next: 3.4 Apps list view model, using `installed_packages()` (3.7) and `required_by`. SI-unit size
formatting (MB = 1 000 000 bytes) belongs in the 3.4 view model, not the parser.
Blockers: none.
Rules bent: red commit used a stub; `none_is_empty` and `missing_required_by_is_empty` passed on the stub (they
pin behaviour) and are named in the red commit message. A package literally named `None` would be read as "no
one" (pacman has the same ambiguity; none exists in the repos).

## 2026-11-11 — 3.4 Apps list view model
Done: `crates/app/src/apps_model.rs` (plain Rust, no Slint, no threads, never runs pacman). `AppsModel` takes
`Result<Vec<InstalledPackage>, PacmanError>` through `set_result` and answers: `rows()` (name, version, description,
SI size text, `selected`), `tab_counts()`, `empty_state()` (`NotEmpty` / `OtherTabHas {tab, count}` / `NothingMatches`),
`details()` and `state()` (`Loading` / `Loaded` / `Error(ErrorText)`). Tabs Installed by you (default) / Dependencies / All;
search is trimmed, case-insensitive, name or description, and the counts follow it; sort is size (ties by name,
default) or name (case-insensitive, then exact text, so it never wobbles); the selection is kept while visible, else
the first row, else none (after load exactly one visible row is selected, tested). `format_size` uses integer maths
and moves to the next unit when rounding would give 1000 ("999.5 kB" is "1.0 MB", never "1000 kB"). Error bodies use
the approved wording; technical lines are pacman's Display text, `exit code N: <stderr>`, or the parse error text.
Setup (approved): `tidewise-app` now has a lib target (`src/lib.rs`, same no-panic clippy lints as `tidewise-pacman`)
and a path dependency on `tidewise-pacman`; nothing new from crates.io. 35 app tests; all standard commands exit 0.
Next: 3.5 (the Apps screen in Slint using this model). Needs: a worker thread to call `installed_packages()` (it has
no timeout yet, see 3.7), and wiring `set_result`. Unhandled on purpose: a loaded list with zero packages in total
reports `NothingMatches` (not a real case on Arch).
Blockers: none.
Rules bent: red commits use stubs that compile. Tests that passed on the stub (they pin behaviour) are named in their
red commit messages; the 3.4 cycle-8 `needed_by` test had a setup bug (dependency-only packages are hidden on the
default tab) fixed in the green commit with the same assertions; the fixture test passed on first run and is
committed as a pin. WBS sheet not updated (asked to skip).

## 2026-11-11 — 3.5 Apps screen in Slint (M1, read-only)
Done: the window now shows the approved Apps design, fed by `installed_packages()`. Three layers: `AppsModel` (state,
UI thread only) -> `apps_view.rs` (new, lib, no Slint types: decides every string and flag, 26 tests) ->
`apps_window.rs` (glue: copies the view into the `AppsBridge` Slint global, wires callbacks, no wording or state
decisions). `main.rs` is 5 lines. Model additions (tested): `begin_reload()` (refuses a second load while one runs, but a
brand-new model, which is `Loading` with nothing running, may start its first; tested), `mark_slow()`/`is_slow()`,
`totals()` (ignores the search, for the subtitle and "Showing N of M"), `sort()`. Threading: worker thread runs pacman,
leaves the answer in an `mpsc` channel, then `invoke_from_event_loop` (weak handle) fires `load-done` on the UI thread,
which reads the channel. One single-shot 10 s `Timer`, restarted per load, adds "This is taking longer than usual."
(pacman is not killed). Slint: widgets (button, segmented, search field, chip, letter tile, logo as two `Path`s from the
design's SVG data) and Apps panels (sidebar, virtualised `ListView`, 8 static skeleton rows, empty states, details
with a Technical details toggle, amber error panel). Tokens only. Window 1200x800, minimum 1100x640. No new
dependencies, pacman crate untouched. 69 app tests; fmt, clippy, test, deny, release build and the ignored
real-pacman test all exit 0.
Follow-up (same task, after the manual check): item 12 failed (Tab only reached the rows on screen), so the list is now
a listbox. It is ONE Tab stop (a `FocusScope` around the `ListView`); rows are no longer Tab stops. Keys: Up/Down
select the previous/next row, Home/End the first/last, Page Up/Page Down move by the rows that fit (at the ends
nothing changes; a page that would overshoot stops at the first/last row). The selected row always scrolls into view
by the smallest amount, also after a sort, tab or search change and when the list is first laid out or resized. New
tested logic: `AppsModel::select_next/previous/first/last/page`, `AppsView.selected_index`, `list_scroll.rs`
(`scroll_to_show`, `rows_per_page`, and `offset_to_content_y`/`content_y_to_offset`, the only place the sign of Slint's
`content-y` flips). `visible-height` is read-only, so a `changed visible-height` handler copies it into the bridge
(and fires `list-resized`). New tokens: `row-height` (64 px), `size-icon-button` (28 px). Also: the clear (x) button
is a 28 px rounded square (`radius-chip`) inside a transparent 44 px target; the details' big size is the number and
the unit as two texts with a `space-s` gap (a real space in the 32 px mono font was far wider than the one in the rows;
`size_number`/`size_unit` are tested). 90 app tests; all standard commands and the ignored test exit 0.
Next: remove/clean screens need the helper (M3). A fixed-height details area is not needed yet (the panel scrolls).
Blockers: none.
Rules bent: (1) UI-only work has no red test; instead a manual checklist, below. Commits are widgets -> panels ->
fix -> wiring. (2) Two intermediate commits (red) have stubs; pin tests passing on a stub are named in their commit
messages. The first green commit of apps_view had an unused-function warning until its error cases landed.
(3) I could not drive the window: the desktop session was locked, so KWin sent no frame callbacks after the first
frame. I checked real screenshots of the first frame in these states (by a temporary local harness, since removed,
that set the state before the first paint): loading, list + details, Dependencies tab, "No match ... Show
Dependencies" empty state, and the amber error panel. That caught 3 layout bugs (fixed). Items 1-11 were then
checked by the human. For the keyboard follow-up the session was unlocked, so I could watch the live window, but I
cannot send real key presses (`wtype` is unsupported on KWin). A temporary local harness (removed) invoked the same
`list-key` callback the Slint `FocusScope` fires and I checked screenshots: End shows the last row fully visible
at the bottom, Page Down moves 10 rows and scrolls just enough, a sort or tab change keeps the selected row in view, the
details follow the selection. The x square was seen at rest only; the real key mapping (Slint `Key.*` to `ListKey`),
focus rings, hover/press and the 12 and 14-16 checks below are for the human.

Manual checklist (run `cargo run -p tidewise-app`, on an unlocked desktop):
1. The window opens about 1200x800. Grey skeleton rows show for under a second, subtitle "Asking pacman for the list
   of installed packages…", tab counts "–". Then the list appears, subtitle "<N> packages on this system · <M>
   installed by you" with the numbers in the monospace font.
2. "Installed by you" is selected, the first row is highlighted and the details panel shows it.
3. Click "Dependencies", then "All": the list, counts and footer ("Showing N of M") change. Click "Name": A-Z order;
   "Size": largest first.
4. Click another row: it highlights and the details change; the list does not jump back to the top.
5. Type "firefox" in the search box: counts shrink, the subtitle numbers do not. Click × (or Tab to it, press
   Enter): the box clears and the full list returns.
6. Type "libx" on "Installed by you": "No match in Installed by you for “libx”", "It is installed, just not in this
   tab: N matches in Dependencies.", and a "Show Dependencies" button. Press it: the Dependencies tab opens.
7. Type "zzqq": "No installed package matches “zzqq”" and "Clear search". Press it: the box clears too.
8. Select a package with needed-by names: up to five chips, "and N more" if more; one nothing needs: "Nothing else
   on this system needs it."
9. "Technical details" opens "source: pacman -Qi <name>" and "<bytes> bytes"; pressing it again hides them.
10. Keyboard only: Tab walks search, clear (when there is text), tabs, sort, rows, buttons, each with a visible ring;
   Enter and Space activate them.
11. Make the window as small as it allows (about 1100x640): nothing overlaps; the details panel scrolls.
12. (Replaced; item 12 first failed.) The list is ONE Tab stop: Tab from the sort buttons enters the list, the next
    Tab leaves it (to "Technical details"). The selected row shows a teal ring only while the list has focus.
14. With the list focused: Down/Up move one row; the details follow; at the top row Up and at the last row Down do
    nothing. Home/End jump to the first/last row (End scrolls to the very bottom, the last row fully visible).
    Page Down/Page Up move by about one screen of rows and stop at the ends. Keep pressing Down past the bottom edge:
    the list scrolls one row at a time and the selected row never leaves the view.
15. Scroll the list far away with the mouse wheel, then press an arrow key: it jumps back to the selected row. Select
    a package far down, then click "Name"/"Size", another tab, or type a search that still matches it: the selected
    row is still on screen. Click a visible row: the list does not jump.
16. Search field: type text; the x button shows only then. Hover it: a small rounded grey square (about 28 px)
    appears with clear space to the field border; press it: slightly darker. Tab to it: a teal ring around that
    square. Its click area is still large: click about 8 px to the right of the visible square and it clears.
17. Details panel: the big size reads like "1.67 GB" with the same one-space gap as in the rows, for kB, MB and GB.
13. Error panel (look only): temporarily change `installed_packages()` in `apps_window.rs` to return
    `Err(PacmanError::Failed { code: Some(1), stderr: "x".into() })`, run, check the amber panel, "Try again",
    "Technical details" and the report line, then `git checkout` the file. 10 s note: same trick with a
    `std::thread::sleep(12 s)` in the worker.

## 2026-11-12 — 3.6 cargo-fuzz targets for the pacman parsers
Done: `fuzz/` (own workspace, outside the root one; `libfuzzer-sys` 0.4.13, licence `(MIT OR Apache-2.0) AND NCSA`,
only in `fuzz/`) with two targets, `qi` (`parse_qi`) and `qq` (`parse_qq`). Each returns early on non-UTF-8 input
(as `run.rs` does) and only calls the parser. Seeds are `tests/fixtures/pacman/` used directly (no copies);
corpus and artifacts are gitignored. Commands are in `fuzz/README.md` and the "Fuzzing" section of CLAUDE.md,
including `cargo fmt --manifest-path fuzz/Cargo.toml --check`. Nightly only through `cargo +nightly`; default
toolchain still stable. Commits: 8420d2a (crate), 721d8ba (CLAUDE.md).
Results (human's 10-minute runs, `-max_total_time=600`): qi: 6 437 138 runs in 601 s, exit 0; qq: 50 636 026 runs
in 601 s, exit 0; `fuzz/artifacts/` empty. No crashes, so no new unit tests. Root commands (fmt, clippy, test,
deny, release build) pass on stable.
Next: fuzz targets for any new parser (e.g. `-Ss`, `-Si`, `/proc`) when it lands; CI fuzzing and a deny.toml
decision on the NCSA licence in M3 (needs the human's OK).
Blockers: none.
Rules bent: no `(red)` commit (the harness adds no behaviour to test). `fuzz/` is not checked by root fmt/clippy or
CI; it has its own fmt check and `cargo +nightly fuzz build`.

## 2026-11-12 — 4.1 spike: CLI parsing vs libalpm (alpm crate) for sync data
Done: two throwaway prototypes on branch `spike/cli-vs-alpm` (own `[workspace]` in `spike/`, like `fuzz/`; root
workspace, CI and `deny.toml` untouched; commits `a821c4b` … `5d9e63c`): A runs `pacman -Ss`/`-Si` like `run.rs`,
B uses `alpm` `=5.0.2`. Read-only, normal user, real DB only through `pacman -Ss/-Si` and libalpm reads; every
broken-DB case ran on temp copies in the scratchpad. Wrote `docs/decisions/0004-pacman-data-source.md` with the
results table, the recommendation and the consequences; added `spike/` to the branch prefixes in CLAUDE.md.
Main findings: search (349) and details (20) are identical in A and B; A is about 2x slower (≈ 0.3 s per pacman call,
the DB load); B stays silent on a missing or corrupt DB, panics on a non-UTF-8 byte in a description, does not
start after a libalpm soname bump (exit 127), needs ≈ 61 MB RSS with a handle open and its own `pacman.conf`
handling, and pulls two GPL-3.0-only crates (`cargo deny`: licences FAILED, rest ok). Lock file `db.lck`: not
created and not needed by either. My recommendation was A.
Decision (human, 2026-11-12): A, CLI parsing. 0004 is Accepted; CLAUDE.md "Reading pacman data" and PLAN.md §6 / D11
updated, with the `-Ss` exit-1 = no results rule.
Next: push `spike/cli-vs-alpm` (kept until 4.3/4.4 are done, then deleted); update WBS row 4.1 (human, by their
instruction); `parse_ss`/`parse_si` with fixtures and fuzz targets (4.3/4.4).
Blockers: none. Not verified: pacman's NEWS (gitlab blocked automated fetches), the C source for when libalpm takes
the lock, soname history between `.11` and `.14`, which build first shipped `.16`, `-3`/`-4` pacman builds,
signature-checked DBs, cold-cache timing.
Rules bent: no red/green commits (spike, nothing merged); branch prefix `spike/` was not in the list (added in this
PR); new dependency `alpm` added without a decision record because it exists only in the throwaway `spike/`, with the
human's OK for version `=5.0.2`. WBS sheet not touched by Claude (the human updates it).

## 2026-11-12 — 4.3 search the repos with `pacman -Ss`
Done: `crates/pacman` gets `parse_ss` (new `ss.rs`), `search(query)` and `SearchResult { repo, name, version, groups,
installed_version: Option<String>, description }`. `[installed]` gives `Some(repo version)`, `[installed: x]` gives
`Some(x)`. `search` builds `pacman -Ss -- <terms>` the `run.rs` way: split on whitespace, each term escaped for POSIX ERE
(`c++` works), `--` always first, empty/blank text returns no results without running pacman, cap of 100 characters,
control characters refused (new `PacmanError::BadQuery`). Exit 1 + empty stdout + empty stderr = no results; exit 1 with
text on stderr (what a bad regex does) is `Failed`. Output order is pacman's own: ranking is the 4.6 view model's job.
Third-party repos are kept, labelled by `repo`; `SearchResult::is_official()` is true for `core`, `extra`, `multilib` only
(testing/staging repos count as third-party: one constant, `OFFICIAL_REPOS` in `ss.rs`).
Repo-name rule: from pacman's source (`src/pacman/conf.c`, `lib/libalpm/db.c`) a repo is any `[section]` name except
`options`; libalpm needs it non-empty, without `/`, and not `local`. So `parse_ss` requires only non-empty (the first `/`
ends it); spaces, dots, capitals, non-ASCII are accepted (test `repo_rule_is_pacmans_not_stricter`).
Real-pacman finding: `-Ss` also matches what a package *provides* (flac for `c++`, via `libFLAC++.so`), so not every hit
shows the term in its name or description.
Fuzz: `ss` target added (`fuzz/`, README and CLAUDE.md updated); 10-second smoke run clean (497 228 runs, nothing in
`fuzz/artifacts/`). Human's 10-minute run (`-max_total_time=600`): ss: 7 124 277 runs in 601 s, exit 0;
`fuzz/artifacts/ss` empty. No crashes, so no new unit tests.
Tests: 88 in `tidewise-pacman` (+2 `#[ignore]`d real-pacman tests, run and passing here), 91 in `tidewise-app`. fmt, clippy,
`cargo test --all`, `cargo deny check`, release build all exit 0 on the last commit. Commits: 516fe67 fixtures, 327deab
red / 689f9b8 `parse_ss`, 453f4b8 red / d3545f6 query building, 193be13 red / 8f717f4 `search()`, b5dd0aa red / ce3ada9 app
error text, ea1427e fuzz target.
Next: 4.4 `parse_si` (details); 4.6 search UI (ranking, official/third-party label,
use `installed_version` vs `version` for "update available"); delete `spike/cli-vs-alpm` after 4.4.
Blockers: none.
Rules bent:
- Touched `crates/app` (outside the stated scope) with the human's OK: one `BadQuery` arm in `error_text` plus a test, because
  the new error variant made its exhaustive `match` fail to compile.
- Commits 453f4b8 to b5dd0aa leave `tidewise-app` not compiling (I ran `-p tidewise-pacman` only and missed it until the
  full clippy); d3545f6 fails `clippy -D warnings` on dead code until `search()` uses it. Only the last commit is fully green.
- `tests/fixtures/pacman/ss-third-party.txt` is hand-made (made-up repos `chaotic-aur` layout, `my repo.x`), not captured: my
  attempt to make a real fake sync DB and run pacman on it was denied, so the made-up lines copy the real layout but were
  not confirmed against pacman. The repo-name rule comes from the pacman source vendored in `alpm-sys` 5.0.1, not
  necessarily 7.1.0.
- `docs/security/threat-model.md` does not exist yet, so nothing was added to it (the parser reads outside text).
WBS sheet not touched by Claude (the human updates it).
