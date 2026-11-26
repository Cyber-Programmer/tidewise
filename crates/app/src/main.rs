mod apps_window;
#[cfg(debug_assertions)]
mod debug_entry;
mod install_window;
mod search_window;
mod update_window;

slint::include_modules!();

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
    install.set_updates(update_window::start(&window, apps_for_updates, search));
    window.run()
}
