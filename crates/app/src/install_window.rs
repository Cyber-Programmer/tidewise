//! Glue between the install screens and the tested code. It decides nothing about wording or state:
//! `InstallModel` holds the state, `install_view` words it, and this file copies the result into Slint and runs
//! pacman on a worker thread.
//!
//! Threads: a resolve (the updates check once, then `tidewise_pacman::resolve`, about 1.5–5 s plus any download of the
//! lists) runs on a worker thread.
//! The `InstallModel` never leaves the UI thread, and it lets only one worker run at a time (a newer wish waits,
//! see its docs). The worker puts its answer in a mailbox (a channel) and asks the UI thread, through a weak window
//! handle, to fire `resolve-done`; the UI thread then gives the model every answer in the mailbox.
//!
//! The helper job (WBS 6.4b) runs on a worker thread of its own for as long as the password window and pacman take.
//! It puts each event, and then the end, in a second mailbox and fires `job-event`. Closing the window ends the app,
//! not the helper.
//!
//! One timer: after 10 s of waiting the card adds "This is taking longer than usual." (pacman is not stopped).
//! pacman only reads here (`-Qu`, and the resolver's `--print`, `-Si`, `-Ss`, `-Sg`, `-Qi`), except the refresh of the
//! app's own private copy of the package lists (`refresh_private_lists`).

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use tidewise_app::helper_run::{JobEnd, run_install};
use tidewise_app::install_model::{Back, InstallModel, JobTicket, Phase, Ticket, WorkAnswer};
use tidewise_app::install_view::{
    CardKind, FocusTarget, InstallView, InstalledRow, OptionView, ReviewRow, StageState, StageView,
    TileView, install_view,
};
use tidewise_app::list_scroll::{
    ScrollKey, content_y_to_offset, moved_selection, offset_to_content_y, scroll_by_key,
    scroll_to_show,
};
use tidewise_app::update_check::{ListsNote, check_for_updates};
use tidewise_helper_protocol::Event;
use tidewise_pacman::{PackageRef, PacmanError, RefreshWhy};

use crate::apps_window::Apps;
use crate::{
    ChoiceOption, InstallBridge, InstallCardKind, InstallFocus, InstallStageData,
    InstallStageState, InstalledRowData, MainWindow, NavBridge, ReviewKey, ReviewRowData,
    Screen as NavScreen, Theme, TileData,
};

/// After this long the card adds "This is taking longer than usual."
const SLOW_AFTER: Duration = Duration::from_secs(10);

/// The ticket's number, what the lists check had to say (`None`: it was not asked), and the answer.
type Answer = (
    u64,
    Option<Option<ListsNote>>,
    Result<WorkAnswer, PacmanError>,
);

/// What the helper's worker thread has to say about job `.0`.
type JobMessage = (u64, JobNews);

enum JobNews {
    Event(Event),
    Ended(JobEnd),
}

/// Everything the install screens own. Lives on the UI thread only.
struct Screen {
    window: slint::Weak<MainWindow>,
    model: InstallModel,
    /// The options Slint is showing now, to work out the smallest update (so a toggled row keeps the focus).
    shown_options: Vec<OptionView>,
    options: Rc<VecModel<ChoiceOption>>,
    installed_rows: Rc<VecModel<InstalledRowData>>,
    mailbox: (mpsc::Sender<Answer>, mpsc::Receiver<Answer>),
    jobs: (mpsc::Sender<JobMessage>, mpsc::Receiver<JobMessage>),
    apps: Apps,
    slow_timer: Timer,
    /// The card, and the number of its last focus move, as last painted: a new card starts its list at the top.
    shown_kind: CardKind,
    shown_serial: u32,
    /// Position of the row to keep in view in a radio list (the chosen one); `None` for other cards.
    chosen_row: Option<usize>,
}

/// How other screens start an install.
#[derive(Clone)]
pub struct Install {
    screen: Rc<RefCell<Screen>>,
}

