# Budget measurements (R7-Q16, 2 Nov 2026)

Command per page: `MEASURE_WAIT=40 MEASURE_WINDOW=60 tools/measure.sh ./target/release/tidewise --page <page>`
(one page at a time, software renderer, release binary built from commit 802e847, real Wayland session,
window visible). Budgets: idle memory ≤ 40 MB PSS, idle CPU < 2 % of one core (`docs/ENGINEERING.md` §5).
Binary size 26 726 168 bytes. One process each.

| Page | PSS (MB) | CPU (% of one core) | Within budget |
| --- | --- | --- | --- |
| Home (with the junk rescan and failed-services read) | 29.7 | 1.0 | yes |
| Clean up | 29.8 | 0.3 | yes |
| Apps (list loaded) | 34.2 | 0.3 | yes |
| History | 28.8 | 0.3 | yes |
| Startup apps | 30.2 | 0.3 | yes |
| Services | 30.2 | 0.3 | yes |
| Settings | 28.9 | 0.3 | yes |
| Health | 28.9 | 0.3 | yes |

Not measured: Performance, Processes and Storage (measured in earlier runs, not repeated), the Ctrl K
box while open, the demo/GPU renderer, a machine other than this one. Measuring at 40 s means the first
scans were over; the first seconds after start use more. A home-file search index was not built, so it
has no number.
