//! Live Services: reads systemd with three read-only `systemctl` calls on a worker thread when the
//! page is opened, then shows the tab and the picked service. Nothing is started or stopped.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use slint::ComponentHandle;
use tidewise_adapters::runner::SystemRunner;
use tidewise_adapters::services::{self, Service};

use crate::services_view::{self, Tab};
use crate::{view, MainWindow, ServicesState};

#[derive(Default)]
pub struct Ctl {
    services: Vec<Service>,
    tab: i32,
    selected: Option<String>,
    job: Option<Receiver<Result<Vec<Service>, String>>>,
}

impl Ctl {
    pub fn new() -> Self {
        Ctl::default()
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// The page was opened: ask systemd again.
    pub fn enter(&mut self, ui: &MainWindow) {
        if self.job.is_some() {
            return;
        }
        view::apply_services(
            ui,
            &services_view::Screen::default(),
            Some((1, "This only reads what is running; nothing is changed.")),
        );
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(services::read(&SystemRunner).map_err(|e| e.to_string()));
        });
    }

    pub fn poll(&mut self, ui: &MainWindow) {
        let Some(rx) = &self.job else { return };
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("the search did not finish".into()),
        };
        self.job = None;
        match result {
            Ok(list) => {
                self.services = list;
                self.refresh(ui);
            }
            Err(why) => view::apply_services(
                ui,
                &services_view::Screen::default(),
                Some((
                    2,
                    &format!("Tidewise could not ask systemd: {why}. Nothing was changed."),
                )),
            ),
        }
    }

    fn refresh(&mut self, ui: &MainWindow) {
        let screen = services_view::screen(
            &self.services,
            Tab::from_index(self.tab),
            self.selected.as_deref(),
        );
        self.selected = screen.detail.as_ref().map(|d| d.unit.clone());
        view::apply_services(ui, &screen, None);
    }
}

pub fn wire(ui: &MainWindow, ctl: &Rc<RefCell<Ctl>>) {
    let st = ui.global::<ServicesState>();
    st.on_set_tab({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |i| {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            c.tab = i;
            c.refresh(&ui);
        }
    });
    st.on_pick({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |unit| {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            c.selected = Some(unit.to_string());
            c.refresh(&ui);
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
