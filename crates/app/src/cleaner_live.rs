//! Live Clean up screen: scans in the background, keeps what the user ticked, runs the confirmed
//! steps on a worker thread and shows the end-of-run dialog. The words come from `cleaner`, the
//! removing from `tidewise_adapters::cleanup`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use slint::ComponentHandle;
use tidewise_adapters::cleanup::{self, Env, Group, GroupId, Step, StepResult};
use tidewise_adapters::junk;
use tidewise_core::history;

use crate::{cleaner, view, CleanState, MainWindow, Shell};

enum Msg {
    Start(usize),
    Finished(Vec<StepResult>),
}

struct Job {
    rx: Receiver<Msg>,
    stop: Arc<AtomicBool>,
    steps: Vec<Step>,
}

#[derive(Default)]
pub struct Ctl {
    groups: Vec<Group>,
    selected: Vec<GroupId>,
    scan: Option<Receiver<(junk::JunkReport, Env)>>,
    job: Option<Job>,
    env: Env,
    /// The steps of the last run and what they did, for "Clean the rest".
    plan: Vec<Step>,
    results: Vec<StepResult>,
    started: bool,
}

impl Ctl {
    pub fn new() -> Self {
        Ctl::default()
    }

    /// Scan when the page is first opened.
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
        let st = ui.global::<CleanState>();
        st.set_phase(0);
        st.set_subtitle("Looking for things to clean…".into());
        st.set_message("This only reads your files; nothing is removed.".into());
        let (tx, rx) = mpsc::channel();
        self.scan = Some(rx);
        std::thread::spawn(move || {
            let sources = junk::live_sources();
            let env = Env {
                cache_root: sources.cache_dir.clone().unwrap_or_default(),
                trash_root: sources.trash_dir.clone().unwrap_or_default(),
                running: Vec::new(),
                owner_uid: None,
            };
            let _ = tx.send((junk::scan(&sources), env));
        });
    }

    pub fn poll(&mut self, ui: &MainWindow) {
        self.poll_scan(ui);
        self.poll_job(ui);
    }

    fn poll_scan(&mut self, ui: &MainWindow) {
        let Some(rx) = &self.scan else { return };
        match rx.try_recv() {
            Ok((report, env)) => {
                self.scan = None;
                self.env = env;
                self.groups = cleanup::groups(&report);
                self.selected = cleanup::default_selection(&self.groups);
                let st = ui.global::<CleanState>();
                if self.groups.is_empty() {
                    st.set_phase(2);
                    st.set_subtitle("Scan finished.".into());
                    st.set_message("Nothing worth cleaning was found right now.".into());
                } else {
                    st.set_phase(1);
                    st.set_subtitle(
                        "Scan finished. Nothing is removed until you press Clean.".into(),
                    );
                }
                view::apply_clean(ui, &self.groups, &self.selected);
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.scan = None;
                let st = ui.global::<CleanState>();
                st.set_phase(3);
                st.set_subtitle("The scan did not finish.".into());
                st.set_message("Something stopped the scan. Try again from the sidebar.".into());
            }
        }
    }

    fn poll_job(&mut self, ui: &MainWindow) {
        let Some(job) = &self.job else { return };
        let st = ui.global::<CleanState>();
        let total: u64 = job.steps.iter().map(|s| s.bytes).sum();
        let mut finished = None;
        while let Ok(msg) = job.rx.try_recv() {
            match msg {
                Msg::Start(i) => {
                    let before: u64 = job.steps[..i].iter().map(|s| s.bytes).sum();
                    st.set_progress(if total == 0 {
                        0.0
                    } else {
                        before as f32 / total as f32
                    });
                    st.set_progress_label(format!("Removing: {}", job.steps[i].title).into());
                    st.set_freed_label(
                        format!(
                            "{} of {}",
                            cleanup::format_bytes(before),
                            cleanup::format_bytes(total)
                        )
                        .into(),
                    );
                }
                Msg::Finished(results) => finished = Some(results),
            }
        }
        if let Some(results) = finished {
            self.job = None;
            self.finish_run(ui, results);
        }
    }

    fn finish_run(&mut self, ui: &MainWindow, results: Vec<StepResult>) {
        let fin = cleaner::finish(&results);
        let mut text = fin.text.clone();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        match history::history_dir() {
            Some(dir) => {
                let id = history::unique_id(&dir, now, history::Kind::CleanUp);
                let record = cleanup::record(&results, now, &id);
                if let Err(e) = history::write(&dir, &record) {
                    text += &format!(" (The report could not be saved in History: {e}.)");
                }
            }
            None => text += " (The report could not be saved: no home folder.)",
        }
        self.results = results;
        let st = ui.global::<CleanState>();
        if fin.all_done {
            st.set_done_title(fin.title.into());
            st.set_done_text(text.into());
            st.set_flow(3);
        } else {
            st.set_partial_title(fin.title.into());
            st.set_partial_text(text.into());
            st.set_retry_label(fin.retry_label.into());
            st.set_can_retry(fin.can_retry);
            st.set_flow(4);
        }
    }

    fn run(&mut self, ui: &MainWindow, steps: Vec<Step>) {
        let mut env = self.env.clone();
        env.running = junk::running_names();
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let worker = (steps.clone(), stop.clone(), tx);
        self.plan = steps.clone();
        self.job = Some(Job { rx, stop, steps });
        let st = ui.global::<CleanState>();
        st.set_progress(0.0);
        st.set_progress_label("Re-checking the list…".into());
        st.set_freed_label("".into());
        st.set_flow(2);
        std::thread::spawn(move || {
            let (steps, stop, tx) = worker;
            let results = cleanup::run(
                &steps,
                &env,
                &|| stop.load(Ordering::Relaxed),
                &mut |i, _| {
                    let _ = tx.send(Msg::Start(i));
                },
            );
            let _ = tx.send(Msg::Finished(results));
        });
    }
}

