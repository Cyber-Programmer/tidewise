# Tidewise

A safe, plain-language PC manager for **Arch Linux**: install and update apps, remove what you don't need,
clean up junk, see what uses your disk, and watch your system's performance — all in one window, with the
technical details one switch away.

> **Status: rebuilding from zero — not usable yet.** There is no release. Follow progress in
> [`docs/progress.md`](docs/progress.md). The first public release (v0.1: apps list, search, install, update
> system) is planned after milestone M4 — see [`docs/PLAN.md`](docs/PLAN.md).
> The previous multi-distro version is kept under the git tag `archive/old-v1` for reference only.

## Why another one?

Arch already has GUI package managers (pamac, Octopi, KDE Discover, GNOME Software). Tidewise aims to be the
**all-in-one** tool for people new to Arch, with **plain language** and **safety first**:

- It never does anything riskier than the correct command-line way would.
- It always shows exactly what will change before changing it.
- It refuses known-dangerous patterns (for example partial upgrades or removing your kernel).

The full safety rules are in [`CLAUDE.md`](CLAUDE.md) ("Golden rules").

## Build from source (developers)

Requirements: Rust (stable, via `rustup`) on Arch Linux.

```bash
git clone https://github.com/Cyber-Programmer/tidewise.git
cd tidewise
cargo run -p tidewise-app
```

On Debian/Ubuntu (used by CI) you also need `libfontconfig-dev` and `libxkbcommon-dev`.

Checks that must pass before any change is merged (CI runs the same ones on every pull request):

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all
cargo deny check
```

## How the project works

- Plan, decisions and milestones: [`docs/PLAN.md`](docs/PLAN.md); decision records in [`docs/decisions/`](docs/decisions/)
- Engineering practices (tests first, security testing): [`docs/ENGINEERING.md`](docs/ENGINEERING.md)
- Every change reaches `main` through a pull request with green CI.
- Parts of the code are written with the help of Claude Code; every change is reviewed by a human.

## Security

Please **don't** open a public issue for security problems. See [`SECURITY.md`](SECURITY.md).

## Licence

Copyright © 2026 Bahauddin Hassan.

Tidewise is free software: you can redistribute it and/or modify it under the terms of the
**GNU General Public License as published by the Free Software Foundation, either version 3 of the License,
or (at your option) any later version.** It is distributed in the hope that it will be useful, but WITHOUT ANY
WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
See [`LICENSE`](LICENSE) for the full text.

Tidewise uses [Slint](https://slint.dev) under its GPLv3 licence option.
