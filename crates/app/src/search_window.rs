//! Glue between the Search window and the tested code. It decides nothing about wording or state:
//! `SearchModel` holds the state, `search_view` words it, and this file copies the result into Slint,
//! runs pacman on worker threads, and owns the three timers.
//!
//! Threads: `search`, `package_details` and `sync_lists` run on worker threads. The `SearchModel` never leaves
//! the UI thread. A worker puts its answer in a mailbox (a channel) and asks the UI thread, through a weak
//! window handle, to fire `search-done` / `details-done` / `lists-done`; the UI thread then reads the mailbox.
//!
//! Timers: a 300 ms debounce after the last keystroke before searching, a 150 ms pause on a row before asking
//! `pacman -Si` about it (so arrowing through rows does not queue lookups), and a 10 s "taking longer" note.
//! The model decides what to run (one search and one lookup at a time, newest wins); this file only starts them.

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use tidewise_app::apps_view::RowUpdate;
use tidewise_app::list_scroll::{
    content_y_to_offset, offset_to_content_y, rows_per_page, scroll_to_show,
};
use tidewise_app::search_model::{SearchModel, SearchTicket};
use tidewise_app::search_view::{
    Body, DetailsKind, DetailsView, ResultRowView, example_query, row_update, search_view,
};
use tidewise_pacman::{DetailsLookup, PackageRef, PacmanError, SearchResult, SyncLists};

use crate::{
    ListKey, MainWindow, ResultRow, SearchBodyKind, SearchBridge, SearchDetailsData,
    SearchDetailsKind, Theme,
};

/// Search this long after the last keystroke.
const DEBOUNCE: Duration = Duration::from_millis(300);
/// Ask for a package's details after the selection has rested on it this long.
const DWELL: Duration = Duration::from_millis(150);
/// After this long the screen adds "This is taking longer than usual." (pacman is not stopped).
const SLOW_AFTER: Duration = Duration::from_secs(10);

type SearchAnswer = (u64, Result<Vec<SearchResult>, PacmanError>);
type DetailsAnswer = (PackageRef, Result<DetailsLookup, PacmanError>);
type ListsAnswer = Result<SyncLists, PacmanError>;

/// Everything the Search screen owns. Lives on the UI thread only.
struct Screen {
    window: slint::Weak<MainWindow>,
    model: SearchModel,
    /// The rows Slint is showing now, to work out the smallest update.
    shown_rows: Vec<ResultRowView>,
    /// Where the selected row is in the list, to keep it on screen.
    selected_index: Option<usize>,
    /// The package the pause timer was last started for.
    watched: Option<PackageRef>,
    rows: Rc<VecModel<ResultRow>>,
    /// Where the worker threads leave their answers (one mailbox per kind of work).
    search_mailbox: (mpsc::Sender<SearchAnswer>, mpsc::Receiver<SearchAnswer>),
    details_mailbox: (mpsc::Sender<DetailsAnswer>, mpsc::Receiver<DetailsAnswer>),
    lists_mailbox: (mpsc::Sender<ListsAnswer>, mpsc::Receiver<ListsAnswer>),
    debounce_timer: Timer,
    dwell_timer: Timer,
    slow_timer: Timer,
}

/// Connects the window to the model and checks the package lists.
pub fn start(window: &MainWindow) {
    let rows = Rc::new(VecModel::default());
    let bridge = window.global::<SearchBridge>();
    bridge.set_rows(ModelRc::from(rows.clone()));

    let screen = Rc::new(RefCell::new(Screen {
        window: window.as_weak(),
        model: SearchModel::new(),
        shown_rows: Vec::new(),
        selected_index: None,
        watched: None,
        rows,
        search_mailbox: mpsc::channel(),
        details_mailbox: mpsc::channel(),
        lists_mailbox: mpsc::channel(),
        debounce_timer: Timer::default(),
        dwell_timer: Timer::default(),
        slow_timer: Timer::default(),
    }));

    let handle = screen.clone();
    bridge.on_search_edited(move |text| search_edited(&handle, &text));
    let handle = screen.clone();
    bridge.on_example_chosen(move |index| {
        if let Some(example) = example_query(index) {
            if let Some(window) = handle.borrow().window.upgrade() {
                window
                    .global::<SearchBridge>()
                    .set_search_text(example.into());
            }
            {
                let mut state = handle.borrow_mut();
                state.model.set_query(example);
                state.debounce_timer.stop();
            }
            begin_search(&handle);
        }
    });
    let handle = screen.clone();
    bridge.on_row_chosen(move |index| row_chosen(&handle, index));
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
    bridge.on_clear_search(move || clear_search(&handle));
    let handle = screen.clone();
    bridge.on_try_again(move || begin_search(&handle));
    let handle = screen.clone();
    bridge.on_details_try_again(move || {
        let package = handle.borrow_mut().model.retry_details();
        if let Some(package) = package {
            spawn_details(&handle, package);
        }
        repaint(&handle);
    });
    let handle = screen.clone();
    bridge.on_search_done(move || search_done(&handle));
    let handle = screen.clone();
    bridge.on_details_done(move || details_done(&handle));
    let handle = screen.clone();
    bridge.on_lists_done(move || lists_done(&handle));

    spawn_lists(&screen);
    repaint(&screen);
}

