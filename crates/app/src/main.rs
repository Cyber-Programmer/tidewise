//! Tidewise UI. The toolkit is chosen in docs/decisions/0001-ui-toolkit.md.
#![deny(unsafe_code)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, Timer, TimerMode};

mod chart;
mod cli;
mod demo;
mod home;
mod live;
mod nav;
mod perf;
mod procs;
mod screenshot;
mod storage;
mod storage_live;
mod tiles;
mod view;

#[cfg(test)]
#[path = "../tests/support/fixture.rs"]
mod fixture;

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

/// Where `.desktop` files that give apps their display names live.
/// Distro name and the line under it, from `/etc/os-release`, `XDG_CURRENT_DESKTOP` and the tools found.
fn shell_info() -> (String, String) {
    let release = std::fs::read_to_string("/etc/os-release")
        .map(|t| tidewise_core::distro::parse_os_release(&t))
        .ok();
    let name = release
        .map(|r| r.name)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Linux".into());
    let desktop =
        tidewise_core::distro::parse_desktop(std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref());
    let tools = tidewise_core::distro::detect_tools(std::path::Path::new("/"));
    (name, home::distro_detail(&desktop, &tools))
}

fn desktop_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = vec![std::path::PathBuf::from("/usr/share/applications")];
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(home);
        dirs.push(home.join(".local/share/applications"));
        dirs.push(home.join(".local/share/flatpak/exports/share/applications"));
    }
    dirs.push("/var/lib/flatpak/exports/share/applications".into());
    dirs
}

fn page_of(screen: &str) -> Result<i32, String> {
    match screen {
        "home" => Ok(0),
        "clean-up" => Ok(1),
        "apps" => Ok(2),
        "storage" => Ok(3),
        "startup" => Ok(6),
        "services" => Ok(7),
        "history" => Ok(8),
        "settings" => Ok(9),
        "performance" => Ok(4),
        "processes" => Ok(5),
        other => Err(format!("unknown screen '{other}'")),
    }
}

/// What the Processes screen shows and what the user picked there.
struct ProcsCtl {
    data: procs::ProcData,
    filter: procs::Filter,
    search: String,
    selected: String,
    /// Ids of the rows on screen, in order.
    shown: Vec<String>,
}

impl ProcsCtl {
    fn new(data: procs::ProcData) -> Self {
        let selected = data.rows.first().map(|r| r.id.clone()).unwrap_or_default();
        ProcsCtl {
            data,
            filter: procs::Filter::All,
            search: String::new(),
            selected,
            shown: Vec::new(),
        }
    }

    fn refresh(&mut self, ui: &MainWindow) {
        self.shown = view::apply_procs(ui, &self.data, self.filter, &self.search, &self.selected);
    }
}

fn wire_procs(ui: &MainWindow, ctl: &Rc<RefCell<ProcsCtl>>) {
    let state = ui.global::<ProcState>();
    state.on_pick({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |i| {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            if let Some(id) = c.shown.get(i.max(0) as usize).cloned() {
                c.selected = id;
                c.refresh(&ui);
            }
        }
    });
    state.on_set_filter({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |i| {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            c.filter = procs::Filter::from_index(i);
            c.refresh(&ui);
        }
    });
    state.on_search_edited({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |text| {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            c.search = text.to_string();
            c.refresh(&ui);
        }
    });
}

fn show_demo(
    ui: &MainWindow,
    screen: &str,
    state: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    match screen {
        "processes" => {
            let view = procs::View::from_state(state)?;
            let mut ctl = ProcsCtl::new(demo::processes());
            ctl.filter = view.filter;
            ctl.selected = view.selected;
            ctl.refresh(ui);
            ui.global::<ProcState>().set_details(view.details);
        }
        "storage" => {
            let downloads = match state {
                None => false,
                Some("downloads") => true,
                Some("live") => return Ok(()), // drawn by `run_screenshot` from a real scan
                Some(other) => return Err(format!("unknown storage state '{other}'").into()),
            };
            view::apply_storage(ui, &demo::storage(downloads));
        }
        "home" => {
            if let Some(other) = state {
                return Err(format!("unknown home state '{other}'").into());
            }
            view::apply_home(ui, &demo::home());
        }
        _ => {
            let view = perf::View::from_state(state)?;
            view::apply_perf(ui, &demo::performance());
            view::apply_view(ui, &view);
        }
    }
    Ok(())
}

