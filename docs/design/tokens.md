# Design tokens

Source: the "Tokens & components" artboard on the Tidewise design canvas. Use only these.

## Colour

| Token | Dark (default) | Light |
| --- | --- | --- |
| bg | #0D0F13 | #F5F6F8 |
| sidebar | #111419 | #ECEEF2 |
| surface | #161A21 | #FFFFFF |
| surface-sunk | #12161C | #F0F2F5 |
| border | #232933 | #E1E4EA |
| text | #ECEEF2 | #14171C |
| text-2 | #A9B0BC | #4A5260 |
| text-3 | #808897 | #5F6775 |
| accent (fill) | #5DDCB0, text on it #052B1F | #12B886, text on it #052B1F |
| accent (as text) | #5DDCB0 | #0B7A5A |
| accent tint bg | #173A2F | #DDF5EC |
| warn | #F2B84B, tint #3A2E14 | #9A6700, tint #FFF3D6 |
| danger | #FF8A80, text on it #3A0B07, tint #3A1714 | #C73A2E, text on it #FFFFFF |
| data: cpu / memory / disk / network | #7AA7FF / #F2B84B / #C3A2FF / accent | #2F5FD0 / #9A6700 / #7048C8 / #0B7A5A |
| source chips: System / Flatpak / AppImage / Snap | #7AA7FF on #1B2A45 / #C3A2FF on #2C2342 / #F2B84B on #3A2E14 / #C9CED6 on #2A303B | |

## Type
Geist (UI) and Geist Mono (all numbers, paths, commands). Fonts are bundled, never fetched.

| Role | Size / weight |
| --- | --- |
| display number | Mono 34 |
| h1 page title | 26 / 650, letter-spacing −0.02em |
| h2 dialog title | 22 / 650 |
| row title | 15 / 600 |
| body | 14 / 400 |
| small | 13 |
| code / caption | Mono 12, caption 12 |

## Spacing, radius, size
- Spacing scale: 4, 8, 12, 16, 20, 24, 28, 36
- Radius: chip 6 · button/input 10 · row 14 · card 16 · hero 18 · dialog 20 · pill 99
- Buttons: 40 px tall (toolbar), 44 px (dialogs/primary), 46 px (big primary)
- Sidebar 220 px; content max width 1120 px, centred
- Minimum window 900 × 600; under 1100 px the sidebar becomes a 60 px icon rail

## Motion and effects
- 120–200 ms ease-out; none when reduced motion is requested
- No shadows (except dialogs/toasts over a scrim), no blur, no glow, no gradients
- Charts redraw at most once per second; pause when hidden

## Copy rules
- Plain language first; technical detail behind "Show technical details"
- Destructive buttons name the thing ("Uninstall GIMP"), never "OK"
- Every error says: what happened, whether anything broke, the one next step
