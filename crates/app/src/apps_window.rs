//! Glue between the Apps window and the tested code. It decides nothing about wording or state:
//! `AppsModel` holds the state, `apps_view` words it, and this file copies the result into Slint
//! and runs pacman on a worker thread.
//!
//! Threads: `installed_packages()` runs on a worker thread. The `AppsModel` never leaves the UI thread.
//! The worker puts its answer in a mailbox (a channel) and asks the UI thread, through a weak window
//! handle, to fire `load-done`; the UI thread then reads the mailbox.

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::mpsc;
use std::time::Duration;

use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use tidewise_app::apps_model::AppsModel;
use tidewise_app::apps_view::{
    AppsView, Body, DetailsView, EmptyAction, RowUpdate, RowView, apps_view, row_update,
    sort_from_index, tab_from_index,
};
use tidewise_app::list_scroll::{
    content_y_to_offset, offset_to_content_y, rows_per_page, scroll_to_show,
};
use tidewise_pacman::{InstalledPackage, PacmanError};

use crate::{AppsBridge, BodyKind, DetailsData, ListKey, MainWindow, PackageRow, Theme};

/// After this long the screen adds "This is taking longer than usual." (pacman is not stopped).
const SLOW_AFTER: Duration = Duration::from_secs(10);

type Answer = Result<Vec<InstalledPackage>, PacmanError>;

/// Everything the Apps screen owns. Lives on the UI thread only.
struct Screen {
    window: slint::Weak<MainWindow>,
    model: AppsModel,
    /// The search text exactly as typed (the model keeps a trimmed, lower-cased copy).
    typed_search: String,
    /// The rows Slint is showing now, to work out the smallest update.
    shown_rows: Vec<RowView>,
    /// Where the selected row is in the list, to keep it on screen.
    selected_index: Option<usize>,
    rows: Rc<VecModel<PackageRow>>,
    /// Where the worker thread leaves its answer.
    mailbox: Option<mpsc::Receiver<Answer>>,
    slow_timer: Timer,
}

/// Connects the window to the model and starts the first load.
pub fn start(window: &MainWindow) {
    let rows = Rc::new(VecModel::default());
    let bridge = window.global::<AppsBridge>();
    bridge.set_rows(ModelRc::from(rows.clone()));

    let screen = Rc::new(RefCell::new(Screen {
        window: window.as_weak(),
        model: AppsModel::new(),
        typed_search: String::new(),
        shown_rows: Vec::new(),
        selected_index: None,
        rows,
        mailbox: None,
        slow_timer: Timer::default(),
    }));

    let handle = screen.clone();
    bridge.on_tab_chosen(move |index| {
        if let Some(tab) = tab_from_index(index) {
            handle.borrow_mut().model.set_tab(tab);
            repaint(&handle);
        }
    });
    let handle = screen.clone();
    bridge.on_sort_chosen(move |index| {
        if let Some(sort) = sort_from_index(index) {
            handle.borrow_mut().model.set_sort(sort);
            repaint(&handle);
        }
    });
    let handle = screen.clone();
    bridge.on_search_edited(move |text| {
        {
            let mut screen = handle.borrow_mut();
            screen.typed_search = text.to_string();
            screen.model.set_search(&text);
        }
        repaint(&handle);
    });
    let handle = screen.clone();
    bridge.on_row_chosen(move |name| {
        handle.borrow_mut().model.select(&name);
        repaint(&handle);
    });
    let handle = screen.clone();
    bridge.on_list_key(move |key| list_key(&handle, key));
    let handle = screen.clone();
    bridge.on_list_resized(move || {
        let (window, selected) = {
            let state = handle.borrow();
            (state.window.upgrade(), state.selected_index)
        };
        if let Some(window) = window {
            keep_in_view(&window, selected);
        }
    });
    let handle = screen.clone();
    bridge.on_empty_button_clicked(move || {
        empty_button_clicked(&handle);
    });
    let handle = screen.clone();
    bridge.on_try_again(move || begin_load(&handle));
    let handle = screen.clone();
    bridge.on_load_done(move || load_done(&handle));

    begin_load(&screen);
}

