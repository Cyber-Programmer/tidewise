//! Support shared by the container tests of the helper (WBS 5.3a and 5.3b). Only compiled into test binaries that say
//! `mod common;`. Everything here runs inside the Tidewise test container, as its root; the guard in each test makes
//! sure of that.
//!
//! Not product code: it may use a shell or edit `/etc/pacman.conf`, because the container is thrown away.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

use tidewise_container_tests::require_container;

pub const HELPER: &str = "/opt/tidewise/tidewise-helper";
pub const JOURNAL_DIR: &str = "/run/systemd/journal";
pub const JOURNAL_SOCKET: &str = "/run/systemd/journal/socket";
pub const WORK_DIR: &str = "/run/tidewise-helper";
pub const REPO_DIR: &str = "/srv/tidewise-test";
pub const LATER_DIR: &str = "/srv/tidewise-later";

/// The protocol's request cap (2 MiB, `MAX_REQUEST_BYTES` in `crates/helper-protocol`).
pub const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;

pub fn assert_inside_the_test_container() {
    let guard = require_container();
    assert!(
        guard.is_ok(),
        "refusing to run: {}",
        guard.err().map(|why| why.to_string()).unwrap_or_default()
    );
}

// ---- a fake journald ------------------------------------------------------------------------------------

pub struct FakeJournal {
    socket: UnixDatagram,
}

impl FakeJournal {
    pub fn start() -> FakeJournal {
        fs::create_dir_all(JOURNAL_DIR).unwrap();
        let _ = fs::remove_file(JOURNAL_SOCKET);
        let socket = UnixDatagram::bind(JOURNAL_SOCKET).unwrap();
        socket.set_nonblocking(true).unwrap();
        FakeJournal { socket }
    }

    /// Everything that was sent so far (the helper has already exited, so the datagrams are queued).
    pub fn datagrams(&self) -> Vec<String> {
        let mut found = Vec::new();
        let mut buffer = vec![0u8; 64 * 1024];
        while let Ok(n) = self.socket.recv(&mut buffer) {
            found.push(String::from_utf8_lossy(&buffer[..n]).into_owned());
        }
        found
    }
}

impl Drop for FakeJournal {
    fn drop(&mut self) {
        let _ = fs::remove_file(JOURNAL_SOCKET);
        let _ = fs::remove_dir(JOURNAL_DIR);
        let _ = fs::remove_dir("/run/systemd");
    }
}

// ---- running the helper ---------------------------------------------------------------------------------

