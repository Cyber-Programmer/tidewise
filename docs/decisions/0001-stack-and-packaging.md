# 0001 — Stack and packaging: Rust + Slint, shipped as an AUR package

- Status: Accepted
- Date: 2026-11-08 (recorded 2026-11-09)
- Decided by: the human (plan v2, decisions D1, D2, D4)

## Context

Tidewise is being rebuilt from zero after the first version grew faster than the human could understand it.
The first version targeted many distros as an AppImage, which forced workarounds: an AppImage can't install
system files, so the root helper and its polkit policy had to be copied into place at first run.
The rebuild targets **Arch Linux only**, so the normal Arch way of installing software is available.

## Options

| Option | For | Against |
|---|---|---|
| **Rust + Slint** | Memory-safe, small and fast; Slint already prototyped and liked; GPL-compatible licence | Smaller ecosystem than GTK/Qt; custom look has to be designed |
| Rust + GTK4/libadwaita | Native GNOME look, mature accessibility | Drops the chosen Hybrid design; looks foreign on KDE |
| Tauri (web UI) | Familiar web tech | Heavier (WebView), larger attack surface for a root-adjacent tool |
| Packaging: AppImage | One file, any distro | Can't install the helper/polkit policy properly; no clean uninstall |
| **Packaging: AUR PKGBUILD** | pacman installs and removes app, helper and policy cleanly; normal for Arch | Arch-only (fine for now); users need an AUR helper or `makepkg` |

## Decision

**Rust (edition 2024) + Slint**, keeping the "Hybrid" design direction. Ship as an **AUR PKGBUILD**.

## Consequences

- The helper and polkit policy are installed by pacman to standard paths (M3), so no first-run copying.
- Other distros mean new packaging later; the code keeps distro specifics inside `crates/pacman`.
- Licence: our code is GPL-3.0-or-later; Slint is used under its GPLv3 option, so the combined app is GPLv3.
