# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/). Nothing has been released yet.

## [Unreleased]

Everything below exists in the source today. None of it has been used by a person other than the
author, and nothing needing administrator access works yet (those buttons say
"needs admin access (coming soon)").

### Added

- **Home:** health score with the reasons behind it, suggestions, live processor, memory, drive and
  network tiles, a "drive almost full" banner from 95 % used.
- **Clean up:** finds junk (app caches, logs, package caches, old installers, trash, orphaned
  packages) in groups with a confirm screen that lists exactly what will go; caches that take long to
  download again are a separate group and off by default; the result names what could not be removed
  and why; "Already tidy" when there is nothing to do.
- **Apps:** one list for pacman, apt, dnf, Flatpak, Snap and AppImage apps; uninstall with a dry run
  and a confirm screen; optional removal of a Flatpak app's settings folder (to the Trash).
- **Storage:** what uses disk space (block map, biggest files); user files go to the Trash with Undo.
- **Performance** and **Processes:** live numbers, refreshed at most once a second; "End task" with a
  confirm screen and a force option.
- **Startup apps:** turn login items off and on (a user `Hidden=true` copy with a backup).
- **Services:** read-only list of systemd services, failed ones first; protected units have no
  destructive button; start, stop and enable need admin access (not available yet).
- **History:** what Tidewise changed, by day, with Undo where the Trash allows it and "Copy for a help
  forum" (home folder name scrubbed).
- **Health** page: each rule, its points and a button to the screen that fixes it.
- **Search (Ctrl K):** pages, actions, settings, apps and running programs. Files are not searched.
- **Settings:** dark, light or follow the system theme; an update-check switch that is stored but does
  nothing (there is no network code).
- Empty, loading and error states for the screens above (`docs/states-audit.md` lists what is missing).
- Fuzz targets for every parser of outside data.

### Not built yet

Admin helper and polkit policy, system-wide cleaning and uninstalling that need root, updates,
AppImage packaging as a release, the Settings sections beyond theme and update check.

### Security

- The app never runs as root and never uses a shell. No network access and no telemetry.
