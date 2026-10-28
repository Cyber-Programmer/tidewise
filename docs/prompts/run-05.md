# Run 05 — Storage: go deep, scroll, readable blocks (unattended)

You are working alone; nobody can answer questions until the run ends.
Read `CLAUDE.md` (and the `@docs/ENGINEERING.md` it imports), `docs/design/DESIGN-CHECK.md`, the last
entry of `docs/progress.md` and the open entries in `docs/questions.md` first.
CLAUDE.md rules apply, with these overrides **for this run only**:

- **No waiting for approval.** Write a 3–5 line plan per task into `docs/progress.md`, then do it.
- Finish one task fully (red commit, green commit, checks, design check if it has a screen) before the next.
- **No new dependencies.** Anything that seems to need one → `docs/questions.md`, take the option without it.
- When stuck on a decision only the human can make: question + recommended answer in
  `docs/questions.md`, take the safest reversible option, keep going.

## Hard limits (never)

- No `sudo`, `pkexec`, `su`, nothing needing root.
- No work on `crates/helper`, `crates/helper-protocol`, `packaging/polkit`.
- No files changed outside this repository (cargo caches excepted).
- No `git push`, merge, rebase, force or history rewrites; no tags. Local commits only.
- Never trash or delete a real file. Every Trash test uses temp dirs with `HOME` and `XDG_DATA_HOME`
  pointed at them, on a real disk (under `target/`), not tmpfs.
- Don't edit `docs/design/`.

## Test-first is not optional this run

Run 04 wrote parts of R4-T3–T7 together with their tests and recorded that as "not reached". That is
not acceptable here: **every behaviour change gets a red commit first**, no exceptions and no
"not reached" escape. For UI behaviour, put the logic in Rust (view model / controller) and test it
there, and use the headless screenshot tests for what only the picture can show. If you catch yourself
having written code first, stash it, write the test, see it fail, commit it, then restore the code.

## Dates and naming

Run `date '+%F %H:%M %Z'` at the start and use **that** date everywhere.
Branch: `run/05-<YYYY-MM-DD>` from `main` (run 04 is already merged into main).
Commits: `test(app): R5-T1 open a folder inside a folder (red)`, `feat(app): R5-T1 …`.

## What the human saw in the live app (screenshots, 1920×1080, Arch)

1. Clicking a block opens one list of its biggest items, but the folders in it **can't be opened**; you
   can only tick them. They want to go as deep as they like, like a treemap tool, without a literal
   treemap at every level.
2. The list shows only 12 rows (`FOLDER_ROWS` in `storage_live.rs`) and has **no scrollbar**; the page
   is cut off at the bottom of the window.
3. A short block ("Android", ~65 px tall) draws its size **on top of** its name ("Android" / "11.2 GB").
4. Small items show **"0.0 GB"** (`fmt_gb1` on every row).
5. Biggest files: "10.7 G" — the unit is **clipped**.
6. The Documents block says **18.9 GB**; opening it says **19.8 GB**. Find out why before changing anything.

## Tasks, in order

