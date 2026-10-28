//! Tidewise UI. The toolkit is chosen in docs/decisions/0001-ui-toolkit.md.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, Timer, TimerMode};

mod chart;
mod cli;
mod demo;
mod live;
mod perf;
mod screenshot;
mod view;

slint::include_modules!();

/// Software renderer unless `TIDEWISE_RENDERER=gpu` asks for FemtoVG (OpenGL).
fn select_renderer() -> Result<(), slint::PlatformError> {
    let renderer = match std::env::var("TIDEWISE_RENDERER").as_deref() {
        Ok("gpu") => "femtovg",
        _ => "software",
    };
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name(renderer.into())
        .select()
}

fn page_of(screen: &str) -> Result<i32, String> {
    match screen {
        "performance" => Ok(4),
        other => Err(format!("unknown screen '{other}'")),
    }
}

fn show_demo(ui: &MainWindow, state: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let view = perf::View::from_state(state)?;
    view::apply_perf(ui, &demo::performance());
    view::apply_view(ui, &view);
    Ok(())
}

fn run_screenshot(shot: &cli::Screenshot) -> Result<(), Box<dyn std::error::Error>> {
    let page = page_of(&shot.screen)?;
    let window = screenshot::install(shot.height)?;
    let ui = MainWindow::new()?;
    ui.global::<Shell>().set_page(page);
    ui.global::<Shell>().set_distro_name("Arch Linux".into());
    ui.global::<Shell>()
        .set_distro_detail("KDE Plasma 6 · pacman + Flatpak".into());
    show_demo(&ui, shot.state.as_deref())?;
    ui.show()?;
    screenshot::save(&window, shot.height, std::path::Path::new(&shot.out))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = cli::parse(std::env::args().skip(1))?;
    if let Some(shot) = &args.screenshot {
        return run_screenshot(shot);
    }
    select_renderer()?;
    let ui = MainWindow::new()?;
    ui.global::<Shell>().on_navigate({
        let weak = ui.as_weak();
        move |page| {
            if let Some(ui) = weak.upgrade() {
                ui.global::<Shell>().set_page(page);
            }
        }
    });
    ui.global::<PerfState>().on_open_processes({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                ui.global::<Shell>().set_page(5);
            }
        }
    });

    if args.demo {
        ui.global::<Shell>().set_distro_name("Arch Linux".into());
        ui.global::<Shell>()
            .set_distro_detail("KDE Plasma 6 · pacman + Flatpak".into());
        show_demo(&ui, None)?;
        return Ok(ui.run()?);
    }

    let sampler = Rc::new(RefCell::new(live::Sampler::new()));
    view::apply_perf(&ui, &sampler.borrow_mut().sample());

    // One sample per second; skip the work while the window is hidden.
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), {
        let weak = ui.as_weak();
        move || {
            let Some(ui) = weak.upgrade() else { return };
            if !ui.window().is_visible() {
                return;
            }
            view::apply_perf(&ui, &sampler.borrow_mut().sample());
        }
    });

    Ok(ui.run()?)
}
