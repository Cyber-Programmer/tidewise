# Tauri spike

The same Performance screen as `spikes/slint`, built with Tauri 2: plain HTML/CSS/JS in `ui/`
(no framework), and one Rust command, `sample`, that returns the `tidewise-core` `LiveStats`
numbers as JSON once per second. Not part of the main workspace: own `Cargo.lock` and `target/`.

## Build

`tauri-cli` is not installed on the build machine (it is a large extra compile), so this uses the
command `cargo tauri build --no-bundle` runs underneath:

```
cd spikes/tauri
cargo build --release --features custom-protocol   # -> target/release/tidewise-spike-tauri
```

`custom-protocol` makes the binary serve the bundled `ui/` folder instead of a dev server.
With `tauri-cli` installed, `cargo tauri build --no-bundle` should be equivalent.

System libraries needed: `webkit2gtk-4.1`, `gtk3` (found on this Arch machine; on Ubuntu 22.04 the
packages are `libwebkit2gtk-4.1-dev` and `libgtk-3-dev`).

First build took about 5 minutes.

## Run

```
./target/release/tidewise-spike-tauri
```

## Known gaps (it is a spike)
- Geist / Geist Mono are not bundled; system fonts stand in.
- No sidebar icons, no "Show technical details", no explanation text.
- Sampling pauses when `document.hidden` is true (window minimised or on another workspace),
  which the browser engine reports for free; Slint has no equivalent signal.
- Process names come from `/proc/<pid>/comm`, cut at 15 characters.
- The app runs a WebKitGTK helper process (`WebKitWebProcess`, plus a network process), so memory
  has to be measured over the whole process tree (see `spikes/measure.sh`).
