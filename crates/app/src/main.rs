mod apps_window;
#[cfg(debug_assertions)]
mod debug_entry;
mod install_window;
mod job_clock;
mod search_window;
mod update_window;

use std::time::Duration;

use slint::CloseRequestResponse;
use tidewise_app::job::{CLOSE_ANYWAY, KEEP_OPEN, close_warning};

slint::include_modules!();

/// How long leaving the app waits for a stopped list refresh: SIGTERM, 2 s, SIGKILL, then a margin.
const REFRESH_EXIT_WAIT: Duration = Duration::from_secs(5);

fn main() -> Result<(), slint::PlatformError> {
    let window = MainWindow::new()?;
    window.set_app_name(tidewise_core::app_name().into());
    let apps = apps_window::start(&window);
    let apps_for_updates = apps.clone();
    let install = install_window::start(&window, apps);
    #[cfg(debug_assertions)]
    debug_entry::start(&install);
    let search = search_window::start(&window, install.clone());
    install.set_search(search.clone());
    let updates = update_window::start(&window, apps_for_updates, search);
    install.set_updates(updates.clone());
    ask_before_closing(&window, install.clone(), updates.clone());
    let result = window.run();
    // A refresh left running would have nobody to end it.
    install.stop_refresh();
    updates.stop_refresh();
    result
}

/// Closing the window during a job asks first: the helper keeps running without the window.
fn ask_before_closing(
    window: &MainWindow,
    install: install_window::Install,
    updates: update_window::Updates,
) {
    let bridge = window.global::<CloseBridge>();
    bridge.set_keep_text(KEEP_OPEN.into());
    bridge.set_close_text(CLOSE_ANYWAY.into());
    let weak = window.as_weak();
    bridge.on_close_anyway(move || {
        if let Some(window) = weak.upgrade() {
            // The last window is gone, so `run` returns. A window that cannot be hidden stays open.
            let _ = window.hide();
        }
    });
    let weak = window.as_weak();
    window.window().on_close_requested(move || {
        let warning = close_warning(install.job_running(), updates.job_running());
        match (weak.upgrade(), warning) {
            (Some(window), Some(warning)) => {
                let bridge = window.global::<CloseBridge>();
                bridge.set_warning(warning.into());
                bridge.set_open(true);
                CloseRequestResponse::KeepWindowShown
            }
            _ => CloseRequestResponse::HideWindow,
        }
    });
}
