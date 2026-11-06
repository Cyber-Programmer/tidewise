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
scans were over; the first seconds after start use more. A home-file search index was not built; its probe (R8-Q08, `crates/core/examples/index_probe.rs`,
200 000 files, compact layout) added 6.3 MB PSS, which does not fit on the Apps page (see docs/questions.md R8-Q08).

## Run 08 (R8-Q09, 6 Nov 2026): pages run 07 did not measure

Command per page: `MEASURE_WAIT=40 MEASURE_WINDOW=60 tools/measure.sh ./target/release/tidewise --page <page>`, one page at a
time, software renderer, release binary of 26 736 280 bytes (built by `tools/ci-local.sh` at commit 29e8c55, docs-only
commits since), real Wayland session, window visible, one process each. MB = kB / 1000, as in the table above.

| Page | PSS (MB) | CPU (% of one core) | Within budget |
| --- | --- | --- | --- |
| Performance | 29.8 | 0.9 | yes |
| Processes | 31.6 | 1.4 | yes |
| Storage | 31.6 | 0.1 | yes |

Start-up peak (PSS sampled every 0.2 s for the first 20 s, `--page home` and `--page apps`, one run each): Home about
29.2 MB, Apps about 34.8 MB, both highest at the last sample, so no start-up spike above the settled value showed at
this sampling rate. The peak resident size (VmHWM, which counts shared library pages and is not the budgeted number)
was about 45.8 MB for Home and 51.5 MB for Apps.

Still not measured: the Ctrl K box while open (it needs a key press; the machine has `wtype`, which types into
whichever window has focus, so I did not use it), the demo/GPU renderer, a machine other than this one, a 60 s wait
instead of 40 s, whether the Storage scan was still running at 40 s (CPU 0.1 % suggests it was not). The index probe
numbers (R8-Q08 above and in `docs/questions.md`) were in MiB (kB / 1024): the compact index's 6.3 MiB is about 6.6 MB
in this table's unit; the decision there does not change.