/// The user typed: remember the text, and search once typing has paused.
fn search_edited(screen: &Rc<RefCell<Screen>>, text: &SharedString) {
    let blank = {
        let mut state = screen.borrow_mut();
        state.model.set_query(text);
        state.model.query().is_empty()
    };
    if blank {
        screen.borrow().debounce_timer.stop();
        repaint(screen);
        watch_selection(screen);
        return;
    }
    let target: Weak<RefCell<Screen>> = Rc::downgrade(screen);
    // Starting the same timer again restarts it, so only the pause after the last keystroke counts.
    screen
        .borrow()
        .debounce_timer
        .start(TimerMode::SingleShot, DEBOUNCE, move || {
            if let Some(screen) = target.upgrade() {
                begin_search(&screen);
            }
        });
}

/// "Clear search": empties the box and goes back to the start card.
fn clear_search(screen: &Rc<RefCell<Screen>>) {
    let window = {
        let mut state = screen.borrow_mut();
        state.model.set_query("");
        state.debounce_timer.stop();
        state.window.clone()
    };
    if let Some(window) = window.upgrade() {
        window.global::<SearchBridge>().set_search_text("".into());
    }
    repaint(screen);
    watch_selection(screen);
}

/// Asks the model to search for the current text and starts the worker if it says so.
fn begin_search(screen: &Rc<RefCell<Screen>>) {
    let ticket = screen.borrow_mut().model.start_search();
    if let Some(ticket) = ticket {
        spawn_search(screen, ticket);
    }
    repaint(screen);
}

fn spawn_search(screen: &Rc<RefCell<Screen>>, ticket: SearchTicket) {
    let (sender, window) = {
        let state = screen.borrow();
        (state.search_mailbox.0.clone(), state.window.clone())
    };
    std::thread::spawn(move || {
        let result = tidewise_pacman::search(&ticket.query);
        // If the window closed meanwhile, nobody reads this and that is fine.
        let _ = sender.send((ticket.number, result));
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = window.upgrade() {
                window.global::<SearchBridge>().invoke_search_done();
            }
        });
    });

    let target: Weak<RefCell<Screen>> = Rc::downgrade(screen);
    // Starting the same timer again restarts it, so an older search's timer can never fire.
    screen
        .borrow()
        .slow_timer
        .start(TimerMode::SingleShot, SLOW_AFTER, move || {
            if let Some(screen) = target.upgrade() {
                screen.borrow_mut().model.mark_slow();
                repaint(&screen);
            }
        });
}

/// A search worker has answered: hand every answer in the mailbox to the model.
fn search_done(screen: &Rc<RefCell<Screen>>) {
    let answers: Vec<SearchAnswer> = {
        let state = screen.borrow();
        state.search_mailbox.1.try_iter().collect()
    };
    for (number, result) in answers {
        let next = screen.borrow_mut().model.search_done(number, result);
        if let Some(next) = next {
            spawn_search(screen, next);
        }
    }
    repaint(screen);
    watch_selection(screen);
}

/// Starts the pause timer when the selected package changed. When it ends, the model is asked for the lookup.
fn watch_selection(screen: &Rc<RefCell<Screen>>) {
    let current = screen.borrow().model.selected_ref();
    if screen.borrow().watched == current {
        return;
    }
    screen.borrow_mut().watched.clone_from(&current);
    if current.is_none() {
        screen.borrow().dwell_timer.stop();
        return;
    }
    let target: Weak<RefCell<Screen>> = Rc::downgrade(screen);
    screen
        .borrow()
        .dwell_timer
        .start(TimerMode::SingleShot, DWELL, move || {
            if let Some(screen) = target.upgrade() {
                let package = screen.borrow_mut().model.request_details();
                if let Some(package) = package {
                    spawn_details(&screen, package);
                }
            }
        });
}

