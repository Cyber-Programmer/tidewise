# Design check

How we prove a screen matches the design, instead of "looks about right".

## What's in `docs/design/`

| Folder | What | Use it for |
| --- | --- | --- |
| `png/` | Every artboard rendered at 1360 px wide, plus extra states (`Name--state.png`) | The target picture for each screen |
| `screens/` | The artboards as `.dc.html` (layout, copy, sample data in the script block) | Exact copy, sample values, spacing in px |
| `icons/` | All icons + the logo as SVG (24 px grid, 1.8 stroke) | Load in Slint with `@image-url("…svg")` and `colorize` |
| `tokens.md` | Colours, type, spacing, radius | The only values allowed |
| `render/` | The script that made `png/` from `screens/` | Re-render if the design changes |

Fonts live in `assets/fonts/` (Geist + Geist Mono, SIL Open Font License, licence file alongside).

## The loop (one screen at a time)

1. **Screenshot the app in demo mode**, headless and deterministic:
   `cargo run -p tidewise-app -- --screenshot performance --out target/design-check/Performance.png`
   (variant: `--screenshot performance --state details`). Demo mode uses the exact sample values from
   the artboard's script block, so the numbers match the design.
2. **Compare:**
   `python3 tools/design_diff.py docs/design/png/Performance.png target/design-check/Performance.png`
3. **Look at the side-by-side image** it writes (`…-side.png`: design | app | differences in red).
   Read the image; don't trust the number alone.
4. **List every difference** you can see under the screen's heading in `docs/design-check-report.md`
   (spacing, size, colour, weight, missing element, wrong copy). Fix the top ones, go back to step 1.
5. **Done** when the LAYOUT MISMATCH is **≤ 10 %** and the remaining list is only accepted
   differences (below). Record the final score.

## What the score means

`LAYOUT MISMATCH` = share of edges (text, borders, chart lines) that have no matching edge within 5 px
in the other image. Calibration on the Performance screen:

| Case | Score |
| --- | --- |
| Design vs itself | 0.0 % |
| Design with different font hinting + letter-spacing | 0.1 % |
| Design shifted by 3 px | 0.0 % |
| Design vs its own "Memory" tab (a real but small change) | 6.9 % |
| Overnight Slint spike vs design | 71.8 % |

Slint shapes text slightly differently from a browser, so a real match may sit a few % above zero.
The 10 % target is a starting guess; the human tunes it after the first screen.

## Accepted differences (don't copy these from the design)

- Performance artboard: the Memory card wraps "of 16 GB" onto two lines and the Network card wraps
  "down". That's a design bug: keep each value on one line.
- Storage artboard: the "Biggest files" sizes wrap onto two lines ("6.1 / GB"). Design bug: keep each
  size on one line.
- Storage drill-in (approved 2026-10-28, after live testing): folder rows carry a chevron and open
  on click; the breadcrumb can be several levels long (middle collapses to "…"); the list scrolls with a
  scrollbar when it overflows; short folder-map blocks put name and size on one line.
- Anything the design shows as live (chart wiggle, numbers) only matches in demo mode.
- Text antialiasing and sub-pixel differences.

## Report format (`docs/design-check-report.md`)

```
## Performance (default)
Score: 71.8 % → 8.4 %
Fixed: sidebar icons, Geist font, explanation box, top-3 list, details toggle, …
Remaining (accepted): Memory card value kept on one line (design bug)
```
