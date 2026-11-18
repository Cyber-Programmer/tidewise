//! The jobs: install, update the system, update and install (threats T-09, T-10, T-11, T-12, T-17, T-18).
//!
//! Every flow is a function of the (already validated) request and a `Runner`, so each path is a unit test with a
//! scripted fake. The order is the point:
//!
//! Install:   lock look -> `-Qu` (pending updates block a plain install) -> dry run must equal the confirmed list ->
//!            real `-S --noconfirm --ask 66` -> `-D --asdeps` for the chosen providers -> post-check.
//! Update:    lock look -> private folder made and checked -> private lock look -> `-Sy` on the PRIVATE copy of the
//!            lists -> dry run `-Su` (plus any targets) on the private copy must equal the confirmed list -> the
//!            refreshed lists are moved into the real `sync/` -> dry run on the real lists must equal it again ->
//!            real `-Su` (no `-y`) -> `-D --asdeps` -> post-check. (Update and install is the same with targets.)
//!            No command refreshes the real lists, so a refusal leaves them as they were (decision 0007, R-02).
//!
//! After every real run each confirmed item is checked with `pacman -Q`; the number that are not installed at the
//! confirmed version (a declined replacement, which `--print` still lists, or a change by another package manager in
//! between) is reported as a `Notice`, count only. Nothing here removes, creates or fixes `db.lck`: it is only looked
//! at, and pacman's own advice about it is never shown (golden rule 9).

use tidewise_helper_protocol::{
    Action, Details, ErrorKind, Event, Install, Notice, Outcome, PackageRef, Request,
    TransactionItem,
};
use tidewise_pacman::{InstallDecision, install_decision, parse_qu};

use crate::lists::{ListsError, MoveError};
use crate::output::{
    count_not_installed, installed_names, is_lock_failure, parse_print, removed_names,
    same_transaction,
};
use crate::pacman_args::PacmanCommand;
use crate::progress::ProgressTracker;
use crate::runner::{RunOutput, Runner, RunnerError};

/// Whether the "did any package disappear?" check ran. A check that did not run must never look like a clean pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RemovalCheck {
    /// No real run started, so there was nothing to compare (a refusal, or pacman could not be started).
    #[default]
    NotRun,
    /// Both `pacman -Q` lists were read and compared.
    Done,
    /// A real run happened but one of the two lists could not be read, so nothing was compared.
    Skipped,
}

/// What an update did to the REAL package lists. The journal says it in one fixed field (`TIDEWISE_LISTS`), because
/// pacman does not log the move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListsState {
    /// No list was renamed into the real `sync/`: the real lists are as they were.
    Untouched,
    /// At least one refreshed list was renamed into the real `sync/`.
    Moved,
}

/// How a job ended, and what the notices said (the same counts go to the journal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobResult {
    pub outcome: Outcome,
    /// Confirmed items not installed at the confirmed version after the run.
    pub not_installed: u32,
    /// Packages that were installed before the real run and are gone after it.
    pub removed: u32,
    pub removal_check: RemovalCheck,
    /// Only for an update (plain installs never touch the lists): what happened to the real lists.
    pub lists: Option<ListsState>,
}

/// Runs the job the request asks for. Progress and notices go to `on_event`; the caller prints the final line.
pub fn run_job(
    request: &Request,
    runner: &mut dyn Runner,
    on_event: &mut dyn FnMut(Event),
) -> JobResult {
    let (step, counts) = match &request.action {
        Action::Install(install) => install_job(install, runner, on_event),
        Action::UpdateSystem(update) => update_job(&update.transaction, &[], &[], runner, on_event),
        Action::UpdateSystemAndInstall(install) => {
            let providers = providers_of(install);
            update_job(
                &install.transaction,
                &names_of(install),
                &providers,
                runner,
                on_event,
            )
        }
    };
    JobResult {
        outcome: match step {
            Ok(()) => Outcome::Ok,
            Err(kind) => Outcome::Error(kind),
        },
        not_installed: counts.not_installed,
        removed: counts.removed,
        removal_check: counts.removal_check,
        lists: counts.lists,
    }
}

/// `Err` is the reason the job stopped; the caller turns it into the final line.
type Step = Result<(), ErrorKind>;

/// What the two `pacman -Q` lists said about the real run.
#[derive(Default)]
struct Counts {
    not_installed: u32,
    removed: u32,
    removal_check: RemovalCheck,
    lists: Option<ListsState>,
}

impl Counts {
    /// Nothing counted, and what the update did to the real lists.
    fn lists(state: ListsState) -> Counts {
        Counts {
            lists: Some(state),
            ..Counts::default()
        }
    }
}

fn install_job(
    install: &Install,
    runner: &mut dyn Runner,
    on_event: &mut dyn FnMut(Event),
) -> (Step, Counts) {
    let names = names_of(install);
    let before_the_real_run = (|| -> Step {
        check_lock(runner)?;
        check_nothing_pending(runner)?;
        dry_run(
            runner,
            PacmanCommand::DryRunInstall(names.clone()),
            &install.transaction,
        )
    })();
    if let Err(kind) = before_the_real_run {
        return (Err(kind), Counts::default());
    }

    let mut tracker = ProgressTracker::new(total_of(&install.transaction));
    real_run_and_after(
        runner,
        on_event,
        &mut tracker,
        PacmanCommand::RealInstall(names),
        &install.transaction,
        &providers_of(install),
    )
}

/// `names` is empty for a plain system update.
fn update_job(
    transaction: &[TransactionItem],
    names: &[PackageRef],
    providers: &[PackageRef],
    runner: &mut dyn Runner,
    on_event: &mut dyn FnMut(Event),
) -> (Step, Counts) {
    // Until a list is renamed into the real `sync/`, every way out leaves the real lists untouched.
    let untouched = || Counts::lists(ListsState::Untouched);
    if let Err(kind) = check_lock(runner) {
        return (Err(kind), untouched());
    }
    if let Err(error) = runner.prepare_private_lists() {
        return (Err(lists_failure(error)), untouched());
    }
    // A look, not a guarantee (decision 0007): a lock that appears right after is still refused by pacman itself.
    if runner.private_lock_present() {
        return (Err(private_lock_left()), untouched());
    }
    let mut tracker = ProgressTracker::new(total_of(transaction));

    // The one and only sync of the job, and of the PRIVATE copy. The real lists are refreshed by nothing: they are
    // replaced by the lists this sync made, after the dry run on them equals the confirmed list (golden rule 4).
    let synced = match runner.run(&PacmanCommand::PrivateSync, &mut |line| {
        forward(&mut tracker, on_event, line)
    }) {
        Ok(output) => output,
        Err(error) => return (Err(runner_failure(error)), untouched()),
    };
    if synced.exit != Some(0) || synced.timed_out {
        return (Err(failure_of(&synced)), untouched());
    }

    if let Err(kind) = dry_run(
        runner,
        PacmanCommand::PrivateDryRunUpdate(names.to_vec()),
        transaction,
    ) {
        return (Err(kind), untouched());
    }

    let after_the_move = match runner.move_lists_in() {
        // Shim of the red commit: any successful move counts as Moved. The fix counts the renames.
        Ok(_renamed) => ListsState::Moved,
        Err(MoveError { error, renamed }) => {
            // A move that stopped half way has changed the real lists, and the journal must say so.
            let state = if renamed > 0 {
                ListsState::Moved
            } else {
                ListsState::Untouched
            };
            return (Err(lists_failure(error)), Counts::lists(state));
        }
    };

    // The same check again, on the real lists: another root process may have changed them since (T-09).
    if let Err(kind) = dry_run(
        runner,
        PacmanCommand::DryRunUpdate(names.to_vec()),
        transaction,
    ) {
        return (Err(kind), Counts::lists(after_the_move));
    }
    let (step, mut counts) = real_run_and_after(
        runner,
        on_event,
        &mut tracker,
        PacmanCommand::RealUpdate(names.to_vec()),
        transaction,
        providers,
    );
    counts.lists = Some(after_the_move);
    (step, counts)
}