fn spawn_details(screen: &Rc<RefCell<Screen>>, package: PackageRef) {
    let (sender, window) = {
        let state = screen.borrow();
        (state.details_mailbox.0.clone(), state.window.clone())
    };
    std::thread::spawn(move || {
        let result = tidewise_pacman::package_details(std::slice::from_ref(&package));
        let _ = sender.send((package, result));
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = window.upgrade() {
                window.global::<SearchBridge>().invoke_details_done();
            }
        });
    });
}

/// A details worker has answered: hand every answer in the mailbox to the model.
fn details_done(screen: &Rc<RefCell<Screen>>) {
    let answers: Vec<DetailsAnswer> = {
        let state = screen.borrow();
        state.details_mailbox.1.try_iter().collect()
    };
    for (package, result) in answers {
        let next = screen.borrow_mut().model.details_done(&package, result);
        if let Some(next) = next {
            spawn_details(screen, next);
        }
    }
    repaint(screen);
}

fn spawn_lists(screen: &Rc<RefCell<Screen>>) {
    let (sender, window) = {
        let state = screen.borrow();
        (state.lists_mailbox.0.clone(), state.window.clone())
    };
    std::thread::spawn(move || {
        let _ = sender.send(tidewise_pacman::sync_lists());
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = window.upgrade() {
                window.global::<SearchBridge>().invoke_lists_done();
            }
        });
    });
}

/// The package-lists check has answered.
fn lists_done(screen: &Rc<RefCell<Screen>>) {
    let answers: Vec<ListsAnswer> = {
        let state = screen.borrow();
        state.lists_mailbox.1.try_iter().collect()
    };
    for answer in answers {
        screen.borrow_mut().model.set_sync_lists(answer);
    }
    repaint(screen);
}

/// A row was clicked: select it.
fn row_chosen(screen: &Rc<RefCell<Screen>>, index: i32) {
    {
        let mut state = screen.borrow_mut();
        let chosen = usize::try_from(index)
            .ok()
            .and_then(|index| state.shown_rows.get(index))
            .map(|row| (row.repo.clone(), row.name.clone()));
        if let Some((repo, name)) = chosen {
            state.model.select(&repo, &name);
        }
    }
    repaint(screen);
    watch_selection(screen);
}

/// An arrow, Home, End or Page key in the list: the model moves the selection, the window follows it.
fn list_key(screen: &Rc<RefCell<Screen>>, key: ListKey) {
    let Some(window) = screen.borrow().window.upgrade() else {
        return;
    };
    let visible = window.global::<SearchBridge>().get_list_visible_height();
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
    watch_selection(screen);
}

/// Scrolls the list the smallest amount that shows the selected row completely.
/// Does nothing while the list has no size yet; `list-resized` calls this again once it has.
fn keep_in_view(window: &MainWindow, selected_index: Option<usize>) {
    let Some(index) = selected_index else {
        return;
    };
    let bridge = window.global::<SearchBridge>();
    let row_height = window.global::<Theme>().get_row_height();
    let current = content_y_to_offset(bridge.get_list_content_y());
    let wanted = scroll_to_show(index, row_height, bridge.get_list_visible_height(), current);
    if wanted != current {
        bridge.set_list_content_y(offset_to_content_y(wanted));
    }
}

