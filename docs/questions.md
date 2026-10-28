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
