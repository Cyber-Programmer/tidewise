//! Process names and grouping for the Processes screen. Pure functions: the callers read `/proc`.

use std::collections::HashMap;
use std::path::Path;

/// Split the contents of `/proc/<pid>/cmdline` (NUL-separated) into arguments.
pub fn parse_cmdline(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|b| *b == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect()
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn is_interpreter(name: &str) -> bool {
    name.starts_with("python")
        || matches!(
            name,
            "node" | "perl" | "ruby" | "bash" | "sh" | "zsh" | "java"
        )
}

/// What an app is called internally: the program name, looking through interpreters
/// (`python3 /usr/bin/foo.py` is "foo"). Falls back to the kernel's 15-character `comm`.
pub fn app_key(cmdline: &[String], comm: &str) -> String {
    let Some(first) = cmdline.first().filter(|a| !a.is_empty()) else {
        return comm.to_string();
    };
    let name = basename(first).trim_start_matches('-');
    if is_interpreter(name) {
        if let Some(script) = cmdline.get(1).filter(|a| !a.starts_with('-')) {
            let script = basename(script);
            return script
                .rsplit_once('.')
                .map_or(script, |(stem, _)| stem)
                .to_string();
        }
    }
    if name.is_empty() {
        comm.to_string()
    } else {
        name.to_string()
    }
}

/// The key of the program an `Exec=` line starts: the app id for `flatpak run`, else the program name.
fn exec_key(exec: &str) -> Option<String> {
    let mut tokens = exec
        .split_whitespace()
        .map(|t| t.trim_matches('"'))
        .skip_while(|t| *t == "env" || (t.contains('=') && !t.starts_with('/')));
    let program = basename(tokens.next()?);
    if program == "flatpak" {
        let mut rest = tokens.skip_while(|t| *t != "run").skip(1);
        return rest.find(|t| !t.starts_with('-')).map(str::to_string);
    }
    (!program.is_empty()).then(|| program.to_string())
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesktopEntry {
    pub name: String,
    /// Same kind of key `app_key` gives for a running process.
    pub key: String,
}

/// The `[Desktop Entry]` group of a `.desktop` file; actions and translations are ignored.
pub fn parse_desktop_entry(text: &str) -> Option<DesktopEntry> {
    let mut in_main = false;
    let (mut name, mut exec) = (None, None);
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
        } else if in_main {
            if let Some(v) = line.strip_prefix("Name=") {
                name.get_or_insert(v.trim().to_string());
            } else if let Some(v) = line.strip_prefix("Exec=") {
                exec.get_or_insert(v.trim().to_string());
            }
        }
    }
    let name = name.filter(|n| !n.is_empty())?;
    Some(DesktopEntry {
        name,
        key: exec_key(&exec?)?,
    })
}

/// Names from every `*.desktop` file in `dir`, keyed by `DesktopEntry::key`.
pub fn load_desktop_names(dir: &Path, into: &mut HashMap<String, String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "desktop"))
        .collect();
    files.sort();
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some(entry) = parse_desktop_entry(&text) {
            into.entry(entry.key).or_insert(entry.name);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    App,
    Background,
    System,
}

pub struct Proc {
    pub pid: u32,
    pub comm: String,
    pub cmdline: Vec<String>,
    /// Share of the whole PC, 0 to 100.
    pub cpu: f64,
    /// Percent of one core (technical details).
    pub cpu_core: f64,
    /// Private memory, safe to add up.
    pub mem: u64,
    /// Resident size including shared libraries (technical details).
    pub rss: u64,
}

#[derive(Debug)]
pub struct Group {
    pub key: String,
    pub name: String,
    pub kind: Kind,
    pub count: usize,
    pub cpu: f64,
    pub cpu_core: f64,
    pub mem: u64,
    /// Resident size of the lead (biggest) process.
    pub rss: u64,
    pub protected: bool,
    pub pid: u32,
    pub cmd: String,
}

/// Protected groups get no End button (golden rule 7): the session, audio, network, the journal.
pub fn is_protected(key: &str) -> bool {
    system_part(key).is_some()
}

/// Programs that together make up one protected part of the system: (id, shown name, members).
const SYSTEM_PARTS: &[(&str, &str, &[&str])] = &[
    (
        "plasma",
        "Plasma desktop",
        &["plasmashell", "kwin_wayland", "kwin_x11", "kded6", "kded5"],
    ),
    (
        "gnome",
        "GNOME desktop",
        &["gnome-shell", "gnome-session-binary", "mutter"],
    ),
    ("display", "Display server", &["Xorg", "Xwayland"]),
    (
        "sound",
        "Sound system",
        &["pipewire", "pipewire-pulse", "wireplumber", "pulseaudio"],
    ),
    (
        "network",
        "Network connection",
        &["NetworkManager", "wpa_supplicant"],
    ),
    (
        "system",
        "System",
        &[
            "systemd",
            "systemd-journald",
            "systemd-logind",
            "dbus-daemon",
            "dbus-broker",
            "polkitd",
            "sddm",
            "gdm",
            "lightdm",
            "init",
        ],
    ),
];

