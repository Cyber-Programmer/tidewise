//! Live Apps screen: finds the apps on a worker thread, keeps the filter, search and pick, shows the
//! uninstall panel and runs a confirmed uninstall on a worker thread. The words come from `apps`,
//! the removing from `tidewise_adapters::uninstall` (AppImages and user Flatpaks only).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use slint::ComponentHandle;
use tidewise_adapters::appimage::AppImageApps;
use tidewise_adapters::apt::AptApps;
use tidewise_adapters::desktop;
use tidewise_adapters::dnf::DnfApps;
use tidewise_adapters::flatpak::FlatpakApps;
use tidewise_adapters::pacman::PacmanApps;
use tidewise_adapters::runner::SystemRunner;
use tidewise_adapters::snap::SnapApps;
use tidewise_adapters::source::{AppEntry, AppSource, RemovalPlan, SourceKind};
use tidewise_adapters::uninstall::{self, Outcome};
use tidewise_core::history;

use crate::apps::{self, Filter};
use crate::{view, AppsState, MainWindow, Shell};

type Found = Vec<(AppEntry, RemovalPlan)>;

struct Job {
    rx: Receiver<(Outcome, bool)>,
    name: String,
    /// The user asked for the settings folder to go to the Trash too.
    delete_settings: bool,
}

/// What a scan finds: the apps, and the settings folder of each user Flatpak that has one.
type Scanned = (Found, HashMap<String, uninstall::SettingsFolder>);

#[derive(Default)]
pub struct Ctl {
    found: Found,
    filter_index: i32,
    query: String,
    picked: Option<String>,
    scan: Option<Receiver<Scanned>>,
    settings: HashMap<String, uninstall::SettingsFolder>,
    delete_settings: bool,
    job: Option<Job>,
    started: bool,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn data_home() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home().map(|h| h.join(".local/share")))
}

/// The AppImage source with its launcher and icon folders.
fn appimage_source() -> Option<AppImageApps> {
    let home = home()?;
    let data = data_home()?;
    Some(
        AppImageApps::for_home(&home)
            .with_menu(data.join("applications"), vec![data.join("icons")]),
    )
}

/// Every app from every source this machine has, with a removal plan each.
fn find_apps() -> Found {
    let mut sources: Vec<Box<dyn AppSource>> = vec![
        Box::new(FlatpakApps::system()),
        Box::new(SnapApps::system()),
    ];
    if let Some(a) = appimage_source() {
        sources.push(Box::new(a));
    }
    let mut found: Found = Vec::new();
    for source in &sources {
        for app in source.list().unwrap_or_default() {
            if let Ok(plan) = source.removal_plan(&app) {
                found.push((app, plan));
            }
        }
    }
    // Packages: only those that own a visible launcher count as apps.
    add_package_apps(&mut found, &PacmanApps::system(), SourceKind::Pacman);
    // Debian/Ubuntu and Fedora machines: the same, through `dpkg -S` and `rpm -qf`. On Arch their
    // tools are missing, `list()` fails and nothing is added.
    add_package_apps(&mut found, &AptApps::system(), SourceKind::Apt);
    add_package_apps(&mut found, &DnfApps::system(), SourceKind::Dnf);
    found
}

/// The packages of one package manager that own a visible launcher, each with its removal plan.
fn add_package_apps(found: &mut Found, source: &dyn AppSource, kind: SourceKind) {
    let Ok(packages) = source.list() else {
        return;
    };
    let launchers = std::path::Path::new("/usr/share/applications");
    for app in desktop::package_apps(packages, kind, &SystemRunner, launchers) {
        if let Ok(plan) = source.removal_plan(&app) {
            found.push((app, plan));
        }
    }
}

/// A fresh plan for one app, from its own source, right before acting.
fn fresh_plan(app: &AppEntry) -> Option<RemovalPlan> {
    let source: Box<dyn AppSource> = match app.kind {
        SourceKind::AppImage => Box::new(appimage_source()?),
        SourceKind::Flatpak => Box::new(FlatpakApps::system()),
        SourceKind::Snap => Box::new(SnapApps::system()),
        SourceKind::Pacman => Box::new(PacmanApps::system()),
        SourceKind::Apt => Box::new(AptApps::system()),
        SourceKind::Dnf => Box::new(DnfApps::system()),
    };
    let current = source.list().ok()?.into_iter().find(|a| a.id == app.id)?;
    source.removal_plan(&current).ok()
}

