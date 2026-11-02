# Questions for the human

Written during unattended runs. Each entry: the question, my recommended answer, what I did meanwhile.

## T4 — no German locale for a real non-English pacman capture
`locale -a` has no `de_*` entry, and generating one needs root (`locale-gen`), which is off limits overnight.
Recommended: on a machine with `de_DE.UTF-8`, run `LC_ALL=de_DE.UTF-8 pacman -Qi asar > tests/fixtures/pacman/Qi-asar-de.txt`
and replace my hand-written sample (the test only needs the parser to refuse it).
Meanwhile: `Qi-asar-de.txt` is hand-written and labelled so in `tests/fixtures/pacman/README.md`.

## M1-T0 — cargo-deny failed once Slint joined the workspace
Slint's tree brought: licence BSL-1.0 (clipboard-win, error-code) and four "unmaintained" advisories
(RUSTSEC-2026-0192 ttf-parser, -0370 zvariant, -0371 zbus_names, -0372 zvariant_derive). No vulnerabilities.
Recommended: keep the allowances in `deny.toml` (BSL-1.0 is GPL-compatible), re-check after each Slint upgrade.
Meanwhile: added exactly those licence and advisory entries to `deny.toml`, each with a reason; nothing else loosened.
`cargo audit` already passes with warnings only (also lists bincode RUSTSEC-2025-0141, unmaintained).

## M1-T3 — details artboards are taller than 960 px; extra colours; chart width
1. `Performance--details.png` (and `--memory-details`) are 1360×1005: the artboard grows with its content.
   Scoring a 960 px render against them stretches the image and gives bogus scores. I added an optional
   `--screenshot … --height <px>` flag (default 960, so the T2 contract holds) and `tools/design_check.sh`
   passes each reference's height. Recommended: keep it; in the real app that content will scroll.
