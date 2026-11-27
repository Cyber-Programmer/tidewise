//! Glue between the update screen and the tested code (WBS 6.5). It decides nothing about wording or state:
//! `UpdateModel` holds the state, `update_view` words it, and this file copies the result into Slint and runs the
//! work on threads, the same way `install_window` does.
//!
//! A check refreshes the app's own private copy of the package lists (as the user, no password, up to 60 s of waiting)
//! and reads it with pacman. The helper job runs on a thread of its own; closing the window ends the app, not the
//! helper.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::SystemTime;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use tidewise_app::helper_run::{JobEnd, run_helper};
use tidewise_app::install_view::{FocusTarget, ReviewRow, StageState, StageView, TileView};
use tidewise_app::job::JobTicket;
use tidewise_app::list_scroll::{
    ScrollKey, content_y_to_offset, offset_to_content_y, scroll_by_key,
};
use tidewise_app::update_check::review_updates;
use tidewise_app::update_model::{CheckResult, CheckTicket, Phase, UpdateModel};
use tidewise_app::update_view::{UpdateKind, UpdateView, update_view};
use tidewise_helper_protocol::Event;

use crate::apps_window::Apps;
use crate::search_window::Search;
use crate::{
    InstallFocus, InstallStageData, InstallStageState, MainWindow, NavBridge, ReviewKey,
    ReviewRowData, Screen as NavScreen, Theme, TileData, UpdateBridge, UpdateCardKind,
};

/// What the helper's worker thread has to say about job `.0`.
type JobMessage = (u64, JobNews);

/// What a check's thread has to say: the check's number and what it found.
type CheckMessage = (u64, CheckResult);

enum JobNews {
    Event(Event),
    Ended(JobEnd),
}

/// Everything the update screen owns. Lives on the UI thread only.
struct State {
    window: slint::Weak<MainWindow>,
    model: UpdateModel,
    checks: (mpsc::Sender<CheckMessage>, mpsc::Receiver<CheckMessage>),
    jobs: (mpsc::Sender<JobMessage>, mpsc::Receiver<JobMessage>),
    apps: Apps,
    search: Search,
    /// The last focus move painted: a new card starts its list at the top.
    shown_serial: u32,
}

/// How other screens open the update screen.
#[derive(Clone)]
pub struct Updates {
    state: Rc<RefCell<State>>,
}

/// Connects the window to the model.
pub fn start(window: &MainWindow, apps: Apps, search: Search) -> Updates {
    let state = Rc::new(RefCell::new(State {
        window: window.as_weak(),
        model: UpdateModel::new(),
        checks: mpsc::channel(),
        jobs: mpsc::channel(),
        apps,
        search,
        shown_serial: 0,
    }));
    let bridge = window.global::<UpdateBridge>();
    let handle = state.clone();
    bridge.on_primary(move || primary(&handle));
    let handle = state.clone();
    bridge.on_secondary(move || {
        handle.borrow_mut().model.cancel();
        repaint(&handle);
    });
    let handle = state.clone();
    bridge.on_review_key(move |key, content| {
        if let Some(window) = handle.borrow().window.upgrade() {
            scroll_list(&window, key, content);
        }
    });
    let handle = state.clone();
    bridge.on_check_done(move || check_done(&handle));
    let handle = state.clone();
    bridge.on_job_event(move || job_news(&handle));
    repaint(&state);
    Updates { state }
}

impl Updates {
    /// The install screen's "Update system" button: shows the screen and checks.
    pub fn open(&self) {
        if let Some(window) = self.state.borrow().window.upgrade() {
            window.global::<NavBridge>().set_current(NavScreen::Updates);
        }
        check(&self.state);
    }
}

fn check(state: &Rc<RefCell<State>>) {
    let ticket = state.borrow_mut().model.check();
    if let Some(ticket) = ticket {
        spawn_check(state, ticket);
    }
    repaint(state);
}

