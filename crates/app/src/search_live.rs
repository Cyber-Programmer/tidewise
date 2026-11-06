//! The live search box: builds the index when it is opened, ranks as you type, moves the pick and
//! opens the chosen page, app or program.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, VecModel};
use tidewise_core::search::{self, Item};

use crate::search_index::{self, Open};
use crate::search_view;
use crate::{MainWindow, SearchRowUi, SearchState};

#[derive(Default)]
pub struct Ctl {
    items: Vec<Item>,
    hits: Vec<usize>,
    selected: usize,
}

impl Ctl {
    pub fn new() -> Self {
        Ctl::default()
    }

    /// Open the box on a fresh index with an empty query (the suggestions).
    pub fn open(&mut self, ui: &MainWindow, items: Vec<Item>) {
        self.open_with(ui, items, "");
    }

    /// Open the box with `query` already typed (screenshots).
    pub fn open_with(&mut self, ui: &MainWindow, items: Vec<Item>, query: &str) {
        self.items = items;
        let st = ui.global::<SearchState>();
        st.set_query(query.into());
        st.set_open(true);
        self.recompute(ui, query);
    }

    fn recompute(&mut self, ui: &MainWindow, query: &str) {
        self.hits = search::search(&self.items, query);
        self.selected = 0;
        self.show(ui);
    }

    fn show(&self, ui: &MainWindow) {
        let rows: Vec<SearchRowUi> = search_view::rows(&self.items, &self.hits, self.selected)
            .into_iter()
            .map(|r| SearchRowUi {
                // The artboard sets the group headings in capitals.
                header: r.header.to_uppercase().into(),
                title: r.title.into(),
                meta: r.meta.into(),
                hint: r.hint.into(),
                icon: r.icon,
                selected: r.selected,
            })
            .collect();
        ui.global::<SearchState>()
            .set_rows(ModelRc::new(VecModel::from(rows)));
    }

    fn move_by(&mut self, ui: &MainWindow, delta: i32) {
        self.selected = search_view::moved(self.selected, delta, self.hits.len());
        self.show(ui);
    }

    /// The thing to open and its title; `row < 0` means the picked one.
    fn chosen(&self, row: i32) -> Option<(Open, String)> {
        let at = if row < 0 { self.selected } else { row as usize };
        let item = self.items.get(*self.hits.get(at)?)?;
        Some((search_index::parse_key(&item.key)?, item.title.clone()))
    }
}

/// Open what was chosen: a page, or a page with its search field filled in.
fn go(ui: &MainWindow, what: Open, title: &str) {
    use crate::{AppsState, ProcState, Shell};
    match what {
        Open::Page(page) => ui.global::<Shell>().invoke_navigate(page),
        Open::App(_) => {
            ui.global::<Shell>().invoke_navigate(2);
            let apps = ui.global::<AppsState>();
            apps.set_query(title.into());
            apps.invoke_search_edited(title.into());
        }
        Open::Process(_) => {
            ui.global::<Shell>().invoke_navigate(5);
            let procs = ui.global::<ProcState>();
            procs.set_search(title.into());
            procs.invoke_search_edited(title.into());
        }
    }
}

/// `build` makes a fresh index each time the box is opened (apps found so far, programs running now).
pub fn wire(
    ui: &MainWindow,
    ctl: &Rc<RefCell<Ctl>>,
    build: impl Fn(&MainWindow) -> Vec<Item> + 'static,
) {
    let st = ui.global::<SearchState>();
    st.on_open_requested({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let items = build(&ui);
            ctl.borrow_mut().open(&ui, items);
        }
    });
    st.on_query_changed({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |q| {
            if let Some(ui) = weak.upgrade() {
                ctl.borrow_mut().recompute(&ui, &q);
            }
        }
    });
    st.on_move({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |d| {
            if let Some(ui) = weak.upgrade() {
                ctl.borrow_mut().move_by(&ui, d);
            }
        }
    });
    st.on_activate({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |row| {
            let Some(ui) = weak.upgrade() else { return };
            let chosen = ctl.borrow().chosen(row);
            if let Some((what, title)) = chosen {
                ui.global::<SearchState>().set_open(false);
                go(&ui, what, &title);
            }
        }
    });
    st.on_close({
        let weak = ui.as_weak();
        move || {
            if let Some(ui) = weak.upgrade() {
                ui.global::<SearchState>().set_open(false);
            }
        }
    });
}