/// Connects the window to the model.
pub fn start(window: &MainWindow, apps: Apps) -> Install {
    let options = Rc::new(VecModel::default());
    let installed_rows = Rc::new(VecModel::default());
    let bridge = window.global::<InstallBridge>();
    bridge.set_options(ModelRc::from(options.clone()));
    bridge.set_installed_rows(ModelRc::from(installed_rows.clone()));

    let screen = Rc::new(RefCell::new(Screen {
        window: window.as_weak(),
        model: InstallModel::new(),
        shown_options: Vec::new(),
        options,
        installed_rows,
        mailbox: mpsc::channel(),
        jobs: mpsc::channel(),
        apps,
        slow_timer: Timer::default(),
        shown_kind: CardKind::Closed,
        shown_serial: 0,
        chosen_row: None,
    }));

    let handle = screen.clone();
    bridge.on_choose(move |index| {
        if let Ok(index) = usize::try_from(index) {
            handle.borrow_mut().model.choose(index);
        }
        repaint(&handle);
    });
    let handle = screen.clone();
    bridge.on_row_focused(move |index| {
        if let (Ok(index), Some(window)) =
            (usize::try_from(index), handle.borrow().window.upgrade())
        {
            keep_in_view(&window, handle.borrow().shown_kind, Some(index));
        }
    });
    let handle = screen.clone();
    bridge.on_row_moved(move |index, delta| {
        let Some(window) = handle.borrow().window.upgrade() else {
            return;
        };
        let count = handle.borrow().shown_options.len();
        let current = usize::try_from(index).ok();
        let target = moved_selection(current, count, if delta < 0 { -1 } else { 1 })
            .and_then(|row| i32::try_from(row).ok());
        if let Some(target) = target {
            let bridge = window.global::<InstallBridge>();
            bridge.set_focus_row(target);
            bridge.set_focus_row_request(bridge.get_focus_row_request().wrapping_add(1));
        }
    });
    let handle = screen.clone();
    bridge.on_list_resized(move || {
        let (window, kind, chosen) = {
            let state = handle.borrow();
            (state.window.upgrade(), state.shown_kind, state.chosen_row)
        };
        if let Some(window) = window {
            keep_in_view(&window, kind, chosen);
        }
    });
    let handle = screen.clone();
    bridge.on_toggle(move |row| {
        // The row's number is not the member's: installed members have no row.
        let member = usize::try_from(row).ok().and_then(|row| {
            let state = handle.borrow();
            state
                .shown_options
                .get(row)
                .and_then(|option| option.member)
        });
        if let Some(member) = member {
            handle.borrow_mut().model.toggle(member);
        }
        repaint(&handle);
    });
    let handle = screen.clone();
    bridge.on_review_key(move |key, content| {
        if let Some(window) = handle.borrow().window.upgrade() {
            scroll_review(&window, key, content);
        }
    });
    let handle = screen.clone();
    bridge.on_select_all(move || {
        handle.borrow_mut().model.select_all();
        repaint(&handle);
    });
    let handle = screen.clone();
    bridge.on_select_none(move || {
        handle.borrow_mut().model.select_none();
        repaint(&handle);
    });
    let handle = screen.clone();
    bridge.on_primary(move || primary(&handle));
    let handle = screen.clone();
    bridge.on_secondary(move || {
        let _: Back = handle.borrow_mut().model.back();
        repaint(&handle);
    });
    let handle = screen.clone();
    bridge.on_cancel(move || {
        handle.borrow_mut().model.cancel();
        repaint(&handle);
    });
    let handle = screen.clone();
    bridge.on_resolve_done(move || resolve_done(&handle));
    let handle = screen.clone();
    bridge.on_job_event(move || job_news(&handle));

    repaint(&screen);
    Install { screen }
}

impl Install {
    /// The Install button of Search details: opens the install screen for this package.
    pub fn open_package(&self, package: PackageRef) {
        let ticket = self.screen.borrow_mut().model.open(package);
        self.show(ticket);
    }

    /// The debug entry's way in (WBS 6.12 adds the real one): install a whole group.
    #[cfg(debug_assertions)]
    pub fn open_group(&self, group: &str) {
        let ticket = self.screen.borrow_mut().model.open_group(group);
        self.show(ticket);
    }

    fn show(&self, ticket: Option<Ticket>) {
        if let Some(window) = self.screen.borrow().window.upgrade() {
            window.global::<NavBridge>().set_current(NavScreen::Search);
        }
        if let Some(ticket) = ticket {
            spawn_worker(&self.screen, ticket);
        }
        repaint(&self.screen);
    }
}

