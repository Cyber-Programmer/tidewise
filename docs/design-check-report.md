# Design check report

Scores are `tools/design_diff.py` LAYOUT MISMATCH, target ≤ 10 %. "Before" is the app right after M1-T2
(the Slint spike layout, built from commit 6fa8dde); "after" is the commit named in each heading.
Run `tools/design_check.sh performance|processes|home` to reproduce.

## Performance (M1-T3)
| Image | Before | After |
| --- | --- | --- |
| Performance | 53.2 % | 7.2 % |
| Performance--details | n/a (state did not exist) | 6.2 % |
| Performance--memory | 50.9 % | 7.4 % |
| Performance--memory-details | n/a (state did not exist) | 7.8 % |
| Performance--network | 51.6 % | 7.3 % |
| Performance--disk | 53.5 % | 7.5 % |

Fixed: Geist/Geist Mono, sidebar (logo, 20 px icons, active state, distro card), header with live dot and
switch, metric tabs, chart stretched to the panel width with area fill, explanation box, facts grid with
wrapping values, per-thread bars, top-3 list with bars, panel insets (24 + 1 px border).
Remaining (accepted):
- The design's Memory and Network tabs wrap "of 16 GB" and "MB/s down" onto two lines (design bug, see
  DESIGN-CHECK.md). Ours keeps one line, so the cards below them sit about 24 px higher.
- Text antialiasing and line-height rounding (Slint `line-height-factor` is a ratio, CSS is absolute).
- Tab sparklines and the chart line differ by a pixel or two in stroke joins.
- `--details` screenshots use `--height 1005`, because the artboard grows with its content (see questions.md).
