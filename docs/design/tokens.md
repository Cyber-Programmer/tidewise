# Design tokens — Hybrid (light-first)

Source: the "Tokens & components (Hybrid handoff)" board on the Tidewise design canvas, approved by the human in
Nov 2026 (Hybrid direction: Apple-like calm grouping + datasheet-style numbers). Transcribed 2026-11-09 (task 2.7).

- **In code, use token names, never raw hex.** Light values live in `crates/app/ui/theme.slint` (`Theme` global).
- Dark values are recorded here for later (milestone M9); the app is forced to light until then.
- Contrast is a test: `crates/app/tests/theme_contrast.rs` reads `theme.slint` and fails CI if any text pair
  below drops under 4.5:1.

## Rules

- **60-30-10.** Off-white page (60), ink, greys and white panels (30), one teal accent (under 10). Status colours
  sit outside the budget.
- **Amber = warning, red = permanent action.** Never decoration.
- **Teal = reversible** (Trash, turn off, stop). **Red = permanent** (delete, uninstall, end process). Neutral =
  cancel, details, navigation. Every red button names what it removes.
- **No shadows, blur, glow or gradients.** Depth comes from panel outlines and hairlines (keeps the software renderer cheap).
- **Every number in IBM Plex Mono.** Big numbers lead rows and panels, like a datasheet.
- Rows sit in one white panel separated by hairlines, never one card per row. Number first, then words, then one action.
- Settings accent choice (later) is limited to teal, blue and graphite, so it never collides with amber or red.
- The tide gauge is the only decorative element, and it carries data: level = sum of fixed points per problem
  (0 = perfect); under 30 calm, 30–60 choppy, over 60 rough.

## Colour tokens

| Token | Light | Dark | Used for |
|---|---|---|---|
| `bg` | `#F6F6F3` | `#141516` | Page background · the 60 |
| `surface` | `#FFFFFF` | `#1C1D1F` | Panels, lists, dialogs |
| `surface-2` | `#F2F1EC` | `#26272A` | Selected row, inset notes |
| `chip` | `#EFEEE9` | `#2A2B2E` | Neutral chips, letter tiles |
| `border` | `#E2E1DC` | `#2C2D30` | Panel outline |
| `rule` | `#ECEBE6` | `#2A2B2E` | Hairlines between rows |
| `control-border` | `#DAD9D3` | `#3A3B40` | Inputs, segmented controls, ghost buttons |
| `text` | `#18191C` | `#ECECE8` | Headings, values |
| `text-2` | `#3D4047` | `#C9CBCF` | Body text |
| `text-3` | `#5E636B` | `#A3A6AB` | Labels, help text, units |
| `accent` | `#0E6A6D` | `#4FB3B0` | Reversible actions, selection, tide · the 10 |
| `accent-hover` | `#0A5759` | `#6CC4C1` | Accent hover |
| `on-accent` | `#FFFFFF` | `#0B1414` | Text on accent buttons |
| `tide-fill` | `#D3E6E5` | `#1F3D3C` | Water in the tide gauge |
| `warning` | `#8A4F00` | `#E0A64A` | Warnings only (not decoration) |
| `warning-bg` | `#F6EBDA` | `#3A2C14` | Warning chip background |
| `danger` | `#B3261E` | `#C2362E` | Buttons for permanent actions |
| `danger-text` | `#B3261E` | `#F08A82` | Red text (permanent-action notes) |
| `on-danger` | `#FFFFFF` | `#FFFFFF` | Text on red buttons |
| `switch-off` | `#8C8B85` | `#6A6C71` | Switch track when off |
| `nav-active` | `#E6E5DF` | `#2A2B2E` | Current sidebar item |

Dark mode keeps the same structure; the accent lightens to stay readable, teal buttons get dark text, red buttons
keep white text.

## Type

One family, two cuts: **IBM Plex Sans** and **IBM Plex Mono** (SIL Open Font License 1.1, embedded in the app,
unmodified; see `crates/app/ui/fonts/OFL.txt`). No other fonts.

| Role | Font · weight · size | Example |
|---|---|---|
| Page title | Sans 600 · 28 · tracking -0.015em | Clean up |
| Hero sentence | Sans 500 · 28 | Running well. |
| Dialog title | Sans 600 · 22 | Remove 12.0 GB for good? |
| Row title | Sans 600 · 15 | Old package downloads |
| Body | Sans 400 · 14 · line-height 1.55 | Nothing changes until you review it. |
| Label | Sans 400 · 13 · `text-3` | Selected to remove |
| Big number | Mono 400 · 32–42 · tracking -0.02em | 12.6 GB |
| Inline number | Mono 400 · 13–16 | 2.1 GB · 14% · 18.4 s |

## Shape and space

- Radius: **12** panels · **8** buttons, inputs, tiles · **4** chips · **99** switches.
- Spacing scale: **4 · 8 · 12 · 16 · 24 · 32**. Page padding **32 / 40**, gap between sections **24**.
- Touch targets at least **44 px**.

## Contrast pairs (text on the surfaces it actually appears on)

Ratios from the approved board. Light values are re-checked by the contrast test on every CI run; dark values
get the same test when dark mode is built.

| Pair | Light | Dark |
|---|---|---|
| text on bg | 16.23 | 15.44 |
| text on surface | 17.58 | 14.24 |
| text-2 on surface | 10.38 | 10.38 |
| text-3 on surface | 6.05 | 6.91 |
| text-3 on bg | 5.58 | 7.49 |
| text-3 on surface-2 | 5.35 | 6.12 |
| accent on surface | 6.36 | 6.75 |
| accent on bg | 5.88 | 7.32 |
| on-accent on accent | 6.36 | 7.48 |
| warning on surface | 6.56 | 7.81 |
| warning on warning-bg | 5.57 | 6.27 |
| danger-text on surface | 6.54 | 6.96 |
| danger-text on bg | 6.04 | 7.54 |
| on-danger on danger | 6.54 | 5.44 |
| text-2 on chip | 8.94 | 8.71 |
| text on nav-active | 13.93 | 11.95 |

Every pair is at least 4.5:1 (WCAG 2.x AA for normal text).