/// The main button of the card: the next question, "Install … instead", "Try again", "Install", "Check again" or
/// "Show in Apps", by what is shown.
fn primary(screen: &Rc<RefCell<Screen>>) {
    let shown = match screen.borrow().model.phase() {
        Phase::Ready(_) => Shown::Review,
        Phase::Installed { .. } => Shown::Installed,
        _ => Shown::Other,
    };
    match shown {
        Shown::Review => {
            let job = screen.borrow_mut().model.start_install();
            if let Some(job) = job {
                spawn_job(screen, job);
            }
            repaint(screen);
            return;
        }
        Shown::Installed => {
            let (apps, name) = {
                let state = screen.borrow();
                let name = state
                    .model
                    .request()
                    .targets
                    .first()
                    .map(|t| t.name.clone());
                (state.apps.clone(), name)
            };
            screen.borrow_mut().model.cancel();
            repaint(screen);
            if let Some(name) = name {
                apps.show_package(&name);
            }
            return;
        }
        Shown::Other => {}
    }
    let ticket = {
        let model = &mut screen.borrow_mut().model;
        match model.phase() {
            Phase::Asking => model.go_on(),
            Phase::Blocked(_) => model.install_instead(),
            Phase::Failed { .. } | Phase::InstallFailed(_) => model.try_again(),
            _ => None,
        }
    };
    if let Some(ticket) = ticket {
        spawn_worker(screen, ticket);
    }
    repaint(screen);
}

/// Which kind of main button was pressed.
enum Shown {
    Review,
    Installed,
    Other,
}

/// Runs the helper on its own thread. `run_install` blocks until the helper is gone.
fn spawn_job(screen: &Rc<RefCell<Screen>>, job: JobTicket) {
    let (sender, window) = {
        let state = screen.borrow();
        (state.jobs.0.clone(), state.window.clone())
    };
    std::thread::spawn(move || {
        let number = job.number;
        let tell = |news: JobNews| {
            // If the window closed meanwhile, nobody reads this and that is fine.
            let _ = sender.send((number, news));
            let window = window.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(window) = window.upgrade() {
                    window.global::<InstallBridge>().invoke_job_event();
                }
            });
        };
        let end = run_install(job.request, |event| tell(JobNews::Event(event)));
        tell(JobNews::Ended(end));
    });
}

/// The helper has said something: hand everything in the mailbox to the model, then paint once.
fn job_news(screen: &Rc<RefCell<Screen>>) {
    let news: Vec<JobMessage> = {
        let state = screen.borrow();
        state.jobs.1.try_iter().collect()
    };
    if news.is_empty() {
        return;
    }
    {
        let model = &mut screen.borrow_mut().model;
        for (number, news) in news {
            match news {
                JobNews::Event(event) => model.job_event(number, event),
                JobNews::Ended(end) => model.job_ended(number, end),
            }
        }
    }
    repaint(screen);
}

fn spawn_worker(screen: &Rc<RefCell<Screen>>, ticket: Ticket) {
    let (sender, window) = {
        let state = screen.borrow();
        (state.mailbox.0.clone(), state.window.clone())
    };
    std::thread::spawn(move || {
        let (checked, result) = work(&ticket);
        // If the window closed meanwhile, nobody reads this and that is fine.
        let _ = sender.send((ticket.number, checked, result));
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(window) = window.upgrade() {
                window.global::<InstallBridge>().invoke_resolve_done();
            }
        });
    });

    let target: Weak<RefCell<Screen>> = Rc::downgrade(screen);
    // Starting the same timer again restarts it, so an older resolve's timer can never fire.
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

/// What the worker thread does for one ticket. pacman only reads here, except that the updates check may refresh the
/// app's own private copy of the package lists (WBS 6.11): as the user, no password, up to 60 s of waiting.
fn work(ticket: &Ticket) -> (Option<Option<ListsNote>>, Result<WorkAnswer, PacmanError>) {
    let mut checked = None;
    if ticket.check_lists {
        // Updates waiting: Tidewise only installs on an up-to-date system, so do not even resolve.
        let check = check_for_updates(
            tidewise_pacman::pending_updates_in,
            || tidewise_pacman::refresh_private_lists(RefreshWhy::Opening),
            || {
                tidewise_pacman::sync_lists()
                    .ok()
                    .and_then(|lists| lists.newest)
            },
            SystemTime::now(),
        );
        match check {
            Ok(check) if check.must_update => {
                return (Some(check.note), Ok(WorkAnswer::MustUpdate));
            }
            Ok(check) => checked = Some(check.note),
            Err(error) => return (checked, Err(error)),
        }
    }
    // Against the real lists: those are what the helper installs from.
    let result = tidewise_pacman::resolve(&ticket.input).map(WorkAnswer::Resolved);
    (checked, result)
}

