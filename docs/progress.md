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

## 2026-11-13 — 4.4 package details with `pacman -Si`
Done: `crates/pacman` gets `parse_si` (new `si.rs`) and `package_details(&[PackageRef]) -> Result<DetailsLookup, PacmanError>`.
`PackageRef { repo, name }` converts from a `&SearchResult`. `PackageDetails` has repo, name, version, description, url,
licenses, groups, `provides` / `depends_on` / `conflicts_with` / `replaces` (each `Dependency { name, rule:
Option<VersionRule { comparison, version }> }`), `optional_deps` (`OptionalDependency { dependency, reason }`) and both
sizes as bytes **and** pacman's own text (`download_size_text`, `installed_size_text`, for Technical details). "None"
means an empty list or string. `DetailsLookup { found, not_found }`: an unknown name is a normal answer, not an error.
How a call goes: every repo (`is_valid_repo_name`: the 4.3 rule plus no control characters) and name
(`is_valid_package_name`) is checked, repeated refs are dropped (first-seen order kept), an empty request never runs
pacman, then `/usr/bin/pacman -Si -- repo/name ...` runs the `run.rs` way. A non-zero exit is a partial result only if
**every** stderr line is `error: package 'repo/name' was not found` for a package we asked for; anything else is
`Failed`. Each record must be one we asked for, once (`ParseError::UnexpectedPackage`), and each request must be found or
reported missing (`ParseError::MissingPackage`). New `PacmanError::BadPackageRef { reason }` with its arm in the app's
`error_text` (plain "this is a bug, please report it" words, reason in the technical line).
Refactor first, alone: `Record` / `split_records` moved from `qi.rs` to `record.rs` (same 88 + 91 tests before and after).
Real-pacman findings (v7.1.0, this machine, read-only; all 15 732 records in core/extra/multilib surveyed):
- `repo/name` works for every repo; the right name in the wrong repo is "not found" (exit 1); an unknown repo adds
  `error: repository 'x' does not exist` (exit 2).
- The exit code counts the unknown names (1, 2, 3, ...) but is 1 for 256 and 257 of them, so it can't be trusted: the
  stderr words decide.
- `pacman -Si` with no names prints every package (15 732 records), hence the empty-request rule.
- No package is in two repos here, so "does a bare `-Si name` print one or all repos" could not be observed; we always
  pass `repo/name`, so it does not matter.
- `-Si` never prints `[installed]` in Optional Deps (0 of 15 732; `-Qi` does). Continuation lines only in Optional Deps.
- Lists are separated by two spaces; 675 licences hold single spaces (`A OR B`), so Licenses split on two spaces.
- Version rules seen: `=` 7436, `>=` 655, `<` 196, `<=` 60, `>` 2; every name before a rule is a valid package name.
  Optional Deps: 8335 `name: reason`, 403 bare names, 13 with a rule.
Fixtures (`tests/fixtures/pacman/`, README updated): `si-zsh`, `si-dmd` (epoch, versioned Provides, 2 groups),
`si-xf86-video-fbdev` (`<` and `>=` conflicts), `si-iana-etc` (all `None`), `si-multi` (2 records, bare optional dep, SPDX
licences), `si-not-found.stderr`, `si-unknown-repo.stderr` (real), `si-broken-truncated`, `si-bad-byte.bin` (hand-made).
Fuzz: `si` target added (`fuzz/`, README and CLAUDE.md updated); 10-second smoke run clean (101 456 runs, nothing in
`fuzz/artifacts/si`). Human's 10-minute run (`-max_total_time=600`): si: 5 259 938 runs in 601 s, exit 0;
`fuzz/artifacts/si` empty. No crashes, so no new unit tests.
Tests: 149 in `tidewise-pacman` (+3 `#[ignore]`d real-pacman tests, run and passing here), 92 in `tidewise-app`. fmt, clippy,
`cargo test --all`, `cargo deny check`, release build all exit 0 on the last commit.
Next: 4.5 (whatever the WBS says); 4.6 UI: search hit -> `PackageRef::from(&result)` -> `package_details` -> details panel;
format `*_size_text` in Technical details; delete `spike/cli-vs-alpm` (still there).
Blockers: none.
Rules bent:
- Touched `crates/app` (outside the stated scope) with the human's OK: one `BadPackageRef` arm in `error_text` plus a test.
  Variant and arm landed in the same (red) commit so every commit compiles.