impl Ctl {
    pub fn new() -> Self {
        Ctl::default()
    }

    pub fn enter(&mut self, ui: &MainWindow) {
        if !self.started {
            self.start_scan(ui);
        }
    }

    pub fn busy(&self) -> bool {
        self.scan.is_some() || self.job.is_some()
    }

    fn start_scan(&mut self, ui: &MainWindow) {
        self.started = true;
        let st = ui.global::<AppsState>();
        st.set_phase(0);
        st.set_subtitle("Looking for your apps…".into());
        st.set_message("This only reads what is installed; nothing is changed.".into());
        let (tx, rx) = mpsc::channel();
        self.scan = Some(rx);
        std::thread::spawn(move || {
            let found = find_apps();
            let settings = home()
                .map(|h| {
                    found
                        .iter()
                        .filter_map(|(a, _)| {
                            Some((a.id.clone(), uninstall::settings_folder(a, &h)?))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let _ = tx.send((found, settings));
        });
    }

    pub fn poll(&mut self, ui: &MainWindow) {
        if let Some(rx) = &self.scan {
            match rx.try_recv() {
                Ok((found, settings)) => {
                    self.scan = None;
                    self.found = found;
                    self.settings = settings;
                    let st = ui.global::<AppsState>();
                    if self.found.is_empty() {
                        st.set_phase(2);
                        st.set_subtitle("No apps found.".into());
                        st.set_message(
                            "Tidewise did not find any apps it knows how to list.".into(),
                        );
                    } else {
                        st.set_phase(1);
                    }
                    self.refresh(ui);
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.scan = None;
                    let st = ui.global::<AppsState>();
                    st.set_phase(3);
                    st.set_subtitle("The search did not finish.".into());
                    st.set_message("Something stopped it. Try again from the sidebar.".into());
                }
            }
        }
        if let Some(job) = &self.job {
            if let Ok((outcome, to_trash)) = job.rx.try_recv() {
                let name = job.name.clone();
                let job_settings = job.delete_settings;
                self.job = None;
                self.finish_job(ui, &name, to_trash, job_settings, &outcome);
            }
        }
    }

    fn entries(&self) -> Vec<AppEntry> {
        self.found.iter().map(|(a, _)| a.clone()).collect()
    }

    fn refresh(&mut self, ui: &MainWindow) {
        let entries = self.entries();
        let screen = apps::screen(
            &entries,
            Filter::from_index(self.filter_index),
            &self.query,
            self.picked.as_deref(),
        );
        self.picked = screen.picked.clone();
        view::apply_apps(ui, &screen);
        let panel = self.picked.as_ref().and_then(|id| {
            self.found
                .iter()
                .find(|(a, _)| a.id == *id)
                .map(|(a, plan)| {
                    let choice = self.settings.get(&a.id).map(|f| apps::SettingsChoice {
                        bytes: f.bytes,
                        delete: self.delete_settings,
                    });
                    (apps::panel_with_settings(a, plan, choice), apps::row(a))
                })
        });
        view::apply_apps_panel(ui, panel.as_ref().map(|(p, r)| (p, r)));
    }

    fn finish_job(
        &mut self,
        ui: &MainWindow,
        name: &str,
        to_trash: bool,
        asked_for_settings: bool,
        outcome: &Outcome,
    ) {
        // The settings went too only when the outcome lists their folder after the app.
        let settings_went = asked_for_settings
            && matches!(outcome, Outcome::Removed { items, .. } if items.len() > 1);
        let done = apps::done_with_settings(name, to_trash, settings_went, outcome);
        let mut text = done.text.clone();
        if let Outcome::Removed { items, freed } = outcome {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            match history::history_dir() {
                Some(dir) => {
                    let id = history::unique_id(&dir, now, history::Kind::Uninstall);
                    let record = history::Record {
                        version: history::VERSION,
                        id: id.clone(),
                        time: now,
                        kind: history::Kind::Uninstall,
                        title: format!("Uninstalled {name}"),
                        freed_bytes: *freed,
                        steps: vec![history::Step {
                            title: name.to_string(),
                            status: history::Status::Done,
                            bytes: *freed,
                            items: items.clone(),
                            entries: vec![],
                            note: (to_trash || settings_went).then(|| {
                                if to_trash {
                                    "moved to the Trash".to_string()
                                } else {
                                    "settings and data moved to the Trash".to_string()
                                }
                            }),
                        }],
                    };
                    if let Err(e) = history::write(&dir, &record) {
                        text += &format!(" (The report could not be saved in History: {e}.)");
                    }
                }
                None => text += " (The report could not be saved: no home folder.)",
            }
        }
        let st = ui.global::<AppsState>();
        st.set_d_title(done.title.into());
        st.set_d_text(text.into());
        st.set_flow(if done.removed { 3 } else { 4 });
    }
}

pub fn wire(ui: &MainWindow, ctl: &Rc<RefCell<Ctl>>) {
    let st = ui.global::<AppsState>();
    st.on_pick({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |i| {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            let entries = c.entries();
            let shown = apps::visible(&entries, Filter::from_index(c.filter_index), &c.query);
            if let Some(&at) = shown.get(i.max(0) as usize) {
                if c.picked.as_deref() != Some(entries[at].id.as_str()) {
                    c.delete_settings = false;
                }
                c.picked = Some(entries[at].id.clone());
                c.refresh(&ui);
            }
        }
    });
    st.on_set_filter({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |i| {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            let entries = c.entries();
            let chips = apps::chips(&entries);
            c.filter_index = chips
                .get(i.max(0) as usize)
                .map_or(0, |chip| match chip.filter {
                    Filter::All => 0,
                    Filter::System => 1,
                    Filter::Flatpak => 2,
                    Filter::AppImage => 3,
                    Filter::Snap => 4,
                });
            c.refresh(&ui);
        }
    });
    st.on_search_edited({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |text| {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            c.query = text.to_string();
            c.refresh(&ui);
        }
    });
    st.on_toggle_settings({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            let offered = c
                .picked
                .as_ref()
                .is_some_and(|id| c.settings.contains_key(id));
            if offered {
                c.delete_settings = !c.delete_settings;
                c.refresh(&ui);
            }
        }
    });
    st.on_uninstall({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                ui.global::<AppsState>().set_flow(1);
            }
        }
    });
    st.on_cancel({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                ui.global::<AppsState>().set_flow(0);
            }
        }
    });
    st.on_confirm({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            let Some((app, plan)) = c
                .picked
                .as_ref()
                .and_then(|id| c.found.iter().find(|(a, _)| a.id == *id))
                .cloned()
            else {
                return;
            };
            let st = ui.global::<AppsState>();
            st.set_d_title(format!("Removing {}…", app.name).into());
            st.set_d_command(plan.command.join(" ").into());
            st.set_d_text(
                format!(
                    "If {} is open, save your work. Tidewise does not close it for you.",
                    app.name
                )
                .into(),
            );
            st.set_flow(2);
            let (tx, rx) = mpsc::channel();
            let delete_settings = c.delete_settings && c.settings.contains_key(&app.id);
            c.job = Some(Job {
                rx,
                name: app.name.clone(),
                delete_settings,
            });
            let to_trash = plan.to_trash;
            std::thread::spawn(move || {
                let outcome = match (
                    fresh_plan(&app),
                    home(),
                    tidewise_core::trash::default_trash_dir(),
                ) {
                    (Some(fresh), Some(home), Some(trash)) => {
                        let env = uninstall::Env {
                            trash_dir: &trash,
                            home: &home,
                            now: SystemTime::now(),
                            runner: &SystemRunner,
                            delete_settings,
                        };
                        uninstall::run(&app, &plan, &fresh, &env)
                    }
                    _ => Outcome::Failed {
                        reason: "Tidewise could not check the app again, so nothing was removed."
                            .into(),
                        removed: Vec::new(),
                    },
                };
                let _ = tx.send((outcome, to_trash));
            });
        }
    });
    st.on_finish({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let st = ui.global::<AppsState>();
            let was_done = st.get_flow() == 3;
            st.set_flow(0);
            if was_done {
                ctl.borrow_mut().start_scan(&ui);
            }
            let _ = ui.global::<Shell>();
        }
    });
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(150), {
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let busy = ctl.borrow().busy();
            if busy {
                ctl.borrow_mut().poll(&ui);
            }
        }
    });
    // The timer lives as long as the window.
    std::mem::forget(timer);
}