/// Starts a load unless one is already running ("Try again" pressed twice must not start two).
fn begin_load(screen: &Rc<RefCell<Screen>>) {
    if !screen.borrow_mut().model.begin_reload() {
        return;
    }
    let (sender, receiver) = mpsc::channel();
    let window = screen.borrow().window.clone();
    std::thread::spawn(move || {
        // If the window closed meanwhile, nobody reads this and that is fine.
        let _ = sender.send(tidewise_pacman::installed_packages());
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = window.upgrade() {
                window.global::<AppsBridge>().invoke_load_done();
            }
        });
    });

    let timer_target: Weak<RefCell<Screen>> = Rc::downgrade(screen);
    {
        let mut state = screen.borrow_mut();
        state.mailbox = Some(receiver);
        // Starting the same timer again restarts it, so an older load's timer can never fire.
        state
            .slow_timer
            .start(TimerMode::SingleShot, SLOW_AFTER, move || {
                if let Some(screen) = timer_target.upgrade() {
                    screen.borrow_mut().model.mark_slow();
                    repaint(&screen);
                }
            });
    }
    repaint(screen);
}

/// The worker has answered: hand the answer to the model.
fn load_done(screen: &Rc<RefCell<Screen>>) {
    let answer = screen
        .borrow()
        .mailbox
        .as_ref()
        .and_then(|m| m.try_recv().ok());
    let Some(answer) = answer else {
        return;
    };
    {
        let mut state = screen.borrow_mut();
        state.mailbox = None;
        state.slow_timer.stop();
        state.model.set_result(answer);
    }
    repaint(screen);
}

/// An arrow, Home, End or Page key in the list: the model moves the selection, the window follows it.
fn list_key(screen: &Rc<RefCell<Screen>>, key: ListKey) {
    let Some(window) = screen.borrow().window.upgrade() else {
        return;
    };
    let visible = window.global::<AppsBridge>().get_list_visible_height();
    let row_height = window.global::<Theme>().get_row_height();
    let page = rows_per_page(visible, row_height) as isize;
    {
        let model = &mut screen.borrow_mut().model;
        match key {
            ListKey::Up => model.select_previous(),
            ListKey::Down => model.select_next(),
            ListKey::Home => model.select_first(),
            ListKey::End => model.select_last(),
            ListKey::PageUp => model.select_page(-page),
            ListKey::PageDown => model.select_page(page),
        }
    }
    repaint(screen);
}

/// Scrolls the list the smallest amount that shows the selected row completely.
/// Does nothing while the list has no size yet; `list-resized` calls this again once it has.
fn keep_in_view(window: &MainWindow, selected_index: Option<usize>) {
    let Some(index) = selected_index else {
        return;
    };
    let bridge = window.global::<AppsBridge>();
    let row_height = window.global::<Theme>().get_row_height();
    let current = content_y_to_offset(bridge.get_list_content_y());
    let wanted = scroll_to_show(index, row_height, bridge.get_list_visible_height(), current);
    if wanted != current {
        bridge.set_list_content_y(offset_to_content_y(wanted));
    }
}

/// The button in an empty state: what it does comes from the view, not from here.
fn empty_button_clicked(screen: &Rc<RefCell<Screen>>) {
    let action = {
        let state = screen.borrow();
        match apps_view(&state.model, &state.typed_search).body {
            Body::Empty(empty) => empty.action,
            _ => EmptyAction::None,
        }
    };
    match action {
        EmptyAction::None => return,
        EmptyAction::ShowTab(tab) => screen.borrow_mut().model.set_tab(tab),
        EmptyAction::ClearSearch => {
            let window = {
                let mut state = screen.borrow_mut();
                state.typed_search.clear();
                state.model.set_search("");
                state.window.clone()
            };
            if let Some(window) = window.upgrade() {
                window.global::<AppsBridge>().set_search_text("".into());
            }
        }
    }
    repaint(screen);
}