/// The lock is only looked at. pacman's own advice about it (to remove the file) is never shown.
fn check_lock(runner: &dyn Runner) -> Step {
    if runner.database_lock_present() {
        Err(ErrorKind::DatabaseLocked)
    } else {
        Ok(())
    }
}

/// A plain install while updates are pending would be a partial upgrade (golden rule 4). Updates held back by
/// `IgnorePkg` do not count: a full update would not install them either.
fn check_nothing_pending(runner: &mut dyn Runner) -> Step {
    let output = runner
        .run(&PacmanCommand::PendingUpdates, &mut |_| {})
        .map_err(runner_failure)?;
    // `pacman -Qu` exits 1 when there is nothing to list, and also for most errors: an "error:" line on stderr decides
    // (a warning, for example about a database that was never synced, does not).
    let has_error_line = output.stderr.lines().any(|line| line.starts_with("error:"));
    let usable = match output.exit {
        Some(0) => true,
        Some(1) => !has_error_line,
        _ => false,
    };
    if output.timed_out || output.truncated || !usable {
        return Err(failure_of(&output));
    }
    let updates = parse_qu(&output.stdout)
        .map_err(|_| failed("pacman's list of updates could not be read."))?;
    match install_decision(&updates) {
        InstallDecision::Allowed { .. } => Ok(()),
        InstallDecision::MustUpdateSystem { .. } => Err(ErrorKind::SystemOutOfDate),
    }
}

/// The dry run must give exactly the list the user confirmed (T-09).
fn dry_run(runner: &mut dyn Runner, command: PacmanCommand, confirmed: &[TransactionItem]) -> Step {
    let output = runner.run(&command, &mut |_| {}).map_err(runner_failure)?;
    if output.timed_out {
        return Err(failure_of(&output));
    }
    if output.truncated {
        return Err(failed("pacman printed more than the helper reads."));
    }
    if output.exit != Some(0) {
        // A target that is no longer in the repositories means the lists changed since the user looked.
        if output
            .stderr
            .lines()
            .any(|line| line.starts_with("error: target not found: "))
        {
            return Err(ErrorKind::TransactionChanged);
        }
        return Err(failure_of(&output));
    }
    let found =
        parse_print(&output.stdout).map_err(|_| failed("pacman's dry run could not be read."))?;
    if same_transaction(confirmed, &found) {
        Ok(())
    } else {
        Err(ErrorKind::TransactionChanged)
    }
}

/// The real transaction between two `pacman -Q` lists, then `-D --asdeps` for the chosen providers that were not
/// installed, then the comparison of the lists.
fn real_run_and_after(
    runner: &mut dyn Runner,
    on_event: &mut dyn FnMut(Event),
    tracker: &mut ProgressTracker,
    real: PacmanCommand,
    transaction: &[TransactionItem],
    providers: &[PackageRef],
) -> (Step, Counts) {
    // What is installed right before the real run. It tells which chosen providers are already there (only the others
    // may be marked as dependencies afterwards: marking one the user installed by hand would hide it from their own
    // list) and is the baseline for "did anything disappear?".
    let before = read_installed_list(runner);
    let to_mark: Vec<PackageRef> = match &before {
        Some(list) => {
            let installed = installed_names(list);
            providers
                .iter()
                .filter(|provider| !installed.contains(&provider.name))
                .cloned()
                .collect()
        }
        // Cannot tell: mark nothing (cosmetic) rather than demote a package.
        None => Vec::new(),
    };

    let output = match runner.run(&real, &mut |line| forward(tracker, on_event, line)) {
        Ok(output) => output,
        // pacman could not be started: there is nothing to compare afterwards. An I/O error while watching it is
        // different (R-04): pacman may have run, so the check is Skipped and the journal says so.
        Err(error) => {
            let counts = if matches!(error, RunnerError::Io) {
                Counts {
                    removal_check: RemovalCheck::Skipped,
                    ..Counts::default()
                }
            } else {
                Counts::default()
            };
            return (Err(runner_failure(error)), counts);
        }
    };

    let mut stopped: Option<ErrorKind> = None;
    if output.exit != Some(0) || output.timed_out {
        stopped = Some(failure_of(&output));
    } else if !to_mark.is_empty() {
        match runner.run(&PacmanCommand::MarkAsDeps(to_mark), &mut |_| {}) {
            Ok(marked) if marked.exit == Some(0) && !marked.timed_out => {}
            Ok(marked) => stopped = Some(failure_of(&marked)),
            Err(error) => stopped = Some(runner_failure(error)),
        }
    }

    let after = read_installed_list(runner);
    let counts = compare_lists(on_event, transaction, before.as_deref(), after.as_deref());
    (stopped.map_or(Ok(()), Err), counts)
}

/// `pacman -Q`: every installed package. The text is returned only if the list is complete: exit 0, not cut off, no
/// timeout. With no names `pacman -Q` exits 1 only on an error and still prints what it could read, and a cut list would
/// look like removals, so exit 1 is not trusted.
fn read_installed_list(runner: &mut dyn Runner) -> Option<String> {
    let output = runner
        .run(&PacmanCommand::ListInstalled, &mut |_| {})
        .ok()?;
    // R-02: exit 0 can hide a skipped, damaged entry (libalpm says so on stderr), and a list with no package at all
    // cannot be a complete one.
    let has_error_line = output.stderr.lines().any(|line| line.starts_with("error:"));
    let complete = output.exit == Some(0)
        && !output.timed_out
        && !output.truncated
        && !has_error_line
        && !installed_names(&output.stdout).is_empty();
    if complete { Some(output.stdout) } else { None }
}

/// After every real run, from the two lists: how many confirmed items are not installed at the confirmed version (a
/// declined replacement, a change by another package manager), and how many installed packages disappeared (nothing in
/// the request removes anything, so that is a tripwire). Each count above zero goes out as a notice, count only. If a
/// list could not be read the thing it feeds is not reported and, for removals, the check is marked Skipped, so a check
/// that did not run is never mistaken for a clean pass.
fn compare_lists(
    on_event: &mut dyn FnMut(Event),
    transaction: &[TransactionItem],
    before: Option<&str>,
    after: Option<&str>,
) -> Counts {
    let mut counts = Counts::default();
    if let Some(after) = after {
        let missing = count_not_installed(transaction, after);
        if missing > 0 {
            on_event(Event::Notice(Notice::PackagesNotInstalled(missing)));
            counts.not_installed = missing;
        }
    }
    match (before, after) {
        (Some(before), Some(after)) => {
            counts.removal_check = RemovalCheck::Done;
            let removed = u32::try_from(removed_names(before, after).len()).unwrap_or(u32::MAX);
            if removed > 0 {
                on_event(Event::Notice(Notice::PackagesRemoved(removed)));
                counts.removed = removed;
            }
        }
        _ => counts.removal_check = RemovalCheck::Skipped,
    }
    counts
}

fn forward(tracker: &mut ProgressTracker, on_event: &mut dyn FnMut(Event), line: &str) {
    if let Some(progress) = tracker.line(line) {
        on_event(Event::Progress(progress));
    }
}

/// What the user asked for, then the providers they chose: the order pacman is given.
fn names_of(install: &Install) -> Vec<PackageRef> {
    let mut names = install.targets.clone();
    names.extend(providers_of(install));
    names
}

fn providers_of(install: &Install) -> Vec<PackageRef> {
    install
        .providers
        .iter()
        .map(|answer| answer.chosen.clone())
        .collect()
}

fn total_of(transaction: &[TransactionItem]) -> u32 {
    u32::try_from(transaction.len()).unwrap_or(u32::MAX)
}