fn system_part(key: &str) -> Option<(&'static str, &'static str)> {
    SYSTEM_PARTS
        .iter()
        .find(|(_, _, members)| members.contains(&key))
        .map(|(id, name, _)| (*id, *name))
}

/// "kwin_wayland" -> "Kwin wayland" when no .desktop file names it.
fn prettify(key: &str) -> String {
    let spaced = key.replace(['-', '_'], " ");
    let mut chars = spaced.chars();
    chars
        .next()
        .map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
}

pub fn group(procs: &[Proc], desktop: &HashMap<String, String>) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    let mut biggest: HashMap<String, u64> = HashMap::new();
    for p in procs {
        let program = app_key(&p.cmdline, &p.comm);
        let (key, name, kind) = match system_part(&program) {
            Some((id, name)) => (id.to_string(), name.to_string(), Kind::System),
            None => match desktop.get(&program) {
                Some(name) => (program.clone(), name.clone(), Kind::App),
                None => (program.clone(), prettify(&program), Kind::Background),
            },
        };
        let at = match groups.iter().position(|g| g.key == key) {
            Some(at) => at,
            None => {
                groups.push(Group {
                    key: key.clone(),
                    name,
                    kind,
                    count: 0,
                    cpu: 0.0,
                    cpu_core: 0.0,
                    mem: 0,
                    rss: 0,
                    protected: is_protected(&program),
                    pid: p.pid,
                    cmd: String::new(),
                });
                groups.len() - 1
            }
        };
        let g = &mut groups[at];
        g.count += 1;
        g.cpu += p.cpu;
        g.cpu_core += p.cpu_core;
        g.mem += p.mem;
        if biggest.get(&key).is_none_or(|m| p.mem > *m) {
            biggest.insert(key, p.mem);
            g.pid = p.pid;
            g.rss = p.rss;
            g.cmd = p.cmdline.join(" ").chars().take(200).collect();
        }
    }
    groups.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.mem.cmp(&a.mem)));
    groups
}

/// `/proc/<pid>/cmdline` and `comm` under `root`; None when the process is gone.
pub fn read_proc(root: &Path, pid: u32) -> Option<(String, Vec<String>)> {
    let dir = root.join(pid.to_string());
    let comm = std::fs::read_to_string(dir.join("comm")).ok()?;
    let cmdline = std::fs::read(dir.join("cmdline"))
        .map(|b| parse_cmdline(&b))
        .unwrap_or_default();
    Some((comm.trim_end().to_string(), cmdline))
}

/// One row of the list, already formatted for display.
#[derive(Clone, Debug)]
pub struct Row {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub sub: String,
    pub sub_details: String,
    pub color: (u8, u8, u8),
    pub protected: bool,
    pub cpu: String,
    pub cpu_hot: bool,
    pub mem: String,
    pub mem_hot: bool,
    pub disk: String,
    pub net: String,
    pub note: String,
    pub cmd: String,
}

pub struct ProcData {
    pub subtitle: String,
    pub rows: Vec<Row>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    Apps,
    Background,
    All,
}

impl Filter {
    pub fn index(self) -> i32 {
        self as i32
    }

    pub fn from_index(i: i32) -> Filter {
        match i {
            0 => Filter::Apps,
            1 => Filter::Background,
            _ => Filter::All,
        }
    }
}

/// Rows that pass the filter tab and the search box (case-insensitive, on the name).
pub fn visible<'a>(rows: &'a [Row], filter: Filter, search: &str) -> Vec<&'a Row> {
    let needle = search.trim().to_lowercase();
    rows.iter()
        .filter(|r| match filter {
            Filter::Apps => r.kind == Kind::App,
            Filter::Background => r.kind != Kind::App,
            Filter::All => true,
        })
        .filter(|r| needle.is_empty() || r.name.to_lowercase().contains(&needle))
        .collect()
}

/// Tab, selection and details switch (the `--state` variants).
#[derive(Debug, PartialEq, Eq)]
pub struct View {
    pub details: bool,
    pub filter: Filter,
    pub selected: String,
}