/// A worker has answered: hand every answer in the mailbox to the model.
fn resolve_done(screen: &Rc<RefCell<Screen>>) {
    let answers: Vec<Answer> = {
        let state = screen.borrow();
        state.mailbox.1.try_iter().collect()
    };
    for (number, checked, result) in answers {
        if let Some(note) = checked {
            screen.borrow_mut().model.lists_checked(number, note);
        }
        let next = screen.borrow_mut().model.done(number, result);
        if let Some(next) = next {
            spawn_worker(screen, next);
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
    let view = install_view(&state.model);
    let bridge = window.global::<InstallBridge>();
    set_texts(&bridge, &view);

    // Patch only the rows that changed, so a ticked checkbox keeps the keyboard focus.
    if state.shown_options.len() == view.options.len() {
        for (index, (before, after)) in state.shown_options.iter().zip(&view.options).enumerate() {
            if before != after {
                state.options.set_row_data(index, choice_option(after));
            }
        }
    } else {
        state
            .options
            .set_vec(view.options.iter().map(choice_option).collect::<Vec<_>>());
    }
    state.installed_rows.set_vec(
        view.installed_rows
            .iter()
            .map(installed_row)
            .collect::<Vec<_>>(),
    );
    state.shown_options = view.options;

    // A new card starts its list at the top; a radio list then shows the chosen row.
    if view.focus_serial != state.shown_serial || view.kind != state.shown_kind {
        bridge.set_list_y(0.0);
    }
    state.shown_serial = view.focus_serial;
    state.shown_kind = view.kind;
    state.chosen_row = match view.kind {
        CardKind::Provider | CardKind::Choice => {
            state.shown_options.iter().position(|option| option.checked)
        }
        _ => None,
    };
    keep_in_view(&window, state.shown_kind, state.chosen_row);
}

/// A scrolling key on the review list: `content` is the height of everything in it.
fn scroll_review(window: &MainWindow, key: ReviewKey, content: f32) {
    let key = match key {
        ReviewKey::Up => ScrollKey::Up,
        ReviewKey::Down => ScrollKey::Down,
        ReviewKey::PageUp => ScrollKey::PageUp,
        ReviewKey::PageDown => ScrollKey::PageDown,
        ReviewKey::Home => ScrollKey::Home,
        ReviewKey::End => ScrollKey::End,
    };
    let line = window.global::<Theme>().get_review_row() + 1.0;
    let bridge = window.global::<InstallBridge>();
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

/// Scrolls the list in the card the smallest amount that shows row `index` completely: the same maths as the
/// package list (`list_scroll`). Rows have fixed heights (`Theme`) with a 1px separator between them.
fn keep_in_view(window: &MainWindow, kind: CardKind, index: Option<usize>) {
    let Some(index) = index else {
        return;
    };
    let theme = window.global::<Theme>();
    let row = match kind {
        CardKind::Members => theme.get_choice_check_row(),
        CardKind::Provider | CardKind::Choice => theme.get_choice_radio_row(),
        _ => return,
    } + 1.0;
    let bridge = window.global::<InstallBridge>();
    let current = content_y_to_offset(bridge.get_list_y());
    let wanted = scroll_to_show(index, row, bridge.get_list_visible_height(), current);
    if wanted != current {
        bridge.set_list_y(offset_to_content_y(wanted));
    }
}

fn set_texts(bridge: &InstallBridge, view: &InstallView) {
    bridge.set_open(view.open);
    bridge.set_heading(view.heading.as_str().into());
    bridge.set_crumb(view.crumb.as_str().into());
    bridge.set_current_pill(i32::try_from(view.current_pill).unwrap_or(0));
    bridge.set_kind(match view.kind {
        CardKind::Closed => InstallCardKind::Closed,
        CardKind::Working => InstallCardKind::Working,
        CardKind::Stale => InstallCardKind::Stale,
        CardKind::Provider => InstallCardKind::Provider,
        CardKind::Members => InstallCardKind::Members,
        CardKind::Choice => InstallCardKind::Choice,
        CardKind::Differs => InstallCardKind::Differs,
        CardKind::Alert => InstallCardKind::Alert,
        CardKind::Review => InstallCardKind::Review,
        CardKind::Progress => InstallCardKind::Progress,
        CardKind::Done => InstallCardKind::Done,
    });
    bridge.set_step_text(view.step_text.as_str().into());
    bridge.set_dots(ModelRc::new(VecModel::from(view.dots.clone())));
    bridge.set_title_before(view.title_before.as_str().into());
    bridge.set_title_mono(view.title_mono.as_str().into());
    bridge.set_title_after(view.title_after.as_str().into());
    bridge.set_body(view.body.as_str().into());
    bridge.set_group_label(view.group_label.as_str().into());
    let selected = view
        .options
        .iter()
        .position(|option| option.checked)
        .and_then(|index| i32::try_from(index).ok());
    bridge.set_selected_index(selected.unwrap_or(-1));
    bridge.set_selection_text(view.selection_text.as_str().into());
    bridge.set_show_select_buttons(view.show_select_buttons);
    bridge.set_installed_heading(view.installed_heading.as_str().into());
    bridge.set_code_lines(strings(&view.code_lines));
    bridge.set_has_primary(view.primary.is_some());
    bridge.set_primary_text(
        view.primary
            .as_ref()
            .map_or("", |button| button.label.as_str())
            .into(),
    );
    bridge.set_primary_enabled(view.primary.as_ref().is_some_and(|button| button.enabled));
    bridge.set_has_secondary(view.secondary.is_some());
    bridge.set_secondary_text(
        view.secondary
            .as_ref()
            .map_or("", |button| button.label.as_str())
            .into(),
    );
    bridge.set_unchanged(view.unchanged.as_str().into());
    bridge.set_slow_note(view.slow_note.as_str().into());
    bridge.set_hint(view.hint.as_str().into());
    bridge.set_lists_note(view.lists_note.as_str().into());
    bridge.set_report(view.report.as_str().into());
    bridge.set_technical(strings(&view.technical));
    bridge.set_tiles(ModelRc::new(VecModel::from(
        view.tiles.iter().map(tile).collect::<Vec<_>>(),
    )));
    bridge.set_review_rows(ModelRc::new(VecModel::from(
        view.rows.iter().map(review_row).collect::<Vec<_>>(),
    )));
    bridge.set_primary_note(view.primary_note.as_str().into());
    bridge.set_stages(ModelRc::new(VecModel::from(
        view.stages.iter().map(stage).collect::<Vec<_>>(),
    )));
    bridge.set_percent(i32::from(view.percent));
    bridge.set_percent_text(view.percent_text.as_str().into());
    bridge.set_focus(match view.focus {
        FocusTarget::List => InstallFocus::List,
        FocusTarget::Primary => InstallFocus::Primary,
        FocusTarget::Secondary => InstallFocus::Secondary,
    });
    // Last, so every property the new card needs is already set when the focus moves.
    bridge.set_focus_serial(i32::try_from(view.focus_serial).unwrap_or(0));
}

fn choice_option(option: &OptionView) -> ChoiceOption {
    ChoiceOption {
        label: option.label.as_str().into(),
        meta: option.meta.as_str().into(),
        detail: option.detail.as_str().into(),
        tag: option.tag.as_str().into(),
        checked: option.checked,
    }
}

fn tile(tile: &TileView) -> TileData {
    TileData {
        label: tile.label.as_str().into(),
        number: tile.number.as_str().into(),
        unit: tile.unit.as_str().into(),
        words: tile.words,
    }
}

fn review_row(row: &ReviewRow) -> ReviewRowData {
    ReviewRowData {
        name: row.name.as_str().into(),
        version: row.version.as_str().into(),
        why: row.why.as_str().into(),
        size: row.size.as_str().into(),
        bold: row.bold,
    }
}

fn installed_row(row: &InstalledRow) -> InstalledRowData {
    InstalledRowData {
        name: row.name.as_str().into(),
        version: row.version.as_str().into(),
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

fn strings(items: &[String]) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = items.iter().map(|s| s.as_str().into()).collect();
    ModelRc::new(VecModel::from(items))
}
