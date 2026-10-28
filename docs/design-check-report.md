# Design check report

Scores are `tools/design_diff.py` LAYOUT MISMATCH, target ≤ 10 %. "Before" is the app right after M1-T2
(the Slint spike layout, built from commit 6fa8dde); "after" is the commit named in each heading.
Run `tools/design_check.sh performance|processes|home` to reproduce.

Where a screen or state did not exist yet, "before" is what the app showed at that commit, scored against the reference
image: (1) its default Performance render (no details state yet), (2) its Memory-tab render, (3) its only page, the
Performance render, since the app had no Processes or Home page. These are real scores, but they measure "wrong screen",
not a near miss.

## Performance (M1-T3)
| Image | Before | After |
| --- | --- | --- |
| Performance | 53.2 % | 5.5 % |
| Performance--details | 60.5 % (1) | 4.7 % |
| Performance--memory | 50.9 % | 5.7 % |
| Performance--memory-details | 52.9 % (2) | 6.2 % |
| Performance--network | 51.6 % | 5.5 % |
| Performance--disk | 53.5 % | 5.7 % |

Fixed: Geist/Geist Mono, sidebar (logo, 20 px icons, active state, distro card), header with live dot and
switch, metric tabs, chart stretched to the panel width with area fill, explanation box, facts grid with
wrapping values, per-thread bars, top-3 list with bars, panel insets (24 + 1 px border).
Remaining (accepted):
- The design's Memory and Network tabs wrap "of 16 GB" and "MB/s down" onto two lines (design bug, see
  DESIGN-CHECK.md). Ours keeps one line, so the cards below them sit about 24 px higher.
- Text antialiasing and line-height rounding (Slint `line-height-factor` is a ratio, CSS is absolute).
- Tab sparklines and the chart line differ by a pixel or two in stroke joins.
- `--details` screenshots use `--height 1005`, because the artboard grows with its content (see questions.md).

## Processes (M1-T4)
| Image | Before | After |
| --- | --- | --- |
| Processes | 65.9 % (3) | 0.1 % |
| Processes--details | 66.3 % (3) | 0.6 % |
| Processes--apps | 58.3 % (3) | 0.1 % |
| Processes--protected-selected | 65.2 % (3) | 0.3 % |

Before: the app had no Processes page, so there is nothing to score; the first render of the new page, with the
column stretched down the window, scored 41.7 % / 51.2 % / 71.4 % / 44.0 % (default / details / apps / protected-selected).
Fixed: header, segmented filter, search box with placeholder and icon, list card with header row, avatars,
lock icon on protected rows, hot (amber) numbers, selected-row tint, selected-process bar with End button,
"Protected · part of your system" badge and, in details, "Force quit" plus the command line.
Remaining (accepted): text antialiasing; "End task" does nothing yet (M4).

## Home (M1-T5)
| Image | Before | After |
| --- | --- | --- |
| Main | 65.4 % (3) | 1.3 % |

Before: the app had no Home page. Built: sidebar with logo, icons, active state and distro card (real `/etc/os-release`,
desktop and tools in live mode), header with search pill and "Scan again", health ring (82 of 100), suggestion rows,
"Right now" tiles with sparklines and bars, apps-by-source strip, and a titled "Coming in a later version" page for the
other sidebar items. Live mode shows the tiles and a "Scan coming soon" card instead of score and suggestions.
Remaining (accepted): text antialiasing; the search pill does nothing yet; "Review & fix" and the tiles only navigate.

## Storage (R4-T3)
Run `tools/design_check.sh storage`.
| Image | Before | After |
| --- | --- | --- |
| Storage | 17.1 % (first render) | 2.7 % |
| Storage--downloads | 33.1 % (first render) | 2.3 % |

Fixed along the way: tile text pinned to the bottom, the 2 px gaps of the drive bar, the map card taking the free
width (the row was `alignment: start`), the 32 px breadcrumb button of the drill-in, and Biggest-files rows that grow
with their wrapped second line (the design's rows are not all the same height).
Remaining (accepted): the "Biggest files" sizes stay on one line (design bug, DESIGN-CHECK.md); text antialiasing; the
treemap in demo mode uses the artboard's own rectangles, live mode uses a squarified treemap.

## Storage drill-in (R5, 2026-10-28)
Run `tools/design_check.sh storage` (demo data).
| Image | Before R5 | After R5 |
| --- | --- | --- |
| Storage | 2.7 % | **3.0 %** |
| Storage--downloads | 2.3 % | **9.0 %** |

Both ≤ 10 %. What moved: the Downloads drill-in card now stretches down to the window bottom with a `ListView`
(the artboard's card ends after its last row), rows carry a chevron and a separate checkbox target, the breadcrumb can
be longer, and the Biggest files size column is as wide as a measured `999.9 GB` (63 px instead of a guessed 47 px; the
gaps in a row went from 12 px to 6 px to pay for it). These are the accepted differences listed in
`docs/design/DESIGN-CHECK.md` (chevron, longer breadcrumb, scrollbar, one-line labels). Along the way the map-level
breadcrumb briefly scored 16.5 % when it picked up the drill-in's 32 px height; it keeps its 22 px height again.
Remaining (accepted): text antialiasing; the thumb of the scroll areas is drawn in `Theme.text-3` because the widget's
own thumb is nearly invisible on the dark surface.