impl View {
    pub fn from_state(name: Option<&str>) -> Result<View, String> {
        let (details, filter, selected) = match name {
            None => (false, Filter::All, "firefox"),
            Some("details") => (true, Filter::All, "firefox"),
            Some("apps") => (false, Filter::Apps, "firefox"),
            Some("protected-selected") => (false, Filter::All, "plasma"),
            Some(other) => return Err(format!("unknown processes state '{other}'")),
        };
        Ok(View {
            details,
            filter,
            selected: selected.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
    }

    fn s(list: &[&str]) -> Vec<String> {
        list.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn cmdline_is_split_on_nul() {
        assert_eq!(
            parse_cmdline(b"/usr/bin/foo\0--bar\0baz\0"),
            s(&["/usr/bin/foo", "--bar", "baz"])
        );
        assert_eq!(parse_cmdline(b"a\0b"), s(&["a", "b"]), "no trailing NUL");
    }

    #[test]
    fn empty_and_junk_cmdlines_do_not_panic() {
        assert!(parse_cmdline(b"").is_empty());
        assert!(parse_cmdline(b"\0\0\0").is_empty());
        assert_eq!(
            parse_cmdline(b"\xff\xfe\0ok\0").len(),
            2,
            "invalid UTF-8 is replaced, not dropped"
        );
    }

    #[test]
    fn real_cmdlines_from_fixtures() {
        let (comm, cmd) = read_proc(&root().join("proc-cmdline"), 2779).unwrap();
        assert_eq!(
            comm, "xdg-desktop-por",
            "the kernel cuts comm at 15 characters"
        );
        assert_eq!(
            app_key(&cmd, &comm),
            "xdg-desktop-portal",
            "the full name comes from cmdline"
        );
        let (comm, cmd) = read_proc(&root().join("proc-cmdline"), 386851).unwrap();
        assert_eq!(app_key(&cmd, &comm), "plasmashell");
        assert_eq!(cmd[1], "--no-respawn");
    }

    #[test]
    fn a_missing_process_is_none() {
        assert!(read_proc(&root().join("proc-cmdline"), 999_999).is_none());
    }

    #[test]
    fn keys_look_through_interpreters_and_login_shells() {
        assert_eq!(
            app_key(&s(&["python3", "/usr/bin/foo.py", "-x"]), "python3"),
            "foo"
        );
        assert_eq!(
            app_key(
                &s(&["/usr/bin/node", "--inspect", "/srv/app/server.js"]),
                "node"
            ),
            "node"
        );
        assert_eq!(app_key(&s(&["-bash"]), "bash"), "bash");
        assert_eq!(
            app_key(
                &s(&["/opt/visual-studio-code/code", "--type=renderer"]),
                "code"
            ),
            "code"
        );
    }

    #[test]
    fn empty_cmdline_falls_back_to_comm() {
        assert_eq!(app_key(&[], "kworker/0:1"), "kworker/0:1");
        assert_eq!(app_key(&s(&[""]), "kthreadd"), "kthreadd");
    }

    #[test]
    fn desktop_entry_main_group_only() {
        let text = fs::read_to_string(root().join("desktop/firefox.desktop")).unwrap();
        assert_eq!(
            parse_desktop_entry(&text),
            Some(DesktopEntry {
                name: "Firefox".into(),
                key: "firefox".into()
            })
        );
        let text = fs::read_to_string(root().join("desktop/code.desktop")).unwrap();
        assert_eq!(
            parse_desktop_entry(&text).unwrap().name,
            "Visual Studio Code"
        );
    }

    #[test]
    fn broken_desktop_entry_is_none() {
        let text = fs::read_to_string(root().join("desktop/broken.desktop")).unwrap();
        assert_eq!(parse_desktop_entry(&text), None);
        assert_eq!(parse_desktop_entry(""), None);
    }

    #[test]
    fn flatpak_exec_uses_the_app_id() {
        let text = "[Desktop Entry]\nName=Steam\nExec=/usr/bin/flatpak run --branch=stable com.valvesoftware.Steam @@u %U @@\n";
        assert_eq!(
            parse_desktop_entry(text).unwrap().key,
            "com.valvesoftware.Steam"
        );
    }

    #[test]
    fn desktop_names_load_from_a_folder() {
        let mut map = HashMap::new();
        load_desktop_names(&root().join("desktop"), &mut map);
        assert_eq!(map.get("firefox").map(String::as_str), Some("Firefox"));
        assert_eq!(
            map.get("code").map(String::as_str),
            Some("Visual Studio Code")
        );
        load_desktop_names(&root().join("no-such-folder"), &mut map);
    }

    #[test]
    fn protected_things_are_refused_in_code() {
        for key in [
            "plasmashell",
            "kwin_wayland",
            "pipewire",
            "wireplumber",
            "systemd",
            "NetworkManager",
            "systemd-journald",
            "gnome-shell",
            "Xorg",
        ] {
            assert!(is_protected(key), "{key}");
        }
        for key in ["firefox", "code", "docker", ""] {
            assert!(!is_protected(key), "{key}");
        }
    }

    fn proc(pid: u32, cmd: &[&str], cpu: f64, mem: u64) -> Proc {
        Proc {
            pid,
            comm: cmd[0]
                .rsplit('/')
                .next()
                .unwrap()
                .chars()
                .take(15)
                .collect(),
            cmdline: s(cmd),
            cpu,
            cpu_core: cpu * 12.0,
            mem,
            rss: mem * 2,
        }
    }

    #[test]
    fn processes_of_one_app_become_one_row() {
        let mut desktop = HashMap::new();
        desktop.insert("firefox".to_string(), "Firefox".to_string());
        let procs = vec![
            proc(10, &["/usr/lib/firefox/firefox"], 4.0, 1_000),
            proc(11, &["/usr/lib/firefox/firefox", "-contentproc"], 2.2, 500),
            proc(12, &["/usr/bin/sleep", "9"], 0.1, 10),
        ];
        let groups = group(&procs, &desktop);
        assert_eq!(groups.len(), 2);
        let ff = &groups[0];
        assert_eq!(
            (ff.name.as_str(), ff.count, ff.kind),
            ("Firefox", 2, Kind::App)
        );
        assert!((ff.cpu - 6.2).abs() < 1e-9);
        assert_eq!(ff.mem, 1_500);
        assert!(
            (ff.cpu_core - 6.2 * 12.0).abs() < 1e-9,
            "per-core values add up too"
        );
        assert_eq!(
            ff.rss,
            2 * 1_000,
            "technical details show the lead process's RSS"
        );
        assert_eq!(ff.pid, 10, "the biggest process stands for the group");
        assert_eq!(groups[1].kind, Kind::Background);
    }

    #[test]
    fn system_parts_share_one_protected_row() {
        let procs = vec![
            proc(1, &["/usr/bin/pipewire"], 0.2, 20),
            proc(2, &["/usr/bin/wireplumber"], 0.1, 20),
            proc(3, &["/usr/bin/pipewire-pulse"], 0.1, 8),
        ];
        let groups = group(&procs, &HashMap::new());
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "Sound system");
        assert_eq!(groups[0].count, 3);
        assert!(groups[0].protected && groups[0].kind == Kind::System);
    }

    #[test]
    fn rows_are_sorted_by_cpu_then_memory() {
        let procs = vec![
            proc(1, &["/bin/a"], 1.0, 5),
            proc(2, &["/bin/b"], 3.0, 1),
            proc(3, &["/bin/c"], 1.0, 9),
        ];
        let names: Vec<_> = group(&procs, &HashMap::new())
            .into_iter()
            .map(|g| g.name)
            .collect();
        assert_eq!(names, ["B", "C", "A"]);
    }

    fn row(id: &str, name: &str, kind: Kind) -> Row {
        Row {
            id: id.into(),
            name: name.into(),
            kind,
            sub: String::new(),
            sub_details: String::new(),
            color: (0, 0, 0),
            protected: kind == Kind::System,
            cpu: String::new(),
            cpu_hot: false,
            mem: String::new(),
            mem_hot: false,
            disk: String::new(),
            net: String::new(),
            note: String::new(),
            cmd: String::new(),
        }
    }

    fn sample() -> Vec<Row> {
        vec![
            row("firefox", "Firefox", Kind::App),
            row("docker", "Docker", Kind::Background),
            row("plasma", "Plasma desktop", Kind::System),
        ]
    }

    #[test]
    fn filter_tabs_pick_the_right_rows() {
        let rows = sample();
        assert_eq!(visible(&rows, Filter::All, "").len(), 3);
        let apps: Vec<_> = visible(&rows, Filter::Apps, "")
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        assert_eq!(apps, ["firefox"]);
        let bg: Vec<_> = visible(&rows, Filter::Background, "")
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        assert_eq!(bg, ["docker", "plasma"], "system parts count as background");
    }

    #[test]
    fn search_ignores_case_and_spaces_around() {
        let rows = sample();
        assert_eq!(visible(&rows, Filter::All, "  FIRE ").len(), 1);
        assert!(visible(&rows, Filter::All, "zzz").is_empty());
        assert!(
            visible(&rows, Filter::Apps, "docker").is_empty(),
            "filter and search both apply"
        );
    }

    #[test]
    fn states_map_to_view() {
        assert_eq!(View::from_state(None).unwrap().filter, Filter::All);
        assert!(View::from_state(Some("details")).unwrap().details);
        assert_eq!(View::from_state(Some("apps")).unwrap().filter, Filter::Apps);
        assert_eq!(
            View::from_state(Some("protected-selected"))
                .unwrap()
                .selected,
            "plasma"
        );
        assert!(View::from_state(Some("nope")).is_err());
    }

    #[test]
    fn filter_index_round_trips() {
        for f in [Filter::Apps, Filter::Background, Filter::All] {
            assert_eq!(Filter::from_index(f.index()), f);
        }
    }
}