fn run_screenshot(shot: &cli::Screenshot) -> Result<(), Box<dyn std::error::Error>> {
    let page = page_of(&shot.screen)?;
    let window = screenshot::install(shot.width, shot.height)?;
    let ui = MainWindow::new()?;
    view::wire_tiles(&ui);
    ui.global::<Shell>().set_page(page);
    ui.global::<Shell>().set_distro_name("Arch Linux".into());
    ui.global::<Shell>()
        .set_distro_detail("KDE Plasma 6 · pacman + Flatpak".into());
    show_demo(&ui, &shot.screen, shot.state.as_deref())?;
    if shot.screen == "storage" && shot.state.as_deref() == Some("live") {
        live_storage_screenshot(&ui, shot)?;
    }
    ui.show()?;
    screenshot::save(
        &window,
        shot.width,
        shot.height,
        std::path::Path::new(&shot.out),
    )
}

/// `--screenshot storage --state live`: scan `TIDEWISE_STORAGE_ROOT` for real (the variable only
/// counts here), wait for the scans, open `--open` and leave the result on the window.
fn live_storage_screenshot(
    ui: &MainWindow,
    shot: &cli::Screenshot,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = cli::storage_root(true, std::env::var_os("TIDEWISE_STORAGE_ROOT").as_deref())
        .ok_or("--state live needs TIDEWISE_STORAGE_ROOT set to an absolute folder")?;
    let mut ctl = storage_live::Ctl::with_home(root.clone());
    ctl.enter(ui);
    wait_for_scans(&mut ctl, ui);
    if let Some(rel) = &shot.open {
        let target = root.join(rel);
        if !ctl.open_now(rel_path(rel), ui) {
            return Err(format!("--open: {} is not a folder", target.display()).into());
        }
        wait_for_scans(&mut ctl, ui);
    }
    Ok(())
}

fn rel_path(rel: &str) -> &std::path::Path {
    std::path::Path::new(rel)
}

