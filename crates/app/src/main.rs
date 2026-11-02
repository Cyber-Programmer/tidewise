//! Tidewise UI. The toolkit is chosen in docs/decisions/0001-ui-toolkit.md.
#![deny(unsafe_code)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, Timer, TimerMode};

mod apps;
mod apps_live;
mod chart;
mod cleaner;
mod cleaner_live;
mod cli;
mod clipboard;
mod demo;
mod endtask;
mod endtask_live;
mod health_live;
mod health_view;
mod history_live;
mod history_view;
mod home;
mod live;
mod nav;
mod perf;
mod procs;
mod records;
mod screenshot;
mod search_index;
mod search_view;
mod services_live;
mod services_view;
mod settings_live;
mod settings_view;
mod startup;
mod startup_live;
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
        "health" => Ok(10),
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
            // `end-confirm|end-frozen|end-protected|end-done` show the End task dialogs on the list.
            let (end, state) = match state {
                Some(s) if s.starts_with("end-") => (Some(s), None),
                other => (None, other),
            };
            if let Some(end) = end {
                show_demo_end(ui, end)?;
            }
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
                Some("sizes") => {
                    // The widest and narrowest sizes the Biggest files column has to hold.
                    let mut data = demo::storage(false);
                    for (row, bytes) in data.big.iter_mut().zip([
                        999_900_000_000,
                        17_500_000_000,
                        840_000_000,
                        12_000,
                        0,
                    ]) {
                        row.size = storage::fmt_item(bytes);
                    }
                    view::apply_storage(ui, &data);
                    return Ok(());
                }
                Some(other) => return Err(format!("unknown storage state '{other}'").into()),
            };
            view::apply_storage(ui, &demo::storage(downloads));
        }
        "clean-up" => show_demo_clean(ui, state)?,
        "apps" => show_demo_apps(ui, state)?,
        "health" => {
            use tidewise_core::health::{report, Inputs};
            let drive = tidewise_core::drive::DriveSpace {
                total: 1_000_000_000_000,
                free: 200_000_000_000,
            };
            let (inputs, open): (Inputs, Vec<usize>) = match state {
                None => (
                    Inputs {
                        drive: Some(drive),
                        junk: Some(12_600_000_000),
                        failed_services: Some(vec!["fwupd-refresh.service".into()]),
                    },
                    vec![0],
                ),
                Some("all-open") => (
                    Inputs {
                        drive: Some(drive),
                        junk: Some(12_600_000_000),
                        failed_services: Some(vec!["fwupd-refresh.service".into()]),
                    },
                    vec![0, 1, 2],
                ),
                Some("healthy") => (
                    Inputs {
                        drive: Some(tidewise_core::drive::DriveSpace {
                            total: 100,
                            free: 60,
                        }),
                        junk: Some(0),
                        failed_services: Some(vec![]),
                    },
                    vec![],
                ),
                Some("unmeasured") => (Inputs::default(), vec![]),
                Some("live") => return Ok(()), // drawn by `run_screenshot` from the real system
                Some(other) => return Err(format!("unknown health state '{other}'").into()),
            };
            view::apply_health(ui, &health_view::screen(&report(&inputs), &open));
        }
        "settings" => {
            // The demo never reads the user's file: the artboard's defaults.
            let st = ui.global::<SettingsState>();
            st.set_about_title(settings_view::about_title(env!("CARGO_PKG_VERSION")).into());
            st.set_about_text("A PC manager for Linux: it cleans junk, shows what uses space, removes apps and watches performance. Free software under the GPL-3.0-or-later. It never runs as root.".into());
            let (theme, update, problem) = match state {
                None => (0, true, ""),
                Some("light") => (2, true, ""),
                Some("off") => (1, false, ""),
                Some("problem") => (0, true, "Your settings file could not be read, so Tidewise is using the defaults. The old file was kept as ~/.config/tidewise/settings.json.broken."),
                Some(other) => return Err(format!("unknown settings state '{other}'").into()),
            };
            ui.global::<Theme>()
                .set_mode(if theme == 2 { 2 } else { 1 });
            st.set_theme(theme);
            st.set_update_check(update);
            st.set_update_note(settings_view::update_note(update).into());
            st.set_problem(problem.into());
        }
        "services" => {
            let st = ui.global::<ServicesState>();
            let list = demo::services();
            let (tab, picked) = match state {
                None | Some("details") => (0, Some("docker.service")),
                Some("running") => (1, Some("docker.service")),
                Some("all") => (2, Some("docker.service")),
                Some("protected") => (2, Some("NetworkManager.service")),
                Some("failed") => (0, Some("fwupd-refresh.service")),
                Some("empty") => (0, None),
                Some("live") => return Ok(()), // drawn by `run_screenshot` from the real system
                Some(other) => return Err(format!("unknown services state '{other}'").into()),
            };
            let screen = services_view::screen(&list, services_view::Tab::from_index(tab), picked);
            if state == Some("empty") {
                view::apply_services(ui, &services_view::Screen::default(), Some((2, "Tidewise could not ask systemd: systemctl could not be run. Nothing was changed.")));
            } else {
                view::apply_services(ui, &screen, None);
            }
            st.set_details(state == Some("details"));
        }
        "startup" => {
            let st = ui.global::<StartupState>();
            let screen = startup::screen(&demo::startup());
            match state {
                None => view::apply_startup(ui, &screen, None, ""),
                Some("details") => {
                    view::apply_startup(ui, &screen, None, "");
                    st.set_details(true);
                }
                Some("toast") => view::apply_startup(
                    ui,
                    &screen,
                    None,
                    &startup::toast_text("Flameshot", false),
                ),
                Some("empty") => view::apply_startup(
                    ui,
                    &startup::Screen::default(),
                    Some((2, "Nothing in your startup folders. Apps that start on their own would be listed here.")),
                    "",
                ),
                Some("live") => {} // drawn by `run_screenshot` from the real folders
                Some(other) => return Err(format!("unknown startup state '{other}'").into()),
            }
        }
        "history" => {
            let st = ui.global::<HistState>();
            let screen = history_view::screen(demo::history(), None, demo::HISTORY_NOW, 0);
            view::apply_history(ui, &screen, None);
            match state {
                None => {}
                Some("details") => st.set_details(true),
                Some("live") => {} // drawn by `run_screenshot` from the real records
                Some("empty") => view::apply_history(
                    ui,
                    &history_view::Screen::default(),
                    Some((1, "Cleaning up, uninstalling, moving files to the Trash and ending apps will be listed here.")),
                ),
                Some(other) => return Err(format!("unknown history state '{other}'").into()),
            }
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

/// The Clean up artboard and the steps of the Clean flow (`--state confirm|cleaning|done|skipped|stopped`).
fn show_demo_clean(ui: &MainWindow, state: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    use tidewise_adapters::cleanup::{default_selection, GroupId};
    let groups = demo::cleanup();
    let mut selected = default_selection(&groups);
    let st = ui.global::<CleanState>();
    match state {
        None => {}
        Some("live") => return Ok(()), // drawn by `run_screenshot` from a real, read-only scan
        Some("details") => st.set_details(true),
        Some("review") => selected.push(GroupId::Trash),
        Some("confirm") => st.set_flow(1),
        Some("cleaning") => {
            st.set_flow(2);
            st.set_progress(0.45);
            st.set_progress_label("Removing: App caches".into());
            st.set_freed_label("5.4 GB of 12.0 GB".into());
        }
        Some("done") => {
            st.set_flow(3);
            st.set_done_title("12.0 GB freed".into());
            st.set_done_text(
                "All 4 groups were cleaned. Your health score went from 82 to 87.".into(),
            );
        }
        Some("skipped") => {
            st.set_flow(4);
            st.set_partial_title("4.0 GB freed, 2 groups skipped".into());
            st.set_partial_text("Old package downloads and old system logs were skipped because the password prompt was closed. Nothing went wrong, and nothing was half-removed.".into());
            st.set_retry_label("Try those 2 again".into());
        }
        Some("stopped") => {
            st.set_flow(4);
            st.set_partial_title("Stopped early".into());
            st.set_partial_text("Tidewise finished the group it was working on and stopped there. Everything removed so far is listed in History; the rest is untouched.".into());
            st.set_retry_label("Clean the rest".into());
        }
        Some("scanning") => {
            st.set_phase(0);
            st.set_subtitle("Looking for things to clean…".into());
            st.set_message("This only reads your files; nothing is removed.".into());
        }
        Some("empty") => {
            st.set_phase(2);
            st.set_subtitle("Scan finished.".into());
            st.set_message("Nothing worth cleaning was found right now.".into());
        }
        Some("error") => {
            st.set_phase(3);
            st.set_subtitle("The scan did not finish.".into());
            st.set_message("Something stopped the scan. Try again from the sidebar.".into());
        }
        Some(other) => return Err(format!("unknown clean-up state '{other}'").into()),
    }
    view::apply_clean(
        ui,
        if matches!(state, Some("empty" | "scanning" | "error")) {
            &[]
        } else {
            &groups
        },
        &selected,
    );
    Ok(())
}

/// The Apps artboard and the steps of the uninstall flow
/// (`--state details|search|confirm|removing|done|admin|blocked|busy|failed|scanning|empty|error|settings|settings-on`).
fn show_demo_apps(ui: &MainWindow, state: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    use tidewise_adapters::source::Blocked;
    use tidewise_adapters::uninstall::Outcome;
    let found = demo::apps();
    let entries: Vec<_> = found.iter().map(|(a, _)| a.clone()).collect();
    let st = ui.global::<AppsState>();
    let mut picked = "org.gimp.GIMP";
    let mut query = "";
    // Demo-only: a Flatpak with a known settings folder (180 MB), unticked or ticked.
    let mut settings: Option<apps::SettingsChoice> = None;
    // The dialog's words are set after the panel, which also fills the confirm texts.
    let mut ending: Option<(&str, Outcome)> = None;
    match state {
        None => {}
        Some("details") => st.set_tech_open(true),
        Some(s @ ("settings" | "settings-on")) => {
            settings = Some(apps::SettingsChoice {
                bytes: 180_000_000,
                delete: s == "settings-on",
            });
        }
        Some("search") => query = "vlc",
        Some("confirm") => st.set_flow(1),
        Some("removing") => {
            st.set_flow(2);
            st.set_d_title("Removing GIMP…".into());
            st.set_d_command("flatpak uninstall org.gimp.GIMP".into());
            st.set_d_text(
                "If GIMP is open, save your work. Tidewise does not close it for you.".into(),
            );
            st.set_d_progress(0.45);
        }
        Some("done") => {
            ending = Some((
                "GIMP",
                Outcome::Removed {
                    items: vec!["org.gimp.GIMP".into()],
                    freed: 732_000_000,
                },
            ));
        }
        Some("admin") => {
            picked = "vlc";
            ending = Some(("VLC", Outcome::NeedsAdmin));
        }
        Some("blocked") => {
            ending = Some((
                "Python",
                Outcome::Blocked(Blocked::RequiredBy(vec![String::new(); 38])),
            ));
        }
        Some("busy") => ending = Some(("LibreOffice", Outcome::Blocked(Blocked::Busy))),
        Some("failed") => {
            ending = Some((
                "GIMP",
                Outcome::Failed {
                    reason: "Flatpak said: error: another operation is in progress.".into(),
                    removed: vec![],
                },
            ));
        }
        Some("scanning") => {
            st.set_phase(0);
            st.set_subtitle("Looking for your apps…".into());
            st.set_message("This only reads what is installed; nothing is changed.".into());
            return Ok(());
        }
        Some("empty") => {
            st.set_phase(2);
            st.set_subtitle("No apps found.".into());
            st.set_message("Tidewise did not find any apps it knows how to list.".into());
            return Ok(());
        }
        Some("error") => {
            st.set_phase(3);
            st.set_subtitle("The search did not finish.".into());
            st.set_message("Something stopped it. Try again from the sidebar.".into());
            return Ok(());
        }
        Some("live") => return Ok(()), // drawn by `run_screenshot` from a real, read-only scan
        Some(other) => return Err(format!("unknown apps state '{other}'").into()),
    }
    st.set_query(query.into());
    let screen = apps::screen(&entries, apps::Filter::All, query, Some(picked));
    view::apply_apps(ui, &screen);
    let panel = screen.picked.as_ref().and_then(|id| {
        found
            .iter()
            .find(|(a, _)| a.id == *id)
            .map(|(a, plan)| (apps::panel_with_settings(a, plan, settings), apps::row(a)))
    });
    view::apply_apps_panel(ui, panel.as_ref().map(|(p, r)| (p, r)));
    if let Some((name, outcome)) = ending {
        let done = apps::done(name, false, &outcome);
        st.set_d_title(done.title.into());
        st.set_d_text(done.text.into());
        st.set_flow(if done.removed { 3 } else { 4 });
    }
    Ok(())
}

/// The End task dialogs with the artboard's Firefox.
fn show_demo_end(ui: &MainWindow, state: &str) -> Result<(), Box<dyn std::error::Error>> {
    let st = ui.global::<EndState>();
    match state {
        "end-confirm" => {
            let t = endtask::confirm("Firefox", 14);
            st.set_title(t.title.into());
            st.set_text(t.text.into());
            st.set_confirm(t.confirm.into());
            st.set_flow(1);
        }
        "end-frozen" => {
            let t = endtask::frozen("Firefox", 5, 2);
            st.set_title(t.title.into());
            st.set_text(t.text.into());
            st.set_confirm(t.confirm.into());
            st.set_details(endtask::details(14, 12, &[2231, 2298]).into());
            st.set_flow(3);
        }
        "end-details" => {
            let t = endtask::frozen("Firefox", 5, 2);
            st.set_title(t.title.into());
            st.set_text(t.text.into());
            st.set_confirm(t.confirm.into());
            st.set_details(endtask::details(14, 12, &[2231, 2298]).into());
            st.set_details_open(true);
            st.set_flow(3);
        }
        "end-protected" => {
            let t = endtask::protected("plasma", "Plasma desktop");
            st.set_title(t.title.into());
            st.set_text(t.text.into());
            st.set_flow(4);
        }
        "end-done" => {
            let toast = ui.global::<ToastState>();
            toast.set_message(endtask::ended("Firefox", 2_100_000_000).into());
            toast.set_can_undo(false);
            toast.set_shown(true);
        }
        other => return Err(format!("unknown processes state '{other}'").into()),
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
    // `TIDEWISE_SCREENSHOT_THEME=light` draws any screen in the light theme (screenshots only).
    if std::env::var("TIDEWISE_SCREENSHOT_THEME").as_deref() == Ok("light") {
        ui.global::<Theme>().set_mode(2);
    }
    if shot.screen == "storage" && shot.state.as_deref() == Some("live") {
        live_storage_screenshot(&ui, shot)?;
    }
    if shot.screen == "apps" && shot.state.as_deref() == Some("live") {
        let mut ctl = apps_live::Ctl::new();
        ctl.enter(&ui);
        while ctl.busy() {
            ctl.poll(&ui);
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    if shot.screen == "health" && shot.state.as_deref() == Some("live") {
        // The same inputs Home uses: the real drive, a real junk scan and the failed services.
        let mut sampler = live::Sampler::new();
        let report = tidewise_adapters::junk::scan(&tidewise_adapters::junk::live_sources());
        sampler.junk = Some(report.user_bytes() + report.admin_bytes());
        sampler.failed_services =
            tidewise_adapters::services::read(&tidewise_adapters::runner::SystemRunner)
                .ok()
                .map(|all| {
                    all.into_iter()
                        .filter(|s| s.state == tidewise_adapters::services::State::Failed)
                        .map(|s| s.unit)
                        .collect()
                });
        health_live::Ctl::new().enter(&ui, sampler.health_report());
    }
    if shot.screen == "services" && shot.state.as_deref() == Some("live") {
        let mut ctl = services_live::Ctl::new();
        ctl.enter(&ui);
        while ctl.busy() {
            ctl.poll(&ui);
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    if shot.screen == "startup" && shot.state.as_deref() == Some("live") {
        startup_live::Ctl::new().enter(&ui);
    }
    if shot.screen == "history" && shot.state.as_deref() == Some("live") {
        history_live::Ctl::new("Arch Linux".into()).enter(&ui);
    }
    if shot.screen == "clean-up" && shot.state.as_deref() == Some("live") {
        let mut ctl = cleaner_live::Ctl::new();
        ctl.enter(&ui);
        while ctl.busy() {
            ctl.poll(&ui);
            std::thread::sleep(Duration::from_millis(50));
        }
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
        show_demo(&ui, "clean-up", None)?;
        show_demo(&ui, "apps", None)?;
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
    let end_ctl = Rc::new(RefCell::new(endtask_live::Ctl::new()));
    endtask_live::wire(&ui, &end_ctl);
    ui.global::<ProcState>().on_end_task({
        let (weak, ctl, end_ctl) = (ui.as_weak(), ctl.clone(), end_ctl.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let c = ctl.borrow();
            if let Some(row) = c.data.rows.iter().find(|r| r.id == c.selected) {
                end_ctl.borrow_mut().start(&ui, row);
            }
        }
    });
    ui.global::<ProcState>().on_force_quit({
        let (weak, ctl, end_ctl) = (ui.as_weak(), ctl.clone(), end_ctl.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let c = ctl.borrow();
            if let Some(row) = c.data.rows.iter().find(|r| r.id == c.selected) {
                end_ctl.borrow_mut().start_force(&ui, row);
            }
        }
    });
    let storage = Rc::new(RefCell::new(storage_live::Ctl::new()));
    storage_live::wire(&ui, &storage);
    let cleaner = Rc::new(RefCell::new(cleaner_live::Ctl::new()));
    cleaner_live::wire(&ui, &cleaner);
    let apps_ctl = Rc::new(RefCell::new(apps_live::Ctl::new()));
    apps_live::wire(&ui, &apps_ctl);
    let health_ctl = Rc::new(RefCell::new(health_live::Ctl::new()));
    health_live::wire(&ui, &health_ctl);
    let settings_ctl = Rc::new(RefCell::new(settings_live::Ctl::load()));
    settings_ctl.borrow().apply(&ui);
    settings_live::wire(&ui, &settings_ctl);
    let services_ctl = Rc::new(RefCell::new(services_live::Ctl::new()));
    services_live::wire(&ui, &services_ctl);
    let startup_ctl = Rc::new(RefCell::new(startup_live::Ctl::new()));
    startup_live::wire(&ui, &startup_ctl);
    let history_ctl = Rc::new(RefCell::new(history_live::Ctl::new(format!(
        "{} · {}",
        shell_info().0,
        shell_info().1
    ))));
    history_live::wire(&ui, &history_ctl);
    let junk_watch = Rc::new(RefCell::new(live::JunkWatch::default()));
    junk_watch
        .borrow_mut()
        .poll(&mut live.borrow_mut().sampler, false);
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
        let (
            weak,
            live,
            ctl,
            storage,
            cleaner,
            apps_ctl,
            history_ctl,
            startup_ctl,
            services_ctl,
            health_ctl,
        ) = (
            ui.as_weak(),
            live.clone(),
            ctl.clone(),
            storage.clone(),
            cleaner.clone(),
            apps_ctl.clone(),
            history_ctl.clone(),
            startup_ctl.clone(),
            services_ctl.clone(),
            health_ctl.clone(),
        );
        move |page| {
            if let Some(ui) = weak.upgrade() {
                ui.global::<Shell>().set_page(page);
                if needs_processes(page) {
                    // The last sample may be a light one; the page needs the process table now.
                    live.borrow_mut().sampler.sample(true);
                }
                if page == 1 {
                    cleaner.borrow_mut().enter(&ui);
                }
                if page == 2 {
                    apps_ctl.borrow_mut().enter(&ui);
                }
                if page == 3 {
                    storage.borrow_mut().enter(&ui);
                }
                if page == 6 {
                    startup_ctl.borrow_mut().enter(&ui);
                }
                if page == 7 {
                    services_ctl.borrow_mut().enter(&ui);
                }
                if page == 10 {
                    let report = live.borrow_mut().sampler.health_report();
                    health_ctl.borrow_mut().enter(&ui, report);
                }
                if page == 8 {
                    history_ctl.borrow_mut().enter(&ui);
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
    if ui.global::<Shell>().get_page() == 1 {
        cleaner.borrow_mut().enter(&ui);
    }
    if ui.global::<Shell>().get_page() == 2 {
        apps_ctl.borrow_mut().enter(&ui);
    }
    if ui.global::<Shell>().get_page() == 6 {
        startup_ctl.borrow_mut().enter(&ui);
    }
    if ui.global::<Shell>().get_page() == 7 {
        services_ctl.borrow_mut().enter(&ui);
    }
    if ui.global::<Shell>().get_page() == 10 {
        let report = live.borrow_mut().sampler.health_report();
        health_ctl.borrow_mut().enter(&ui, report);
    }
    if ui.global::<Shell>().get_page() == 8 {
        history_ctl.borrow_mut().enter(&ui);
    }
    if ui.global::<Shell>().get_page() == 3 {
        storage.borrow_mut().enter(&ui);
        if let Some(rel) = &args.open {
            if !storage.borrow_mut().open_now(rel_path(rel), &ui) {
                return Err(
                    format!("--open: {rel} is not a folder inside the scanned folder").into(),
                );
            }
        }
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
            let home_visible = ui.global::<Shell>().get_page() == 0;
            junk_watch
                .borrow_mut()
                .poll(&mut live.borrow_mut().sampler, home_visible);
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