pub struct Ran {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl Ran {
    /// The lines of stdout, without their new lines.
    pub fn lines(&self) -> Vec<&str> {
        self.stdout
            .split_inclusive('\n')
            .map(|l| l.trim_end())
            .collect()
    }
}

/// Starts the helper with an empty environment plus `envs`, stdin as a pipe.
pub fn spawn(
    program: &str,
    args: &[&str],
    envs: &[(&str, &str)],
    stdout: Stdio,
    stderr: Stdio,
) -> Child {
    Command::new(program)
        .args(args)
        .env_clear()
        .envs(envs.iter().copied())
        .stdin(Stdio::piped())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .unwrap()
}

/// Writes `stdin` to the child on a thread (so a child that stops reading cannot block us) and closes it.
pub fn feed(child: &mut Child, stdin: &[u8]) -> std::thread::JoinHandle<()> {
    let mut pipe = child.stdin.take().unwrap();
    let data = stdin.to_vec();
    std::thread::spawn(move || {
        let _ = pipe.write_all(&data);
    })
}

pub fn run_with(args: &[&str], envs: &[(&str, &str)], stdin: &[u8]) -> Ran {
    let mut child = spawn(HELPER, args, envs, Stdio::piped(), Stdio::piped());
    let writer = feed(&mut child, stdin);
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    Ran {
        status: output.status,
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

pub fn run(summary: &str, stdin: &[u8]) -> Ran {
    run_with(&[summary], &[], stdin)
}

pub fn full() -> Stdio {
    Stdio::from(OpenOptions::new().write(true).open("/dev/full").unwrap())
}

pub fn wait_up_to(child: &mut Child, limit: Duration) -> Option<ExitStatus> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return Some(status);
        }
        if start.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Waits until `path` exists (true) or `limit` passes (false).
pub fn wait_for_file(path: &str, limit: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if Path::new(path).exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

pub fn assert_fields(datagram: &str, expected: &[(&str, &str)]) {
    for (key, value) in expected {
        let line = format!("{key}={value}\n");
        assert!(datagram.contains(&line), "missing {line:?} in {datagram:?}");
    }
}

// ---- pacman, as the test's own tool ----------------------------------------------------------------------

/// pacman as root with the same fixed environment the helper uses.
pub fn pacman(args: &[&str]) -> Output {
    Command::new("/usr/bin/pacman")
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

pub fn pacman_stdout(args: &[&str]) -> String {
    String::from_utf8_lossy(&pacman(args).stdout).into_owned()
}

pub fn pacman_stderr(args: &[&str]) -> String {
    String::from_utf8_lossy(&pacman(args).stderr).into_owned()
}

/// `pacman -Q`: every installed package and version, as text (used to see that nothing changed).
pub fn package_list() -> String {
    pacman_stdout(&["-Q"])
}

/// The installed version of `name`, if it is installed.
pub fn installed_version(name: &str) -> Option<String> {
    let output = pacman(&["-Q", "--", name]);
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
}

/// The "Install Reason" line of `pacman -Qi`.
pub fn install_reason(name: &str) -> String {
    pacman_stdout(&["-Qi", "--", name])
        .lines()
        .find(|line| line.starts_with("Install Reason"))
        .map(|line| {
            line.split_once(':')
                .map(|(_, v)| v.trim().to_string())
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// (size, mode, mtime seconds, mtime nanoseconds) of everything under the folders where a helper could do harm.
/// Metadata only: no file is opened, no link is followed.
pub fn snapshot() -> BTreeMap<String, (u64, u32, i64, i64)> {
    fn walk(path: &Path, into: &mut BTreeMap<String, (u64, u32, i64, i64)>) {
        let Ok(meta) = fs::symlink_metadata(path) else {
            return;
        };
        into.insert(
            path.display().to_string(),
            (meta.size(), meta.mode(), meta.mtime(), meta.mtime_nsec()),
        );
        if meta.is_dir()
            && let Ok(entries) = fs::read_dir(path)
        {
            for entry in entries.flatten() {
                walk(&entry.path(), into);
            }
        }
    }
    let mut files = BTreeMap::new();
    for root in [
        "/etc",
        "/usr",
        "/var/lib/pacman",
        "/var/cache/pacman",
        "/var/log",
        "/root",
        "/opt",
        "/srv",
        "/home",
        "/boot",
        "/tmp",
    ] {
        walk(&PathBuf::from(root), &mut files);
    }
    files
}

/// The paths whose snapshot entry differs between the two (at most `limit`), for a readable failure.
pub fn differences(
    before: &BTreeMap<String, (u64, u32, i64, i64)>,
    after: &BTreeMap<String, (u64, u32, i64, i64)>,
) -> Vec<String> {
    let all: std::collections::BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    all.into_iter()
        .filter(|path| before.get(*path) != after.get(*path))
        .take(10)
        .cloned()
        .collect()
}

// ---- the container's world: reset, newer packages -------------------------------------------------------

// Not named tidewise-*: reset_world removes every /tmp/tidewise-* marker, and this backup must survive it.
const ORIGINAL_DB: &str = "/tmp/original-test-repo.db";
const LATER_FILES: [&str; 2] = [
    "tidewise-dummy-upd-2.0-1-any.pkg.tar.zst",
    "tidewise-dummy-new-1.0-1-any.pkg.tar.zst",
];

/// Only the local test repository: core and extra would need the network to sync, and the helper's update flow runs
/// `pacman -Sy`. (Test container only.)
pub fn local_repository_only() {
    let conf = fs::read_to_string("/etc/pacman.conf").unwrap();
    let mut kept = String::new();
    let mut skipping = false;
    for line in conf.lines() {
        if line.starts_with('[') {
            skipping = line == "[core]" || line == "[extra]";
        }
        if !skipping {
            kept.push_str(line);
            kept.push('\n');
        }
    }
    fs::write("/etc/pacman.conf", kept).unwrap();
}

/// Puts the container back the way the image left it, as far as the tests care: no stale lock, the original
/// repository database, no dummy packages installed, no markers, no work folder. Call it first in every job test.
pub fn reset_world() {
    let _ = fs::remove_file("/var/lib/pacman/db.lck");
    // tidewise-dummy-breaker's install script removes the database's version file (keeping a copy); put it back, or
    // every pacman command after it fails.
    let version_file = "/var/lib/pacman/local/ALPM_DB_VERSION";
    let version_backup = "/tmp/tidewise-db-version-backup";
    if !Path::new(version_file).exists() && Path::new(version_backup).exists() {
        let _ = fs::copy(version_backup, version_file);
    }
    local_repository_only();
    if !Path::new(ORIGINAL_DB).exists() {
        fs::copy(format!("{REPO_DIR}/tidewise-test.db.tar.gz"), ORIGINAL_DB).unwrap();
    }
    for file in LATER_FILES {
        let _ = fs::remove_file(format!("{REPO_DIR}/{file}"));
    }
    fs::copy(ORIGINAL_DB, format!("{REPO_DIR}/tidewise-test.db.tar.gz")).unwrap();
    // -yy: refresh even if the database looks unchanged (its time may be the same second as the last one).
    let refreshed = pacman(&["-Syy"]);
    assert!(
        refreshed.status.success(),
        "reset: pacman -Syy failed: {}{}",
        String::from_utf8_lossy(&refreshed.stdout),
        String::from_utf8_lossy(&refreshed.stderr)
    );

    let installed: Vec<String> = pacman_stdout(&["-Qq"])
        .lines()
        .filter(|name| name.starts_with("tidewise-dummy-"))
        .map(str::to_string)
        .collect();
    if !installed.is_empty() {
        let mut args = vec!["-Rdd", "--noconfirm", "--"];
        args.extend(installed.iter().map(String::as_str));
        let _ = pacman(&args);
    }

    if let Ok(entries) = fs::read_dir("/tmp") {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with("tidewise-") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    let _ = fs::remove_dir_all(WORK_DIR);
    let _ = fs::remove_file(WORK_DIR);
}

/// Makes a newer package (from `/srv/tidewise-later`, not in the repository database) appear in the repository: copy,
/// `repo-add`, refresh the package lists.
pub fn add_newer(file: &str) {
    fs::copy(format!("{LATER_DIR}/{file}"), format!("{REPO_DIR}/{file}")).unwrap();
    let status = Command::new("/usr/bin/repo-add")
        .arg(format!("{REPO_DIR}/tidewise-test.db.tar.gz"))
        .arg(format!("{REPO_DIR}/{file}"))
        .env_clear()
        .env("PATH", "/usr/bin")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "repo-add failed");
    assert!(pacman(&["-Syy"]).status.success(), "pacman -Syy failed");
}

/// Installs packages with plain pacman (the test's own setup, not the helper).
pub fn install_directly(names: &[&str]) {
    let mut args = vec!["-S", "--noconfirm", "--"];
    args.extend_from_slice(names);
    let output = pacman(&args);
    assert!(
        output.status.success(),
        "setup install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ---- requests the way the app builds them ---------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub repo: String,
    pub name: String,
    pub version: String,
}

/// What `pacman <operation> --print --print-format '%r/%n %v' -- <names>` lists: the confirmed list the app would
/// show. `operation` is `-S` or `-Su`; `names` are `repo/name` targets (none for a plain system update).
pub fn confirmed(operation: &str, names: &[&str]) -> Vec<Item> {
    let mut full = vec![operation, "--print", "--print-format", "%r/%n %v"];
    if !names.is_empty() {
        full.push("--");
        full.extend_from_slice(names);
    }
    let text = pacman_stdout(&full);
    text.lines()
        .map(|line| {
            let (spec, version) = line.split_once(' ').unwrap();
            let (repo, name) = spec.split_once('/').unwrap();
            Item {
                repo: repo.to_string(),
                name: name.to_string(),
                version: version.to_string(),
            }
        })
        .collect()
}

fn count_text(count: usize) -> String {
    if count == 1 {
        " (1 package)".to_string()
    } else {
        format!(" ({count} packages)")
    }
}

fn items_json(items: &[Item]) -> String {
    let parts: Vec<String> = items
        .iter()
        .map(|i| {
            format!(
                r#"{{"repo":"{}","name":"{}","version":"{}"}}"#,
                i.repo, i.name, i.version
            )
        })
        .collect();
    format!("[{}]", parts.join(","))
}

fn refs_json(refs: &[(&str, &str)]) -> String {
    let parts: Vec<String> = refs
        .iter()
        .map(|(repo, name)| format!(r#"{{"repo":"{repo}","name":"{name}"}}"#))
        .collect();
    format!("[{}]", parts.join(","))
}

fn providers_json(providers: &[(&str, &str, &str)]) -> String {
    let parts: Vec<String> = providers
        .iter()
        .map(|(dependency, repo, name)| {
            format!(
                r#"{{"dependency":"{dependency}","chosen":{{"repo":"{repo}","name":"{name}"}}}}"#
            )
        })
        .collect();
    format!("[{}]", parts.join(","))
}

fn words(
    verb: &str,
    targets: &[(&str, &str)],
    providers: &[(&str, &str, &str)],
    total: usize,
) -> String {
    let mut text = verb.to_string();
    for (repo, name) in targets {
        text.push_str(&format!(" {repo}/{name}"));
    }
    if !providers.is_empty() {
        text.push_str(" with");
        for (_, repo, name) in providers {
            text.push_str(&format!(" {repo}/{name}"));
        }
    }
    text.push_str(&count_text(total));
    text
}

/// (the JSON of an install request, the summary the helper must be given in argv).
pub fn install_request(
    targets: &[(&str, &str)],
    providers: &[(&str, &str, &str)],
    items: &[Item],
) -> (String, String) {
    let json = format!(
        r#"{{"protocol_version":1,"action":{{"install":{{"targets":{},"providers":{},"transaction":{}}}}}}}"#,
        refs_json(targets),
        providers_json(providers),
        items_json(items)
    );
    (json, words("install", targets, providers, items.len()))
}

pub fn update_request(items: &[Item]) -> (String, String) {
    let json = format!(
        r#"{{"protocol_version":1,"action":{{"update_system":{{"transaction":{}}}}}}}"#,
        items_json(items)
    );
    (json, format!("update system{}", count_text(items.len())))
}

pub fn update_and_install_request(
    targets: &[(&str, &str)],
    providers: &[(&str, &str, &str)],
    items: &[Item],
) -> (String, String) {
    let json = format!(
        r#"{{"protocol_version":1,"action":{{"update_system_and_install":{{"targets":{},"providers":{},"transaction":{}}}}}}}"#,
        refs_json(targets),
        providers_json(providers),
        items_json(items)
    );
    (
        json,
        words("update system and install", targets, providers, items.len()),
    )
}

/// The repository name pacman prints for the test repository.
pub const REPO: &str = "tidewise-test";

// ---- more ways to start the helper ----------------------------------------------------------------------

/// Starts a job without waiting for it: stdout is a pipe the caller may drop, and the helper may lead its own process
/// group (so a test can signal exactly the helper's group). Returns the child and the thread that feeds the request.
pub fn spawn_job(
    summary: &str,
    request: &str,
    own_group: bool,
) -> (Child, std::thread::JoinHandle<()>) {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new(HELPER);
    command
        .arg(summary)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if own_group {
        command.process_group(0);
    }
    let mut child = command.spawn().unwrap();
    let writer = feed(&mut child, request.as_bytes());
    (child, writer)
}

/// Runs the helper with the given umask (`"077"`, `"000"`). The standard library cannot set a child's umask without
/// `unsafe`, so this test-only helper goes through a shell; the product never does.
pub fn run_under_umask(mask: &str, summary: &str, stdin: &[u8]) -> Ran {
    let mut child = Command::new("/usr/bin/bash")
        .args([
            "-c",
            "umask \"$1\" && exec \"$2\" \"$3\"",
            "bash",
            mask,
            HELPER,
            summary,
        ])
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let writer = feed(&mut child, stdin);
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    Ran {
        status: output.status,
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

/// The permission bits of `path` (not following a symlink).
pub fn mode_of(path: &str) -> u32 {
    fs::symlink_metadata(path).unwrap().mode() & 0o7777
}

pub fn pacman_log() -> String {
    fs::read_to_string("/var/log/pacman.log").unwrap_or_default()
}