fn wait_for_scans(ctl: &mut storage_live::Ctl, ui: &MainWindow) {
    loop {
        ctl.poll(ui);
        if ctl.is_idle() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Sampling pauses while nobody can see the window.
fn is_paused(visible: bool, minimized: bool) -> bool {
    !visible || minimized
}

/// Only Performance (top lists) and Processes need the process table; everything else gets the
/// cheap system-wide sample.
fn needs_processes(page: i32) -> bool {
    matches!(page, 4 | 5)
}

/// Everything live mode keeps between ticks.
struct Live {
    sampler: live::Sampler,
    desktop: std::collections::HashMap<String, String>,
}

/// Draw the page on screen from the latest sample. Other pages are not touched: the process
/// list in particular reads `/proc` for every process, so it is only built when it is shown.
fn show_page(
    ui: &MainWindow,
    live: &mut Live,
    ctl: &mut ProcsCtl,
    storage: &mut storage_live::Ctl,
) {
    match ui.global::<Shell>().get_page() {
        3 => storage.poll(ui),
        0 => {
            live.sampler.refresh_biggest(&live.desktop);
            view::apply_home(ui, &live.sampler.home());
        }
        4 => {
            if let Some(perf) = live.sampler.perf() {
                view::apply_perf(ui, &perf);
            }
        }
        5 => {
            ctl.data = live.sampler.processes(&live.desktop);
            if !ctl.data.rows.iter().any(|r| r.id == ctl.selected) {
                ctl.selected = ctl
                    .data
                    .rows
                    .first()
                    .map(|r| r.id.clone())
                    .unwrap_or_default();
            }
            ctl.refresh(ui);
        }
        _ => {}
    }
}

/// `--add-to-menu` / `--remove-from-menu`: files go in the user's own data folder only.
fn app_menu(action: cli::MenuAction) -> Result<(), Box<dyn std::error::Error>> {
    use tidewise_core::appmenu;
    let data = appmenu::data_home().ok_or("cannot find your data folder (HOME is not set)")?;
    match action {
        cli::MenuAction::Add => {
            // Inside an AppImage the running binary is a temporary mount; the launcher must start the AppImage.
            let exe = std::env::var_os("APPIMAGE")
                .map(std::path::PathBuf::from)
                .map_or_else(std::env::current_exe, Ok)?;
            appmenu::install(&data, &exe)?;
            println!("Tidewise added to your app menu ({}).", data.display());
        }
        cli::MenuAction::Remove => {
            appmenu::remove(&data)?;
            println!("Tidewise removed from your app menu.");
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = cli::parse(std::env::args().skip(1))?;
    if let Some(action) = args.menu {
        return app_menu(action);
    }
    if let Some(shot) = &args.screenshot {
        return run_screenshot(shot);
    }
    select_renderer()?;
    let ui = MainWindow::new()?;
    view::wire_tiles(&ui);
    ui.global::<PerfState>().on_open_processes({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                ui.global::<Shell>().invoke_navigate(5);
            }
        }
    });

    if args.demo {
        ui.global::<Shell>().on_navigate({
            let weak = ui.as_weak();
            move |page| {
                if let Some(ui) = weak.upgrade() {
                    ui.global::<Shell>().set_page(page);
                }
            }
        });
        ui.global::<Shell>().set_distro_name("Arch Linux".into());
        ui.global::<Shell>()
            .set_distro_detail("KDE Plasma 6 · pacman + Flatpak".into());
        show_demo(&ui, "performance", None)?;
        show_demo(&ui, "home", None)?;
        show_demo(&ui, "storage", None)?;
        for (open, folder) in [(true, true), (false, false)] {
            let state = ui.global::<StorageState>();
            let weak = ui.as_weak();
            let show = move || {
                if let Some(ui) = weak.upgrade() {
                    view::apply_storage(&ui, &demo::storage(folder));
                }
            };
            if open {
                state.on_open_hint(show.clone());
                state.on_open_block({
                    let show = show.clone();
                    move |_| show()
                });
            } else {
                state.on_jump({
                    let show = show.clone();
                    move |_| show()
                });
            }
        }
        let ctl = Rc::new(RefCell::new(ProcsCtl::new(demo::processes())));
        ctl.borrow_mut().refresh(&ui);
        wire_procs(&ui, &ctl);
        return Ok(ui.run()?);
    }

    let (name, detail) = shell_info();
    ui.global::<Shell>().set_distro_name(name.into());
    ui.global::<Shell>().set_distro_detail(detail.into());
    let mut desktop = std::collections::HashMap::new();
    for dir in desktop_dirs() {
        procs::load_desktop_names(&dir, &mut desktop);
    }
    let live = Rc::new(RefCell::new(Live {
        sampler: live::Sampler::new(),
        desktop,
    }));
    let ctl = Rc::new(RefCell::new(ProcsCtl::new(procs::ProcData {
        subtitle: String::new(),
        rows: Vec::new(),
    })));
    wire_procs(&ui, &ctl);
    let storage = Rc::new(RefCell::new(storage_live::Ctl::new()));
    storage_live::wire(&ui, &storage);
    let sample = {
        let (live, weak) = (live.clone(), ui.as_weak());
        move || {
            let page = weak
                .upgrade()
                .map_or(0, |ui| ui.global::<Shell>().get_page());
            live.borrow_mut().sampler.sample(needs_processes(page));
        }
    };
    if let Some(screen) = &args.page {
        ui.global::<Shell>().set_page(page_of(screen)?);
    }
    sample();
    ui.global::<Shell>().on_navigate({
        let (weak, live, ctl, storage) = (ui.as_weak(), live.clone(), ctl.clone(), storage.clone());
        move |page| {
            if let Some(ui) = weak.upgrade() {
                ui.global::<Shell>().set_page(page);
                if needs_processes(page) {
                    // The last sample may be a light one; the page needs the process table now.
                    live.borrow_mut().sampler.sample(true);
                }
                if page == 3 {
                    storage.borrow_mut().enter(&ui);
                }
                show_page(
                    &ui,
                    &mut live.borrow_mut(),
                    &mut ctl.borrow_mut(),
                    &mut storage.borrow_mut(),
                );
            }
        }
    });
    if ui.global::<Shell>().get_page() == 3 {
        storage.borrow_mut().enter(&ui);
    }
    show_page(
        &ui,
        &mut live.borrow_mut(),
        &mut ctl.borrow_mut(),
        &mut storage.borrow_mut(),
    );

    // One sample per second; nothing while the window is hidden or minimised.
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), {
        let weak = ui.as_weak();
        move || {
            let Some(ui) = weak.upgrade() else { return };
            if is_paused(ui.window().is_visible(), ui.window().is_minimized()) {
                return;
            }
            sample();
            show_page(
                &ui,
                &mut live.borrow_mut(),
                &mut ctl.borrow_mut(),
                &mut storage.borrow_mut(),
            );
        }
    });

    Ok(ui.run()?)
}

#[cfg(test)]
mod tests {
    use super::{is_paused, needs_processes};

    #[test]
    fn only_performance_and_processes_read_the_process_table() {
        assert!(!needs_processes(0), "Home");
        assert!(needs_processes(4), "Performance");
        assert!(needs_processes(5), "Processes");
        assert!(!needs_processes(9), "Settings");
    }

    #[test]
    fn sampling_pauses_when_hidden_or_minimised() {
        assert!(!is_paused(true, false));
        assert!(is_paused(false, false));
        assert!(is_paused(true, true));
        assert!(is_paused(false, true));
    }
}
