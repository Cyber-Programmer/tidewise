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
