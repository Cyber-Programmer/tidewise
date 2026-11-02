//! Live End task: confirm, ask the app to close, wait 5 seconds, offer Force quit. The words come
//! from `endtask`, the signals from `tidewise_adapters::endtask` (SIGTERM first, SIGKILL only when
//! the user asks). Protected parts of the system get a message and no button.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use slint::ComponentHandle;
use tidewise_adapters::endtask::{plan, read_table, send, wait_until_gone, Signal};
use tidewise_adapters::runner::SystemRunner;

use crate::procs::Row;
use crate::{endtask, EndState, MainWindow, ToastState};

const WAIT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(100);

#[derive(Clone)]
struct Target {
    name: String,
    pids: Vec<u32>,
    /// How many processes there were when the End button was pressed.
    total: usize,
    freed: u64,
}

enum Msg {
    /// Who is still running, and whether the signals could be sent at all.
    Result {
        remaining: Vec<u32>,
        error: Option<String>,
    },
}

#[derive(Default)]
pub struct Ctl {
    target: Option<Target>,
    waited: u64,
    /// The Force quit button of the details card: confirm once, then SIGKILL (no polite request).
    force_direct: bool,
    /// SIGKILL was sent at some point: the History record says "force quit".
    forced: bool,
    job: Option<Receiver<Msg>>,
    toast_timer: slint::Timer,
}

/// Send `signal` to `pids` (when given) and wait up to `wait` for them to go.
fn run(pids: Vec<u32>, signal: Option<Signal>, wait: Duration) -> Msg {
    let proc = Path::new("/proc");
    let mut error = None;
    if let Some(signal) = signal {
        let table = read_table(proc);
        let own = std::process::id();
        let own_group = table.iter().find(|p| p.pid == own).map_or(0, |p| p.pgid);
        match plan(&pids, &table, own, own_group) {
            Ok(dests) => {
                for dest in &dests {
                    if let Err(e) = send(&SystemRunner, signal, dest) {
                        error = Some(e);
                    }
                }
            }
            Err(e) => error = Some(e.to_string()),
        }
    }
    let remaining = wait_until_gone(proc, &pids, wait, POLL);
    Msg::Result { remaining, error }
}

impl Ctl {
    pub fn new() -> Self {
        Ctl::default()
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// The End button on the Processes screen.
    pub fn start(&mut self, ui: &MainWindow, row: &Row) {
        self.open(ui, row, false);
    }

    /// The Force quit button in the details card: the same dialog, but the answer is SIGKILL.
    pub fn start_force(&mut self, ui: &MainWindow, row: &Row) {
        self.open(ui, row, true);
    }

    fn open(&mut self, ui: &MainWindow, row: &Row, force: bool) {
        let st = ui.global::<EndState>();
        if row.protected {
            let t = endtask::protected(&row.id, &row.name);
            st.set_title(t.title.into());
            st.set_text(t.text.into());
            st.set_confirm("".into());
            st.set_flow(4);
            return;
        }
        if row.pids.is_empty() {
            return; // sample data: nothing real to end
        }
        let t = if force {
            endtask::force_confirm(&row.name, row.pids.len())
        } else {
            endtask::confirm(&row.name, row.pids.len())
        };
        st.set_title(t.title.into());
        st.set_text(t.text.into());
        st.set_confirm(t.confirm.into());
        st.set_details_open(false);
        st.set_flow(1);
        self.target = Some(Target {
            name: row.name.clone(),
            pids: row.pids.clone(),
            total: row.pids.len(),
            freed: row.mem_bytes,
        });
        self.waited = if force { 10 } else { 0 };
        self.force_direct = force;
        self.forced = false;
    }

    fn begin(&mut self, ui: &MainWindow, signal: Option<Signal>, wait: Duration) {
        let Some(target) = self.target.clone() else {
            return;
        };
        let st = ui.global::<EndState>();
        if signal == Some(Signal::Kill) {
            self.forced = true;
        }
        let t = endtask::closing(&target.name, signal == Some(Signal::Kill));
        st.set_title(t.title.into());
        st.set_text(t.text.into());
        st.set_flow(2);
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(run(target.pids, signal, wait));
        });
    }

    pub fn poll(&mut self, ui: &MainWindow) {
        let Some(rx) = &self.job else { return };
        let Ok(Msg::Result { remaining, error }) = rx.try_recv() else {
            return;
        };
        self.job = None;
        let Some(target) = self.target.clone() else {
            return;
        };
        let st = ui.global::<EndState>();
        if remaining.is_empty() {
            st.set_flow(0);
            let forced = self.forced;
            let _ = crate::records::save(tidewise_core::history::Kind::EndTask, |time, id| {
                tidewise_core::history::end_task_record(
                    &target.name,
                    target.total,
                    target.freed,
                    forced,
                    time,
                    id,
                )
            });
            self.target = None;
            self.toast(ui, endtask::ended(&target.name, target.freed), false);
            return;
        }
        if let Some(e) = error.filter(|_| self.waited >= 10) {
            st.set_flow(0);
            self.target = None;
            self.toast(
                ui,
                format!("{} could not be stopped: {e}", target.name),
                true,
            );
            return;
        }
        self.waited += 5;
        let t = endtask::frozen(&target.name, self.waited, remaining.len());
        st.set_title(t.title.into());
        st.set_text(t.text.into());
        st.set_confirm(t.confirm.into());
        st.set_details(
            endtask::details(
                target.pids.len(),
                target.pids.len() - remaining.len(),
                &remaining,
            )
            .into(),
        );
        if let Some(t) = &mut self.target {
            t.pids = remaining;
        }
        st.set_flow(3);
    }

    fn toast(&mut self, ui: &MainWindow, message: String, error: bool) {
        let t = ui.global::<ToastState>();
        t.set_message(message.into());
        t.set_can_undo(false);
        t.set_error(error);
        t.set_shown(true);
        let weak = ui.as_weak();
        self.toast_timer.start(
            slint::TimerMode::SingleShot,
            Duration::from_secs(8),
            move || {
                if let Some(ui) = weak.upgrade() {
                    ui.global::<ToastState>().set_shown(false);
                }
            },
        );
    }
}

pub fn wire(ui: &MainWindow, ctl: &Rc<RefCell<Ctl>>) {
    let st = ui.global::<EndState>();
    st.on_cancel({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            if let Some(ui) = weak.upgrade() {
                ui.global::<EndState>().set_flow(0);
                ctl.borrow_mut().target = None;
            }
        }
    });
    st.on_dismiss({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                ui.global::<EndState>().set_flow(0);
            }
        }
    });
    st.on_confirmed({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            if let Some(ui) = weak.upgrade() {
                let mut c = ctl.borrow_mut();
                if c.force_direct {
                    c.force_direct = false;
                    c.begin(&ui, Some(Signal::Kill), Duration::from_secs(3));
                } else {
                    c.begin(&ui, Some(Signal::Term), WAIT);
                }
            }
        }
    });
    st.on_keep_waiting({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            if let Some(ui) = weak.upgrade() {
                ctl.borrow_mut().begin(&ui, None, WAIT);
            }
        }
    });
    st.on_force({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            if let Some(ui) = weak.upgrade() {
                let mut c = ctl.borrow_mut();
                // After this the next answer is final: a second "not closing" becomes an error toast.
                c.waited = c.waited.max(10);
                c.begin(&ui, Some(Signal::Kill), Duration::from_secs(3));
            }
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
    std::mem::forget(timer);
}