2. The artboards use shades that `tokens.md` does not list (#1E232C active nav, #B4BAC5 nav text, #1C222B selected tab,
   #1F242D sidebar border, #C9CED6 soft text, #222832, #1E232B, #2A303B, #1A1F27). They are in `Theme` under a
   separate comment. Recommended: add them to `tokens.md` (or snap the design to existing tokens).
3. Slint scales a `Path` uniformly, so the chart is drawn by Rust at the real pixel width, computed from the window size
   (`view::chart_width`). Recommended: replace with a Slint-side solution if a later Slint adds non-uniform viewboxes.

## M1-T4 — sidebar does not fit windows shorter than about 700 px
The sidebar (ten 44 px items plus logo and distro card) overflows below roughly 700 px height, while `tokens.md` allows a
600 px minimum window. Recommended: let the sidebar scroll, or compress item height under 720 px (the design has no
state for it). Meanwhile the window minimum stays 900×600 as designed and the overflow is clipped.

## M1-T6 — pausing when the window is covered; idle CPU is higher than hoped
1. Slint 1.18 gives `Window::is_visible()` and `Window::is_minimized()`; the app skips sampling when either says so.
   It has no public "occluded / on another workspace" signal (winit has one internally, `Occluded`, but Slint does not forward it).
   So a window that is open but fully covered, or on another virtual desktop, keeps sampling. Recommended: accept for v1.0;
   revisit if Slint adds an occlusion event.
2. Idle CPU measured 7.5 % (software) and 5.0 % (GPU) of one core on the Home page, higher than expected for one sample a second.
   Not yet investigated: likely the full-window repaint each tick plus `sysinfo` refreshing every process. Recommended:
   profile next; cheap wins are to update models only when values changed and to refresh the process list only on the Processes page.

## R3-T1 — CPU unit and memory meaning
Decision taken: per-process and per-group CPU is a share of the whole PC (÷ logical CPUs), memory in rows is private memory (resident − shared). Both were in the run prompt. A group's total is the sum of private memory, so it can differ from what `top` shows per process (RSS); the details line shows RSS and "x % of one core". Recommended: keep. Open: kernel threads still show as rows (0 MB); say if they should be hidden.

## R3-T2 — process data comes from `/proc` directly
`core::procfs` replaces sysinfo's process refresh (32 ms/tick). It assumes USER_HZ = 100 (true on Linux for user space) and reads the page size from `/proc/self/smaps`. Recommended: keep. Per-process CPU is 0 for the first second after opening Performance or Processes (no earlier reading to compare with); say if that blank second matters.

## R3-T3 — sidebar scroll has no scrollbar
Below about 700 px the main nav items scroll with the wheel or touch; Slint's Flickable draws no scrollbar. Recommended: fine for now; the 60 px icon rail under 1100 px (tokens.md) would make this moot.

## R4-T3 — Storage: colours, categories and what live mode can measure
1. The artboard uses shades `tokens.md` does not list (tile tints #2E2547 #262B34 #23303F, drive-bar #4D72B8 #8E6FD6 #5A6272 #0F1217, amber banner #1E1A10 #2A2312 #4A3B1A #E6CF9E #F5D898, #C9BCEB). They are in `Theme` under their own comment. Recommended: add them to `tokens.md`.
2. Live drive bar shows only "Your files" (scan of the home folder) and "Apps & system" (everything else used). Docker images, Flatpak apps and "Junk you can clean" need root-readable folders or the M3 helper; they stay out until then rather than showing guesses. Recommended: keep.
3. Folder-map blocks are the biggest folders of the home folder; hidden folders get plain names for a few well-known ones (.cache → Caches …). "Steam games" is not a special case yet.
4. In the drill-in, a folder's "not opened" age is its own timestamp, not the newest file inside. Recommended: keep for v1.
5. The Downloads hint no longer claims "old installers make up most of it" (only true sometimes).
6. I did not launch the live window (no display use in an unattended run); live logic is covered by `live_data` and `scan_root` unit tests plus the demo screenshots. Please click through Storage once.

## R4-T5 — health score choices not on the artboard
1. Score bands (Good shape ≥ 80, Could be better 60–79, Needs attention < 60) and the memory threshold (an app group using ≥ 1 GB private memory gets a suggestion, no points) are mine. Recommended: keep, or give me the intended numbers.
2. Until the cleaner exists the score ignores junk, services and updates, and says so on Home. Recommended: keep the honest stub.
3. `tools/measure.sh` was not re-run for Home after adding the 30-second process read. Recommended: run it once.

## R4-T6 — AppImage: what is not done or not verified
1. `appimagetool` is not installed here, so the real AppImage was **not built**; its test skips itself (it passes by skipping). The script, desktop file, icon, AppDir test (starts the AppDir's `AppRun --screenshot home`, 1360×960) and `release.yml` are in place. Install appimagetool and run `packaging/appimage/build.sh`.
2. The script bundles only the Tidewise binary, not shared libraries (fontconfig, xkbcommon, Wayland/X11 …). It relies on the host's, built on Ubuntu 22.04 for old-glibc compatibility. Recommended: add `linuxdeploy` to bundle libraries before v1.0 (new tool, so your call).
3. `release.yml` downloads appimagetool 1.9.1 and checks a sha256 I computed from my own download today (trust on first use, not run). Please verify the hash against a download you trust.
4. The workflow was only syntax-checked, never run on GitHub.

## R5 — Storage drill-in: decisions I took without you
1. `--page storage --open <relative/folder>` also works in the live app (not only with `--screenshot`), because `tools/measure.sh` needs to start the real app three levels deep. `TIDEWISE_STORAGE_ROOT` still only counts with `--screenshot`. Recommended: keep; tell me if `--open` should be screenshot-only and measure another way.
2. Scrollbars: Slint's std-widgets thumb is 2 px and darker than the surface, so the folder list, the page and Biggest files get a 4 px thumb in `Theme.text-3` drawn on top. It does not drag (wheel, touch and the widget's own bar still do). Recommended: keep for v1.0, replace with a real styled scrollbar when Slint allows it.
3. Slint 1.18 prints deprecation warnings for `viewport-width/height/y` (use `content-*`). Not changed in this run (a full rebuild per change, no visible effect). Recommended: rename in a chore commit.
4. Sizes: the size formatter turns 999.5 GB and up into `1 TB`, so `999.9 GB` never appears; the column is nonetheless measured with that text (8 characters, the widest any size can be). The Trash button keeps the design's `22.0 GB` from 1 GB up and uses the item format below.
5. Hard links: a file linked into two folders is now counted in both folders' sizes and once in the total, so a folder's block always equals its total when opened, and rows can add up to more than the total (one line says so). On this machine's home that is 817 MB. Recommended: keep (it is what `du` shows for each folder); the alternative, attributing a shared file to only one folder, is what caused 18.9 vs 19.8 GB.
6. Not done by a click: keyboard handling and hit targets (checkbox vs row) are covered by tests of the logic they call and by pictures, not by a person or a UI-automation test. Please click through Storage once.

## Run 06 — Q02 AppImage size budget (proposal only, not set)
`docs/ENGINEERING.md` §5 still says the AppImage size budget is "to be set in M2". Measured today: 10.0 MiB
(10,467,832 bytes) with no bundled libraries. Bundling fontconfig/xkbcommon/Wayland/X11 with `linuxdeploy`
(R4-T6 point 2) would add a lot, number unknown, not measured. Recommended: budget **≤ 25 MB** for now
(2.5× today, leaves room for bundled libraries), tighten after the libraries are bundled. The human sets it;
I did not edit ENGINEERING.md.

## Run 06 — Q06 Clean up screen: decisions I took without you
1. **New colours** (the design needs them, tokens.md does not list them): `note-card-bg #13221D`, `note-card-border #1F3A31`, `note-card-title #8BEACB`, `note-card-text #B9D9CC` (the "Never touched" card), `scrim #06070A` at 60 % (the dim behind dialogs) and `dialog-border #2E3541`, in `ui/theme.slint` as "extra shades". The design's dialog shadow is **not** drawn (no shadows). Recommended: add them to `docs/design/tokens.md`.
2. **No "Packages nothing uses" group** (orphans, `pacman -Qdtq`): the junk scanner (Q05) does not look for them, so the screen shows 5 groups, not the design's 6, and "of 12.4 GB found · 4 of 5 groups" instead of "12.6 GB … 6". Recommended: add the orphan listing with the apt/dnf adapters (Q08–Q12).
3. **No password step.** The admin helper does not exist in this run, so "Old package downloads" and "Old system logs" end as *skipped* ("they need admin access, which Tidewise does not have yet (coming soon)") and the dialog says so. The design's "Waiting for your password…" step and the "You'll be asked for your password once" line (still shown on the screen) belong to the helper milestone.
4. **Unused Flatpak parts are never removed** (queue forbids `flatpak uninstall` this run); the step ends as skipped ("Tidewise cannot remove … yet").
5. **Freed bytes** are the sizes measured at scan time for the items that were removed, not a fresh measurement.
6. **"History" in step 4 of "What happens"** is plain text, not a link (Slint `Text` has no inline link); "See full report" goes to the History page, which is still "Coming in a later version".
7. **`--demo` mode** shows the Cleaner page with sample numbers but its buttons are not wired (nothing could be removed there, by design).
8. **Q05 slip found and fixed in Q06:** the new `tidewise-core` path dependency in `tidewise-adapters` had no `version`, so `cargo deny check` exited 2 (wildcard ban) from commit a492bd8 until 37e9ea6.

## Run 06 — /tmp is full of leaked AppImage unpackings (please delete)
`/tmp` holds 14 folders `appimage_extracted_*` of about 407 MB each (5.5 GB, most of the 7.7 GB tmpfs), created between 22:47 and 00:00 on 2026-11-01 by `cargo test` runs of `crates/app/tests/appimage.rs`: the test ran `appimagetool` and the built AppImage with `--appimage-extract-and-run`, which unpack to `$TMPDIR` and are not removed. When `/tmp` filled up, two AppImage tests failed with "Disk quota exceeded" (not a code bug). I tried to delete them myself and the permission check refused, so they are still there. Please run: `rm -rf /tmp/appimage_extracted_*` (check `ls -d /tmp/appimage_extracted_*` first). Fixed in the test (commit fbe0306): both now unpack into the test's own temp folder. Until you delete them, run the app tests with `TMPDIR=$PWD/target/tmp cargo test -p tidewise-app`, and the rest on the default `/tmp`: `core::scan::tests::cancelling_stops_the_scan_and_says_so` expects 0 bytes for an empty folder, which is only true on tmpfs (on ext4 a folder's own block counts) — a test that depends on the file system, not flaky; not changed.

## Run 07 — R7-Q01 decisions taken without you
1. **Cancelled-scan test now expects the root's own blocks** (`fs::metadata(dir).blocks() * 512`), not 0, and runs on the project disk (`disk_tempdir`) so ext4 behaviour is exercised. Stricter, not weaker: it still proves nothing below the root was counted. Alternative: keep `tempdir()` and only skip on non-tmpfs (rejected: hides the case). Recommended: keep as is.
2. **A second test-only bug found by the `TMPDIR=$PWD/target/tmp` form:** three `uninstall` tests failed (exit 101) because their fixture wrote `Exec=/path with spaces/Krita.AppImage %U` unquoted (this repo's path has a space: "Creative Chaos"). Unquoted spaces are not valid in `Exec=` (Desktop Entry spec), so the parser was right and the fixture wrong. I quoted the path in the fixture and added `a_quoted_exec_path_with_spaces_finds_its_launcher`. No production code changed. This fix has **no red→green pair**: the three tests were red under `TMPDIR` before the fix (log of that run, exit 101), but the fix commit is test-only. Recommended: accept.
3. **Scratch log in `/tmp/claude-1000/baseline.log`** from the baseline run (written outside the repo; small). I did not delete it (rule: no deleting in `/tmp`). Recommended: delete if you like.

## Run 06 — decisions taken before they were listed here
Copied from the run 06 report (`docs/loop/archive/run-06-report.md`), each with a recommended answer. Nothing here was approved by you yet.
1. **R6-Q07 Junk in the health score.** Junk counts everything the scanner finds (user **and** admin-only items, Trash included); the "clean up" suggestion appears from **1 GB**; the junk scan reruns every **5 minutes** while Home is open (and once at start). Open product question: should the Trash count? Recommended: keep 1 GB and 5 min; do not count the Trash (the user may want it); measure the rescan's CPU in the budget task (R7-Q16).
2. **R6-Q08 `PackageSource` deleted, plain-name rule.** The M1 trait `PackageSource` was removed instead of extended (replaced by `AppSource`; `Pacman::installed()` stays). `is_plain_name` allows ASCII letters, digits and `. _ + - @ :`, no leading `-`, at most 255 characters. Recommended: keep; compare with the helper's own name pattern when the helper is worked on, so the two cannot disagree.
3. **R6-Q10 `Blocked::Protected` and `Blocked::Busy`.** Two variants added to `source::Blocked`. apt dry-run text is trusted only when it carries the English `Reading package lists...` line (or the English lock error); anything else (other languages, garbage) is refused as `Unrecognized`, never guessed. Recommended: keep (safe side); replace the hand-written apt fixtures with a real capture from an Ubuntu/Debian machine.
4. **R6-Q13 One `CommandRunner`.** A single `CommandRunner` trait in the adapters crate (absolute program, argument list, `LC_ALL=C`, no shell). `uninstall_user` omits `--` before the Flatpak id because it validates the id instead (flatpak's behaviour with `--` is not confirmed). Recommended: keep; confirm the flags on a machine with Flatpak apps before 1.0.
5. **R6-Q14 "kept" row in the uninstall panel.** The design's "Also delete my settings & data" checkbox was not built; the row says "kept" (Tidewise does not know per-source settings folders). Avatar colours are picked by hash from existing data tokens (no new colours). Adapters got an `uninstall` module: AppImage + launcher + icon go to the Trash; only user Flatpaks are removed directly. Recommended: keep; R7-Q04 adds the checkbox for user Flatpaks only.
6. **R6-Q15 Group-signal rule in End task.** A signal goes to a whole process group only when **every** process in that group belongs to the app; shared groups fall back to individual pids. Never pid 0 or 1, never Tidewise itself or its own group. "Memory freed" is the group's private memory read **before** the signal (not measured after). A protected app is refused in the app controller (`procs::is_protected`), not inside the adapters crate. Recommended: keep the group rule; move the protected check into the adapters crate too, so it is refused in the library as golden rule 7 says ("refuse in code").

## Run 07 — R7-Q18 Clean up tells the truth: decisions taken without you
1. **Read-only folders are made writable before removal** (your queue text asked for this decision to be listed). Only folders **owned by the user** (owner of `~/.cache`, or `Env.owner_uid` in tests), only inside the app-cache item being removed, never through a symlink (the walk uses `symlink_metadata` and `DirEntry::file_type`, which do not follow links); a folder owned by someone else is left unchanged and the removal reports "read-only folder: ~/.cache/x". Alternative: leave read-only folders alone and report them (safer, but then tools that create 0555 folders can never be cleaned). Residual risk: between the ownership check and the `chmod` a link could be swapped in; closing that needs `unsafe`/`O_NOFOLLOW`. Only a process running as the same user could win it, and it could do the same without Tidewise. Recommended: keep; revisit when the helper does this as root (there it matters).
2. **Record `version` 2 adds `steps[].entries`** (`path`, `bytes`, `result` = `removed`|`left`, `reason`) and a new status `partial`; the old `items` (removed paths) stays, so v1 records still read (`entries` is empty for them; their `done` status for the 4 real `dotslash` records is not corrected). Alternative: replace `items` with the objects (cleaner, but breaks Uninstall's use of `items`). Recommended: drop `items` in a later cleanup once History reads `entries`.
3. **"N folder(s) left" vs "N item(s) left":** the word is "folder" only when every left item is a folder; otherwise "item". Applied to the record title ("Cleaned 19.5 GB · 1 folder left") and the end dialog title ("… freed · 1 folder left").
4. **End dialog text changed.** The old sentence "changed after the scan, so Tidewise left it alone" was untrue for failures, and "Nothing went wrong, and nothing was half-removed" is now only shown when nothing was left. The new text lists up to 3 reasons and "and N more" and says a left folder may be partly emptied. I changed one existing test (`items_left_alone_at_the_last_check…`: title now "1 MB freed · 1 item left") because its old wording is exactly what Q18 removes. Not shown in the design artboards (live-only states).
5. **Freed bytes:** a folder that could not be removed counts the part that went away (scan-time size minus a fresh measurement with `core::scan`, which skips other file systems). **The Trash step was not converted:** its freed bytes are still the scan-time size and its record has no `entries` (it does turn `partial` when something is refused). Recommended: convert it in the History task (R7-Q07).
6. **Not a red→green pair for one test:** `a_link_inside_a_cache_is_never_followed…` and `a_version_1_record_still_reads` passed before the code change (they guard behaviour that was already right).
