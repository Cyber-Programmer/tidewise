//! The jobs: install, update the system, update and install (threats T-09, T-10, T-11, T-12, T-17, T-18).
//!
//! Every flow is a function of the (already validated) request and a `Runner`, so each path is a unit test with a
//! scripted fake. The order is the point:
//!
//! Install:   lock look -> `-Qu` (pending updates block a plain install) -> dry run must equal the confirmed list ->
//!            real `-S --noconfirm --ask 66` -> `-D --asdeps` for the chosen providers -> post-check.
//! Update:    lock look -> `-Sy` once -> dry run `-Su` (plus any targets) must equal the confirmed list -> real `-Su`
//!            with no second sync -> `-D --asdeps` -> post-check. (Update and install is the same with targets.)
//!
//! After every real run each confirmed item is checked with `pacman -Q`; the number that are not installed at the
//! confirmed version (a declined replacement, which `--print` still lists, or a change by another package manager in
//! between) is reported as a `Notice`, count only. Nothing here removes, creates or fixes `db.lck`: it is only looked
//! at, and pacman's own advice about it is never shown (golden rule 9).

use tidewise_helper_protocol::{Action, ErrorKind, Event, Install, Outcome, Request};

use crate::runner::Runner;

/// Runs the job the request asks for. Progress and notices go to `on_event`; the caller prints the final line.
pub fn run_job(
    request: &Request,
    runner: &mut dyn Runner,
    on_event: &mut dyn FnMut(Event),
) -> Outcome {
    let _ = (
        request,
        runner,
        on_event,
        Action::UpdateSystem,
        ErrorKind::Malformed,
    );
    let _: Option<Install> = None;
    Outcome::Ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pacman_args::PacmanCommand;
    use crate::runner::{RunOutput, RunnerError};
    use crate::workdir::WorkDirError;
    use std::collections::VecDeque;
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

    // ---- a scripted runner --------------------------------------------------------------------------------

    /// Answers each command with the next scripted result and remembers what was run.
    struct Fake {
        lock: bool,
        script: VecDeque<Result<RunOutput, RunnerError>>,
        ran: Vec<PacmanCommand>,
    }

    impl Fake {
        fn new(script: Vec<Result<RunOutput, RunnerError>>) -> Fake {
            Fake {
                lock: false,
                script: script.into(),
                ran: Vec::new(),
            }
        }
    }

    impl Runner for Fake {
        fn run(
            &mut self,
            command: &PacmanCommand,
            on_stdout_line: &mut dyn FnMut(&str),
        ) -> Result<RunOutput, RunnerError> {
            self.ran.push(command.clone());
            let answer = self.script.pop_front().unwrap_or(Err(RunnerError::Io))?;
            for line in answer.stdout.split('\n') {
                on_stdout_line(line);
            }
            Ok(answer)
        }

        fn database_lock_present(&self) -> bool {
            self.lock
        }
    }

    fn out(exit: i32, stdout: &str, stderr: &str) -> Result<RunOutput, RunnerError> {
        Ok(RunOutput {
            exit: Some(exit),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            timed_out: false,
            truncated: false,
        })
    }

    fn ok(stdout: &str) -> Result<RunOutput, RunnerError> {
        out(0, stdout, "")
    }

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
        items
            .iter()
            .map(|i| format!("{} {}\n", i.name, i.version))
            .collect()
    }

    fn pkg(repo: &str, name: &str) -> PackageRef {
        PackageRef {
            repo: repo.to_string(),
            name: name.to_string(),
        }
    }

    fn refs_of(items: &[TransactionItem]) -> Vec<PackageRef> {
        items.iter().map(|i| pkg(&i.repo, &i.name)).collect()
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

    fn go(request: &Request, fake: &mut Fake) -> (Outcome, Vec<Event>) {
        let mut events = Vec::new();
        let outcome = run_job(request, fake, &mut |event| events.push(event));
        (outcome, events)
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
                PacmanCommand::RealInstall(names_of(&install)),
                PacmanCommand::MarkAsDeps(vec![pkg("extra", "jre-openjdk")]),
                PacmanCommand::VerifyInstalled(refs_of(&install.transaction)),
            ]
        );
        assert_eq!(notices(&events), vec![]);
    }

    #[test]
    fn the_targets_come_before_the_chosen_providers() {
        let request = request(INSTALL);
        let mut fake = Fake::new(clean_install_script(&request));
        go(&request, &mut fake);
        let PacmanCommand::RealInstall(names) = &fake.ran[2] else {
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
            out(1, CONFLICT_OUT, CONFLICT_ERR),
            ok(""),
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
            Some(PacmanCommand::VerifyInstalled(_))
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
            Some(PacmanCommand::VerifyInstalled(_))
        ));
        assert_eq!(notices(&events), vec![]);
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
            let mut fake = Fake::new(vec![out(1, "", ""), ok(&print_text(&items)), Err(error)]);
            let (outcome, events) = go(&request, &mut fake);
            assert!(
                matches!(outcome, Outcome::Error(ErrorKind::PacmanFailed(_))),
                "{error:?}"
            );
            // pacman did not run, so there is nothing to check afterwards.
            assert_eq!(fake.ran.len(), 3);
            assert_eq!(notices(&events), vec![]);
        }
    }

    // ---- update -------------------------------------------------------------------------------------------

    #[test]
    fn a_system_update_syncs_once_compares_then_updates_without_another_sync() {
        let request = request(UPDATE);
        let items = transaction_of(&request);
        let mut fake = Fake::new(vec![
            ok(SYNC_OUT),
            ok(&print_text(&items)),
            ok(REAL_INSTALL_OUT),
            ok(&installed_text(&items)),
        ]);
        let (outcome, _) = go(&request, &mut fake);
        assert_eq!(outcome, Outcome::Ok);
        assert_eq!(
            fake.ran,
            vec![
                PacmanCommand::Sync,
                PacmanCommand::DryRunUpdate(vec![]),
                PacmanCommand::RealUpdate(vec![]),
                PacmanCommand::VerifyInstalled(refs_of(&items)),
            ]
        );
        assert_eq!(
            fake.ran
                .iter()
                .filter(|c| **c == PacmanCommand::Sync)
                .count(),
            1
        );
        assert!(!fake.ran.contains(&PacmanCommand::PendingUpdates));
    }

    #[test]
    fn a_locked_database_stops_the_sync_and_nothing_else_runs() {
        let request = request(UPDATE);
        let mut fake = Fake::new(vec![out(1, "", LOCK_SYNC_ERR)]);
        let (outcome, _) = go(&request, &mut fake);
        assert_eq!(outcome, Outcome::Error(ErrorKind::DatabaseLocked));
        assert_eq!(fake.ran, vec![PacmanCommand::Sync]);
    }

    #[test]
    fn a_sync_that_fails_for_another_reason_is_pacman_failed() {
        let request = request(UPDATE);
        let mut fake = Fake::new(vec![out(
            1,
            "",
            "error: failed to synchronize all databases (invalid url)\n",
        )]);
        let (outcome, _) = go(&request, &mut fake);
        assert!(matches!(
            outcome,
            Outcome::Error(ErrorKind::PacmanFailed(_))
        ));
        assert_eq!(fake.ran, vec![PacmanCommand::Sync]);
    }

    #[test]
    fn an_update_whose_dry_run_differs_after_the_sync_is_a_changed_transaction() {
        let request = request(UPDATE);
        let mut items = transaction_of(&request);
        items.pop();
        let mut fake = Fake::new(vec![ok(SYNC_OUT), ok(&print_text(&items))]);
        let (outcome, _) = go(&request, &mut fake);
        assert_eq!(outcome, Outcome::Error(ErrorKind::TransactionChanged));
        assert_eq!(
            fake.ran,
            vec![PacmanCommand::Sync, PacmanCommand::DryRunUpdate(vec![])]
        );
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
            ok(REAL_INSTALL_OUT),
            ok(""),
            ok(&installed_text(&items)),
        ]);
        assert_eq!(go(&request, &mut fake).0, Outcome::Ok);
        assert_eq!(
            fake.ran,
            vec![
                PacmanCommand::Sync,
                PacmanCommand::DryRunUpdate(names_of(&install)),
                PacmanCommand::RealUpdate(names_of(&install)),
                PacmanCommand::MarkAsDeps(vec![pkg("extra", "vlc")]),
                PacmanCommand::VerifyInstalled(refs_of(&items)),
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
        let mut fake = Fake::new(vec![
            ok(SYNC_OUT),
            ok(&print_text(&items)),
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
            Err(RunnerError::Spawn),
        ]);
        let (outcome, _) = go(&request, &mut fake);
        let Outcome::Error(ErrorKind::PacmanFailed(details)) = outcome else {
            panic!("expected PacmanFailed");
        };
        assert_eq!(details, Details::from_raw("pacman could not be started."));
    }

    #[test]
    fn a_plain_install_never_syncs_and_never_uses_the_update_commands() {
        let request = request(INSTALL);
        let mut fake = Fake::new(clean_install_script(&request));
        go(&request, &mut fake);
        for command in &fake.ran {
            assert!(
                !matches!(
                    command,
                    PacmanCommand::Sync
                        | PacmanCommand::DryRunUpdate(_)
                        | PacmanCommand::RealUpdate(_)
                ),
                "{command:?}"
            );
        }
    }
}
