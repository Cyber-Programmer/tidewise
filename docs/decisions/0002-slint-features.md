# 0002 — Slint 1.18 without default features (no system tray)

- Status: Accepted
- Date: 2026-11-09
- Decided by: the human (option "B" in task 2.3)

## Context

Slint's default features include a system-tray icon, which on Linux pulls in the `ksni` crate. Tidewise has no
tray icon. Every extra crate is code we have to trust and keep updated.
Since recent versions Slint defaults to the Winit backend with the FemtoVG (GPU) renderer; Qt is opt-in.
Slint's docs mark `compat-1-18` as mandatory when default features are turned off.

## Options

| Option | Result |
|---|---|
| A. Default features | Works, but includes the tray icon code |
| **B. Defaults minus tray** | Same behaviour, fewer dependencies |
| C. Software renderer only | No GPU needed, but can't compare renderers later |

## Decision

Option B, in `crates/app/Cargo.toml`:

```
slint = { version = "1.18.1", default-features = false, features = ["compat-1-18", "std", "accessibility",
          "backend-winit", "renderer-femtovg", "renderer-software"] }
slint-build = "1.18.1"   # build-dependency only
```

UI lives in `crates/app/ui/*.slint`, compiled by `build.rs` (not the `slint!` macro), so it scales to many screens.

## Consequences

- Verified: `cargo tree -p tidewise-app -i ksni` finds nothing.
- Both renderers are available: `SLINT_BACKEND=winit-software` vs `winit-femtovg`. Compare memory with real
  numbers in task 11.4 before deciding whether to drop one.
- Upgrading Slint to a new minor version means checking for a new `compat-1-NN` feature.