/// Why a pacman run that did not succeed stopped. A held lock has its own fixed text; anything else carries pacman's
/// own words from stderr (cleaned and capped by `Details`).
fn failure_of(output: &RunOutput) -> ErrorKind {
    if output.timed_out {
        return failed("pacman was stopped because it took too long.");
    }
    if is_lock_failure(&output.stderr) {
        return ErrorKind::DatabaseLocked;
    }
    if output.stderr.trim().is_empty() {
        return match output.exit {
            Some(code) => failed(&format!(
                "pacman stopped without a message (exit status {code})."
            )),
            None => failed("pacman was stopped by a signal."),
        };
    }
    ErrorKind::PacmanFailed(Details::from_raw(&output.stderr))
}

/// The private lock is there, so the private sync did not start. Fixed text. The helper cannot tell a killed sync (a
/// restart clears the lock: the folder is on tmpfs) from another helper syncing right now, so the text covers both.
fn private_lock_left() -> ErrorKind {
    failed(
        "An earlier refresh of the package lists was interrupted, or is still running, and left its lock. If no update is running now, restart the computer and try again. Nothing was changed.",
    )
}

/// Why the private copy could not be prepared or moved in. Fixed texts; a held real lock is the usual one.
fn lists_failure(error: ListsError) -> ErrorKind {
    match error {
        ListsError::Locked => ErrorKind::DatabaseLocked,
        ListsError::FolderUnusable => {
            failed("the helper's private folder for the package lists could not be used.")
        }
        ListsError::StoppedSigning => failed(
            "A repository stopped signing its package list, so the refreshed list was not used. Nothing was changed. Refreshing by hand with pacman -Syy would leave pacman unable to read that list.",
        ),
        ListsError::BadFile => {
            failed("a refreshed package list was not a file the helper could trust.")
        }
        ListsError::Io => failed("the helper could not copy the refreshed package lists."),
    }
}

/// The work folder could not be used, so no job was started. A fixed text.
pub fn work_folder_unusable() -> ErrorKind {
    failed("the helper's work folder could not be used.")
}

/// The helper could not run pacman at all. Fixed texts.
fn runner_failure(error: RunnerError) -> ErrorKind {
    if matches!(error, RunnerError::WorkDir(_)) {
        return work_folder_unusable();
    }
    failed(match error {
        RunnerError::Args(_) => "the helper could not build a pacman command.",
        RunnerError::WorkDir(_) => "",
        RunnerError::Spawn => "pacman could not be started.",
        RunnerError::Io => "the helper could not read pacman's output.",
    })
}

