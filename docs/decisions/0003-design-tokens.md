# 0003 — Design tokens: Hybrid light theme, embedded IBM Plex, tested contrast

- Status: Accepted
- Date: 2026-11-11
- Decided by: the human (task 2.7)

## Context

The human approved the "Hybrid" redesign (Apple-like calm grouping + datasheet-style numbers) on the design
canvas before the rebuild, including a full token board: 21 colours with light and dark values, type roles,
radii, spacing and 16 contrast pairs. The 2.3 window followed the desktop's dark theme and used a system font.

## Decisions

1. **Tokens** are transcribed unchanged into `docs/design/tokens.md` (light + dark) and `crates/app/ui/theme.slint`
   (a `Theme` global, light only). Screens use token names, never raw values.
2. **Light only for now.** The app forces light via `Palette.color-scheme`; dark mode is built in M9 from the
   recorded dark values.
3. **Fonts embedded, unmodified:** IBM Plex Sans 400/500/600 and Plex Mono 400 from IBM's official releases
   (`@ibm/plex-sans@1.1.0`, `@ibm/plex-mono@2.5.0`), ~760 KB in total, plus `OFL.txt`.

   | Option | Why not |
   |---|---|
   | Depend on Arch's `ttf-ibm-plex` | 99.4 MB installed for four weights |
   | System default font | Drops an approved design decision |

   OFL-1.1 allows bundling with GPL software if copyright and licence text ship with it. The files must not be
   subset or modified: "Plex" is a Reserved Font Name.
4. **Contrast is a test.** `tidewise_core::contrast` implements the WCAG 2.x ratio; `crates/app/tests/theme_contrast.rs`
   reads the real colours from `theme.slint` and fails if any of the 16 text pairs drops below 4.5:1.

## Consequences

- `theme.slint` has a strict line format (`out property <color> name: #RRGGBB;`) because the test parses it.
- Changing a colour can break CI on purpose; fix the colour, never the test's threshold.
- Dark mode must add its own pair check before it ships.
- Font weight selection is checked visually in the M2 design check (family names verified with `fc-scan`).