pub fn wire(ui: &MainWindow, ctl: &Rc<RefCell<Ctl>>) {
    let st = ui.global::<CleanState>();
    st.on_toggle({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |i| {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            if let Some(id) = c.groups.get(i.max(0) as usize).map(|g| g.id) {
                cleaner::toggle(&mut c.selected, id);
                view::apply_clean(&ui, &c.groups, &c.selected);
            }
        }
    });
    st.on_clean({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                ui.global::<CleanState>().set_flow(1);
            }
        }
    });
    st.on_cancel({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                ui.global::<CleanState>().set_flow(0);
            }
        }
    });
    st.on_confirm({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            let steps = cleanup::plan(&c.groups, &c.selected);
            if !steps.is_empty() {
                c.run(&ui, steps);
            }
        }
    });
    st.on_stop_early({
        let ctl = ctl.clone();
        move || {
            if let Some(job) = &ctl.borrow().job {
                job.stop.store(true, Ordering::Relaxed);
            }
        }
    });
    st.on_retry({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            let rest: Vec<Step> = c
                .plan
                .iter()
                .zip(&c.results)
                .filter(|(_, r)| r.outcome == cleanup::Outcome::Stopped)
                .map(|(s, _)| s.clone())
                .collect();
            if !rest.is_empty() {
                c.run(&ui, rest);
            }
        }
    });
    st.on_finish({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let st = ui.global::<CleanState>();
            let was_done = st.get_flow() == 3;
            st.set_flow(0);
            ctl.borrow_mut().start_scan(&ui);
            if was_done {
                ui.global::<Shell>().invoke_navigate(0);
            }
        }
    });
    st.on_see_report({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            ui.global::<CleanState>().set_flow(0);
            ctl.borrow_mut().start_scan(&ui);
            ui.global::<Shell>().invoke_navigate(8);
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
    // The timer lives as long as the window: leak it deliberately into the UI's lifetime.
    std::mem::forget(timer);
}
