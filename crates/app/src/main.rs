mod apps_window;
#[cfg(debug_assertions)]
mod debug_entry;
mod install_window;
mod search_window;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let window = MainWindow::new()?;
    window.set_app_name(tidewise_core::app_name().into());
    let apps = apps_window::start(&window);
    let install = install_window::start(&window, apps);
    #[cfg(debug_assertions)]
    debug_entry::start(&install);
    let search = search_window::start(&window, install.clone());
    install.set_search(search);
    window.run()
}
