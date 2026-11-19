//! A way into the group install for the developer (WBS 6.2). Compiled only when `debug_assertions` is on, which a
//! release build turns off; `crates/app/tests/debug_entry_guard.rs` checks that this is the only file that reads
//! the variable and that `main.rs` includes this module only under `#[cfg(debug_assertions)]`.
//!
//! `TIDEWISE_DEBUG_INSTALL_GROUP=plasma cargo run -p tidewise-app` opens the install screen for the group
//! `plasma` at start. The real way in from Search is WBS 6.12.

use crate::install_window::Install;

pub fn start(install: &Install) {
    if let Ok(group) = std::env::var("TIDEWISE_DEBUG_INSTALL_GROUP") {
        install.open_group(&group);
    }
}
