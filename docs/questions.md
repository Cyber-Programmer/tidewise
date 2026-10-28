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