- Intermediate commits fail `clippy -D warnings` on dead code (`parse_dependency`, `parse_optional_dep`, `parse_si` and the
  request helpers are unused until `package_details` calls them); red commits fail their tests on purpose. Only the last
  commit is fully green. `cargo test --all` was run before every commit (red commits: only the new tests fail).
- A red test had a wrong expected size (2.31 MiB is 2 422 211 bytes, not 2 422 374); fixed in the green commit `ce92d6c`.
- `si-broken-truncated.txt` and `si-bad-byte.bin` are hand-made (derived from real output; both described in the README).
- Plan changes agreed: new error name `UnexpectedPackage` (was `NotRequested`) plus `MissingPackage`; `VersionRule` struct
  instead of a `(Comparison, String)` tuple. No cap on the number of names in one call (Linux argv limit applies).
- `docs/security/threat-model.md` does not exist yet, so nothing was added to it (the parser reads outside text).
WBS sheet not touched by Claude (the human updates it).

## 2026-11-13 — 4.5 detect when installing would be a partial upgrade
Done: three readers and one decision in `crates/pacman` (offline, no root, no `-Sy`, no network), a fuzz target, one
rule added to CLAUDE.md. Commits (red then green): `parse_qu` (`72a77d9` / `24bbc1c`), `pending_updates()`
(`58b161d` / `bdefbb0`), `sync_lists()` (`2088b1a` / `67bd9c9`), `install_decision` (`9f7645a` / `c6c4793`); fixtures
`c667ec7`; fuzz `ab91816`; CLAUDE.md commit-message rule `6b3faea`.
- `pending_updates() -> Vec<PendingUpdate { name, installed_version, new_version, ignored }>` from `pacman -Qu`.
  Rule (as in 4.3): exit 1 with empty stdout AND empty stderr is "no updates"; everything else not exit 0 is `Failed`.
- `sync_lists() -> SyncLists { newest: Option<SystemTime>, missing: Vec<String> }`: DBPath and repo list come from
  `pacman-conf` (nothing hard-coded), only `<DBPath>/sync/<repo>.db` is read. `newest == None` is "never synced".
  The clock stays out: the caller compares `newest` with its own "now".
