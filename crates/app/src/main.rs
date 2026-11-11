mod apps_window;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let window = MainWindow::new()?;
    window.set_app_name(tidewise_core::app_name().into());
    apps_window::start(&window);
    window.run()
}
