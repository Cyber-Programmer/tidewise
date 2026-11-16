//! The hostile-input table, through the real `pkexec`, to the helper the built package installed (WBS 5.6; threats
//! T-01, T-02, T-04 to T-08, T-14, T-15; golden rules 1, 2, 3, 6, 10). Real polkitd, real pkexec, as root, in the
//! package image only (`tests/container/run.sh package`).
//!
//! **Read this first: the caller here is root.** The container's tests run as root, and pkexec run by root does no
//! setuid transition, so the kernel's AT_SECURE flag is off and pkexec's *own* dynamic loader honours `LD_PRELOAD` and
//! `LD_LIBRARY_PATH`. So these tests prove **what reaches the helper** (its environment, its file descriptors, its
//! argv, its stdin) and how the helper answers. They do **not** prove pkexec's own protection when it is started by an
//! ordinary user (setuid, AT_SECURE on, the loader ignoring `LD_*`). That, and "a valid request from another uid",
//! belong to the VM checklist (WBS 6.8). Also not shown: which polkit action pkexec picked (root is authorized
//! whatever the action), and the password dialog (the hand test of threat model question 21).
//!
//! One test, one table. Every row is: how pkexec is called (the program name as typed, argv, extra environment, an
//! extra open file) and what is on stdin; and what must come back, one of three kinds:
//!  - `Refused`: the helper ran its gate and said no. Exit 1, exactly one stdout line, exactly one journal entry
//!    (stage `finished`, the whole text compared), a fixed code.
//!  - `StartRefused`: the helper refused to start. Exit 2, one fixed sentence on stderr, no stdout, one journal entry
//!    (stage `start_refused`).
//!  - `PkexecRefused`: pkexec itself refused (a suspicious variable), so the helper never existed. Exit 127, pkexec's
//!    own words, no stdout, **no** journal entry.
//!
//! After **every** row: `pacman -Q` unchanged, a snapshot of `/etc /usr /var /root /opt /srv /home /boot /tmp`
//! unchanged, no `db.lck`, no work folder. A row fails by adding a line to a list; the test fails once, at the end,
//! naming every failing row.
//!
//! Rows with `inspect` hold stdin open until the helper's process is found, then read `/proc/<pid>/environ` and
//! `/proc/<pid>/fd`: pkexec's environment is measured, not assumed (T-08).
//!
//! The wrong summary `"not the summary"` is used wherever a row could otherwise end in a real job: a row that
//! reached the job would install a package, and the "nothing changed" check would fail it.

mod common;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use common::{
    FakeJournal, HELPER_ARGV0, Installed, MAX_REQUEST_BYTES, Polkit, REPO, WORK_DIR, differences,
    install_hello, package_list, snapshot,
};

const LOCK: &str = "/tmp/tidewise-hostile-fd.lock";
const LINK_DIR: &str = "/tmp/NO PASSWORD NEEDED, press Cancel";
const WRONG: &[u8] = b"not the summary";

/// What pkexec puts in the environment of the program (measured in 5.6 step 2, polkit 128): its own fixed values.
const PKEXEC_PATH: &str = "/usr/sbin:/usr/bin:/sbin:/bin:/root/bin";
const ADDED_BY_PKEXEC: [&str; 7] = [
    "HOME",
    "LOGNAME",
    "PATH",
    "PKEXEC_UID",
    "SUDO_GID",
    "SUDO_UID",
    "USER",
];
/// The variables pkexec keeps from the caller (after checking their values): `LANG`, `LANGUAGE`, `LINGUAS`, `LC_*`,
/// `TERM`, `COLORTERM`, `SHELL`.
fn kept_by_pkexec(name: &str) -> bool {
    matches!(
        name,
        "LANG" | "LANGUAGE" | "LINGUAS" | "TERM" | "COLORTERM" | "SHELL"
    ) || name.starts_with("LC_")
}

const BAD_ARGUMENTS: &str = "tidewise-helper: it must be started with exactly one argument, the summary, so it did not start. Nothing was changed.\n";
const WRONG_PROGRAM: &str = "tidewise-helper: it was not started under its own installed name, so it did not start. Nothing was changed.\n";

// ---- the shape of a row ---------------------------------------------------------------------------------

