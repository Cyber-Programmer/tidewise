# Tidewise

[![CI](https://github.com/Cyber-Programmer/tidewise/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Cyber-Programmer/tidewise/actions/workflows/ci.yml)

## What it is

An open-source "PC manager" for Linux, for people switching from Windows or macOS. It cleans junk,
shows what uses disk space, lists and removes apps from every source, and shows live performance and
processes, in plain language. Technical details are one switch away.

It works with pacman, apt and dnf packages, and with Flatpak, Snap and AppImage apps.

**It is early software. Nothing is released yet, and nobody except its author has used it.** Treat every
button as untested by a person.

## Status

What exists on `main` today. "Built" means the screen is in the app, wired to real data, and covered by
automated tests. It does not mean a person has tried it. Details: [CHANGELOG.md](CHANGELOG.md).

| Page | Status | What works, what is missing |
| --- | --- | --- |
| Home | Built | Health score with reasons, suggestions, live processor, memory, drive and network tiles, a banner when the drive is nearly full. |
| Clean up | Partial | Finds junk and shows a confirm screen listing exactly what will go. Anything that needs admin access shows "needs admin access (coming soon)". |
| Apps | Partial | One list for all six sources, uninstall with a dry run and a confirm screen. Only what needs no admin access can be removed today. |
| Storage | Built | What uses disk space, biggest files; your own files go to the Trash, with Undo. |
| Performance | Built | Live numbers, refreshed at most once a second. The "sensor not available" state is missing. |
| Processes | Built | Live list, "End task" with a confirm screen and a force option. |
| Startup apps | Built | Turn login items off and on (a per-user copy, with a backup). |
| Services | Partial | Read-only list of systemd services, failed ones first. Start, stop and enable need admin access (not built). |
| History | Built | What Tidewise changed, by day, with Undo where the Trash allows it, and a "copy for a help forum" button. |
| Health | Built | Each rule, its points, and a button to the screen that fixes it. |
| Search | Partial | Ctrl K searches pages, actions, settings, apps and running programs. Files are not searched. |
| Settings | Partial | Theme (follow system, dark, light) and an update-check switch that is stored but does nothing, because the app has no network code. |

Also not built: the admin helper and its polkit policy (so nothing that needs root works yet), updates,
and an AppImage release. Seven screens do not yet meet the project's design-match target; the numbers
are in [run 07's report](docs/loop/archive/run-07-report.md).

## Features

These pictures are drawn with `--screenshot` from built-in sample data (no real files or apps).

| | |
| --- | --- |
| ![Home](docs/screenshots/home.png) | ![Clean up](docs/screenshots/clean-up.png) |
| ![Apps](docs/screenshots/apps.png) | ![Storage](docs/screenshots/storage.png) |
| ![Performance](docs/screenshots/performance.png) | ![Processes](docs/screenshots/processes.png) |
| ![Startup apps](docs/screenshots/startup.png) | ![Services](docs/screenshots/services.png) |
| ![History](docs/screenshots/history.png) | ![Health](docs/screenshots/health.png) |
| ![Settings](docs/screenshots/settings.png) | |

(The footer note under "Biggest files" in the Storage picture is cut off; that is a known bug.)

## Install

**There is no release yet, so there is no AppImage to download.** Build it from source.

You need a stable Rust toolchain (built here with cargo 1.98.0; CI uses the current stable) and the
fontconfig development files:

- Arch Linux (the only system this has been built and run on): `pacman -S --needed base-devel rust fontconfig pkgconf`
- Ubuntu 22.04 and newer: `sudo apt install build-essential pkg-config libfontconfig1-dev`, plus Rust from
  [rustup.rs](https://rustup.rs). The CI build runs on Ubuntu 22.04; its result has not been checked by the author.

Then build:

```
cargo build --release -p tidewise-app
```

The program is `target/release/tidewise`.

To build an AppImage on your own machine (not a release; needs
[appimagetool](https://github.com/AppImage/appimagetool) on your PATH):

```
./packaging/appimage/build.sh
```

It writes `dist/Tidewise-<version>-x86_64.AppImage`. To try it: `chmod +x` it and run it.

## Run

```
./target/release/tidewise
```

shows your real data (read-only until you confirm an action),

```
./target/release/tidewise --demo
```

shows the design's sample data, and

```
./target/release/tidewise --screenshot home --out home.png
```

draws one screen to a PNG file without opening a window. To use the GPU renderer instead of the
software one:

```
TIDEWISE_RENDERER=gpu ./target/release/tidewise
```

Measured with the software renderer on one machine, 40 seconds after start, for eight pages: 28.8 MB to
34.2 MB of memory (PSS) and 0.3 % to 1.0 % of one core. The goal is at most 40 MB and under 2 %.
Conditions and the pages not measured: [docs/budgets.md](docs/budgets.md).

## What it never does

- **It does not run as root.** Anything that needs admin access is not built yet and says so.
- **It does not run a shell.** Programs are started directly with a list of arguments.
- **It sends nothing anywhere.** Tidewise's own code has no network calls and there is no telemetry.
  (Checked with a text search of `crates/`; a test also checks that the `core` crate has no network
  dependency.)
- **It does not delete your own files.** They go to the Trash. Caches and logs are removed only after a
  confirm screen that lists exactly what will go.
- **It does not touch protected things:** essential packages, the desktop session, audio, network, the
  journal. The code refuses, not only the buttons.

These are the project's rules (`CLAUDE.md`). Many have tests, not every one is proven by a test, and
nobody outside the project has reviewed them yet.

## Security

Threat model, fuzzing results and the penetration-test checklist are in
[docs/security/](docs/security/). To report a vulnerability privately, read [SECURITY.md](SECURITY.md).

## Contributing

How the project is built and tested (tests first, small commits, security review) is in
[docs/ENGINEERING.md](docs/ENGINEERING.md). To run the same checks as the CI pipeline on your machine:

```
tools/ci-local.sh
```

Read [CONTRIBUTING.md](CONTRIBUTING.md) before you open a pull request.

## Licence

GPL-3.0-or-later. See [LICENSE](LICENSE).