/// The main button, whatever the card calls it: check, update, or "Done".
fn primary(state: &Rc<RefCell<State>>) {
    enum Press {
        Update,
        Done,
        Check,
    }
    let press = match state.borrow().model.phase() {
        Phase::Review(_) => Press::Update,
        Phase::Updated { .. } => Press::Done,
        _ => Press::Check,
    };
    match press {
        Press::Update => {
            let job = state.borrow_mut().model.start_update();
            if let Some(job) = job {
                spawn_job(state, job);
            }
        }
        Press::Done => state.borrow_mut().model.cancel(),
        Press::Check => return check(state),
    }
    repaint(state);
}

fn spawn_check(state: &Rc<RefCell<State>>, ticket: CheckTicket) {
    let (sender, window) = {
        let state = state.borrow();
        (state.checks.0.clone(), state.window.clone())
    };
    std::thread::spawn(move || {
        let result = review_updates(
            || tidewise_pacman::refresh_private_lists(ticket.why),
            tidewise_pacman::update_lookup,
            || {
                tidewise_pacman::sync_lists()
                    .ok()
                    .and_then(|lists| lists.newest)
            },
            SystemTime::now(),
        );
        // If the window closed meanwhile, nobody reads this and that is fine.
        let _ = sender.send((ticket.number, result));
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = window.upgrade() {
                window.global::<UpdateBridge>().invoke_check_done();
            }
        });
    });
}

fn check_done(state: &Rc<RefCell<State>>) {
    let answers: Vec<CheckMessage> = state.borrow().checks.1.try_iter().collect();
    for (number, result) in answers {
        state.borrow_mut().model.checked(number, result);
    }
    repaint(state);
}

/// Runs the helper on its own thread. `run_helper` blocks until the helper is gone.
fn spawn_job(state: &Rc<RefCell<State>>, job: JobTicket) {
    let (sender, window) = {
        let state = state.borrow();
        (state.jobs.0.clone(), state.window.clone())
    };
    std::thread::spawn(move || {
        let number = job.number;
        let tell = |news: JobNews| {
            let _ = sender.send((number, news));
            let window = window.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(window) = window.upgrade() {
                    window.global::<UpdateBridge>().invoke_job_event();
                }
            });
        };
        let end = run_helper(job.request, |event| tell(JobNews::Event(event)));
        tell(JobNews::Ended(end));
    });
}

fn job_news(state: &Rc<RefCell<State>>) {
    let news: Vec<JobMessage> = state.borrow().jobs.1.try_iter().collect();
    if news.is_empty() {
        return;
    }
    let mut changed = false;
    let mut recheck = None;
    for (number, news) in news {
        let model = &mut state.borrow_mut().model;
        match news {
            JobNews::Event(event) => model.job_event(number, event),
            JobNews::Ended(end) => {
                let ended = model.job_ended(number, end);
                changed |= ended.may_have_changed;
                recheck = ended.check.or(recheck);
            }
        }
    }
    if let Some(ticket) = recheck {
        spawn_check(state, ticket);
    }
    repaint(state);
    if changed {
        let (apps, search) = {
            let state = state.borrow();
            (state.apps.clone(), state.search.clone())
        };
        apps.reload();
        search.refresh();
    }
}

/// Copies the current view into the Slint window.
fn repaint(state: &Rc<RefCell<State>>) {
    let mut state = state.borrow_mut();
    let Some(window) = state.window.upgrade() else {
        return;
    };
    let view = update_view(&state.model);
    let bridge = window.global::<UpdateBridge>();
    if view.focus_serial != state.shown_serial {
        bridge.set_list_y(0.0);
        state.shown_serial = view.focus_serial;
    }
    set_texts(&bridge, &view);
}

