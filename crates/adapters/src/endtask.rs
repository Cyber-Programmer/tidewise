//! Ending a running app: ask it to close (SIGTERM), wait, and only if it is stuck force it (SIGKILL).
//! The signals are sent with `/usr/bin/kill` by absolute path through the runner (no shell, no
//! `unsafe`). A whole process group is signalled only when every process in it belongs to the app, so
//! a launcher script or a terminal that shares the group is never hit.

use std::path::Path;
use std::time::Duration;

use crate::runner::CommandRunner;

/// A process and the process group it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub pgid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dest {
    /// Every process of this group (`kill -- -PGID`).
    Group(u32),
    /// Just these processes.
    Pids(Vec<u32>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    #[error("there is nothing to end")]
    Empty,
    #[error("Tidewise never ends itself or the first process of the system")]
    NotAllowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// Ask politely: the app may save and close.
    Term,
    /// Stop at once, without a chance to save.
    Kill,
}

/// Every process and its group, from `/proc/<pid>/stat` under `root`.
pub fn read_table(root: &Path) -> Vec<ProcInfo> {
    let _ = root;
    Vec::new()
}

/// Where the signals go for an app made of `members`. `own_pid`/`own_pgid` are Tidewise's own.
pub fn plan(
    members: &[u32],
    table: &[ProcInfo],
    own_pid: u32,
    own_pgid: u32,
) -> Result<Vec<Dest>, Refused> {
    let _ = (members, table, own_pid, own_pgid);
    Err(Refused::Empty)
}

/// Send one signal to one destination.
pub fn send(runner: &dyn CommandRunner, signal: Signal, dest: &Dest) -> Result<(), String> {
    let _ = (runner, signal, dest);
    Err("not sent".into())
}

/// Is this process still running? A zombie (finished, not yet collected) counts as gone.
pub fn is_alive(root: &Path, pid: u32) -> bool {
    let _ = (root, pid);
    true
}

