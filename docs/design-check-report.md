# Design check report

Scores are `tools/design_diff.py` LAYOUT MISMATCH, target ≤ 10 %. "Before" is the app right after M1-T2
(the Slint spike layout, built from commit 6fa8dde); "after" is the commit named in each heading.
Run `tools/design_check.sh performance|processes|home` to reproduce.

## Performance (M1-T3)
| Image | Before | After |
| --- | --- | --- |
| Performance | 53.2 % | 5.5 % |
| Performance--details | n/a (state did not exist) | 4.7 % |
| Performance--memory | 50.9 % | 5.7 % |
| Performance--memory-details | n/a (state did not exist) | 6.2 % |
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
| Processes | n/a (screen did not exist; spike list only) | 0.1 % |
| Processes--details | n/a | 0.6 % |
| Processes--apps | n/a | 0.1 % |
| Processes--protected-selected | n/a | 0.3 % |

Before: the app had no Processes page, so there is nothing to score; the first render of the new page, with the
column stretched down the window, scored 41.7 % / 51.2 % / 71.4 % / 44.0 % (default / details / apps / protected-selected).
Fixed: header, segmented filter, search box with placeholder and icon, list card with header row, avatars,
lock icon on protected rows, hot (amber) numbers, selected-row tint, selected-process bar with End button,
"Protected · part of your system" badge and, in details, "Force quit" plus the command line.
Remaining (accepted): text antialiasing; "End task" does nothing yet (M4).

## Home (M1-T5)
| Image | Before | After |
| --- | --- | --- |
| Main | n/a (screen did not exist) | 1.3 % |

Before: the app had no Home page. Built: sidebar with logo, icons, active state and distro card (real `/etc/os-release`,
desktop and tools in live mode), header with search pill and "Scan again", health ring (82 of 100), suggestion rows,
"Right now" tiles with sparklines and bars, apps-by-source strip, and a titled "Coming in a later version" page for the
other sidebar items. Live mode shows the tiles and a "Scan coming soon" card instead of score and suggestions.
Remaining (accepted): text antialiasing; the search pill does nothing yet; "Review & fix" and the tiles only navigate.