fn scroll_list(window: &MainWindow, key: ReviewKey, content: f32) {
    let key = match key {
        ReviewKey::Up => ScrollKey::Up,
        ReviewKey::Down => ScrollKey::Down,
        ReviewKey::PageUp => ScrollKey::PageUp,
        ReviewKey::PageDown => ScrollKey::PageDown,
        ReviewKey::Home => ScrollKey::Home,
        ReviewKey::End => ScrollKey::End,
    };
    let line = window.global::<Theme>().get_review_row() + 1.0;
    let bridge = window.global::<UpdateBridge>();
    let current = content_y_to_offset(bridge.get_list_y());
    let wanted = scroll_by_key(
        key,
        current,
        line,
        content,
        bridge.get_list_visible_height(),
    );
    bridge.set_list_y(offset_to_content_y(wanted));
}

fn set_texts(bridge: &UpdateBridge, view: &UpdateView) {
    bridge.set_kind(match view.kind {
        UpdateKind::Start => UpdateCardKind::Start,
        UpdateKind::Checking => UpdateCardKind::Checking,
        UpdateKind::Review => UpdateCardKind::Review,
        UpdateKind::UpToDate => UpdateCardKind::UpToDate,
        UpdateKind::Blocked => UpdateCardKind::Blocked,
        UpdateKind::Alert => UpdateCardKind::Alert,
        UpdateKind::Progress => UpdateCardKind::Progress,
        UpdateKind::Done => UpdateCardKind::Done,
    });
    bridge.set_title(view.title.as_str().into());
    bridge.set_body(view.body.as_str().into());
    bridge.set_advice(view.advice.as_str().into());
    bridge.set_tiles(model(view.tiles.iter().map(tile)));
    bridge.set_rows(model(view.rows.iter().map(row)));
    bridge.set_held_title(view.held_title.as_str().into());
    bridge.set_held_lines(strings(&view.held_lines));
    bridge.set_held_warning(view.held_warning.as_str().into());
    bridge.set_notes(strings(&view.notes));
    bridge.set_has_primary(view.primary.is_some());
    bridge.set_primary_text(
        view.primary
            .as_ref()
            .map_or("", |button| button.label.as_str())
            .into(),
    );
    bridge.set_has_secondary(view.secondary.is_some());
    bridge.set_secondary_text(
        view.secondary
            .as_ref()
            .map_or("", |button| button.label.as_str())
            .into(),
    );
    bridge.set_primary_note(view.primary_note.as_str().into());
    bridge.set_hint(view.hint.as_str().into());
    bridge.set_technical(strings(&view.technical));
    bridge.set_stages(model(view.stages.iter().map(stage)));
    bridge.set_percent(i32::from(view.percent));
    bridge.set_percent_text(view.percent_text.as_str().into());
    bridge.set_attention(view.attention);
    bridge.set_focus(match view.focus {
        FocusTarget::List => InstallFocus::List,
        FocusTarget::Primary => InstallFocus::Primary,
        FocusTarget::Secondary => InstallFocus::Secondary,
    });
    // Last, so every property the new card needs is already set when the focus moves.
    bridge.set_focus_serial(i32::try_from(view.focus_serial).unwrap_or(0));
}

fn model<T: Clone + 'static>(items: impl Iterator<Item = T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(items.collect::<Vec<_>>()))
}

fn strings(items: &[String]) -> ModelRc<SharedString> {
    model(items.iter().map(|s| SharedString::from(s.as_str())))
}

fn tile(tile: &TileView) -> TileData {
    TileData {
        label: tile.label.as_str().into(),
        number: tile.number.as_str().into(),
        unit: tile.unit.as_str().into(),
        words: tile.words,
    }
}

fn row(row: &ReviewRow) -> ReviewRowData {
    ReviewRowData {
        name: row.name.as_str().into(),
        version: row.version.as_str().into(),
        why: row.why.as_str().into(),
        size: row.size.as_str().into(),
        bold: row.bold,
    }
}

fn stage(stage: &StageView) -> InstallStageData {
    InstallStageData {
        label: stage.label.as_str().into(),
        state: match stage.state {
            StageState::Done => InstallStageState::Done,
            StageState::Now => InstallStageState::Now,
            StageState::Later => InstallStageState::Later,
        },
    }
}
