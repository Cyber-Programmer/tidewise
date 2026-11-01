# Test build notes

For people trying an early Tidewise build. It is unfinished: read "Known gaps" before reporting a bug.

## Before you start

- Run it as your normal user. Tidewise never needs `sudo`; if something asks for your password, stop and tell us.
- Download/build: `chmod +x Tidewise-<version>-x86_64.AppImage && ./Tidewise-<version>-x86_64.AppImage`
  (from source: `cargo build --release -p tidewise-app`, then `./target/release/tidewise`).
- Try `--demo` to see the design's sample data instead of your own: `./Tidewise-….AppImage --demo`.
- Optional: `--add-to-menu` puts Tidewise in your app menu (writes `io.github.tidewise.desktop` and an icon under
  `~/.local/share`); `--remove-from-menu` removes exactly those two files.

## What to try

1. **Home**: does it open quickly, and do the numbers look right for your machine?
2. **Performance**: CPU, memory and disk figures move about once a second. Compare with `top`, `free -h` or your
   system monitor. Minimise or hide the window and check the app goes quiet.
3. **Processes**: the list, the filter tabs, search, selecting a row. The "End …" button does nothing yet (see below).
4. **Storage**: wait for the scan, then go deep: open folders, use the breadcrumb, use the keyboard (arrows,
   Enter, Space, Backspace, Alt+Left), tick items and move them to the Trash, then press Undo.
   Check the sizes against `du -sh <folder>` (hard links are counted once, other file systems are skipped).
   Items you move to the Trash must show up in your file manager's Trash and be restorable from there.
5. **Window**: resize it down to the minimum (900×600) and tab through controls with the keyboard.
6. Different machines help most: other distros, Wayland and X11, a very full or very empty home folder, huge
   folders, network drives mounted under your home.

## Known gaps (not bugs)

- Only Home, Performance, Processes and Storage exist. Clean up, Apps, Startup apps, Services, History and
  Settings show "Coming in a later version". Home's scan button says "Scan coming soon".
- Processes: "End task" does nothing yet.
- Nothing is removed except what you move to the Trash from Storage.
- The AppImage bundles only Tidewise, not system libraries, so on a minimal system it may fail to start with a
  missing-library message. That message is useful, please send it.
- Text in non-Western scripts may not render with the default software renderer; try
  `TIDEWISE_RENDERER=gpu ./Tidewise-….AppImage`.
- No update check, no network access, no telemetry.
- There is no `--version` or `--help` flag yet (an unknown option prints `Error: "unknown option …"`). Tell us the
  file name of the AppImage and when you got it instead.

## How to report a problem

Open an issue at <https://github.com/Cyber-Programmer/tidewise/issues> (the repository named in `Cargo.toml`).
Please include:

- what you did, what you expected, what happened (a screenshot helps);
- the AppImage file name (it contains the version) or the git commit if you built it;
- your distro and version (`cat /etc/os-release`), desktop and session (`echo $XDG_CURRENT_DESKTOP $XDG_SESSION_TYPE`);
- the terminal output if you started it from a terminal;
- for wrong sizes: the folder, the output of `du -sx <folder>`, and whether it has hard links, bind mounts or symlinks.

Do not attach private files or file names you don't want public. A security problem (anything that deletes or
changes things it should not) should be reported privately as described in `SECURITY.md`, not in a public issue.