/// Wait until none of `pids` is running, at most `timeout`. Returns who is still running.
pub fn wait_until_gone(root: &Path, pids: &[u32], timeout: Duration, poll: Duration) -> Vec<u32> {
    let _ = (root, timeout, poll);
    pids.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::SystemRunner;
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    use std::process::{Child, Command};
    use std::time::Instant;

    fn proc() -> &'static Path {
        Path::new("/proc")
    }

    fn own() -> (u32, u32) {
        let pid = std::process::id();
        let table = read_table(proc());
        let pgid = table.iter().find(|p| p.pid == pid).map(|p| p.pgid).unwrap();
        (pid, pgid)
    }

    /// A child the test started; it is killed when the test ends, pass or fail, so a failing test
    /// never leaves a process behind.
    struct Guard(Child);

    impl std::ops::Deref for Guard {
        type Target = Child;
        fn deref(&self) -> &Child {
            &self.0
        }
    }

    impl std::ops::DerefMut for Guard {
        fn deref_mut(&mut self) -> &mut Child {
            &mut self.0
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// A sleeping child in its own process group. Only ever signalled by these tests.
    fn sleeper() -> Guard {
        Guard(
            Command::new("/usr/bin/sleep")
                .arg("60")
                .process_group(0)
                .spawn()
                .unwrap(),
        )
    }

    fn wait_exit(child: &mut Child) -> Option<std::process::ExitStatus> {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if let Some(status) = child.try_wait().unwrap() {
                return Some(status);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }

    fn info(pid: u32, pgid: u32) -> ProcInfo {
        ProcInfo { pid, pgid }
    }

    #[test]
    fn the_table_lists_this_process_with_its_group() {
        let table = read_table(proc());
        let (pid, pgid) = own();
        assert!(table.len() > 5);
        assert!(table.contains(&info(pid, pgid)));
        assert!(table.iter().any(|p| p.pid == 1));
        assert!(read_table(Path::new("/no/such/proc")).is_empty());
    }

    #[test]
    fn a_group_is_signalled_only_when_every_process_in_it_belongs_to_the_app() {
        let table = [
            info(100, 100),
            info(101, 100),
            info(200, 200),
            info(201, 100),
            info(300, 300),
        ];
        // 100, 101 and 201 are the whole group 100.
        assert_eq!(
            plan(&[100, 101, 201], &table, 1, 1),
            Ok(vec![Dest::Group(100)])
        );
        // 201 (in the same group) is not ours: signal the two by pid instead.
        assert_eq!(
            plan(&[100, 101], &table, 1, 1),
            Ok(vec![Dest::Pids(vec![100, 101])])
        );
        // Two whole groups and a lone process in a shared one.
        let table2 = [
            info(10, 10),
            info(11, 10),
            info(20, 20),
            info(30, 99),
            info(99, 99),
        ];
        assert_eq!(
            plan(&[10, 11, 20, 30], &table2, 1, 1),
            Ok(vec![Dest::Group(10), Dest::Group(20), Dest::Pids(vec![30])])
        );
    }

    #[test]
    fn the_own_group_the_first_process_and_ourselves_are_never_hit() {
        let table = [info(1, 1), info(50, 50), info(60, 50), info(70, 70)];
        assert_eq!(plan(&[], &table, 5, 5), Err(Refused::Empty));
        assert_eq!(plan(&[1], &table, 5, 5), Err(Refused::NotAllowed));
        assert_eq!(plan(&[70, 5], &table, 5, 5), Err(Refused::NotAllowed));
        assert_eq!(plan(&[0], &table, 5, 5), Err(Refused::NotAllowed));
        // The app shares *our* group: by pid, never the group.
        let shared = [info(5, 5), info(80, 5)];
        assert_eq!(plan(&[80], &shared, 5, 5), Ok(vec![Dest::Pids(vec![80])]));
        // Group 0 and 1 are never signalled as groups.
        let odd = [info(90, 0), info(91, 1)];
        assert_eq!(
            plan(&[90, 91], &odd, 5, 5),
            Ok(vec![Dest::Pids(vec![90, 91])])
        );
        // A pid that is not in the table is simply signalled by pid.
        assert_eq!(
            plan(&[4242], &table, 5, 5),
            Ok(vec![Dest::Pids(vec![4242])])
        );
    }

    #[test]
    fn term_ends_a_process_that_listens_and_its_whole_group() {
        let mut child = sleeper();
        let pid = child.id();
        let table = read_table(proc());
        let (own_pid, own_pgid) = own();
        let dests = plan(&[pid], &table, own_pid, own_pgid).unwrap();
        assert_eq!(dests, [Dest::Group(pid)], "it leads its own group");
        for d in &dests {
            send(&SystemRunner, Signal::Term, d).unwrap();
        }
        let status = wait_exit(&mut child).expect("it ended");
        assert_eq!(status.signal(), Some(15));
        assert!(!is_alive(proc(), pid));
    }

    #[test]
    fn a_process_that_ignores_term_needs_kill() {
        let mut child = Guard(
            Command::new("/usr/bin/sh")
                .args(["-c", "trap '' TERM; while :; do /usr/bin/sleep 1; done"])
                .process_group(0)
                .spawn()
                .unwrap(),
        );
        let pid = child.id();
        std::thread::sleep(Duration::from_millis(300));
        // Only the shell is "the app": the group also holds its sleep, so it is signalled by pid.
        let table = read_table(proc());
        let (own_pid, own_pgid) = own();
        let dests = plan(&[pid], &table, own_pid, own_pgid).unwrap();
        assert_eq!(dests, [Dest::Pids(vec![pid])]);
        send(&SystemRunner, Signal::Term, &dests[0]).unwrap();
        let still = wait_until_gone(
            proc(),
            &[pid],
            Duration::from_millis(600),
            Duration::from_millis(50),
        );
        assert_eq!(still, [pid], "it ignored the polite request");
        send(&SystemRunner, Signal::Kill, &dests[0]).unwrap();
        let status = wait_exit(&mut child).expect("killed");
        assert_eq!(status.signal(), Some(9));
    }

    #[test]
    fn waiting_returns_at_once_when_everything_is_gone_and_names_the_stragglers() {
        let started = Instant::now();
        let gone = wait_until_gone(
            proc(),
            &[4_000_000_000, 4_000_000_001],
            Duration::from_secs(5),
            Duration::from_millis(50),
        );
        assert!(gone.is_empty());
        assert!(started.elapsed() < Duration::from_secs(1));
        let (own_pid, _) = own();
        let left = wait_until_gone(
            proc(),
            &[own_pid, 4_000_000_000],
            Duration::from_millis(200),
            Duration::from_millis(50),
        );
        assert_eq!(left, [own_pid]);
    }

    #[test]
    fn a_finished_but_uncollected_child_counts_as_gone() {
        let mut child = Command::new("/usr/bin/true").spawn().unwrap();
        let pid = child.id();
        let start = Instant::now();
        while is_alive(proc(), pid) && start.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!is_alive(proc(), pid), "zombie must not count as running");
        child.wait().unwrap();
    }

    #[test]
    fn nothing_is_sent_for_an_empty_or_nonsense_destination() {
        struct Never;
        impl CommandRunner for Never {
            fn run(&self, _p: &str, _a: &[&str]) -> Result<crate::runner::Output, String> {
                panic!("must not run");
            }
        }
        for dest in [
            Dest::Pids(vec![]),
            Dest::Pids(vec![0]),
            Dest::Group(0),
            Dest::Group(1),
            Dest::Pids(vec![1]),
        ] {
            assert!(send(&Never, Signal::Term, &dest).is_err(), "{dest:?}");
        }
    }

    #[test]
    fn the_command_is_kill_by_absolute_path_with_the_signal_and_no_shell() {
        use std::cell::RefCell;
        struct Rec(RefCell<Vec<(String, Vec<String>)>>);
        impl CommandRunner for Rec {
            fn run(&self, p: &str, a: &[&str]) -> Result<crate::runner::Output, String> {
                self.0
                    .borrow_mut()
                    .push((p.into(), a.iter().map(|s| s.to_string()).collect()));
                Ok(crate::runner::Output {
                    success: true,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            }
        }
        let rec = Rec(RefCell::new(vec![]));
        send(&rec, Signal::Term, &Dest::Group(4242)).unwrap();
        send(&rec, Signal::Kill, &Dest::Pids(vec![11, 12])).unwrap();
        let calls = rec.0.borrow();
        assert_eq!(
            calls[0],
            (
                "/usr/bin/kill".to_string(),
                vec!["-s".into(), "TERM".into(), "--".into(), "-4242".into()]
            )
        );
        assert_eq!(
            calls[1],
            (
                "/usr/bin/kill".to_string(),
                vec![
                    "-s".into(),
                    "KILL".into(),
                    "--".into(),
                    "11".into(),
                    "12".into()
                ]
            )
        );
    }
}
