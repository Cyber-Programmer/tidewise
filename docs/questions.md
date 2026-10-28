# Questions for the human

Written during unattended runs. Each entry: the question, my recommended answer, what I did meanwhile.

## T4 — no German locale for a real non-English pacman capture
`locale -a` has no `de_*` entry, and generating one needs root (`locale-gen`), which is off limits overnight.
Recommended: on a machine with `de_DE.UTF-8`, run `LC_ALL=de_DE.UTF-8 pacman -Qi asar > tests/fixtures/pacman/Qi-asar-de.txt`
and replace my hand-written sample (the test only needs the parser to refuse it).
Meanwhile: `Qi-asar-de.txt` is hand-written and labelled so in `tests/fixtures/pacman/README.md`.

## M1-T0 — cargo-deny failed once Slint joined the workspace
Slint's tree brought: licence BSL-1.0 (clipboard-win, error-code) and four "unmaintained" advisories
(RUSTSEC-2026-0192 ttf-parser, -0370 zvariant, -0371 zbus_names, -0372 zvariant_derive). No vulnerabilities.
Recommended: keep the allowances in `deny.toml` (BSL-1.0 is GPL-compatible), re-check after each Slint upgrade.
Meanwhile: added exactly those licence and advisory entries to `deny.toml`, each with a reason; nothing else loosened.
`cargo audit` already passes with warnings only (also lists bincode RUSTSEC-2025-0141, unmaintained).

## M1-T3 — details artboards are taller than 960 px; extra colours; chart width
1. `Performance--details.png` (and `--memory-details`) are 1360×1005: the artboard grows with its content.
   Scoring a 960 px render against them stretches the image and gives bogus scores. I added an optional
   `--screenshot … --height <px>` flag (default 960, so the T2 contract holds) and `tools/design_check.sh`
   passes each reference's height. Recommended: keep it; in the real app that content will scroll.
2. The artboards use shades that `tokens.md` does not list (#1E232C active nav, #B4BAC5 nav text, #1C222B selected tab,
   #1F242D sidebar border, #C9CED6 soft text, #222832, #1E232B, #2A303B, #1A1F27). They are in `Theme` under a
   separate comment. Recommended: add them to `tokens.md` (or snap the design to existing tokens).
3. Slint scales a `Path` uniformly, so the chart is drawn by Rust at the real pixel width, computed from the window size
   (`view::chart_width`). Recommended: replace with a Slint-side solution if a later Slint adds non-uniform viewboxes.
