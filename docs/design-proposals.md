# Design proposals (written by the loop, not approved)

`docs/design/` is not edited by the loop. Differences between the app and the artboards that the loop
thinks should be written into `docs/design/DESIGN-CHECK.md` as **accepted differences** are listed here
for the human to approve, change or reject. Nothing below changes how the check is scored; the scores
stay as measured. Each entry says which kind it is: (a) a real bug in our layout or copy, (b) the artboard
shows something we deliberately do not build, (c) a limit of the checker.

## Settings (R8-Q10): 70.3 % at the artboard's height

Measured with `tools/design_check.sh settings` → `Settings 70.3 %` (target 10 %, **not met**).

- **(c) Crop:** run 07's 86.3 % was the app rendered at the default 960 px height against a 2011 px
  artboard, so the lower two thirds of the design had nothing to compare to. At 960 px the score is 86.3 %, at
  1100 px 80.4 %, at the reference height 2011 px 70.3 % (`design_diff.py`, same pictures otherwise).
  `tools/design_check.sh` now has `settings` and `health` and renders at the reference height.
- **(b) Not built, so absent from our page** (each needs a feature the app does not have):
  - the section index on the left (Appearance, Behaviour, Cleaning, Admin access, Privacy & updates, About);
  - Appearance: Accent colour (four swatches) and Text size;
  - the whole Behaviour group: technical details everywhere, notify me about problems, start Tidewise when I
    log in, size units (GB 1000 / GiB 1024), and the "always asks before removing" note;
  - the whole Cleaning group: package versions to keep, keep system logs for, remind me to clean up, never
    touch these folders;
  - Admin access (needs the admin helper, which does not exist);
  - Privacy & updates: the "no account, no analytics" banner copy and the update-status box with "Check now"
    (there is no network code, golden rule 6);
  - About: the "Source code", "Report a problem" and "Copy system info" buttons and the artboard's copy.
- **(a) Nothing found that is a layout bug** in what we do build: the Appearance card, the Theme row, the
  switch and the About card line up with the artboard's equivalents in the side-by-side picture
  (`target/design-check/Settings-side.png`); the content column is wider only because the section index is
  missing. Copy differs on purpose where the artboard describes features that do not exist.

**Proposed accepted difference:** "Settings: the artboard's six sections are v1.0 scope but not built yet;
until then the score measures missing groups, not layout. Compare the groups that exist."

## Health (R8-Q10): 33.7 % unchanged

Measured with `tools/design_check.sh health` → `Health 33.7 %` (same as run 07).

- **(b) Not measured, so not shown as artboard shows it:** "Checks that passed" (memory pressure,
  temperatures, start-up time) and the "23 updates waiting, -2" row. Our page shows these as "Not measured"
  cards. "Drive health" is one card with a "Check now" button in the artboard and a plain "Not checked" card
  in ours (it needs admin access).
- **(c) Demo values:** the artboard's sample score is 82 (100 - 8 - 5 - 3 - 2), drive 95 GB free, and the
  failed-service text "The firmware update check failed 2 hours ago". Our demo page is computed by the real
  health code from our own sample data: 84, 200 GB free, and a generic failed-service sentence. The
  rings, numbers and some row texts therefore differ. Matching them would need findings the code does not
  produce (updates) or hard-wired text; not done.
- **(c) Sidebar bottom block:** in this artboard the distro card, History and Settings sit about 22 px higher
  than in every other artboard (and in our app); measured in the side-by-side picture, so this artboard's
  sidebar frame is shorter than its page frame.
- **(a) Small real differences:** the artboard shows the first row open with a down chevron, ours shows a
  right chevron on every row (the open state of the first row in the demo is not drawn). Not fixed this round.

**Proposed accepted difference:** "Health: updates, memory pressure, temperature and start-up checks are not
built; the artboard's sample numbers (82, 95 GB) are not reproduced by our demo. Compare layout of the rows
that exist."

## History (R8-Q11): 32.8 % unchanged

Measured with `tools/design_check.sh history` → `History 32.8 %` (reference 1360 x 960, same as the render: no crop problem).

- **(b) Not built:** two entries in the artboard's sample have no record kind in the app: "Stopped Docker" and
  "Tidewise set up (admin access added)" (Docker control and the admin helper do not exist). Our sample list is
  shorter, so everything below "Yesterday" is shifted.
- **(c) Sample values and copy:** our demo is computed from our own record types: "900 MB" instead of "0.9 GB", "4
  groups cleaned" instead of "4 groups · password asked once", a shorter "cleaning cannot be undone" note, "force quit"
  instead of "it had stopped responding". Not changed to match (copy is not tuned to reach a number).
- **(a) Candidate, not fixed:** read off the side-by-side picture (scaled 1.6), our list rows look about 2 px shorter
  each than the artboard's (three rows of "Today": about 123 against 116 displayed px) and the right card's rows
  about 1 px shorter. Not measured with a tool; to be checked against the artboard's CSS before touching the layout.
- **Proposed accepted difference:** "History: record kinds for Docker and admin set-up do not exist yet; compare
  the rows that exist."

## Startup apps (R8-Q11): 18.7 % unchanged

Measured with `tools/design_check.sh startup` → `Startup 18.7 %`.

- **(b) Not built, deliberately:** the "Memory they use right after login" and "Last start-up took" cards (not
  measurable honestly, see R7-Q08), the Impact column (High / Medium / Low), the "Part of your desktop" chips, the
  "Add an app" button, and the artboard's longer "Good to know" copy. Our page shows one stat card (7 of 8 apps).
- **(a) Nothing found** in what we build: the row pitch, switch position, avatars and the right-hand cards line up in
  the side-by-side picture; the stat card is narrower only because the other two cards are absent.
- **Proposed accepted difference:** "Startup apps: impact, memory, start-up time, 'Add an app' and the desktop
  chips are not built; compare the list rows and the switches."

## Services (R8-Q11): 17.1 % unchanged

Measured with `tools/design_check.sh services` → `Services 17.1 %`.

- **(b) Deliberate:** the list puts failed services first (the artboard puts Docker first); start, stop and enable
  are shown as "needs admin access (coming soon)" and the buttons are disabled (the artboard shows them enabled
  with "needs your password"), because the admin helper does not exist; rows show the memory of a service, the
  artboard shows a descriptive sentence from a curated text we do not have for every unit.
- **(c) Demo values:** tab counts 47 and 212 in the artboard against 6 and 8 in our sample list.
- **(a) Nothing found** in the layout of the tabs, rows and the detail card.
- **Proposed accepted difference:** "Services: ordering (failed first), the disabled admin buttons and the sample
  counts differ on purpose until the helper exists."