fn failed(text: &str) -> ErrorKind {
    ErrorKind::PacmanFailed(Details::from_raw(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pacman_args::PacmanCommand;
    use crate::runner::{RunOutput, RunnerError};
    use crate::testing::{Fake, ok, out};
    use crate::workdir::WorkDirError;
    use tidewise_helper_protocol::{
        Details, Notice, PackageRef, Phase, ProviderChoice, TransactionItem, parse_request,
    };

    const INSTALL: &str = include_str!("../../../tests/fixtures/helper-protocol/install.json");
    const UPDATE: &str = include_str!("../../../tests/fixtures/helper-protocol/update-system.json");
    const BOTH: &str =
        include_str!("../../../tests/fixtures/helper-protocol/update-system-and-install.json");

    const QU_SAMPLE: &str = include_str!("../../../tests/fixtures/pacman/qu-sample.txt");
    const QU_IGNORED: &str = include_str!("../../../tests/fixtures/pacman/qu-ignored.txt");
    const REAL_INSTALL_OUT: &str =
        include_str!("../../../tests/fixtures/helper-jobs/real-install-two.out");
    const SYNC_OUT: &str = include_str!("../../../tests/fixtures/helper-jobs/sync-ok.out");
    const CONFLICT_OUT: &str = include_str!("../../../tests/fixtures/helper-jobs/conflict.out");
    const CONFLICT_ERR: &str = include_str!("../../../tests/fixtures/helper-jobs/conflict.err");
    const LOCK_ERR: &str = include_str!("../../../tests/fixtures/helper-jobs/lock-install.err");
    const LOCK_SYNC_ERR: &str = include_str!("../../../tests/fixtures/helper-jobs/lock-sync.err");
    const NOT_FOUND_ERR: &str =
        include_str!("../../../tests/fixtures/helper-jobs/print-not-found.err");
    const ASDEPS_MISSING_ERR: &str =
        include_str!("../../../tests/fixtures/helper-jobs/asdeps-missing.err");

    fn request(text: &str) -> Request {
        parse_request(text.as_bytes()).unwrap()
    }

    fn transaction_of(request: &Request) -> Vec<TransactionItem> {
        match &request.action {
            Action::Install(install) | Action::UpdateSystemAndInstall(install) => {
                install.transaction.clone()
            }
            Action::UpdateSystem(update) => update.transaction.clone(),
        }
    }

    fn print_text(items: &[TransactionItem]) -> String {
        items
            .iter()
            .map(|i| format!("{}/{} {}\n", i.repo, i.name, i.version))
            .collect()
    }

    fn installed_text(items: &[TransactionItem]) -> String {
        // Every real system has pacman in its list, and an empty list is never taken as a complete one (R-02).
        std::iter::once("pacman 7.1.0-1\n".to_string())
            .chain(items.iter().map(|i| format!("{} {}\n", i.name, i.version)))
            .collect()
    }

    fn pkg(repo: &str, name: &str) -> PackageRef {
        PackageRef {
            repo: repo.to_string(),
            name: name.to_string(),
        }
    }

    /// Targets first, then the chosen providers (the order the helper must use).
    fn names_of(install: &Install) -> Vec<PackageRef> {
        let mut names = install.targets.clone();
        names.extend(install.providers.iter().map(|answer| answer.chosen.clone()));
        names
    }

    fn the_install(request: &Request) -> Install {
        match &request.action {
            Action::Install(install) | Action::UpdateSystemAndInstall(install) => install.clone(),
            Action::UpdateSystem(_) => panic!("not an install"),
        }
    }

    fn go_full(request: &Request, fake: &mut Fake) -> (JobResult, Vec<Event>) {
        let mut events = Vec::new();
        let result = run_job(request, fake, &mut |event| events.push(event));
        (result, events)
    }

    fn go(request: &Request, fake: &mut Fake) -> (Outcome, Vec<Event>) {
        let (result, events) = go_full(request, fake);
        (result.outcome, events)
    }

    fn notices(events: &[Event]) -> Vec<Notice> {
        events
            .iter()
            .filter_map(|event| match event {
                Event::Notice(notice) => Some(*notice),
                _ => None,
            })
            .collect()
    }

    /// The five answers of a clean install of `INSTALL`.
    fn clean_install_script(request: &Request) -> Vec<Result<RunOutput, RunnerError>> {
        let items = transaction_of(request);
        vec![
            out(1, "", ""),
            ok(&print_text(&items)),
            ok("pacman 7.1.0-1\n"), // the providers are not installed yet
            ok(REAL_INSTALL_OUT),
            ok(""),
            ok(&installed_text(&items)),
        ]
    }

    // ---- install ------------------------------------------------------------------------------------------

    #[test]
    fn a_clean_install_runs_the_five_commands_in_order_and_ends_ok() {
        let request = request(INSTALL);
        let install = the_install(&request);
        let mut fake = Fake::new(clean_install_script(&request));
        let (outcome, events) = go(&request, &mut fake);
        assert_eq!(outcome, Outcome::Ok);
        assert_eq!(
            fake.ran,
            vec![
                PacmanCommand::PendingUpdates,
                PacmanCommand::DryRunInstall(names_of(&install)),
                PacmanCommand::ListInstalled,
                PacmanCommand::RealInstall(names_of(&install)),
                PacmanCommand::MarkAsDeps(vec![pkg("extra", "jre-openjdk")]),
                PacmanCommand::ListInstalled,
            ]
        );
        assert_eq!(notices(&events), vec![]);
    }

    #[test]
    fn the_targets_come_before_the_chosen_providers() {
        let request = request(INSTALL);
        let mut fake = Fake::new(clean_install_script(&request));
        go(&request, &mut fake);
        let PacmanCommand::RealInstall(names) = &fake.ran[3] else {
            panic!("third command is not the real install");
        };
        assert_eq!(
            names,
            &vec![pkg("extra", "dbeaver"), pkg("extra", "jre-openjdk")]
        );
    }

    #[test]
    fn progress_follows_the_real_output_and_counts_against_the_confirmed_total() {
        let request = request(INSTALL);
        let mut fake = Fake::new(clean_install_script(&request));
        let (_, events) = go(&request, &mut fake);
        let progress: Vec<(Phase, u32, u32)> = events
            .iter()
            .filter_map(|event| match event {
                Event::Progress(p) => Some((p.phase, p.current, p.total)),
                _ => None,
            })
            .collect();
        assert_eq!(progress.first(), Some(&(Phase::Checking, 0, 5)));
        assert_eq!(progress.last(), Some(&(Phase::Hooks, 0, 5)));
        assert!(
            progress
                .iter()
                .all(|(_, current, total)| *total == 5 && current <= total)
        );
    }

    #[test]
    fn a_held_lock_ends_the_job_before_any_command_runs() {
        for text in [INSTALL, UPDATE, BOTH] {
            let request = request(text);
            let mut fake = Fake::new(vec![]);
            fake.lock = true;
            let (outcome, events) = go(&request, &mut fake);
            assert_eq!(outcome, Outcome::Error(ErrorKind::DatabaseLocked));
            assert!(fake.ran.is_empty(), "{:?}", fake.ran);
            assert!(events.is_empty());
        }
    }

    #[test]
    fn a_pending_update_blocks_a_plain_install_and_nothing_else_runs() {
        let request = request(INSTALL);
        let mut fake = Fake::new(vec![ok(QU_SAMPLE)]);
        let (outcome, _) = go(&request, &mut fake);
        assert_eq!(outcome, Outcome::Error(ErrorKind::SystemOutOfDate));
        assert_eq!(fake.ran, vec![PacmanCommand::PendingUpdates]);
    }

    #[test]
    fn updates_held_back_by_ignorepkg_do_not_block_an_install() {
        let request = request(INSTALL);
        let only_ignored: String = QU_IGNORED
            .lines()
            .filter(|l| l.ends_with("[ignored]"))
            .map(|l| format!("{l}\n"))
            .collect();
        assert!(!only_ignored.is_empty());
        let items = transaction_of(&request);
        let mut fake = Fake::new(vec![
            ok(&only_ignored),
            ok(&print_text(&items)),
            ok("pacman 7.1.0-1\n"),
            ok(REAL_INSTALL_OUT),
            ok(""),
            ok(&installed_text(&items)),
        ]);
        let (outcome, _) = go(&request, &mut fake);
        assert_eq!(outcome, Outcome::Ok);
    }

    #[test]
    fn an_unreadable_update_list_is_a_failure_and_nothing_else_runs() {
        let request = request(INSTALL);
        for stdout in ["garbage line\n", "7zip 1 -> \n"] {
            let mut fake = Fake::new(vec![ok(stdout)]);
            let (outcome, _) = go(&request, &mut fake);
            assert!(
                matches!(outcome, Outcome::Error(ErrorKind::PacmanFailed(_))),
                "{outcome:?}"
            );
            assert_eq!(fake.ran, vec![PacmanCommand::PendingUpdates]);
        }
        let mut broken = Fake::new(vec![out(2, "", "error: whatever\n")]);
        let (outcome, _) = go(&request, &mut broken);
        assert!(matches!(
            outcome,
            Outcome::Error(ErrorKind::PacmanFailed(_))
        ));
    }

    #[test]
    fn an_update_list_that_failed_with_exit_1_is_a_failure_not_no_updates() {
        // `pacman -Qu` exits 1 for "nothing to list" and also for errors; an error line decides (review R-03).
        let request = request(INSTALL);
        for stderr in [
            "error: failed to read the database\n",
            "warning: x\nerror: y\n",
        ] {
            let mut fake = Fake::new(vec![out(1, "", stderr)]);
            let (outcome, _) = go(&request, &mut fake);
            assert!(
                matches!(outcome, Outcome::Error(ErrorKind::PacmanFailed(_))),
                "{stderr}: {outcome:?}"
            );
            assert_eq!(fake.ran, vec![PacmanCommand::PendingUpdates], "{stderr}");
        }
    }

    #[test]
    fn exit_1_with_only_warnings_still_means_nothing_pending() {
        let request = request(INSTALL);
        let mut script = clean_install_script(&request);
        script[0] = out(
            1,
            "",
            "warning: database file for 'extra' does not exist (use '-Sy' to download)\n",
        );
        let mut fake = Fake::new(script);
        assert_eq!(go(&request, &mut fake).0, Outcome::Ok);
    }

    #[test]
    fn a_dry_run_that_differs_is_a_changed_transaction_and_nothing_real_runs() {
        let request = request(INSTALL);
        let mut items = transaction_of(&request);
        items[4].version = "25.2.2-1".to_string();
        let mut fake = Fake::new(vec![out(1, "", ""), ok(&print_text(&items))]);
        let (outcome, events) = go(&request, &mut fake);
        assert_eq!(outcome, Outcome::Error(ErrorKind::TransactionChanged));
        assert_eq!(fake.ran.len(), 2);
        assert!(matches!(fake.ran[1], PacmanCommand::DryRunInstall(_)));
        assert!(events.is_empty());
    }

    #[test]
    fn the_same_lines_in_another_order_are_the_same_transaction() {
        let request = request(INSTALL);
        let mut items = transaction_of(&request);
        items.reverse();
        let mut fake = Fake::new(vec![
            out(1, "", ""),
            ok(&print_text(&items)),
            ok("pacman 7.1.0-1\n"),
            ok(REAL_INSTALL_OUT),
            ok(""),
            ok(&installed_text(&items)),
        ]);
        assert_eq!(go(&request, &mut fake).0, Outcome::Ok);
    }

    #[test]
    fn a_target_that_is_gone_from_the_repositories_is_a_changed_transaction() {
        let request = request(INSTALL);
        let mut fake = Fake::new(vec![out(1, "", ""), out(1, "", NOT_FOUND_ERR)]);
        let (outcome, _) = go(&request, &mut fake);
        assert_eq!(outcome, Outcome::Error(ErrorKind::TransactionChanged));
        assert_eq!(fake.ran.len(), 2);
    }

    #[test]
    fn other_dry_run_failures_are_pacman_failed_and_nothing_real_runs() {
        let request = request(INSTALL);
        let items = transaction_of(&request);
        let good = print_text(&items);
        let timed_out = Ok(RunOutput {
            exit: None,
            stdout: good.clone(),
            stderr: String::new(),
            timed_out: true,
            truncated: false,
        });
        let truncated = Ok(RunOutput {
            exit: Some(0),
            stdout: good,
            stderr: String::new(),
            truncated: true,
            timed_out: false,
        });
        for dry in [
            out(1, "", "error: something else\n"),
            out(0, "not a transaction line\n", ""),
            timed_out,
            truncated,
            Err(RunnerError::Spawn),
        ] {
            let mut fake = Fake::new(vec![out(1, "", ""), dry]);
            let (outcome, _) = go(&request, &mut fake);
            assert!(
                matches!(outcome, Outcome::Error(ErrorKind::PacmanFailed(_))),
                "{outcome:?}"
            );
            assert_eq!(fake.ran.len(), 2, "{:?}", fake.ran);
        }
    }

    #[test]
    fn a_conflict_aborts_with_pacmans_words_and_the_post_check_counts_the_list() {
        let request = request(INSTALL);
        let items = transaction_of(&request);
        let mut fake = Fake::new(vec![
            out(1, "", ""),
            ok(&print_text(&items)),
            ok("pacman 7.1.0-1\n"),
            out(1, CONFLICT_OUT, CONFLICT_ERR),
            ok("pacman 7.1.0-1\n"),
        ]);
        let (outcome, events) = go(&request, &mut fake);
        let Outcome::Error(ErrorKind::PacmanFailed(details)) = outcome else {
            panic!("expected PacmanFailed, got {outcome:?}");
        };
        assert!(
            details
                .as_str()
                .contains("unresolvable package conflicts detected")
        );
        // No -D after a failed install; the post-check ran and found nothing installed.
        assert!(
            !fake
                .ran
                .iter()
                .any(|c| matches!(c, PacmanCommand::MarkAsDeps(_)))
        );
        assert!(matches!(
            fake.ran.last(),
            Some(PacmanCommand::ListInstalled)
        ));
        assert_eq!(notices(&events), vec![Notice::PackagesNotInstalled(5)]);
    }

    #[test]
    fn a_lock_taken_in_the_meantime_is_database_locked_without_pacmans_advice() {
        let request = request(INSTALL);
        let items = transaction_of(&request);
        let mut fake = Fake::new(vec![
            out(1, "", ""),
            ok(&print_text(&items)),
            ok(""),
            out(1, "", LOCK_ERR),
            ok(""),
        ]);
        let (outcome, _) = go(&request, &mut fake);
        // The lock text is a fixed one: pacman's "you can remove db.lck" never reaches the user (golden rule 9).
        assert_eq!(outcome, Outcome::Error(ErrorKind::DatabaseLocked));
    }

    #[test]
    fn marking_providers_as_dependencies_that_fails_is_reported_and_the_post_check_still_runs() {
        let request = request(INSTALL);
        let items = transaction_of(&request);
        let mut fake = Fake::new(vec![
            out(1, "", ""),
            ok(&print_text(&items)),
            ok("pacman 7.1.0-1\n"),
            ok(REAL_INSTALL_OUT),
            out(1, "", ASDEPS_MISSING_ERR),
            ok(&installed_text(&items)),
        ]);
        let (outcome, events) = go(&request, &mut fake);
        let Outcome::Error(ErrorKind::PacmanFailed(details)) = outcome else {
            panic!("expected PacmanFailed, got {outcome:?}");
        };
        assert!(details.as_str().contains("could not set install reason"));
        assert!(matches!(
            fake.ran.last(),
            Some(PacmanCommand::ListInstalled)
        ));
        assert_eq!(notices(&events), vec![]);
    }

    #[test]
    fn a_provider_the_user_already_has_is_not_marked_as_a_dependency() {
        // Review R-02: marking an explicitly installed package as a dependency would hide it from the user's own list.
        let request = request(INSTALL);
        let install = the_install(&request);
        let items = transaction_of(&request);
        let mut fake = Fake::new(vec![
            out(1, "", ""),
            ok(&print_text(&items)),
            ok("jre-openjdk 25.0.1.u8-1\n"), // the chosen provider is already installed
            ok(REAL_INSTALL_OUT),
            ok(&installed_text(&items)),
        ]);
        assert_eq!(go(&request, &mut fake).0, Outcome::Ok);
        assert_eq!(
            fake.ran,
            vec![
                PacmanCommand::PendingUpdates,
                PacmanCommand::DryRunInstall(names_of(&install)),
                PacmanCommand::ListInstalled,
                PacmanCommand::RealInstall(names_of(&install)),
                PacmanCommand::ListInstalled,
            ]
        );
    }

    #[test]
    fn only_the_providers_that_were_not_installed_are_marked() {
        let mut request = request(INSTALL);
        if let Action::Install(install) = &mut request.action {
            install.providers.push(ProviderChoice {
                dependency: "a-jdbc".to_string(),
                chosen: pkg("extra", "dbeaver-jdbc"),
            });
        }
        let items = transaction_of(&request);
        let mut fake = Fake::new(vec![
            out(1, "", ""),
            ok(&print_text(&items)),
            ok("jre-openjdk 25.0.1.u8-1\n"),
            ok(REAL_INSTALL_OUT),
            ok(""),
            ok(&installed_text(&items)),
        ]);
        assert_eq!(go(&request, &mut fake).0, Outcome::Ok);
        assert!(fake.ran.contains(&PacmanCommand::MarkAsDeps(vec![pkg(
            "extra",
            "dbeaver-jdbc"
        )])));
    }

    #[test]
    fn a_provider_check_that_cannot_be_read_marks_nothing_and_the_job_goes_on() {
        let request = request(INSTALL);
        let items = transaction_of(&request);
        let timed_out = Ok(RunOutput {
            exit: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: true,
            truncated: false,
        });
        for check in [Err(RunnerError::Spawn), out(2, "", "error: x\n"), timed_out] {
            let mut fake = Fake::new(vec![
                out(1, "", ""),
                ok(&print_text(&items)),
                check,
                ok(REAL_INSTALL_OUT),
                ok(&installed_text(&items)),
            ]);
            assert_eq!(go(&request, &mut fake).0, Outcome::Ok);
            assert!(
                !fake
                    .ran
                    .iter()
                    .any(|c| matches!(c, PacmanCommand::MarkAsDeps(_))),
                "{:?}",
                fake.ran
            );
        }
    }

    /// The answers of a clean install, with the lists you choose before and after the real run. `marks_providers` says
    /// whether the helper will run `-D --asdeps` (it does only for a provider that the before-list does not show).
    fn script_with_lists(
        request: &Request,
        before: Result<RunOutput, RunnerError>,
        after: Result<RunOutput, RunnerError>,
        marks_providers: bool,
    ) -> Vec<Result<RunOutput, RunnerError>> {
        let items = transaction_of(request);
        let mut script = vec![
            out(1, "", ""),
            ok(&print_text(&items)),
            before,
            ok(REAL_INSTALL_OUT),
        ];
        if marks_providers {
            script.push(ok(""));
        }
        script.push(after);
        script
    }

    #[test]
    fn a_package_that_disappears_during_the_run_is_reported_and_counted() {
        let request = request(INSTALL);
        let items = transaction_of(&request);
        let before = "old-one 1-1\nold-two 2-1\n".to_string();
        let script = script_with_lists(&request, ok(&before), ok(&installed_text(&items)), true);
        let (result, events) = go_full(&request, &mut Fake::new(script));
        assert_eq!(result.outcome, Outcome::Ok);
        assert_eq!((result.removed, result.not_installed), (2, 0));
        assert_eq!(result.removal_check, RemovalCheck::Done);
        assert_eq!(notices(&events), vec![Notice::PackagesRemoved(2)]);
    }

    #[test]
    fn nothing_removed_is_a_done_check_without_a_notice() {
        let request = request(INSTALL);
        let items = transaction_of(&request);
        let script = script_with_lists(
            &request,
            ok("other 1-1\n"),
            ok(&format!("other 1-2\n{}", installed_text(&items))),
            true,
        );
        let (result, events) = go_full(&request, &mut Fake::new(script));
        assert_eq!(
            (result.removed, result.removal_check),
            (0, RemovalCheck::Done)
        );
        assert_eq!(notices(&events), vec![]);
    }

    #[test]
    fn both_notices_come_not_installed_first_then_removed() {
        let request = request(INSTALL);
        let items = transaction_of(&request);
        let mut after = installed_text(&items);
        after = after.replace("dbeaver 25.2.1-1\n", "");
        let script = script_with_lists(&request, ok("gone 1-1\n"), ok(&after), true);
        let (result, events) = go_full(&request, &mut Fake::new(script));
        assert_eq!((result.not_installed, result.removed), (1, 1));
        assert_eq!(
            notices(&events),
            vec![Notice::PackagesNotInstalled(1), Notice::PackagesRemoved(1)]
        );
        assert!(matches!(
            events.last(),
            Some(Event::Notice(Notice::PackagesRemoved(1)))
        ));
    }

    #[test]
    fn a_list_that_cannot_be_read_skips_the_check_and_says_so_without_changing_the_outcome() {
        let request = request(INSTALL);
        let items = transaction_of(&request);
        let timed_out = Ok(RunOutput {
            exit: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: true,
            truncated: false,
        });
        let truncated = Ok(RunOutput {
            exit: Some(0),
            stdout: "a 1-1\n".into(),
            stderr: String::new(),
            timed_out: false,
            truncated: true,
        });
        let unreadable = || {
            vec![
                Err(RunnerError::Spawn),
                out(1, "a 1-1\n", "error: broken entry\n"),
                out(2, "", "error: x\n"),
                timed_out.clone(),
                truncated.clone(),
            ]
        };
        for bad in unreadable() {
            // Before the real run:
            let script =
                script_with_lists(&request, bad.clone(), ok(&installed_text(&items)), false);
            let (result, events) = go_full(&request, &mut Fake::new(script));
            assert_eq!(result.outcome, Outcome::Ok, "{bad:?}");
            assert_eq!(
                (result.removed, result.removal_check),
                (0, RemovalCheck::Skipped),
                "{bad:?}"
            );
            assert_eq!(notices(&events), vec![], "{bad:?}");
            // After it:
            let script =
                script_with_lists(&request, ok(&installed_text(&items)), bad.clone(), false);
            let (result, events) = go_full(&request, &mut Fake::new(script));
            assert_eq!(result.outcome, Outcome::Ok, "{bad:?}");
            assert_eq!(
                (result.removed, result.removal_check),
                (0, RemovalCheck::Skipped),
                "{bad:?}"
            );
            assert_eq!(notices(&events), vec![], "{bad:?}");
        }
    }

    #[test]
    fn a_list_that_failed_with_exit_1_is_not_a_partial_answer_to_trust() {
        // `pacman -Q` with no names exits 1 only on an error, and still prints the packages it could read: a cut list
        // would look like removals.
        let request = request(INSTALL);
        let items = transaction_of(&request);
        let partial = out(1, "a 1-1\n", "error: could not read the entry of b\n");
        let script = script_with_lists(&request, ok("a 1-1\nb 2-1\n"), partial, true);
        let (result, events) = go_full(&request, &mut Fake::new(script));
        let _ = items;
        assert_eq!(
            (result.removed, result.removal_check),
            (0, RemovalCheck::Skipped)
        );
        assert_eq!(notices(&events), vec![]);
    }

    #[test]
    fn a_list_with_an_error_line_or_no_packages_is_not_a_clean_list() {
        // R-02: exit 0 is not enough. libalpm skips a damaged entry with a message on stderr, and an empty list on a
        // system that has pacman installed cannot be a complete one.
        let request = request(INSTALL);
        let items = transaction_of(&request);
        let bad_lists = [
            out(
                0,
                "a 1-1\nb 2-1\n",
                "error: could not read the entry of c\n",
            ),
            out(0, "", ""),
        ];
        for bad in bad_lists {
            let script = script_with_lists(&request, ok("a 1-1\nb 2-1\n"), bad.clone(), true);
            let (result, _) = go_full(&request, &mut Fake::new(script));
            assert_eq!(result.removal_check, RemovalCheck::Skipped, "{bad:?}");
            let script =
                script_with_lists(&request, bad.clone(), ok(&installed_text(&items)), false);
            let (result, _) = go_full(&request, &mut Fake::new(script));
            assert_eq!(result.removal_check, RemovalCheck::Skipped, "{bad:?}");
        }
    }

    #[test]
    fn a_real_run_that_failed_to_be_watched_skips_the_check_instead_of_not_running_it() {
        // R-04: after pacman has started, an I/O error means it may have run. NotRun (no journal field) would hide that.
        let request = request(INSTALL);
        let items = transaction_of(&request);
        let script = vec![
            out(1, "", ""),
            ok(&print_text(&items)),
            ok("a 1-1\n"),
            Err(RunnerError::Io),
        ];
        let (result, _) = go_full(&request, &mut Fake::new(script));
        assert_ne!(result.outcome, Outcome::Ok);
        assert_eq!(result.removal_check, RemovalCheck::Skipped);
    }

    #[test]
    fn no_real_run_means_no_removal_check() {
        // A held lock, a pending update, a changed list, and a pacman that could not even be started.
        let request = request(INSTALL);
        let items = transaction_of(&request);
        let mut locked = Fake::new(vec![]);
        locked.lock = true;
        let scripts: Vec<(Fake, &str)> = vec![
            (locked, "lock"),
            (Fake::new(vec![ok(QU_SAMPLE)]), "pending update"),
            (
                Fake::new(vec![out(1, "", ""), ok("extra/other 1-1\n")]),
                "changed list",
            ),
            (
                Fake::new(vec![
                    out(1, "", ""),
                    ok(&print_text(&items)),
                    ok(""),
                    Err(RunnerError::Spawn),
                ]),
                "no pacman",
            ),
        ];
        for (mut fake, what) in scripts {
            let (result, _) = go_full(&request, &mut fake);
            assert_eq!(result.removal_check, RemovalCheck::NotRun, "{what}");
            assert_eq!((result.removed, result.not_installed), (0, 0), "{what}");
        }
    }

    #[test]
    fn a_failed_real_run_is_still_checked_for_removals() {
        let request = request(INSTALL);
        let items = transaction_of(&request);
        let script = vec![
            out(1, "", ""),
            ok(&print_text(&items)),
            ok("keep 1-1\nlost 1-1\n"),
            out(1, CONFLICT_OUT, CONFLICT_ERR),
            ok("keep 1-1\n"),
        ];
        let (result, events) = go_full(&request, &mut Fake::new(script));
        assert!(matches!(
            result.outcome,
            Outcome::Error(ErrorKind::PacmanFailed(_))
        ));
        assert_eq!(
            (result.removed, result.removal_check),
            (1, RemovalCheck::Done)
        );
        assert!(notices(&events).contains(&Notice::PackagesRemoved(1)));
    }

    #[test]
    fn without_chosen_providers_there_is_no_dependency_marking() {
        let mut request = request(INSTALL);
        if let Action::Install(install) = &mut request.action {
            install.providers.clear();
        }
        let items = transaction_of(&request);
        let mut fake = Fake::new(vec![
            out(1, "", ""),
            ok(&print_text(&items)),
            ok("pacman 7.1.0-1\n"),
            ok(REAL_INSTALL_OUT),
            ok(&installed_text(&items)),
        ]);
        assert_eq!(go(&request, &mut fake).0, Outcome::Ok);
        assert!(
            !fake
                .ran
                .iter()
                .any(|c| matches!(c, PacmanCommand::MarkAsDeps(_)))
        );
    }

    #[test]
    fn items_that_are_not_installed_at_the_confirmed_version_are_counted_in_a_notice() {
        let request = request(INSTALL);
        let items = transaction_of(&request);
        let mut installed = installed_text(&items);
        installed = installed.replace("dbeaver 25.2.1-1\n", "dbeaver 25.1.0-1\n");
        let mut fake = Fake::new(vec![
            out(1, "", ""),
            ok(&print_text(&items)),
            ok("pacman 7.1.0-1\n"),
            ok(REAL_INSTALL_OUT),
            ok(""),
            ok(&installed),
        ]);
        let (outcome, events) = go(&request, &mut fake);
        assert_eq!(outcome, Outcome::Ok);
        assert_eq!(notices(&events), vec![Notice::PackagesNotInstalled(1)]);
        assert!(
            matches!(events.last(), Some(Event::Notice(_))),
            "the notice comes last"
        );
    }

    #[test]
    fn a_runner_that_cannot_run_is_a_fixed_failure_and_runs_nothing_more() {
        let request = request(INSTALL);
        let items = transaction_of(&request);
        for error in [
            RunnerError::Spawn,
            RunnerError::Io,
            RunnerError::WorkDir(WorkDirError::WrongMode),
        ] {
            let mut fake = Fake::new(vec![
                out(1, "", ""),
                ok(&print_text(&items)),
                ok(""),
                Err(error),
            ]);
            let (outcome, events) = go(&request, &mut fake);
            assert!(
                matches!(outcome, Outcome::Error(ErrorKind::PacmanFailed(_))),
                "{error:?}"
            );
            // pacman did not run, so there is nothing to check afterwards.
            assert_eq!(fake.ran.len(), 4);
            assert_eq!(notices(&events), vec![]);
        }
    }

    // ---- update -------------------------------------------------------------------------------------------

    #[test]
    fn a_system_update_syncs_a_private_copy_compares_moves_it_in_and_compares_again_before_updating()
     {
        let request = request(UPDATE);
        let items = transaction_of(&request);
        let mut fake = Fake::new(vec![
            ok(SYNC_OUT),
            ok(&print_text(&items)), // the private copy
            ok(&print_text(&items)), // the real lists, after the move
            ok("pacman 7.1.0-1\n"),
            ok(REAL_INSTALL_OUT),
            ok(&installed_text(&items)),
        ]);
        let (result, _) = go_full(&request, &mut fake);
        assert_eq!(result.outcome, Outcome::Ok);
        assert_eq!(
            fake.ran,
            vec![
                PacmanCommand::PrivateSync,
                PacmanCommand::PrivateDryRunUpdate(vec![]),
                PacmanCommand::DryRunUpdate(vec![]),
                PacmanCommand::ListInstalled,
                PacmanCommand::RealUpdate(vec![]),
                PacmanCommand::ListInstalled,
            ]
        );
        // The folder was made and checked once, before the first command; the move happened after the private dry run
        // (two commands had run) and before the dry run on the real lists.
        assert_eq!(fake.prepared, 1);
        assert_eq!(fake.moved_after, Some(2));
        assert_eq!(result.lists, Some(ListsState::Moved));
        // One refresh, and it is the private one: no command refreshes the real lists.
        assert_eq!(
            fake.ran
                .iter()
                .filter(|c| **c == PacmanCommand::PrivateSync)
                .count(),
            1
        );
        assert!(fake.ran.iter().all(|c| !refreshes_the_real_lists(c)));
        assert!(!fake.ran.contains(&PacmanCommand::PendingUpdates));
    }

    #[test]
    fn a_locked_database_stops_the_private_sync_and_nothing_else_runs() {
        let request = request(UPDATE);
        let mut fake = Fake::new(vec![out(1, "", LOCK_SYNC_ERR)]);
        let (result, _) = go_full(&request, &mut fake);
        assert_eq!(result.outcome, Outcome::Error(ErrorKind::DatabaseLocked));
        assert_eq!(fake.ran, vec![PacmanCommand::PrivateSync]);
        assert_eq!(fake.moved_after, None);
        assert_eq!(result.lists, Some(ListsState::Untouched));
    }

    #[test]
    fn a_private_sync_that_fails_for_another_reason_is_pacman_failed() {
        let request = request(UPDATE);
        let mut fake = Fake::new(vec![out(
            1,
            "",
            "error: failed to synchronize all databases (invalid url)\n",
        )]);
        let (result, _) = go_full(&request, &mut fake);
        assert!(matches!(
            result.outcome,
            Outcome::Error(ErrorKind::PacmanFailed(_))
        ));
        assert_eq!(fake.ran, vec![PacmanCommand::PrivateSync]);
        assert_eq!(fake.moved_after, None);
        assert_eq!(result.lists, Some(ListsState::Untouched));
    }

    #[test]
    fn an_update_whose_private_dry_run_differs_is_a_changed_transaction_and_moves_nothing() {
        let request = request(UPDATE);
        let mut items = transaction_of(&request);
        items.pop();
        let mut fake = Fake::new(vec![ok(SYNC_OUT), ok(&print_text(&items))]);
        let (result, _) = go_full(&request, &mut fake);
        assert_eq!(
            result.outcome,
            Outcome::Error(ErrorKind::TransactionChanged)
        );
        assert_eq!(
            fake.ran,
            vec![
                PacmanCommand::PrivateSync,
                PacmanCommand::PrivateDryRunUpdate(vec![])
            ]
        );
        assert_eq!(fake.moved_after, None, "the lists were moved in anyway");
        assert_eq!(result.lists, Some(ListsState::Untouched));
    }

    #[test]
    fn a_dry_run_that_differs_on_the_real_lists_after_the_move_is_reported_as_moved() {
        // Another root process changed the lists between the move and the check (T-09): the helper refuses, and the
        // journal must say the real lists were replaced.
        let request = request(UPDATE);
        let items = transaction_of(&request);
        let mut changed = items.clone();
        changed.pop();
        let mut fake = Fake::new(vec![
            ok(SYNC_OUT),
            ok(&print_text(&items)),
            ok(&print_text(&changed)),
        ]);
        let (result, _) = go_full(&request, &mut fake);
        assert_eq!(
            result.outcome,
            Outcome::Error(ErrorKind::TransactionChanged)
        );
        assert_eq!(fake.ran.len(), 3);
        assert_eq!(fake.moved_after, Some(2));
        assert_eq!(result.lists, Some(ListsState::Moved));
    }

    #[test]
    fn a_private_lock_refuses_before_pacman_runs_with_the_restart_text() {
        let request = request(UPDATE);
        let mut fake = Fake::new(vec![]);
        fake.private_lock = true;
        let (result, _) = go_full(&request, &mut fake);
        let Outcome::Error(ErrorKind::PacmanFailed(details)) = &result.outcome else {
            panic!("{:?}", result.outcome);
        };
        assert!(details.as_str().contains("restart the computer"));
        assert!(details.as_str().contains("Nothing was changed"));
        assert_eq!(fake.ran, vec![]);
        assert_eq!(fake.prepared, 1);
        assert_eq!(fake.moved_after, None);
        assert_eq!(result.lists, Some(ListsState::Untouched));
    }

    #[test]
    fn a_private_folder_that_is_not_as_required_refuses_before_pacman_runs() {
        let request = request(UPDATE);
        let mut fake = Fake::new(vec![]);
        fake.prepare_result = Err(ListsError::FolderUnusable);
        let (result, _) = go_full(&request, &mut fake);
        assert!(matches!(
            result.outcome,
            Outcome::Error(ErrorKind::PacmanFailed(_))
        ));
        assert_eq!(fake.ran, vec![]);
        assert_eq!(result.lists, Some(ListsState::Untouched));
    }

    #[test]
    fn a_refused_move_stops_the_update_with_its_own_text_and_the_lists_untouched() {
        let request = request(UPDATE);
        let items = transaction_of(&request);
        for (error, expected) in [
            (ListsError::Locked, None),
            (ListsError::StoppedSigning, Some("stopped signing")),
            (ListsError::BadFile, Some("could trust")),
            (ListsError::Io, Some("could not copy")),
        ] {
            let mut fake = Fake::new(vec![ok(SYNC_OUT), ok(&print_text(&items))]);
            fake.move_result = Err(MoveError { error, renamed: 0 });
            let (result, _) = go_full(&request, &mut fake);
            match expected {
                None => assert_eq!(result.outcome, Outcome::Error(ErrorKind::DatabaseLocked)),
                Some(words) => {
                    let Outcome::Error(ErrorKind::PacmanFailed(details)) = &result.outcome else {
                        panic!("{error:?}: {:?}", result.outcome);
                    };
                    assert!(details.as_str().contains(words), "{error:?}");
                }
            }
            assert_eq!(
                fake.ran.len(),
                2,
                "{error:?}: nothing runs after a refused move"
            );
            assert_eq!(result.lists, Some(ListsState::Untouched), "{error:?}");
        }
    }

    #[test]
    fn a_move_that_renamed_nothing_is_not_reported_as_moved() {
        // Review 6.10b R-05: the journal's TIDEWISE_LISTS must tell the truth about the real lists.
        let request = request(UPDATE);
        let items = transaction_of(&request);
        let mut fake = Fake::new(vec![
            ok(SYNC_OUT),
            ok(&print_text(&items)),
            ok(&print_text(&items)), // the real dry run still equals the list
            ok("pacman 7.1.0-1\n"),
            ok(REAL_INSTALL_OUT),
            ok(&installed_text(&items)),
        ]);
        fake.move_result = Ok(0);
        let (result, _) = go_full(&request, &mut fake);
        assert_eq!(result.outcome, Outcome::Ok);
        assert_eq!(result.lists, Some(ListsState::Untouched));
    }

    #[test]
    fn a_move_that_stopped_after_some_renames_is_reported_as_moved() {
        let request = request(UPDATE);
        let items = transaction_of(&request);
        let mut fake = Fake::new(vec![ok(SYNC_OUT), ok(&print_text(&items))]);
        fake.move_result = Err(MoveError {
            error: ListsError::Io,
            renamed: 1,
        });
        let (result, _) = go_full(&request, &mut fake);
        assert!(matches!(
            result.outcome,
            Outcome::Error(ErrorKind::PacmanFailed(_))
        ));
        assert_eq!(fake.ran.len(), 2);
        assert_eq!(result.lists, Some(ListsState::Moved));
    }

    /// The folder the private copy of the package lists lives in (decision 0007).
    const PRIVATE_LISTS: &str = "/run/tidewise-helper-lists/";

    /// True if `command` refreshes package lists (`-y`, in any short option) anywhere but in the private copy.
    fn refreshes_the_real_lists(command: &PacmanCommand) -> bool {
        let args = crate::pacman_args::args(command).unwrap();
        let before_the_names: Vec<&String> = args.iter().take_while(|a| *a != "--").collect();
        let refreshes = before_the_names
            .iter()
            .any(|a| a.starts_with('-') && !a.starts_with("--") && a.contains('y'));
        let private = args
            .windows(2)
            .any(|pair| pair[0] == "--dbpath" && pair[1] == PRIVATE_LISTS);
        refreshes && !private
    }

    #[test]
    fn a_refused_update_does_not_leave_the_real_lists_refreshed() {
        // Both flows that sync (review 5.8 R-02, golden rule 4): the dry run differs from the confirmed list, so the
        // helper refuses, and no command it ran may have refreshed the real package lists.
        for text in [UPDATE, BOTH] {
            let request = request(text);
            let mut items = transaction_of(&request);
            items.pop();
            let mut fake = Fake::new(vec![ok(SYNC_OUT), ok(&print_text(&items))]);
            let (outcome, _) = go(&request, &mut fake);
            assert_eq!(outcome, Outcome::Error(ErrorKind::TransactionChanged));
            assert_eq!(fake.ran.len(), 2, "{:?}", fake.ran);
            assert_eq!(fake.moved_after, None, "the lists were moved in anyway");
            for command in &fake.ran {
                assert!(
                    !refreshes_the_real_lists(command),
                    "a refused update refreshed the real package lists: {command:?}"
                );
            }
        }
    }

    #[test]
    fn update_and_install_passes_the_targets_and_marks_the_providers() {
        let mut request = request(BOTH);
        if let Action::UpdateSystemAndInstall(install) = &mut request.action {
            install.providers.push(ProviderChoice {
                dependency: "a-virtual".to_string(),
                chosen: pkg("extra", "vlc"),
            });
        }
        let install = the_install(&request);
        let items = transaction_of(&request);
        let mut fake = Fake::new(vec![
            ok(SYNC_OUT),
            ok(&print_text(&items)),
            ok(&print_text(&items)),
            ok("pacman 7.1.0-1\n"),
            ok(REAL_INSTALL_OUT),
            ok(""),
            ok(&installed_text(&items)),
        ]);
        assert_eq!(go(&request, &mut fake).0, Outcome::Ok);
        // The targets are in BOTH dry runs, and the move sits between them.
        assert_eq!(fake.moved_after, Some(2));
        assert_eq!(
            fake.ran,
            vec![
                PacmanCommand::PrivateSync,
                PacmanCommand::PrivateDryRunUpdate(names_of(&install)),
                PacmanCommand::DryRunUpdate(names_of(&install)),
                PacmanCommand::ListInstalled,
                PacmanCommand::RealUpdate(names_of(&install)),
                PacmanCommand::MarkAsDeps(vec![pkg("extra", "vlc")]),
                PacmanCommand::ListInstalled,
            ]
        );
    }

    #[test]
    fn a_declined_replacement_in_an_update_is_a_notice_not_an_error() {
        // The dry run lists new-pkg (it always does); --ask 66 declined the replacement, so it is not installed.
        let request = request(UPDATE);
        let items = transaction_of(&request);
        let partial = items
            .iter()
            .skip(1)
            .map(|i| format!("{} {}\n", i.name, i.version))
            .collect::<String>();
        let partial = format!("pacman 7.1.0-1\n{partial}");
        let mut fake = Fake::new(vec![
            ok(SYNC_OUT),
            ok(&print_text(&items)),
            ok(&print_text(&items)),
            ok("pacman 7.1.0-1\n"),
            ok(REAL_INSTALL_OUT),
            ok(&partial),
        ]);
        let (outcome, events) = go(&request, &mut fake);
        assert_eq!(outcome, Outcome::Ok);
        assert_eq!(notices(&events), vec![Notice::PackagesNotInstalled(1)]);
    }

    // ---- shape --------------------------------------------------------------------------------------------

    #[test]
    fn failure_texts_are_ours_or_pacmans_stderr_never_the_requests() {
        let request = request(INSTALL);
        let items = transaction_of(&request);
        let mut fake = Fake::new(vec![
            out(1, "", ""),
            ok(&print_text(&items)),
            ok(""),
            Err(RunnerError::Spawn),
        ]);
        let (outcome, _) = go(&request, &mut fake);
        let Outcome::Error(ErrorKind::PacmanFailed(details)) = outcome else {
            panic!("expected PacmanFailed");
        };
        assert_eq!(details, Details::from_raw("pacman could not be started."));
    }

    #[test]
    fn an_unusable_work_folder_has_one_fixed_text_everywhere() {
        let expected = ErrorKind::PacmanFailed(Details::from_raw(
            "the helper's work folder could not be used.",
        ));
        assert_eq!(work_folder_unusable(), expected);
        assert_eq!(
            runner_failure(RunnerError::WorkDir(WorkDirError::WrongMode)),
            expected
        );
    }

    #[test]
    fn a_plain_install_never_syncs_and_never_uses_the_update_commands() {
        let request = request(INSTALL);
        let mut fake = Fake::new(clean_install_script(&request));
        let (result, _) = go_full(&request, &mut fake);
        // A plain install never prepares, looks at, syncs or moves the private copy, and says nothing about the lists.
        assert_eq!((fake.prepared, fake.moved_after), (0, None));
        assert_eq!(result.lists, None);
        for command in &fake.ran {
            assert!(
                !matches!(
                    command,
                    PacmanCommand::PrivateSync
                        | PacmanCommand::PrivateDryRunUpdate(_)
                        | PacmanCommand::DryRunUpdate(_)
                        | PacmanCommand::RealUpdate(_)
                ),
                "{command:?}"
            );
        }
    }
}