- `install_decision(&[PendingUpdate]) -> Allowed { ignored } | MustUpdateSystem { count, ignored }`.
  Ignored updates do not block (the human's decision, matches `-Syu`) but are counted for the review screen.
- New errors: `ParseError::BadUpdateLine`, `ParseError::BadConfigLine`, `PacmanError::ReadLists { path, source }`
  (app `error_text` arm and test, same commit as the variant).
Real-pacman findings (v7.1.0, this machine, read-only):
- `pacman -Qu`: 493 lines, exit 0, stderr empty; every line has 4 words `name old -> new`, epochs included.
- Nothing pending: exit 1, stdout and stderr both empty (observed with `pacman -Qu <up-to-date package>`; the no-names
  case could not be observed because this system was 493 packages behind).
- `[ignored]`: only seen with a private copy of pacman.conf (`--config`, adds `IgnorePkg = 7zip aom`); the line gets
  one space and the literal ` [ignored]`, exit still 0.
- Never synced (private DBPath, empty `sync/`): exit 1, empty stdout, stderr
  `warning: database file for 'core' does not exist (use '-Sy' to download)`. The stderr rule turns it into `Failed`.
- `pacman-conf DBPath` is `/var/lib/pacman/` (trailing slash); `--repo-list` is `core extra multilib`.
- `sync/` also holds 6 leftover `download-*` folders, so only the configured `<repo>.db` files are read.
- `.db` times are the mirror's timestamps (whole seconds), so "age" means how old the lists are, not when this
  machine last synced.
Research (no code): `checkupdates` (pacman-contrib 1.13.1) keeps a private DB folder `${TMPDIR:-/tmp}/checkup-db-$UID/`,
symlinks the real `local/` into it, and runs `fakeroot -- pacman -Sy --disable-sandbox-filesystem --dbpath <private>
--logfile /dev/null`; then `pacman -Qu --dbpath <private>` and drops every line containing `[...]` (so ignored packages
vanish). A trap removes only its own `db.lck`. Details and risks are in the PR description.
Fixtures (`tests/fixtures/pacman/`, README updated): `qu-sample`, `qu-ignored`, `qu-never-synced.stderr` (real),
`pacman-conf-dbpath`, `pacman-conf-repo-list` (real), `qu-broken-truncated`, `qu-bad-byte.bin` (hand-made).
Fuzz: `qu` target (`fuzz/`, README and CLAUDE.md updated); 10-second smoke run clean (331 119 runs, nothing in
`fuzz/artifacts/qu`). Human's 10-minute run (`-max_total_time=600`): qu: 5 254 056 runs in 601 s, exit 0;
`fuzz/artifacts/qu` empty. No crashes, so no new unit tests. `parse_dbpath` / `parse_repo_list` are not fuzzed (small,
`pub(crate)`, unit-tested).
Tests: 200 in `tidewise-pacman` (was 149; +5 `#[ignore]`d real-pacman tests, all run and passing here), 93 in
`tidewise-app` (was 92). fmt, clippy, `cargo test --all`, `cargo deny check`, release build all exit 0 on the last
commit.
Next: M4 must decide the private-copy refresh as its own decision record (fakeroot, `$XDG_CACHE_HOME` not `/tmp`, a
download that is network use under rule 11). 4.6 UI: call `install_decision(&pending_updates()?)` and `sync_lists()`;
"never synced" card when `newest` is `None`. M4: when `ignored > 0` say "N packages held back by your pacman.conf
(IgnorePkg) won't be updated"; the dry run must catch any dependency conflict with a held-back package before anything
installs. Delete `spike/cli-vs-alpm` (still there, as in 4.4).
Blockers: none.
Rules bent:
- Touched `crates/app` (outside the stated scope) with the human's OK: one `ReadLists` arm in `error_text` plus a test.
  The red commit has a placeholder arm (empty title and body) so every commit compiles; the green commit fills it in.
- Red commits (`72a77d9`, `58b161d`, `2088b1a`, `9f7645a`) fail their new tests on purpose. `cargo test --all` was run
  before every commit (red ones: only the new tests fail); clippy was NOT run on the red commits. Only the last commit
  was checked with the full "done" list.
- The no-names "nothing pending" case of `pacman -Qu` could not be run on this machine (493 behind); the exit-1 rule
  rests on `pacman -Qu <up-to-date package>`, the no-repos case, and pacman's source behaviour as described in 4.3.
- `qu-broken-truncated.txt` and `qu-bad-byte.bin` are hand-made (described in the README).
- `docs/security/threat-model.md` does not exist yet, so nothing was added to it (new parsers read outside text).
WBS sheet not touched by Claude (the human updates it).

## 2026-11-13 — 4.6 Search screen (Slint), wired to search / package_details / sync_lists
Done: the Search screen next to Apps, all logic in tested view models. 27 commits on `feat/4.6-search` (red then green):
refactors `d15b96e` `49cf4fa` `6cafd2a` `2a3d0f1` `32f8778`; core age helper `e40233c` / `918e764`; error words
`86adda1` / `81da4b0` and (checking the lists) `cea96fb` / `e4dcfbd`; ranking `888e963` / `1b715ef`; numbered one-at-a-time
searching `fc49f9d` / `9c7edd7`; selection and keys `8c348eb` / `f46403e`; details, latest selection wins `ab8d1f5` /
`02d7b4d`; package lists `69182ef` / `10fbe77`; the screen's words `0bfc1e1` / `01cd96a`; UI `590a20d` (sidebar) `0e29283`
(panels) `b58439c` (hairline fix) `dcd1158` (wiring).
- `tidewise-core::age`: `whole_days(earlier, now)` (0 for a future time) and `utc_timestamp` (integer civil-from-days, no
  date dependency). The header says "Package lists updated today / 1 day ago / N days ago"; the exact UTC time is only in
  the details' technical lines.
- `SearchModel` (`crates/app/src/search_model.rs`): every search is numbered; **one `pacman -Ss` at a time**, and while it
  runs only the newest query waits and runs next; an answer that is not for the latest query is dropped (also a stale
  failure); clearing the box makes the running answer stale; typing back to the running text keeps its answer. Ranking
  (`rank`): exact name, then name, then description, then provides-only, pacman's order kept inside each group
  (stable sort); `repo_kind`: core/extra/multilib plain, `*-testing` "testing", else "third-party".
- Details: **latest selection wins**: the glue waits 150 ms on a row before `request_details`; one `pacman -Si` at a time,
  only the newest request waits, an answer for a row that is no longer selected is dropped (not cached), and the waiting
  request is skipped if its row was left (no queue while arrowing). Found / not-found answers are cached for the session;
  a failure is shown, not cached, and "Try again" asks again.
- `search_view` words every state of the board (start, skeleton, results, no results, refused text, pacman error, details
  loading / found / not found / failed, no package lists). Sizes use the shared `size_text` (SI, number and unit split).
- Slint: `ui/nav/` (sidebar buttons Apps and Search, `NavBridge`), `ui/search/` (bridge, screen, start card, no-lists card,
  list, row, details). `ErrorPanel` now takes properties, so both screens use it. No new colour token: the "Installed"
  pill is outlined teal (accent on surface, an already-tested pair).
- Wording differs from the board on purpose: "official" is dropped (Tidewise searches every repository pacman knows),
  "about 15 000 packages" is dropped (the count is not known), no sizes on rows (human's decision: `-Ss` has none), the date
  became an age (human's decision, a UTC day can be off by one), no "Show in Apps" link yet (Apps can't select a package).
Real-pacman findings (v7.1.0, this machine, read-only, through the real app): `vlc` 86 results, exact `vlc` first, details
loaded (6.7 MB download, 44.2 MB on disk, needs bash, hicolor-icon-theme, vlc-gui-qt); `c++` 359 results and
`libstdc++` shows "Also counts as libstdc++.so=6-64"; `spotfy` 0 results, exit handled as "no results"; a 101-character
text is refused before pacman starts. `-Ss` matching provides is the 4.3 finding (flac via `libFLAC++.so`), so no new fixture.
Tests: 204 in `tidewise-app` (was 93), 15 in `tidewise-core` (was 7), 200 in `tidewise-pacman` (+5 `#[ignore]`d real-pacman tests,
all run and passing). fmt, clippy, `cargo test --all`, `cargo deny check`, release build all exit 0 on the last commit.
Next: M4 "Get package lists" (the no-lists card has one explanatory line instead of the button); focus the search box when
Search opens (not done); a "Show in Apps" link once Apps can select a package; `OFFICIAL_REPOS` is repeated in
`search_model.rs` because `SearchResult::is_official` keeps its own list private (expose one `is_official_repo(&str)` in
`tidewise-pacman` to remove the copy); `spike/cli-vs-alpm` still to delete.
Blockers: none.
Rules bent:
- UI-only commits (`590a20d` `0e29283` `b58439c` `dcd1158`) have no red test; checked by rendering the real window to
  snapshots (see below) and the manual checklist.
- Red commits fail their new tests on purpose (each commit message names the tests that already pass on the stub);
  `cargo test --all` was run before every commit, clippy only on green ones.
- One red-commit test was corrected in its green commit (`f46403e`, `a_new_answer_keeps_the_selection...`): it searched
  "x2", and ranking rightly moves an exact name to the top, so the expected index was wrong; it now checks the same thing
  with a query that keeps pacman's order. Stated in that commit's message.
- Plan reorder: ranking came before the model, and the "one search at a time" and "numbering" steps are one pair (their
  tests would have contradicted each other). Two extra small commits: `Doing::CheckingLists` words (a `pacman-conf`
  failure must not say "couldn't list your packages") and the `ErrorView` move. `crates/core` got a new module (`age`), as planned.
- Three unpushed commits were amended: two messages that had a wrong test count, and the wiring commit, which had slipped
  past `cargo fmt --check` (restored file was unformatted; the gate was read after the commit). Fixed before anything was pushed.
- I could not drive the window with key presses: `xdotool` made KDE show a "xdotool is asking to control input devices"
  prompt. I did not approve it (that is yours to answer; it may still be on your screen: choose Deny). Instead a temporary
  local harness (removed, not committed) opened the Search screen, fed the real `search-edited` callback and
  `list-key` callbacks, and wrote snapshots with `Window::take_snapshot`, so the compositor was not involved. Seen:
  `vlc` (results + details), `c++`, `spotfy`, a 101-character text, the End key (last row selected and in view, details
  followed), the skeleton. NOT seen: the no-package-lists card, the amber pacman-error panel, the 10 s slow note, the details
  not-found / failed states, real key presses and focus rings. Their words are unit-tested; the rest is in the checklist.
- `docs/security/threat-model.md` still does not exist; nothing was added (Search reads pacman output the existing
  parsers already cover, and adds no privileged action, file write or network call).
WBS sheet not touched by Claude (the human updates it).

Manual checklist (run `cargo run -p tidewise-app`, on an unlocked desktop; the human):
1. The sidebar has Apps and Search with icons. Tab reaches each (teal ring), Enter and Space switch screens. Switch to
   Search and back: Apps shows the same list, tab, sort and selected row as before.
2. Search opens: title "Search", "Package lists updated N days ago" (or today), a search box with a magnifier and the
   placeholder "Find an app, e.g. vlc or video editor", and the start card with four buttons (java, video, vlc, spotfy).
3. Press "vlc": the box fills at once and results appear (no 300 ms wait). Skeleton rows may flash for a moment.
4. Type "vlc" with the keyboard: nothing happens while you type, results appear about 0.3 s after you stop. `vlc` is the first
   row, caption "Name matches first, then description matches", footer "N results for “vlc”".
5. Type "a", then quickly "ab", "abc", "abcd": the old rows stay (never a flash of results for a shorter text), the footer
   says "Searching for “abcd”…" and ends with the results for "abcd" only.
6. Click rows: the row highlights, the details follow after a short pause ("Loading the details…" first), with Download and
   "On disk after install" in big mono numbers, Version, Repository, Needs (up to 5 chips, "and N more"), "Already
   installed" when it is. There is NO Install button; the note says "Looking only. Installing comes in Tidewise 0.1."
7. "Technical details" shows `source: pacman -Si repo/name`, pacman's own sizes (MiB), Depends On and the lists' UTC time.
8. `jre-openjdk` or `java`: "Also counts as" shows things like `java-runtime=25`.
9. Type `c++`, select a row labelled with the provides-only group (e.g. `flac`): "Matched because it provides libFLAC++.so".
10. A third-party or testing repository, if you have one in pacman.conf: its rows carry a "third-party" / "testing" chip.
11. Keyboard only: Tab goes search box, × (when there is text), list (ONE stop), "Technical details". In the list Up/Down move
    one row, Home/End the first/last, Page Up/Down about a screen; the selected row never leaves the view; the teal ring
    shows only while the list has focus. Hold Down for a few seconds: the details do not flicker row by row; the last row's
    details appear a moment after you stop.
12. `spotfy`: "No package matches “spotfy”" and a "Clear search" button that empties the box and shows the start card.
13. Paste 101 or more characters: an amber note under the box ("That search can't be used…"), no results; shorten it and
    search works again. The × button clears the box and returns to the start card.
14. Make the window as small as it allows (about 1100x640): nothing overlaps; the details panel scrolls.
15. Not testable on this machine: the "no package lists" card (needs a system that never ran `pacman -Sy`; use a fresh Arch
    VM), the amber error panel and "Try again" (needs pacman to fail), the 10 s "taking longer" note.

## 2026-11-13 — 5.1 threat model v1
Done: `docs/security/threat-model.md` (STRIDE): data-flow diagram with three trust boundaries, assets, pkexec vs
D-Bus comparison (recommendation: pkexec + a summary in argv that the helper checks; **decision pending, yours**),
21 threats T-01..T-21 with defence, proof and status, out-of-scope list, open questions, sources. Your three additions
are T-01 (any process can call the helper; base assumption A1), T-02 (authorization confusion; what each dialog can
show is a main point of the comparison) and T-03 (installed-file tampering; proof planned in 5.5). `docs/PLAN.md`
records the new M3 order (5.7 container harness right after 5.1). Document only: no code touched.
- Checked locally: `pkexec(1)`, `pacman(8)` 7.1, `pacman.conf(5)`, `alpm-hooks(5)`, polkit 128-1 default rules.
  Cited existing test names were checked to exist. Web sources: Qualys PwnKit, GHSL-2021-074, CVE-2011-1485, polkit
  docs, namcap.
- NOT verified: how KDE/GNOME agents render `$(command_line)` / `polkit.message`; PackageKit's dialog; fixed polkit
  versions; serde_json nesting limit. All listed in section 6 of the model.
Next: 5.7 container harness (rootless Podman). You decide pkexec vs D-Bus, and rule 10 vs running pacman without
`--noconfirm` (open question 5), before the first install job.
Blockers: none.
Rules bent: no `(red)` commit (document only, allowed for docs). WBS sheet not touched by Claude (the human updates it).

## 2026-11-14 — 5.7 Arch container test harness (rootless Podman)
Done: first, your two decisions as docs: `docs/decisions/0005-helper-invocation.md` (pkexec + summary in argv that the
helper checks; KDE evidence; 5.4 re-test with our own action; D-Bus fallback), golden rule 10 reworded in CLAUDE.md and
PLAN.md (`--noconfirm` only for a verified transaction), threat model updated (section 3, open questions 1 and 5,
T-02, T-04, source 14). Then `tests/container/` (workspace member `tidewise-container-tests`, no dependencies):
- Guard (red, then green): `require_container_at(root)` needs BOTH `run/.containerenv` and
  `srv/tidewise-test/tidewise-test.db.tar.gz` as regular files; `require_container()` returns `Result<(), GuardError>`
  (`NotInContainer` / `NotOurImage`), no panic in library code. 8 unit tests run in `cargo test --all` on the host.
- Smoke tests (`#[ignore]`): `pacman -S` of a dummy package then `pacman -Q`; and a package whose dependency comes along.
  Red: both fail at the guard on the host (and `NotOurImage` in the plain base image). Green: `tests/container/run.sh`.
- Image: `archlinux:base` pinned by digest `sha256:fcee73ea…` (20261108.0.623074), Arch Archive snapshot of that day,
  three dummy packages built with makepkg, local repo via repo-add, lists refreshed once at build. `run.sh`: refuses
  root, rootless only, `--network=none`, `--security-opt=no-new-privileges`, only the test binary mounted `:ro`.
- Fixtures from the container in `tests/fixtures/pacman/container/` (dry run `-S --print`, not-found, third-party
  `-Si`/`-Ss`, `-Q`/`-Qi` after an install with a dependency).
- Real findings: pacman 7.1 works as container root, including its download sandbox with a `file://` repo; no
  `CheckSpace`/pacman.conf change was needed. `-S --print` lists the dependency first (lib, then app) as `file://` URLs.
  The image build first failed with `pasta ... /dev/net/tun: No such device`: the running kernel (7.2.9) was older than
  the installed `linux` (7.3.4), so `tun` could not load; fixed by your reboot (noted in the README). `makepkg` wanted
  `debugedit`; fixed with `options=('!debug' '!strip')` in the dummy PKGBUILDs. `userdel` prints a harmless
  "mail spool not found" during the build.
- Checked: host has no `tidewise-dummy-*` and no `tidewise-test` in pacman.conf; inside the container only `lo`, uid 0.
  Two runs in a row both green (fresh container each time).
Next: helper work in M3 (protocol, validation, polkit action with the 5.4 dialog re-test, PKGBUILD + namcap in 5.5),
each helper test using this harness. A CI job for the container tests (GitHub's Ubuntu runners should have podman; not
checked). Possible small follow-up: unit tests that the existing parsers read the new container fixtures.
Blockers: none.
Rules bent: no `(red)` commit for docs. `makepkg --nodeps` during the image build only (the lib is not in any
repository yet). `SigLevel = Never` for the local test repository in the test image only. WBS sheet not touched by
Claude (the human updates it).

## 2026-11-14 — 5.2 helper-protocol: request/response types + allow-list
Done: two new crates, no root, no pacman, no I/O.
- `crates/names` (`tidewise-names`, no dependencies): the package/repo name rules moved out of `tidewise-pacman` in a
  refactor-only commit, plus the helper's stricter `is_helper_repo_name` (ASCII only, ≤ 64 bytes, never `local`; you
  chose this over "same as the read side").
- `crates/helper-protocol` (`forbid(unsafe_code)`, same no-panic lints; serde 1.0.229 `derive` + serde_json 1.0.154
  default features, both already in Cargo.lock through Slint, so no new external crate; `cargo deny` ok):
  - requests: `Install`, `UpdateSystem`, `UpdateSystemAndInstall` only; every struct `deny_unknown_fields`;
    `protocol_version` (1); no field maps to a pacman flag, path or URL; each carries the confirmed `transaction`
    (repo, name, version per line) for the 5.3 re-check (T-09).
  - `parse_request(&[u8])`: 2 MiB cap before parsing → strict JSON → version → `validate`. `validate`: sizes first, then
    names/repos/versions, duplicates, and "every target and chosen provider is in the confirmed list from the same
    repo". Errors say field + position + reason, never the text.
  - `summary(&Request)`: ≤ 200 bytes, printable ASCII, deterministic; lists targets, then the chosen providers when all
    fit ("install extra/dbeaver with extra/jre-openjdk (5 packages)"), else "and N more".
  - events: `Progress` / `Finished(Ok | Error(kind))`, one JSON line each; `ErrorKind::plain_text` is one fixed sentence
    per kind; `TransactionChanged` tells the user to finish with a full system update (your note 2); `Details` is the
    only free text (cleaned, 4 KiB).
- Tests: 6 in names, 69 unit + 12 hostile-input table in helper-protocol; whole workspace green. Fuzz target `request`:
  clean. Done 7 182 452 runs in 601 seconds, exit 0, nothing in `fuzz/artifacts/request`.
- Real findings: (1) serde_json 1.0.154 has no 128-level nesting limit (my belief in threat model question 8 was
  wrong): deep values under unknown keys are skipped in a loop, ~150 ms at 1 million levels; the byte cap plus the typed
  shape protect us. Two red-commit tests encoded the wrong belief and were corrected in the green commit (explained
  there). (2) `cargo deny` caught a path dependency without a version (wildcard ban); fixed in its own commit.
- Threat model updated (rows T-02, T-04..T-07, T-09..T-11, T-15; question 8 answered; questions 11 and 12 added).
Next: 5.3 helper binary (needs your OK before any `crates/helper*` change).
- **Requirement (your note 1):** chosen providers passed to `pacman -S` become "explicitly installed". The helper must
  follow with `pacman -D --asdeps <providers>` in the same run, and a container test must show `pacman -Qi` printing
  "Installed as a dependency" for them. `-D --asdeps` is a new allow-listed invocation: needs your explicit OK in 5.3.
- **For M4 (your note 2):** the helper's `-Syu --print` refreshes the REAL lists first; a `TransactionChanged` refusal
  leaves "lists newer than packages". Wording covers it for now; settle with the private-copy decision before the
  update flow (threat model question 12; deferred to M4 by you on 13 Nov 2026, see the 4.5 entry above).
- **For M4 (your repo-name note):** if a search result comes from a repo whose name the helper refuses (non-ASCII), the
  details panel must say plainly that Tidewise can't install from that repository, instead of showing an Install button
  that will fail.
- Check `Phase` (`Checking/Downloading/Installing/Hooks`) against real pacman output in the container; any new error
  kind or request variant needs your OK. Run the 10-minute fuzz again if the protocol changes.
Blockers: none.
Rules bent: no `(red)` commit for the hostile-input table (characterisation tests of existing behaviour) or for docs.
Two red-commit tests were corrected, not weakened, in the green commit (serde_json depth). `Details` keeps a new line
(the plan said all control characters) so multi-line pacman errors stay readable. WBS sheet not touched by Claude (the
human updates it).
