//! Live Health page: shows the report Home's score comes from, and opens or closes one item.

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle;
use tidewise_core::health::Report;

use crate::{health_view, view, HealthState, MainWindow};

#[derive(Default)]
pub struct Ctl {
    report: Option<Report>,
    open: Vec<usize>,
}

impl Ctl {
    pub fn new() -> Self {
        Ctl::default()
    }

    /// The page was opened with this report (the first problem is open, like the artboard).
    pub fn enter(&mut self, ui: &MainWindow, report: Report) {
        self.open = if report.findings.is_empty() {
            vec![]
        } else {
            vec![0]
        };
        self.report = Some(report);
        self.refresh(ui);
    }

    fn refresh(&self, ui: &MainWindow) {
        if let Some(report) = &self.report {
            view::apply_health(ui, &health_view::screen(report, &self.open));
        }
    }
}

pub fn wire(ui: &MainWindow, ctl: &Rc<RefCell<Ctl>>) {
    ui.global::<HealthState>().on_toggle({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |i| {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            let i = i.max(0) as usize;
            if let Some(at) = c.open.iter().position(|o| *o == i) {
                c.open.remove(at);
            } else {
                c.open.push(i);
            }
            c.refresh(&ui);
        }
    });
}
