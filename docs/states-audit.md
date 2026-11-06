# Empty, loading and error states: audit (run 07, R7-Q13)

Design reference: `docs/design/screens/States.dc.html` ("Empty, error and edge states"). Every message
should say what happened, whether anything broke, and the one thing to do next. "Demo state" is the
`--screenshot <screen> --state <name>` that draws it. **Seen** means a person or I looked at the picture;
nobody has met these states in a real session.

## The artboard's twelve cards

| Card | Status | Where / how to see it |
| --- | --- | --- |
| Clean up · nothing found | **Built** ("Already tidy", "Check again" runs the scan again) | `--screenshot clean-up --state empty` |
| Apps · a source isn't installed | **Built** as a note under the filter chips (no "—" chip) | `--screenshot apps --state notes`; live from `detect_tools` |
| Performance · sensor not available | Not built: there is no temperature card in Performance | n/a |
| Any admin item · access not set up | Not built: no admin helper; admin items say "Needs admin access (coming soon)" | n/a |
| Storage · folders not readable | Partly: a sentence says some folders were skipped; no list of folders, no "Measure with admin" | Storage `scan.skipped` text |
| Apps · unsupported package manager | **Built** (title names the distro, says what still works) | `--screenshot apps --state unsupported` |
| Home · drive critically full | **Built** (banner from 95 % used, "Free up space" opens Clean up) | `--screenshot home --state critical` |
| Any screen · update ready | Not built: Tidewise has no updater (WBS 9.6) | n/a |
| Settings · offline | Not built: no update check, so nothing can be offline | n/a |
| Any reversible action · toast | Exists (8 s, Undo); "longer while hovered" is not built; "also saved in History" is true for Storage Trash moves, End task and Startup changes | Storage / Startup |
| Any list · loading | **Built** as skeleton rows (no spinner) on Apps, Services and Startup apps | `--state scanning` (Apps), `--state loading` (Services, Startup) |
| Any action · failed | Partly: the Clean up end dialog names what stayed and why (R7-Q18); no "Try again" or "Copy error details" buttons | Clean up flow |

## Every built screen

| Screen | Empty | Loading | Error |
| --- | --- | --- | --- |
| Home | no scan yet: "arrive in a later version" card is gone; tiles appear with the first sample | tiles empty until the first sample | n/a (nothing can fail) |
| Clean up | "Already tidy" + Check again | text card "Looking for things to clean…" (kept: it says nothing is removed) | "Could not look…" + Try again |
| Apps | "No apps found" | skeleton rows | "Could not list your apps" |
| Storage | "nothing found" card | scanning card | error card |
| Performance | n/a | n/a | n/a |
| Processes | no matches for the search | first sample | n/a |
| Startup apps | "Nothing in your startup folders…" | skeleton rows | "Could not read your startup apps" (no home folder) |
| Services | "Nothing here." in an empty tab | skeleton rows | "Could not ask systemd…" |
| History | "Nothing here yet" | n/a (instant) | "Could not read your history" is drawn but no code path reaches it |
| Settings | defaults | n/a | broken file moved aside, plain sentence |
| Health | healthy PC: passed list; unmeasured: "Not measured" cards | n/a (instant) | n/a |
| Search | "Nothing matches …" + hint | n/a | n/a |

Only Clean up (empty), Apps (notes, unsupported), Home (critical) and the three skeleton lists were
changed in this task; the rest is what earlier tasks built. Keyboard reach and 4.5:1 contrast of these
states were **not** checked.