/// Copies the current view into the Slint window.
fn repaint(screen: &Rc<RefCell<Screen>>) {
    let mut state = screen.borrow_mut();
    let Some(window) = state.window.upgrade() else {
        return;
    };
    let view = search_view(&state.model, SystemTime::now());
    let bridge = window.global::<SearchBridge>();

    bridge.set_subtitle(view.subtitle.as_str().into());
    bridge.set_status(view.status.as_str().into());
    bridge.set_inline_note(view.inline_note.as_str().into());
    bridge.set_start_title(view.start.title.as_str().into());
    bridge.set_start_body(view.start.body.as_str().into());
    bridge.set_start_examples(strings(&view.start.examples));
    bridge.set_no_lists_title(view.no_lists.title.as_str().into());
    bridge.set_no_lists_body(view.no_lists.body.as_str().into());
    bridge.set_no_lists_next_step(view.no_lists.next_step.as_str().into());
    bridge.set_no_lists_technical(strings(&view.no_lists.technical));
    bridge.set_no_results_title(view.no_results.title.as_str().into());
    bridge.set_no_results_body(view.no_results.body.as_str().into());
    bridge.set_no_results_button(view.no_results.button.as_str().into());
    bridge.set_caption(view.caption.as_str().into());
    bridge.set_footer(view.footer.as_str().into());
    match &view.body {
        Body::Start => bridge.set_body(SearchBodyKind::Start),
        Body::NoLists => bridge.set_body(SearchBodyKind::NoLists),
        Body::Loading => bridge.set_body(SearchBodyKind::Loading),
        Body::Results => bridge.set_body(SearchBodyKind::Results),
        Body::NoResults => bridge.set_body(SearchBodyKind::NoResults),
        Body::BadQuery => bridge.set_body(SearchBodyKind::BadQuery),
        Body::Error(error) => {
            bridge.set_error_title(error.title.as_str().into());
            bridge.set_error_body(error.body.as_str().into());
            bridge.set_error_technical(error.technical.as_str().into());
            bridge.set_error_unchanged(error.unchanged.as_str().into());
            bridge.set_error_report(error.report.as_str().into());
            bridge.set_body(SearchBodyKind::Error);
        }
    }
    show_details(&bridge, view.details.as_ref());

    match row_update(&state.shown_rows, &view.rows) {
        RowUpdate::Replace => state
            .rows
            .set_vec(view.rows.iter().map(result_row).collect::<Vec<_>>()),
        RowUpdate::Patch(changed) => {
            for index in changed {
                if let Some(row) = view.rows.get(index) {
                    state.rows.set_row_data(index, result_row(row));
                }
            }
        }
    }
    state.selected_index = view.selected_index;
    state.shown_rows = view.rows;

    // A list that is not on screen starts from the top next time; otherwise keep the selection visible.
    if matches!(view.body, Body::Results) {
        keep_in_view(&window, state.selected_index);
    } else {
        bridge.set_list_content_y(0.0);
    }
}

fn show_details(bridge: &SearchBridge, details: Option<&DetailsView>) {
    let Some(details) = details else {
        bridge.set_has_details(false);
        return;
    };
    let (error_title, error_body, error_technical, error_unchanged, error_report) =
        match &details.error {
            Some(error) => (
                error.title.as_str(),
                error.body.as_str(),
                error.technical.as_str(),
                error.unchanged.as_str(),
                error.report.as_str(),
            ),
            None => ("", "", "", "", ""),
        };
    bridge.set_details(SearchDetailsData {
        kind: match details.kind {
            DetailsKind::Loading => SearchDetailsKind::Loading,
            DetailsKind::Found => SearchDetailsKind::Found,
            DetailsKind::NotFound => SearchDetailsKind::NotFound,
            DetailsKind::Failed => SearchDetailsKind::Failed,
        },
        letter: details.letter.as_str().into(),
        name: details.name.as_str().into(),
        description: details.description.as_str().into(),
        version: details.version.as_str().into(),
        repo: details.repo.as_str().into(),
        repo_tag: details.repo_tag.as_str().into(),
        installed_note: details.installed_note.as_str().into(),
        matched_note: details.matched_note.as_str().into(),
        message: details.message.as_str().into(),
        download_number: details.download_number.as_str().into(),
        download_unit: details.download_unit.as_str().into(),
        installed_number: details.installed_number.as_str().into(),
        installed_unit: details.installed_unit.as_str().into(),
        needs_count: details.needs_count.as_str().into(),
        needs: strings(&details.needs),
        needs_more: details.needs_more.as_str().into(),
        needs_nothing: details.needs_nothing.as_str().into(),
        provides: strings(&details.provides),
        provides_more: details.provides_more.as_str().into(),
        note: details.note.as_str().into(),
        error_title: error_title.into(),
        error_body: error_body.into(),
        error_technical: error_technical.into(),
        error_unchanged: error_unchanged.into(),
        error_report: error_report.into(),
        technical: strings(&details.technical),
    });
    bridge.set_has_details(true);
}

fn result_row(row: &ResultRowView) -> ResultRow {
    ResultRow {
        letter: row.letter.as_str().into(),
        name: row.name.as_str().into(),
        version: row.version.as_str().into(),
        repo: row.repo.as_str().into(),
        tag: row.tag.as_str().into(),
        installed: row.installed,
        description: row.description.as_str().into(),
        selected: row.selected,
    }
}

fn strings(items: &[String]) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = items.iter().map(|s| s.as_str().into()).collect();
    ModelRc::new(VecModel::from(items))
}