**R5-T0 — A fixture tree and a live screenshot.** Add a way to point the live Storage scan at a folder
(e.g. `TIDEWISE_STORAGE_ROOT`, honoured only together with `--screenshot`, so it can't surprise a user)
and a test fixture builder that makes a nested tree on a real disk: 3+ levels, 40+ items in one
folder, a folder whose block comes out short and wide, files under 1 MB and over 1 GB (sparse files are
fine, but sizes are on-disk blocks, so check what `du` says). Add `--open <relative/path>` for the
screenshot so a test can capture any level. Every later task uses this to show its change in a picture;
look at those pictures yourself, don't only diff them.

**R5-T1 — Go as deep as you like.**
- Keep a **path stack**. The breadcrumb shows `Main drive › Your files › Documents › Creative Chaos …`;
  every part except the last is a link back to that level. When it doesn't fit, collapse the middle
  into `…` (keep the first and the last two).
- **Folder rows open** on click (row body) and show a chevron after the size. The **checkbox** is its
  own hit target and only ticks. **File rows** tick on click, as now. Keyboard: Enter opens, Space ticks,
  Backspace / Alt+Left goes up one level. Accessible names say "Open Documents" / "Select a.iso".
- **Every block opens**, "Other" included: "Other" lists the folders grouped into it.
- Each level is scanned when opened (background, progress, cancel, as now). **Cache levels already
  seen**, so going back is instant; a Trash or Undo rescans only the levels that contain the paths.
- Ticks belong to the open folder. Leaving a folder clears its ticks, so the Move button only ever
  moves what is on screen. "Not opened in 6 months" pre-ticking applies at every level.
- Never follow symlinks, never cross into another filesystem (show such a folder as a row that says
  "On another drive" and doesn't open).
- Tests: stack push/pop/jump, breadcrumb collapse, cache hit on back, cache invalidation after Trash,
  ticks cleared on leave, Other's members, symlink and other-filesystem rows.

**R5-T2 — Show everything, scroll when needed.**
- List **all** children of the open folder, biggest first. Use a virtualised `ListView` that fills the
  card down to the bottom of the window; the scrollbar shows only when the rows don't fit. Cap at 2,000
  rows with a last row "and N smaller items · X MB".
- The page itself must never be cut off: at a 600 px tall window every control is reachable
  (scroll the page if needed). Same for Biggest files: scroll inside its card if it overflows.
- Tests: row count = children (and the cap), screenshot at 1360×960 and 1360×600 showing the scrollbar
  for a 40-item folder and none for a 3-item folder.

**R5-T3 — Blocks that can't overlap.**
- Each block picks a label layout from its **size in pixels**, computed by one Rust function with unit
  tests (boundaries included):
  - tall enough: name at the top, size at the bottom (as now);
  - short: name and size on **one line** (name elides first, size never does);
  - too small for one line: no text; it keeps its accessible label and shows a tooltip on hover.
- The block font tier must not pick a size the block can't hold.
- Test: on the fixture's short wide block, screenshots at 1360 and 1920 wide show no text overlapping
  (assert the layout chosen, then look at the picture).

**R5-T4 — Sizes people can read.**
- One size format for rows and Biggest files: `17.5 GB`, `840 MB`, `12 KB`, `Empty` for 0. Never
  `0.0 GB`. Keep the design's GB rule for large values.
- Biggest files: the size column fits `999.9 GB` without clipping (measure it; don't guess a width).
- Unit tests at every unit boundary.

**R5-T5 — Why 18.9 GB became 19.8 GB.** Reproduce with the fixture, find the cause (for example: hard
links counted once across the whole home in the first scan but again per folder; a block sized from a
different rule; files changing between scans), and write it in `docs/progress.md` with the evidence.
Then make the block and the opened folder use **one rule**, with a test that a block's size equals the
total shown when it is opened. If the difference is real and correct (say, shared hard links), keep it
and explain it in the UI with one line instead.

**R5-T6 — Budgets on the real app.** Run 04 never ran `tools/measure.sh`. Run it on Home and on Storage
after opening 3 levels deep in the fixture: idle memory ≤ 40 MB PSS, idle CPU < 2 %. Write the numbers
in `docs/progress.md`. Over budget is a bug to fix in this run.

## Design

The human approved these changes to the Storage drill-in design (already listed as accepted
differences in `docs/design/DESIGN-CHECK.md`): chevron on folder rows, longer breadcrumb, scrollbar,
one-line labels on short blocks. Storage and Storage--downloads must stay ≤ 10 % in demo mode.

## Done means

- Every task has a red commit before its behaviour change, then the green commit.
- `docs/progress.md`: plans, per-task notes, screenshots looked at (paths), any bent rule, summary at end.
- Final checks on the final commit, all exit 0: `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --all`, `cargo audit`, `cargo deny check`.
- Build last and end with the **Run it** block from CLAUDE.md, plus
  `./target/release/tidewise --page storage`.