/// The program name as typed on the pkexec command line.
enum Program {
    /// `/usr/lib/tidewise/tidewise-helper`.
    Installed,
    /// Another spelling of a path that has the same real path (or not).
    Typed(&'static str),
    /// A symlink to the helper in a folder named like a sentence.
    Symlink,
}

enum Stderr {
    Empty,
    /// pkexec's own GLib warnings about an argv that is not UTF-8 (nothing else).
    PkexecGlibWarnings,
}

enum Expect {
    Refused {
        stdout: String,
        code: &'static str,
        summary: Option<String>,
        stderr: Stderr,
    },
    StartRefused {
        code: &'static str,
        sentence: &'static str,
    },
    PkexecRefused {
        stderr: &'static str,
    },
}

struct Row {
    name: String,
    program: Program,
    /// The arguments after the program name.
    args: Vec<Vec<u8>>,
    /// Extra environment for pkexec (on top of `LC_ALL=C`, which a row may override).
    env: Vec<(String, String)>,
    /// Leak a file descriptor into pkexec (through `flock(1)`, no shell).
    extra_fd: bool,
    /// Hold stdin open, find the helper, read its environment and descriptors.
    inspect: bool,
    stdin: Vec<u8>,
    /// Hostile text that must not appear in stdout, stderr or the journal entry.
    needle: Option<Vec<u8>>,
    expect: Expect,
}

/// What every row starts from: one valid install request of a dummy package and the summary it must travel with.
struct World {
    request: String,
    summary: String,
}

fn json(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

impl World {
    /// The request with the first target's name replaced.
    fn with_name(&self, name: &str) -> String {
        let from = r#""name":"tidewise-dummy-hello""#;
        assert!(self.request.contains(from), "{}", self.request);
        self.request
            .replacen(from, &format!(r#""name":"{}""#, json(name)), 1)
    }

    /// The request with the first target's repository replaced.
    fn with_repo(&self, repo: &str) -> String {
        let from = format!(r#""repo":"{REPO}""#);
        assert!(self.request.contains(&from), "{}", self.request);
        self.request
            .replacen(&from, &format!(r#""repo":"{}""#, json(repo)), 1)
    }

    /// The request with the first confirmed line's version replaced.
    fn with_version(&self, version: &str) -> String {
        let key = r#""version":""#;
        let start = self.request.find(key).expect("no version in the request") + key.len();
        let end = start
            + self.request[start..]
                .find('"')
                .expect("unterminated version");
        format!(
            "{}{}{}",
            &self.request[..start],
            json(version),
            &self.request[end..]
        )
    }
}

impl Row {
    /// A row that sends the valid request with the true summary; change what the row is about with the methods below.
    fn new(world: &World, name: impl Into<String>, expect: Expect) -> Row {
        Row {
            name: name.into(),
            program: Program::Installed,
            args: vec![world.summary.clone().into_bytes()],
            env: Vec::new(),
            extra_fd: false,
            inspect: false,
            stdin: world.request.clone().into_bytes(),
            needle: None,
            expect,
        }
    }

    fn stdin(mut self, bytes: impl Into<Vec<u8>>) -> Row {
        self.stdin = bytes.into();
        self
    }

    fn args(mut self, args: &[&[u8]]) -> Row {
        self.args = args.iter().map(|a| a.to_vec()).collect();
        self
    }

    fn program(mut self, program: Program) -> Row {
        self.program = program;
        self
    }

    fn env(mut self, key: &str, value: &str) -> Row {
        self.env.push((key.to_string(), value.to_string()));
        self
    }

    fn inspect(mut self) -> Row {
        self.inspect = true;
        self
    }

    fn extra_fd(mut self) -> Row {
        self.extra_fd = true;
        self
    }

    /// Remember hostile text that must not be repeated anywhere (only text long enough to be distinctive).
    fn needle(mut self, text: &[u8]) -> Row {
        if (4..=200).contains(&text.len()) {
            self.needle = Some(text.to_vec());
        }
        self
    }

    /// A write to a helper that exits before it reads everything is allowed to fail with a broken pipe: when the
    /// request is bigger than the pipe, or when the row ends before the helper reads stdin at all.
    fn may_break_the_pipe(&self) -> bool {
        self.stdin.len() > 60_000 || !matches!(self.expect, Expect::Refused { .. })
    }
}

fn plain(code: &'static str) -> Expect {
    Expect::Refused {
        stdout: format!(r#"{{"finished":{{"error":"{code}"}}}}"#),
        code,
        summary: None,
        stderr: Stderr::Empty,
    }
}

fn invalid(field: &str, reason: &str) -> Expect {
    Expect::Refused {
        stdout: format!(
            r#"{{"finished":{{"error":{{"invalid":{{"field":"{field}","index":0,"reason":"{reason}"}}}}}}}}"#
        ),
        code: "invalid",
        summary: None,
        stderr: Stderr::Empty,
    }
}

fn mismatch(world: &World, stderr: Stderr) -> Expect {
    Expect::Refused {
        stdout: r#"{"finished":{"error":"summary_mismatch"}}"#.to_string(),
        code: "summary_mismatch",
        summary: Some(world.summary.clone()),
        stderr,
    }
}

fn pkexec_refused(stderr: &'static str) -> Expect {
    Expect::PkexecRefused { stderr }
}

// ---- the table ------------------------------------------------------------------------------------------

fn table(w: &World) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    let request = w.request.as_str();

    // -- hostile package names (ENGINEERING 2: --force, -dd, --root, ../, *, absolute paths, NUL, look-alikes,
    //    empty, names starting with -, one huge name)
    let long = "a".repeat(1_000_000);
    let names: Vec<(&str, &str)> = vec![
        ("force flag", "--force"),
        ("nodeps flag", "-dd"),
        ("root option", "--root=/x"),
        ("double dash", "--"),
        ("single dash", "-"),
        ("leading dash", "-x"),
        ("leading dot", ".x"),
        ("parent directory", "../x"),
        ("absolute path", "/etc/passwd"),
        ("star", "*"),
        ("local package file", "/tmp/a.pkg.tar.zst"),
        ("file url", "file:///x"),
        ("http url", "http://x"),
        ("NUL", "a\u{0}b"),
        ("newline", "a\nb"),
        ("space", "a b"),
        ("semicolon", "a;b"),
        ("command substitution", "$(x)"),
        ("backtick", "`x`"),
        ("escape sequence", "\u{1b}[31mx"),
        ("right-to-left override", "\u{202e}x"),
        ("Cyrillic look-alike", "\u{430}pp"),
        ("full-width letters", "\u{ff41}\u{ff50}\u{ff50}"),
        ("accent", "caf\u{e9}"),
        ("empty", ""),
        ("one million bytes", long.as_str()),
    ];
    for (label, name) in names {
        rows.push(
            Row::new(
                w,
                format!("name: {label}"),
                invalid("targets", "bad_package_name"),
            )
            .stdin(w.with_name(name))
            .needle(name.as_bytes()),
        );
    }

    // -- hostile repository names and versions
    for (label, repo) in [
        ("local database", "local"),
        ("slash", "a/b"),
        ("option", "--root=/x"),
        ("Cyrillic look-alike", "extr\u{430}"),
        ("empty", ""),
    ] {
        rows.push(
            Row::new(
                w,
                format!("repo: {label}"),
                invalid("targets", "bad_repo_name"),
            )
            .stdin(w.with_repo(repo))
            .needle(repo.as_bytes()),
        );
    }
    for (label, version) in [("option", "--force"), ("semicolon", "1;rm")] {
        rows.push(
            Row::new(
                w,
                format!("version: {label}"),
                invalid("transaction", "bad_version"),
            )
            .stdin(w.with_version(version))
            .needle(version.as_bytes()),
        );
    }

    // -- the shape of the request: lists, JSON, size, version, fields, actions
    let many: Vec<String> = (0..10_000)
        .map(|n| format!(r#"{{"repo":"{REPO}","name":"p{n}"}}"#))
        .collect();
    let ten_thousand = format!(
        r#"{{"protocol_version":1,"action":{{"install":{{"targets":[{}],"providers":[],"transaction":[{{"repo":"{REPO}","name":"p0","version":"1-1"}}]}}}}}}"#,
        many.join(",")
    );
    rows.push(
        Row::new(w, "list: 10 000 targets", invalid("targets", "too_many")).stdin(ten_thousand),
    );

    let deep = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
    for (label, bytes) in [
        ("only brackets", deep.clone()),
        (
            "as the action",
            format!(r#"{{"protocol_version":1,"action":{deep}}}"#),
        ),
        (
            "as the targets",
            format!(r#"{{"protocol_version":1,"action":{{"install":{{"targets":{deep}}}}}}}"#),
        ),
    ] {
        rows.push(
            Row::new(
                w,
                format!("json: 10 000 levels, {label}"),
                plain("malformed"),
            )
            .stdin(bytes),
        );
    }

    let mut over_cap = request.as_bytes().to_vec();
    over_cap.resize(MAX_REQUEST_BYTES + 1, b' ');
    rows.push(
        Row::new(
            w,
            "size: a valid request padded to 2 MiB + 1",
            plain("too_large"),
        )
        .stdin(over_cap),
    );
    rows.push(
        Row::new(w, "size: 2 MiB + 1 of spaces", plain("too_large")).stdin(vec![
            b' ';
            MAX_REQUEST_BYTES
                + 1
        ]),
    );
    rows.push(
        Row::new(w, "size: 8 MiB of zeros", plain("too_large")).stdin(vec![0u8; 8 * 1024 * 1024]),
    );

    let broken: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("only whitespace", b"   \n".to_vec()),
        ("open brace", b"{".to_vec()),
        ("not UTF-8", vec![0xff, 0xfe, 0x00]),
        ("trailing letter", format!("{request}x").into_bytes()),
        ("trailing NUL", format!("{request}\0").into_bytes()),
        ("two requests", format!("{request}{request}").into_bytes()),
        ("two lines", format!("{request}\n{request}").into_bytes()),
        ("comment first", format!("/* x */{request}").into_bytes()),
        (
            "byte order mark",
            [&[0xef, 0xbb, 0xbf][..], request.as_bytes()].concat(),
        ),
        (
            "repeated key",
            request
                .replacen(
                    r#""protocol_version":1"#,
                    r#""protocol_version":1,"protocol_version":1"#,
                    1,
                )
                .into_bytes(),
        ),
        (
            "unknown field at the top",
            request
                .replacen(
                    r#""protocol_version":1"#,
                    r#""protocol_version":1,"noconfirm":true"#,
                    1,
                )
                .into_bytes(),
        ),
        (
            "unknown field in the action",
            request
                .replacen(r#""providers":[]"#, r#""providers":[],"overwrite":"*""#, 1)
                .into_bytes(),
        ),
        (
            "an action that does not exist",
            br#"{"protocol_version":1,"action":{"run":{"command":"id"}}}"#.to_vec(),
        ),
        (
            "a removal",
            br#"{"protocol_version":1,"action":{"remove":{"transaction":[]}}}"#.to_vec(),
        ),
        (
            "a version that is a string",
            request
                .replacen(r#""protocol_version":1"#, r#""protocol_version":"1""#, 1)
                .into_bytes(),
        ),
        (
            "version -1",
            request
                .replacen(r#""protocol_version":1"#, r#""protocol_version":-1"#, 1)
                .into_bytes(),
        ),
    ];
    for (label, bytes) in broken {
        rows.push(Row::new(w, format!("json: {label}"), plain("malformed")).stdin(bytes));
    }
    for version in ["0", "2", "4294967295"] {
        rows.push(
            Row::new(
                w,
                format!("version: protocol {version}"),
                plain("version_mismatch"),
            )
            .stdin(request.replacen(
                r#""protocol_version":1"#,
                &format!(r#""protocol_version":{version}"#),
                1,
            )),
        );
    }

    // -- the summary in argv: what the dialog shows. The request is valid; the text is hostile.
    let twenty_kb = "x".repeat(20_000);
    let two_hundred_lines: String = (1..=200).map(|n| format!("line {n}\n")).collect();
    let with_space = format!("{} ", w.summary);
    let shouting = w.summary.to_uppercase();
    let argv_texts: Vec<(&str, Vec<u8>)> = vec![
        (
            "bold markup",
            b"<b>bold</b> &amp; <i>no password needed</i>".to_vec(),
        ),
        (
            "link markup",
            br#"<a href="https://example.org">click here</a>"#.to_vec(),
        ),
        ("four lines", b"one\ntwo\nthree\nfour".to_vec()),
        ("two hundred lines", two_hundred_lines.into_bytes()),
        ("20 KB", twenty_kb.into_bytes()),
        (
            "polkit substitutions",
            b"$(user) $(program) $(command_line)".to_vec(),
        ),
        (
            "right-to-left override",
            "install extra/firefox \u{202e}gnp.exe\u{202c} (1 package)"
                .as_bytes()
                .to_vec(),
        ),
        ("not UTF-8", b"bad \xff\xfe end".to_vec()),
        ("empty", Vec::new()),
        ("the true text and a space", with_space.into_bytes()),
        ("the true text in capitals", shouting.into_bytes()),
    ];
    for (label, text) in argv_texts {
        let stderr = if std::str::from_utf8(&text).is_ok() {
            Stderr::Empty
        } else {
            Stderr::PkexecGlibWarnings
        };
        rows.push(
            Row::new(w, format!("argv: {label}"), mismatch(w, stderr))
                .args(&[&text])
                .needle(&text),
        );
    }

    // -- how the helper is started: the number of arguments, the name it is given
    rows.push(
        Row::new(
            w,
            "start: no argument",
            Expect::StartRefused {
                code: "bad_arguments",
                sentence: BAD_ARGUMENTS,
            },
        )
        .args(&[]),
    );
    let summary = w.summary.clone();
    rows.push(
        Row::new(
            w,
            "start: two arguments",
            Expect::StartRefused {
                code: "bad_arguments",
                sentence: BAD_ARGUMENTS,
            },
        )
        .args(&[summary.as_bytes(), b"extra"]),
    );
    rows.push(
        Row::new(
            w,
            "start: --help and the summary",
            Expect::StartRefused {
                code: "bad_arguments",
                sentence: BAD_ARGUMENTS,
            },
        )
        .args(&[b"--help", summary.as_bytes()]),
    );
    for (label, typed) in [
        ("dot in the path", "/usr/lib/tidewise/./tidewise-helper"),
        ("double slash", "/usr/lib//tidewise/tidewise-helper"),
        ("dot dot", "/usr/lib/tidewise/../tidewise/tidewise-helper"),
        ("through /lib", "/lib/tidewise/tidewise-helper"),
    ] {
        rows.push(
            Row::new(
                w,
                format!("start: program typed with {label}"),
                Expect::StartRefused {
                    code: "wrong_program_path",
                    sentence: WRONG_PROGRAM,
                },
            )
            .program(Program::Typed(typed)),
        );
    }
    rows.push(
        Row::new(
            w,
            "start: through a symlink named like a sentence",
            Expect::StartRefused {
                code: "wrong_program_path",
                sentence: WRONG_PROGRAM,
            },
        )
        .program(Program::Symlink),
    );

    // -- the environment the helper is given by pkexec (the caller is root: see the header). Each row ends in a
    //    summary mismatch, which shows the start checks passed.
    let env_rows: Vec<(&str, Vec<(&str, &str)>)> = vec![
        ("LD_PRELOAD", vec![("LD_PRELOAD", "/usr/lib/libc.so.6")]),
        (
            "LD_LIBRARY_PATH",
            vec![("LD_LIBRARY_PATH", "/tmp/tidewise-evil-libs")],
        ),
        ("PATH", vec![("PATH", "/tmp/evil")]),
        ("IFS", vec![("IFS", "x")]),
        ("HOME", vec![("HOME", "/tmp/evil")]),
        ("PACMAN_ROOT", vec![("PACMAN_ROOT", "/tmp/evil")]),
        ("a variable pkexec does not know", vec![("FOO", "bar")]),
        ("LANG", vec![("LANG", "tr_TR.UTF-8")]),
        ("LC_ALL", vec![("LC_ALL", "tr_TR.UTF-8")]),
        (
            "TERM, COLORTERM and SHELL",
            vec![
                ("TERM", "xterm"),
                ("COLORTERM", "truecolor"),
                ("SHELL", "/bin/bash"),
            ],
        ),
        ("PKEXEC_UID forged as 77", vec![("PKEXEC_UID", "77")]),
        (
            "PKEXEC_UID with a new line",
            vec![("PKEXEC_UID", "0\nMESSAGE=pwned")],
        ),
        (
            "all of them at once",
            vec![
                ("LD_PRELOAD", "/usr/lib/libc.so.6"),
                ("LD_LIBRARY_PATH", "/tmp/tidewise-evil-libs"),
                ("PATH", "/tmp/evil"),
                ("IFS", "x"),
                ("HOME", "/tmp/evil"),
                ("PACMAN_ROOT", "/tmp/evil"),
                ("LANG", "tr_TR.UTF-8"),
                ("LC_MESSAGES", "tr_TR.UTF-8"),
                ("PKEXEC_UID", "77"),
            ],
        ),
    ];
    for (label, vars) in env_rows {
        let mut row = Row::new(w, format!("env: {label}"), mismatch(w, Stderr::Empty))
            .args(&[WRONG])
            .inspect();
        for (key, value) in vars {
            row = row.env(key, value);
        }
        rows.push(row);
    }

    // -- a file descriptor leaked into pkexec: pkexec closes it, the helper never sees it
    rows.push(
        Row::new(
            w,
            "fd: an open file leaked into pkexec",
            invalid("targets", "bad_package_name"),
        )
        .stdin(w.with_name("--force"))
        .extra_fd()
        .inspect(),
    );

    // -- pkexec refuses (it validates what it keeps): the helper never starts
    for (label, key, value, text) in [
        (
            "LC_ALL with ../",
            "LC_ALL",
            "../x",
            "The value for environment variable LC_ALL contains suspicious content\n\nThis incident has been reported.\n",
        ),
        (
            "LC_ALL with ..",
            "LC_ALL",
            "a..b",
            "The value for environment variable LC_ALL contains suspicious content\n\nThis incident has been reported.\n",
        ),
        (
            "LANG with a slash",
            "LANG",
            "a/b",
            "The value for environment variable LANG contains suspicious content\n\nThis incident has been reported.\n",
        ),
        (
            "LANGUAGE with ../",
            "LANGUAGE",
            "../x",
            "The value for environment variable LANGUAGE contains suspicious content\n\nThis incident has been reported.\n",
        ),
        (
            "TERM with %n",
            "TERM",
            "%n",
            "The value for environment variable TERM contains suspicious content\n\nThis incident has been reported.\n",
        ),
        (
            "COLORTERM with %s",
            "COLORTERM",
            "%s",
            "The value for environment variable COLORTERM contains suspicious content\n\nThis incident has been reported.\n",
        ),
        (
            "SHELL that is not in /etc/shells",
            "SHELL",
            "/tmp/evil",
            "The value for the SHELL variable was not found in the /etc/shells file\n\nThis incident has been reported.\n",
        ),
    ] {
        rows.push(
            Row::new(w, format!("pkexec refuses: {label}"), pkexec_refused(text))
                .args(&[WRONG])
                .env(key, value),
        );
    }

    rows
}

// ---- running one row ------------------------------------------------------------------------------------

struct Seen {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    entries: Vec<String>,
    write: io::Result<()>,
    /// Found only for `inspect` rows.
    helper_env: Option<BTreeMap<String, String>>,
    helper_fds: Option<Vec<String>>,
    could_not_find_the_helper: bool,
}

fn find_helper(limit: Duration) -> Option<u32> {
    let start = Instant::now();
    while start.elapsed() < limit {
        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                let Some(pid) = entry
                    .file_name()
                    .to_str()
                    .and_then(|n| n.parse::<u32>().ok())
                else {
                    continue;
                };
                if fs::read_link(format!("/proc/{pid}/exe"))
                    .is_ok_and(|exe| exe == Path::new(HELPER_ARGV0))
                {
                    return Some(pid);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    None
}

fn read_environ(pid: u32) -> Option<BTreeMap<String, String>> {
    let raw = fs::read(format!("/proc/{pid}/environ")).ok()?;
    Some(
        raw.split(|b| *b == 0)
            .filter(|part| !part.is_empty())
            .map(|part| {
                let text = String::from_utf8_lossy(part).into_owned();
                match text.split_once('=') {
                    Some((key, value)) => (key.to_string(), value.to_string()),
                    None => (text, String::new()),
                }
            })
            .collect(),
    )
}

/// The helper's open descriptors, except the one it opens itself to list them (its start check reads
/// `/proc/self/fd`; that descriptor points at `/proc/<pid>/fd` and is allowed, see `fds.rs`).
fn read_fds(pid: u32) -> Option<Vec<String>> {
    let own_listing = format!("/proc/{pid}/fd");
    let mut names: Vec<String> = fs::read_dir(format!("/proc/{pid}/fd"))
        .ok()?
        .flatten()
        .filter(|entry| {
            fs::read_link(entry.path()).is_ok_and(|target| target != Path::new(&own_listing))
        })
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    Some(names)
}

/// The helper's environment and descriptors once they have stopped changing. Right after `exec` the kernel has not yet
/// filled in `environ` and has not yet closed pkexec's close-on-exec descriptors, so a first look can show an empty
/// environment or a descriptor that is about to go. The helper then waits for stdin (we hold it open), so two looks
/// 30 ms apart that agree, with a non-empty environment, are the settled state.
fn settled_helper(limit: Duration) -> Option<(BTreeMap<String, String>, Vec<String>)> {
    let start = Instant::now();
    let pid = find_helper(limit)?;
    let mut previous: Option<(BTreeMap<String, String>, Vec<String>)> = None;
    while start.elapsed() < limit {
        let now = read_environ(pid).zip(read_fds(pid));
        if let Some(now) = now
            && !now.0.is_empty()
        {
            if previous.as_ref() == Some(&now) {
                return Some(now);
            }
            previous = Some(now);
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    None
}

fn run_row(row: &Row) -> Seen {
    let journal = FakeJournal::start();
    let program: OsString = match row.program {
        Program::Installed => HELPER_ARGV0.into(),
        Program::Typed(typed) => typed.into(),
        Program::Symlink => format!("{LINK_DIR}/tidewise-helper").into(),
    };
    // No shell anywhere: flock(1) and timeout(1) run the next program by its absolute path. The test binary is
    // process 1 of the container and pkexec refuses such a parent, hence timeout (as in `common::pkexec`).
    let mut command = if row.extra_fd {
        let mut command = Command::new("/usr/sbin/flock");
        command.args([LOCK, "/usr/bin/timeout", "60", "/usr/bin/pkexec"]);
        command
    } else {
        let mut command = Command::new("/usr/bin/timeout");
        command.args(["60", "/usr/bin/pkexec"]);
        command
    };
    command.arg(&program);
    for arg in &row.args {
        command.arg(OsString::from_vec(arg.clone()));
    }
    command.env_clear().env("LC_ALL", "C");
    for (key, value) in &row.env {
        command.env(key, value);
    }
    let mut child = command
        .current_dir("/")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("could not start pkexec");
    let mut stdin = child.stdin.take().expect("no stdin pipe");

    // Look at the helper while it waits for its request.
    let (mut helper_env, mut helper_fds, mut missing) = (None, None, false);
    if row.inspect {
        match settled_helper(Duration::from_secs(10)) {
            Some((env, fds)) => {
                helper_env = Some(env);
                helper_fds = Some(fds);
            }
            None => missing = true,
        }
    }

    let data = row.stdin.clone();
    let writer = std::thread::spawn(move || stdin.write_all(&data));
    let output = child.wait_with_output().expect("could not wait for pkexec");
    let write = writer.join().expect("the writer thread panicked");
    Seen {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        entries: journal.datagrams(),
        write,
        helper_env,
        helper_fds,
        could_not_find_the_helper: missing,
    }
}

// ---- judging one row ------------------------------------------------------------------------------------

/// The whole journal entry the helper writes for a refusal by a root caller (`PKEXEC_UID` is 0 here).
fn entry(stage: &str, code: &str, summary: Option<&str>) -> String {
    let mut text = format!(
        "SYSLOG_IDENTIFIER=tidewise-helper\nPRIORITY=4\nTIDEWISE_CALLER_UID=0\nTIDEWISE_STAGE={stage}\nTIDEWISE_OUTCOME={code}\n"
    );
    match (stage, summary) {
        (_, Some(summary)) => {
            text.push_str(&format!(
                "TIDEWISE_SUMMARY={summary}\nMESSAGE=uid 0: {code}: {summary}\n"
            ));
        }
        ("start_refused", None) => {
            text.push_str(&format!("MESSAGE=uid 0: refused to start: {code}\n"))
        }
        (_, None) => text.push_str(&format!("MESSAGE=uid 0: {code}\n")),
    }
    text
}

fn check(problems: &mut Vec<String>, ok: bool, what: String) {
    if !ok {
        problems.push(what);
    }
}

fn judge(row: &Row, seen: &Seen) -> Vec<String> {
    let mut problems = Vec::new();

    match &row.expect {
        Expect::Refused {
            stdout,
            code,
            summary,
            stderr,
        } => {
            check(
                &mut problems,
                seen.code == Some(1),
                format!("exit {:?}, wanted 1", seen.code),
            );
            check(
                &mut problems,
                seen.stdout == format!("{stdout}\n"),
                format!("stdout {:?}, wanted {stdout:?}", seen.stdout),
            );
            match stderr {
                Stderr::Empty => check(
                    &mut problems,
                    seen.stderr.is_empty(),
                    format!("stderr {:?}, wanted nothing", seen.stderr),
                ),
                Stderr::PkexecGlibWarnings => check(
                    &mut problems,
                    seen.stderr.lines().filter(|l| !l.is_empty()).all(|line| {
                        line.contains("GLib-CRITICAL") && line.contains("requires valid UTF-8")
                    }),
                    format!(
                        "stderr is not only pkexec's GLib warnings: {:?}",
                        seen.stderr
                    ),
                ),
            }
            let wanted = entry("finished", code, summary.as_deref());
            check(
                &mut problems,
                seen.entries.len() == 1 && seen.entries[0] == wanted,
                format!(
                    "journal {:?}, wanted exactly one entry {wanted:?}",
                    seen.entries
                ),
            );
        }
        Expect::StartRefused { code, sentence } => {
            check(
                &mut problems,
                seen.code == Some(2),
                format!("exit {:?}, wanted 2", seen.code),
            );
            check(
                &mut problems,
                seen.stdout.is_empty(),
                format!("stdout {:?}", seen.stdout),
            );
            check(
                &mut problems,
                seen.stderr == *sentence,
                format!("stderr {:?}, wanted {sentence:?}", seen.stderr),
            );
            let wanted = entry("start_refused", code, None);
            check(
                &mut problems,
                seen.entries.len() == 1 && seen.entries[0] == wanted,
                format!(
                    "journal {:?}, wanted exactly one entry {wanted:?}",
                    seen.entries
                ),
            );
        }
        Expect::PkexecRefused { stderr } => {
            check(
                &mut problems,
                seen.code == Some(127),
                format!("exit {:?}, wanted 127", seen.code),
            );
            check(
                &mut problems,
                seen.stdout.is_empty(),
                format!("stdout {:?}", seen.stdout),
            );
            check(
                &mut problems,
                seen.stderr == *stderr,
                format!("stderr {:?}, wanted {stderr:?}", seen.stderr),
            );
            check(
                &mut problems,
                seen.entries.is_empty(),
                format!(
                    "the helper must never have started, but the journal has {:?}",
                    seen.entries
                ),
            );
        }
    }

    // The request was written, or the helper ended first (a broken pipe), when that is how this row ends.
    if let Err(error) = &seen.write {
        check(
            &mut problems,
            error.kind() == io::ErrorKind::BrokenPipe && row.may_break_the_pipe(),
            format!("writing the request failed: {error}"),
        );
    }

    // Hostile text is repeated nowhere.
    if let Some(needle) = &row.needle {
        let needle = String::from_utf8_lossy(needle).into_owned();
        let everywhere = [&seen.stdout, &seen.stderr]
            .into_iter()
            .chain(seen.entries.iter());
        for text in everywhere {
            check(
                &mut problems,
                !text.contains(&needle),
                format!("the hostile text {needle:?} was repeated in {text:?}"),
            );
        }
    }

    // What reached the helper (inspect rows).
    if row.inspect {
        if seen.could_not_find_the_helper {
            problems.push("the helper's process was not found".to_string());
        }
        if let Some(env) = &seen.helper_env {
            problems.extend(judge_environment(row, env));
        } else if !seen.could_not_find_the_helper {
            problems.push("the helper's environment could not be read".to_string());
        }
        if let Some(fds) = &seen.helper_fds {
            let extra: Vec<&String> = fds
                .iter()
                .filter(|fd| !["0", "1", "2"].contains(&fd.as_str()))
                .collect();
            if !extra.is_empty() {
                problems.push(format!(
                    "the helper had open descriptors besides 0, 1, 2: {extra:?}"
                ));
            }
        } else if !seen.could_not_find_the_helper {
            problems.push("the helper's descriptors could not be read".to_string());
        }
    }
    problems
}

/// pkexec builds the environment: its own fixed values, plus the few variables it keeps from the caller.
fn judge_environment(row: &Row, env: &BTreeMap<String, String>) -> Vec<String> {
    let mut given: BTreeMap<&str, &str> = BTreeMap::from([("LC_ALL", "C")]);
    for (key, value) in &row.env {
        given.insert(key, value);
    }
    let mut problems = Vec::new();
    for (key, value) in env {
        if ADDED_BY_PKEXEC.contains(&key.as_str()) {
            continue;
        }
        match given.get(key.as_str()) {
            Some(sent) if kept_by_pkexec(key) => {
                if sent != value {
                    problems.push(format!(
                        "{key} changed on the way: {sent:?} became {value:?}"
                    ));
                }
            }
            _ => problems.push(format!(
                "{key}={value:?} reached the helper and should not have"
            )),
        }
    }
    for (key, wanted) in [
        ("PATH", PKEXEC_PATH),
        ("HOME", "/root"),
        ("USER", "root"),
        ("PKEXEC_UID", "0"),
    ] {
        if env.get(key).map(String::as_str) != Some(wanted) {
            problems.push(format!(
                "{key} is {:?}, wanted pkexec's {wanted:?}",
                env.get(key)
            ));
        }
    }
    for (key, sent) in given {
        if kept_by_pkexec(key) && env.get(key).map(String::as_str) != Some(sent) {
            problems.push(format!(
                "{key} should have been kept as {sent:?}, is {:?}",
                env.get(key)
            ));
        }
    }
    problems
}

// ---- the test -------------------------------------------------------------------------------------------

#[test]
#[ignore = "runs real pacman and pkexec as root: only inside the package image (tests/container/run.sh package)"]
fn package_hostile_every_row_is_refused_and_changes_nothing() {
    let _installed = Installed::new();
    let _polkit = Polkit::start_installed();
    let world = {
        let (request, summary) = install_hello();
        World { request, summary }
    };

    // Fixtures, made before the first snapshot so they are part of it.
    fs::create_dir_all(LINK_DIR).unwrap();
    let link = format!("{LINK_DIR}/tidewise-helper");
    let _ = fs::remove_file(&link);
    symlink(HELPER_ARGV0, &link).unwrap();
    fs::write(LOCK, "").unwrap();

    let rows = table(&world);
    let mut names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
    names.sort_unstable();
    let total = names.len();
    names.dedup();
    assert_eq!(names.len(), total, "two rows have the same name");
    assert!(total >= 90, "the table looks short: {total} rows");

    let packages_before = package_list();
    let files_before = snapshot();
    assert!(files_before.len() > 1000, "the snapshot looks empty");

    let mut failures: Vec<String> = Vec::new();
    for row in &rows {
        let seen = run_row(row);
        let mut problems = judge(row, &seen);

        // Nothing changed, row by row.
        if package_list() != packages_before {
            problems.push("the installed packages changed".to_string());
        }
        let changed = differences(&files_before, &snapshot());
        if !changed.is_empty() {
            problems.push(format!("these paths changed: {changed:?}"));
        }
        if Path::new("/var/lib/pacman/db.lck").exists() {
            problems.push("the lock file exists".to_string());
        }
        if Path::new(WORK_DIR).exists() {
            problems.push("the work folder was created".to_string());
        }
        for problem in problems {
            failures.push(format!("{}: {problem}", row.name));
        }
    }

    let _ = fs::remove_file(&link);
    let _ = fs::remove_dir(LINK_DIR);
    let _ = fs::remove_file(LOCK);
    assert!(
        failures.is_empty(),
        "{} problems in the {total} rows:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