/// Copies the current view into the Slint window.
fn repaint(screen: &Rc<RefCell<Screen>>) {
    let mut state = screen.borrow_mut();
    let Some(window) = state.window.upgrade() else {
        return;
    };
    let view = apps_view(&state.model, &state.typed_search);
    let bridge = window.global::<AppsBridge>();

    bridge.set_subtitle_message(view.subtitle.message.as_str().into());
    bridge.set_subtitle_slow_note(view.subtitle.slow_note.as_str().into());
    bridge.set_subtitle_all(view.subtitle.all.as_str().into());
    bridge.set_subtitle_explicit(view.subtitle.explicit.as_str().into());
    bridge.set_tab_counts(strings(&view.tab_counts));
    bridge.set_current_tab(view.current_tab);
    bridge.set_current_sort(view.current_sort);
    bridge.set_footer(view.footer.as_str().into());
    show_body(&bridge, &view);
    show_details(&bridge, view.details.as_ref());

    match row_update(&state.shown_rows, &view.rows) {
        RowUpdate::Replace => state
            .rows
            .set_vec(view.rows.iter().map(package_row).collect::<Vec<_>>()),
        RowUpdate::Patch(changed) => {
            for index in changed {
                if let Some(row) = view.rows.get(index) {
                    state.rows.set_row_data(index, package_row(row));
                }
            }
        }
    }
    state.selected_index = view.selected_index;
    state.shown_rows = view.rows;

    // A list that is not on screen starts from the top next time; otherwise keep the selection visible.
    if matches!(view.body, Body::List) {
        keep_in_view(&window, state.selected_index);
    } else {
        bridge.set_list_content_y(0.0);
    }
}

fn show_body(bridge: &AppsBridge, view: &AppsView) {
    match &view.body {
        Body::Loading => bridge.set_body(BodyKind::Loading),
        Body::List => bridge.set_body(BodyKind::List),
        Body::Empty(empty) => {
            bridge.set_empty_title(empty.title.as_str().into());
            bridge.set_empty_body(empty.body.as_str().into());
            bridge.set_empty_button(empty.button.as_str().into());
            bridge.set_body(BodyKind::Empty);
        }
        Body::Error(error) => {
            bridge.set_error_title(error.title.as_str().into());
            bridge.set_error_body(error.body.as_str().into());
            bridge.set_error_technical(error.technical.as_str().into());
            bridge.set_error_unchanged(error.unchanged.as_str().into());
            bridge.set_error_report(error.report.as_str().into());
            bridge.set_body(BodyKind::Error);
        }
    }
}

fn show_details(bridge: &AppsBridge, details: Option<&DetailsView>) {
    let Some(details) = details else {
        bridge.set_has_details(false);
        return;
    };
    bridge.set_details(DetailsData {
        name: details.name.as_str().into(),
        description: details.description.as_str().into(),
        size: details.size.as_str().into(),
        version: details.version.as_str().into(),
        why_here: details.why_here.as_str().into(),
        needed_by: strings(&details.needed_by),
        more: details.more.as_str().into(),
        nothing_needs_it: details.nothing_needs_it.as_str().into(),
        note: details.note.as_str().into(),
        technical_source: details.technical_source.as_str().into(),
        technical_bytes: details.technical_bytes.as_str().into(),
    });
    bridge.set_has_details(true);
}

fn package_row(row: &RowView) -> PackageRow {
    PackageRow {
        letter: row.letter.as_str().into(),
        name: row.name.as_str().into(),
        version: row.version.as_str().into(),
        description: row.description.as_str().into(),
        size: row.size.as_str().into(),
        selected: row.selected,
    }
}

fn strings(items: &[String]) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = items.iter().map(|s| s.as_str().into()).collect();
    ModelRc::new(VecModel::from(items))
}
