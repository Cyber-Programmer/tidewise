# 0001 — UI toolkit: Slint or Tauri

**DRAFT — human decides.** Written by Claude Code during overnight run 1 (2026-10-27/28). It contains
measurements only; the toolkit is **not** chosen here.

## What was built
The Performance screen (`docs/design/screens/Performance.dc.html`, dark theme) twice, both reading live
numbers from the same `crates/core` (`LiveStats`, one sample per second):

| | Slint spike (`spikes/slint`) | Tauri spike (`spikes/tauri`) |
| --- | --- | --- |
| Versions | slint 1.18.1 | tauri 2.x, WebKitGTK 2.54.1 (system), plain HTML/CSS/JS |
| Renderers | FemtoVG (OpenGL) and software, switched at run time with `SLINT_BACKEND` | WebKit |
| Clean release build | 8 min 32 s | 5 min 15 s |
| Parts built | sidebar, 4 metric cards with sparklines, 60 s line chart, 200-row scrollable process list | same |
| Not built in either | Geist fonts, sidebar icons, "Show technical details", explanation text, light theme | same |

## Machine
- CPU: 12th Gen Intel Core i5-1235U (12 threads), 16 GB RAM
- GPU: Intel Iris Xe (Alder Lake-UP3 GT2)
- Distro: Arch Linux, kernel 7.2.9-arch1-1
- Session: KDE Plasma on **Wayland** (an X11 display is also present). Whether winit picked native
  Wayland or XWayland for the Slint window was not checked.
- The machine was in normal use during the runs (editor, browser, a VM, this agent), so treat small
  differences as noise.

## Numbers
One run each, with `spikes/measure.sh`: start the app, wait 60 s, sum `Pss` over the whole process tree,
then measure CPU of the tree over the next 60 s. The app sat idle on the Performance screen (CPU card
selected) the whole time.

| Build | PSS, whole tree | Processes | CPU, 60 s window (100 = one core) | Binary size |
| --- | --- | --- | --- | --- |
| Slint, FemtoVG | 58,161 kB (56.8 MiB) | 1 | 4.3 % | 17,989,784 B (17.2 MiB) |
| Slint, software | 23,383 kB (22.8 MiB) | 1 | 4.8 % | same binary |
| Tauri | 313,420 kB (306.1 MiB) | 3 | 9.1 % | 5,787,272 B (5.5 MiB), **without** WebKitGTK |

Raw lines: `slint-femtovg,17989784,58161,4.3,1` · `slint-software,17989784,23383,4.8,1` · `tauri,5787272,313420,9.1,3`.

### The agreed decision rule applied to these numbers (`docs/prompts/phase-0-spike.md`)
- Slint PSS as a share of Tauri's: FemtoVG 18.6 %, software 7.5 % (rule threshold: ≤ 60 %).
- Gap to Tauri: 255 MB (FemtoVG), 290 MB (software) (rule threshold: under 40 MB favours Tauri).
- The other half of the rule, "design matched within the 6-hour timebox", is a judgement for the human:
  compare the two screenshots/runs against the artboard. Both spikes took one agent session; the only
  fixes after first run were a stretched title block (Slint) and a font on the card units (Tauri).

## Not measured (the phase prompt asks for them; the overnight prompt did not)
- Time to first window (3 cold runs)
- AppImage size. The Tauri AppImage must bundle or depend on WebKitGTK, so its real download size will be
  far above the 5.5 MiB binary. The Slint binary is self-contained apart from system libraries.
- Effort in human hours. Agent wall-clock was dominated by compile time and is not comparable.

## Caveats on the numbers
- Single run each, no repeats, noisy machine.
- CPU includes `LiveStats` itself: `sysinfo` rescans every process once per second in both builds.
- The two UIs redraw differently: Slint uses a virtualised `ListView`; the Tauri page rebuilds all 200 DOM
  rows every second. A tuned page (row reuse) would likely use less CPU.
- PSS is taken once, 60 s after launch, not tracked over time. No memory-growth test was done.
- The Slint software renderer is much smaller in memory here because it needs no GPU context; its frame
  speed on a weaker CPU was not tested.

## Glitches seen
- Slint: first layout let the title block stretch and squeeze the process list (fixed with
  `vertical-stretch`/`alignment`). Sampling pauses on `Window::is_visible`, but Slint has no
  "minimised" signal, so a minimised window may keep sampling.
- Tauri: none at runtime. The WebView reports `document.hidden`, so pausing when hidden works for free.
- Both: process names are cut at 15 characters because `sysinfo` reads `/proc/<pid>/comm` (for example
  `WebKitWebProces`, `tidewise-spike-`). A real Processes screen should read `cmdline` for display names.
- `cargo tauri build --no-bundle` was not used (`tauri-cli` not installed); `cargo build --release
  --features custom-protocol` was used instead, which is what it runs underneath.

## Choice
_Not made. To be filled in by the human._
