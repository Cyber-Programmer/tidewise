//! System services, read-only: what `systemctl` says is running, failed or off, which of them start
//! with the PC, how much memory each holds, and a short curated list of the few worth a look.
//! Nothing here starts, stops or changes a service: that needs admin access and is not built.
//!
//! Input is machine-readable output only (`systemctl list-units --output=json`, `list-unit-files
//! --output=json`, `show --property=Id,MemoryCurrent`), read with `LC_ALL=C`.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("not the JSON systemctl prints: {0}")]
    Json(String),
    #[error("entry {index}: {what}")]
    Entry { index: usize, what: String },
    #[error("line {line}: not `key=value`")]
    Line { line: usize },
}

/// One row of `systemctl list-units --type=service --all --output=json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub unit: String,
    pub load: String,
    pub active: String,
    pub sub: String,
    pub description: String,
}

/// One row of `systemctl list-unit-files --type=service --output=json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitFile {
    pub unit_file: String,
    pub state: String,
    pub preset: Option<String>,
}

fn rows(json: &str) -> Result<Vec<serde_json::Value>, ParseError> {
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(serde_json::Value::Array(rows)) => Ok(rows),
        Ok(_) => Err(ParseError::Json("expected a list".into())),
        Err(e) => Err(ParseError::Json(e.to_string())),
    }
}

fn text(row: &serde_json::Value, key: &str, index: usize) -> Result<String, ParseError> {
    row.get(key)
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| ParseError::Entry {
            index,
            what: format!("`{key}` is missing or not text"),
        })
}

/// A unit name as systemd prints it: not empty, not long, no control characters.
fn unit_name(row: &serde_json::Value, key: &str, index: usize) -> Result<String, ParseError> {
    let name = text(row, key, index)?;
    if name.is_empty() || name.len() > 255 || name.chars().any(char::is_control) {
        return Err(ParseError::Entry {
            index,
            what: format!("`{key}` is not a unit name"),
        });
    }
    Ok(name)
}

pub fn parse_units(json: &str) -> Result<Vec<Unit>, ParseError> {
    rows(json)?
        .iter()
        .enumerate()
        .map(|(i, row)| {
            Ok(Unit {
                unit: unit_name(row, "unit", i)?,
                load: text(row, "load", i)?,
                active: text(row, "active", i)?,
                sub: text(row, "sub", i)?,
                description: text(row, "description", i).unwrap_or_default(),
            })
        })
        .collect()
}

pub fn parse_unit_files(json: &str) -> Result<Vec<UnitFile>, ParseError> {
    rows(json)?
        .iter()
        .enumerate()
        .map(|(i, row)| {
            Ok(UnitFile {
                unit_file: unit_name(row, "unit_file", i)?,
                state: text(row, "state", i)?,
                preset: row.get("preset").and_then(|v| v.as_str()).map(String::from),
            })
        })
        .collect()
}

/// `Id=x.service` / `MemoryCurrent=123` blocks of `systemctl show`: bytes per unit. Units that do
/// not count memory (`[not set]`, or the "unlimited" all-ones value) are left out.
pub fn parse_memory(text: &str) -> Result<HashMap<String, u64>, ParseError> {
    let mut out = HashMap::new();
    let (mut id, mut memory): (Option<String>, Option<(usize, String)>) = (None, None);
    let mut flush = |id: &mut Option<String>, memory: &mut Option<(usize, String)>| {
        let (id, memory) = (id.take(), memory.take());
        if let (Some(id), Some((line, value))) = (id, memory) {
            if value == "[not set]" {
                return Ok(());
            }
            match value.parse::<u64>() {
                Ok(u64::MAX) => {}
                Ok(bytes) => {
                    out.insert(id, bytes);
                }
                Err(_) => return Err(ParseError::Line { line }),
            }
        }
        Ok(())
    };
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            flush(&mut id, &mut memory)?;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(ParseError::Line { line: index + 1 });
        };
        match key {
            "Id" => id = Some(value.to_string()),
            "MemoryCurrent" => memory = Some((index + 1, value.to_string())),
            _ => {}
        }
    }
    flush(&mut id, &mut memory)?;
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Running,
    /// Ran and finished (a one-shot job): active, nothing keeps running.
    Finished,
    Failed,
    Off,
}

/// Does it start with the PC?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boot {
    Yes,
    No,
    /// `static`/`indirect`: another unit or an event starts it when needed.
    WhenNeeded,
    /// Masked: it cannot be started at all.
    Masked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    pub unit: String,
    /// Plain name: curated, else systemd's description, else the unit's stem.
    pub name: String,
    /// One plain sentence.
    pub desc: String,
    pub state: State,
    pub boot: Boot,
    pub memory: Option<u64>,
    /// Refused in code: no action is ever offered.
    pub protected: bool,
    /// Why it is worth a look, with a suggestion; `None` for most.
    pub tip: Option<String>,
    /// `active/running · unit file: enabled`, for "Show technical details".
    pub technical: String,
}

/// Parts of the system that must keep running (golden rule 7): the journal, the network, the login
/// and session machinery, the display manager, udev, D-Bus, polkit, audio, and everything named
/// `systemd-*`.
pub fn is_protected(unit: &str) -> bool {
    const EXACT: [&str; 11] = [
        "networkmanager.service",
        "dbus.service",
        "polkit.service",
        "sddm.service",
        "gdm.service",
        "lightdm.service",
        "display-manager.service",
        "wpa_supplicant.service",
        "pipewire.service",
        "wireplumber.service",
        "pulseaudio.service",
    ];
    const PREFIX: [&str; 4] = ["systemd-", "user@", "getty@", "dbus-"];
    let lower = unit.to_lowercase();
    EXACT.contains(&lower.as_str()) || PREFIX.iter().any(|p| lower.starts_with(p))
}

/// Plain names, descriptions and suggestions for the few services people ask about. `review`: worth
/// a look when it runs and starts with the PC.
struct Curated {
    unit: &'static str,
    name: &'static str,
    desc: &'static str,
    review: bool,
    tip: &'static str,
}

const CURATED: [Curated; 8] = [
    Curated {
        unit: "docker.service",
        name: "Docker",
        desc: "Runs containers for developer tools. It uses memory all the time, even when you are not using any containers.",
        review: true,
        tip: "Turn off \"Start with your PC\" and start it only when you need it.",
    },
    Curated {
        unit: "cups.service",
        name: "Printing",
        desc: "Lets you print. If you never print from this PC, it does not need to run.",
        review: true,
        tip: "Turn it off if you have no printer. Turn it back on any time.",
    },
    Curated {
        unit: "bluetooth.service",
        name: "Bluetooth",
        desc: "Connects Bluetooth headphones, mice and keyboards.",
        review: true,
        tip: "Safe to turn off only if you never use Bluetooth.",
    },
    Curated {
        unit: "avahi-daemon.service",
        name: "Network device discovery",
        desc: "Finds printers and shared devices on your home network automatically.",
        review: false,
        tip: "",
    },
    Curated {
        unit: "sshd.service",
        name: "Remote login (SSH)",
        desc: "Lets other computers log in to yours over the network. Leave it off unless you need it.",
        review: false,
        tip: "",
    },
    Curated {
        unit: "fwupd-refresh.service",
        name: "Firmware update check",
        desc: "Checks for firmware updates for your devices.",
        review: false,
        tip: "",
    },
    Curated {
        unit: "NetworkManager.service",
        name: "Network",
        desc: "Keeps you connected to Wi-Fi and wired networks.",
        review: false,
        tip: "",
    },
    Curated {
        unit: "systemd-journald.service",
        name: "System log",
        desc: "Records what your system does so problems can be traced.",
        review: false,
        tip: "",
    },
];

fn state_of(active: &str, sub: &str) -> State {
    match (active, sub) {
        ("active", "exited") => State::Finished,
        ("active" | "activating" | "deactivating" | "reloading", _) => State::Running,
        ("failed", _) => State::Failed,
        _ => State::Off,
    }
}

fn boot_of(file_state: Option<&str>) -> Boot {
    match file_state {
        Some("enabled" | "enabled-runtime") => Boot::Yes,
        Some("disabled") => Boot::No,
        Some("masked" | "masked-runtime") => Boot::Masked,
        _ => Boot::WhenNeeded,
    }
}

/// Every installed service: the loaded ones from `units` plus unit files that are not loaded
/// (listed as Off), without templates (`name@.service`) and aliases.
pub fn build(units: &[Unit], files: &[UnitFile], memory: &HashMap<String, u64>) -> Vec<Service> {
    let file_state: HashMap<&str, &str> = files
        .iter()
        .map(|f| (f.unit_file.as_str(), f.state.as_str()))
        .collect();
    let make = |unit: &str, description: &str, state: State, technical: String| {
        let curated = CURATED.iter().find(|c| c.unit == unit);
        let boot = boot_of(file_state.get(unit).copied());
        let protected = is_protected(unit);
        let stem = unit.strip_suffix(".service").unwrap_or(unit);
        let name = match curated {
            Some(c) => c.name.to_string(),
            None if !description.is_empty() && description != unit => description.to_string(),
            None => stem.to_string(),
        };
        let tip = if protected {
            None
        } else if state == State::Failed {
            Some("It failed. Nothing is necessarily broken: many services fail when there is no network and try again later. The technical details say what happened.".to_string())
        } else {
            curated
                .filter(|c| c.review && state == State::Running && boot == Boot::Yes)
                .map(|c| c.tip.to_string())
        };
        Service {
            unit: unit.to_string(),
            name,
            desc: curated.map_or_else(|| description.to_string(), |c| c.desc.to_string()),
            state,
            boot,
            memory: memory.get(unit).copied(),
            protected,
            tip,
            technical,
        }
    };
    let mut out: Vec<Service> = Vec::new();
    for u in units.iter().filter(|u| u.load == "loaded") {
        let fs = file_state.get(u.unit.as_str()).copied().unwrap_or("none");
        out.push(make(
            &u.unit,
            &u.description,
            state_of(&u.active, &u.sub),
            format!("{}/{} · unit file: {fs}", u.active, u.sub),
        ));
    }
    for f in files {
        let (name, state) = (f.unit_file.as_str(), f.state.as_str());
        if name.ends_with("@.service") || state == "alias" || out.iter().any(|s| s.unit == name) {
            continue;
        }
        out.push(make(
            name,
            "",
            State::Off,
            format!("not loaded · unit file: {state}"),
        ));
    }
    out.sort_by_key(|s| (s.name.to_lowercase(), s.unit.clone()));
    out
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    #[error("systemctl could not be run: {0}")]
    NoSystemctl(String),
    #[error("systemctl failed: {0}")]
    Failed(String),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

/// Ask systemd (three read-only `systemctl` calls through `runner`, by absolute path) and build the
/// list. A failing memory query only leaves the memory column empty.
pub fn read(runner: &dyn crate::runner::CommandRunner) -> Result<Vec<Service>, ReadError> {
    const SYSTEMCTL: &str = "/usr/bin/systemctl";
    let ask = |args: &[&str]| -> Result<String, ReadError> {
        let out = runner
            .run(SYSTEMCTL, args)
            .map_err(ReadError::NoSystemctl)?;
        if out.success {
            Ok(out.stdout)
        } else {
            Err(ReadError::Failed(out.stderr.trim().to_string()))
        }
    };
    let units = parse_units(&ask(&[
        "list-units",
        "--type=service",
        "--all",
        "--output=json",
        "--no-pager",
    ])?)?;
    let files = parse_unit_files(&ask(&[
        "list-unit-files",
        "--type=service",
        "--output=json",
        "--no-pager",
    ])?)?;
    // Memory is a nice extra: when it cannot be read the column is simply empty.
    let memory = ask(&[
        "show",
        "--type=service",
        "--property=Id,MemoryCurrent",
        "--no-pager",
    ])
    .ok()
    .and_then(|text| parse_memory(&text).ok())
    .unwrap_or_default();
    Ok(build(&units, &files, &memory))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/../../tests/fixtures/services/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    fn real() -> (Vec<Unit>, Vec<UnitFile>, HashMap<String, u64>) {
        (
            parse_units(&fx("list-units-real.json")).unwrap(),
            parse_unit_files(&fx("list-unit-files-real.json")).unwrap(),
            parse_memory(&fx("show-memory-real.txt")).unwrap(),
        )
    }

    fn find<'a>(s: &'a [Service], unit: &str) -> &'a Service {
        s.iter()
            .find(|s| s.unit == unit)
            .unwrap_or_else(|| panic!("no {unit}"))
    }

    #[test]
    fn real_list_units_output_is_read() {
        let units = parse_units(&fx("list-units-real.json")).unwrap();
        assert_eq!(units.len(), 129);
        let nm = units
            .iter()
            .find(|u| u.unit == "NetworkManager.service")
            .unwrap();
        assert_eq!(
            (nm.load.as_str(), nm.active.as_str(), nm.sub.as_str()),
            ("loaded", "active", "running")
        );
        assert!(!nm.description.is_empty());
    }

    #[test]
    fn real_list_unit_files_output_is_read() {
        let files = parse_unit_files(&fx("list-unit-files-real.json")).unwrap();
        assert_eq!(files.len(), 331);
        assert_eq!(files.iter().filter(|f| f.state == "enabled").count(), 12);
        assert!(files.iter().any(|f| f.unit_file == "docker.service"));
    }

    #[test]
    fn real_show_output_gives_bytes_per_unit_and_skips_unset_ones() {
        let m = parse_memory(&fx("show-memory-real.txt")).unwrap();
        assert!(m["docker.service"] > 90_000_000, "{}", m["docker.service"]);
        assert!(
            !m.contains_key("alsa-restore.service"),
            "[not set] is left out"
        );
        assert!(parse_memory("").unwrap().is_empty());
        let max = format!("Id=a.service\nMemoryCurrent={}\n", u64::MAX);
        assert!(
            parse_memory(&max).unwrap().is_empty(),
            "the all-ones value means unknown"
        );
    }

    #[test]
    fn broken_or_hostile_output_is_an_error_not_a_guess() {
        for bad in [
            "",
            "not json",
            "{}",
            "[1]",
            r#"[{"load":"loaded"}]"#,
            r#"[{"unit":"a\u0007.service","load":"l","active":"a","sub":"s","description":"d"}]"#,
        ] {
            assert!(parse_units(bad).is_err(), "{bad:?}");
        }
        assert!(parse_unit_files(r#"[{"state":"enabled"}]"#).is_err());
        assert!(parse_unit_files("nope").is_err());
        assert_eq!(parse_units("[]").unwrap(), vec![]);
        assert!(matches!(
            parse_memory("no equals sign\n"),
            Err(ParseError::Line { line: 1 })
        ));
        assert!(parse_memory("Id=a.service\nMemoryCurrent=lots\n").is_err());
    }

    use crate::runner::{CommandRunner, Output};
    use std::cell::RefCell;

    struct Fake {
        calls: RefCell<Vec<(String, Vec<String>)>>,
        fail: Option<&'static str>,
        missing: bool,
    }

    impl Fake {
        fn new() -> Self {
            Fake {
                calls: RefCell::new(vec![]),
                fail: None,
                missing: false,
            }
        }
    }

    impl CommandRunner for Fake {
        fn run(&self, program: &str, args: &[&str]) -> Result<Output, String> {
            self.calls
                .borrow_mut()
                .push((program.into(), args.iter().map(|a| a.to_string()).collect()));
            if self.missing {
                return Err("no such file".into());
            }
            let sub = args[0];
            let stdout = match sub {
                "list-units" => fx("list-units-real.json"),
                "list-unit-files" => fx("list-unit-files-real.json"),
                _ => fx("show-memory-real.txt"),
            };
            let ok = self.fail != Some(sub);
            Ok(Output {
                success: ok,
                stdout: if ok { stdout } else { String::new() },
                stderr: if ok {
                    String::new()
                } else {
                    "Failed to connect to bus".into()
                },
            })
        }
    }

    #[test]
    fn reading_runs_three_read_only_systemctl_calls_and_builds_the_list() {
        let runner = Fake::new();
        let got = read(&runner).unwrap();
        let (u, f, m) = real();
        assert_eq!(got, build(&u, &f, &m));
        let calls = runner.calls.borrow();
        assert_eq!(calls.len(), 3);
        assert!(calls.iter().all(|(p, _)| p == "/usr/bin/systemctl"));
        let subs: Vec<_> = calls.iter().map(|(_, a)| a[0].as_str()).collect();
        assert_eq!(subs, ["list-units", "list-unit-files", "show"]);
        // Only reading verbs, never a change.
        for (_, args) in calls.iter() {
            assert!(!args.iter().any(|a| [
                "start", "stop", "enable", "disable", "mask", "restart", "kill"
            ]
            .contains(&a.as_str())));
        }
        assert!(
            calls[0].1.contains(&"--output=json".to_string())
                && calls[0].1.contains(&"--no-pager".to_string())
        );
    }

    #[test]
    fn a_missing_systemctl_or_a_failing_list_is_an_error_but_missing_memory_is_not() {
        let mut none = Fake::new();
        none.missing = true;
        assert!(matches!(read(&none), Err(ReadError::NoSystemctl(_))));
        for sub in ["list-units", "list-unit-files"] {
            let mut f = Fake::new();
            f.fail = Some(sub);
            assert!(matches!(read(&f), Err(ReadError::Failed(_))), "{sub}");
        }
        let mut no_memory = Fake::new();
        no_memory.fail = Some("show");
        let got = read(&no_memory).unwrap();
        assert!(!got.is_empty());
        assert!(got.iter().all(|s| s.memory.is_none()));
    }

    #[test]
    fn unknown_json_fields_are_ignored() {
        let j = r#"[{"unit":"a.service","load":"loaded","active":"active","sub":"running","description":"A","job":"x","new_field":[1,2]}]"#;
        assert_eq!(parse_units(j).unwrap()[0].unit, "a.service");
    }

    #[test]
    fn session_and_system_parts_are_protected_and_ordinary_services_are_not() {
        for p in [
            "NetworkManager.service",
            "systemd-journald.service",
            "systemd-logind.service",
            "dbus.service",
            "dbus-broker.service",
            "polkit.service",
            "sddm.service",
            "gdm.service",
            "lightdm.service",
            "display-manager.service",
            "wpa_supplicant.service",
            "pipewire.service",
            "wireplumber.service",
            "user@1000.service",
            "getty@tty1.service",
            "SYSTEMD-RESOLVED.SERVICE",
        ] {
            assert!(is_protected(p), "{p}");
        }
        for p in [
            "docker.service",
            "cups.service",
            "bluetooth.service",
            "sshd.service",
            "avahi-daemon.service",
            "NetworkManager-wait-online.service",
        ] {
            assert!(!is_protected(p), "{p}");
        }
    }

    #[test]
    fn the_real_machine_builds_a_service_list_with_names_state_boot_and_memory() {
        let (u, f, m) = real();
        let s = build(&u, &f, &m);
        let docker = find(&s, "docker.service");
        assert_eq!(docker.name, "Docker");
        assert_eq!(
            (docker.state, docker.boot, docker.protected),
            (State::Running, Boot::Yes, false)
        );
        assert!(docker.memory.unwrap() > 90_000_000);
        assert!(
            docker.technical.contains("active/running") && docker.technical.contains("enabled"),
            "{}",
            docker.technical
        );
        let nm = find(&s, "NetworkManager.service");
        assert_eq!((nm.name.as_str(), nm.protected), ("Network", true));
        assert!(nm.tip.is_none(), "protected services are never suggestions");
        let journal = find(&s, "systemd-journald.service");
        assert!(journal.protected && journal.name == "System log");
        // A plain service takes systemd's description as its name.
        assert_eq!(find(&s, "accounts-daemon.service").name, "Accounts Service");
    }

    #[test]
    fn the_list_has_no_templates_aliases_or_duplicates_and_includes_unit_files_that_are_not_loaded()
    {
        let (u, f, m) = real();
        let s = build(&u, &f, &m);
        assert!(
            s.iter().all(|x| !x.unit.ends_with("@.service")),
            "templates are not services"
        );
        let mut seen = std::collections::HashSet::new();
        assert!(
            s.iter().all(|x| seen.insert(x.unit.clone())),
            "no unit twice"
        );
        assert!(s.len() > 129, "{}", s.len());
        let loaded: std::collections::HashSet<_> = u
            .iter()
            .filter(|x| x.load == "loaded")
            .map(|x| &x.unit)
            .collect();
        let only_file = s
            .iter()
            .find(|x| !loaded.contains(&x.unit))
            .expect("a unit that only has a file");
        assert_eq!(only_file.state, State::Off);
        assert!(only_file.memory.is_none());
    }

    fn unit(name: &str, active: &str, sub: &str, desc: &str) -> Unit {
        Unit {
            unit: name.into(),
            load: "loaded".into(),
            active: active.into(),
            sub: sub.into(),
            description: desc.into(),
        }
    }

    fn file(name: &str, state: &str) -> UnitFile {
        UnitFile {
            unit_file: name.into(),
            state: state.into(),
            preset: None,
        }
    }

    #[test]
    fn states_come_from_active_and_sub() {
        let units = [
            unit("a.service", "active", "running", "A"),
            unit("b.service", "active", "exited", "B"),
            unit("c.service", "failed", "failed", "C"),
            unit("d.service", "inactive", "dead", "D"),
            unit("e.service", "activating", "start", "E"),
        ];
        let s = build(&units, &[], &HashMap::new());
        let st = |n: &str| find(&s, n).state;
        assert_eq!(
            [
                st("a.service"),
                st("b.service"),
                st("c.service"),
                st("d.service"),
                st("e.service")
            ],
            [
                State::Running,
                State::Finished,
                State::Failed,
                State::Off,
                State::Running
            ]
        );
    }

    #[test]
    fn does_it_start_with_the_pc_comes_from_the_unit_file_state() {
        let units = [unit("a.service", "active", "running", "A")];
        let boot = |state: &str| {
            find(
                &build(&units, &[file("a.service", state)], &HashMap::new()),
                "a.service",
            )
            .boot
        };
        assert_eq!(boot("enabled"), Boot::Yes);
        assert_eq!(boot("enabled-runtime"), Boot::Yes);
        assert_eq!(boot("disabled"), Boot::No);
        assert_eq!(boot("static"), Boot::WhenNeeded);
        assert_eq!(boot("indirect"), Boot::WhenNeeded);
        assert_eq!(boot("masked"), Boot::Masked);
        assert_eq!(boot("masked-runtime"), Boot::Masked);
        assert_eq!(boot("something-new"), Boot::WhenNeeded);
        assert_eq!(
            find(&build(&units, &[], &HashMap::new()), "a.service").boot,
            Boot::WhenNeeded,
            "no file known"
        );
    }

    #[test]
    fn the_curated_few_are_worth_a_look_only_when_they_cost_something() {
        let mk = |name: &str, active: &str, sub: &str, state: &str| {
            let s = build(
                &[unit(name, active, sub, "x")],
                &[file(name, state)],
                &HashMap::new(),
            );
            s[0].clone()
        };
        let on = mk("docker.service", "active", "running", "enabled");
        assert!(
            on.tip.as_deref().unwrap().contains("Start with your PC"),
            "{:?}",
            on.tip
        );
        assert!(
            mk("docker.service", "inactive", "dead", "enabled")
                .tip
                .is_none(),
            "not running"
        );
        assert!(
            mk("docker.service", "active", "running", "disabled")
                .tip
                .is_none(),
            "started by hand"
        );
        assert!(mk("cups.service", "active", "running", "enabled")
            .tip
            .is_some());
        assert!(mk("bluetooth.service", "active", "running", "enabled")
            .tip
            .is_some());
        assert!(
            mk("avahi-daemon.service", "active", "running", "enabled")
                .tip
                .is_none(),
            "curated but not a suggestion"
        );
        assert_eq!(
            mk("avahi-daemon.service", "active", "running", "enabled").name,
            "Network device discovery"
        );
        assert_eq!(
            mk("sshd.service", "inactive", "dead", "disabled").name,
            "Remote login (SSH)"
        );
    }

    #[test]
    fn a_failed_service_is_always_worth_a_look_and_says_nothing_may_be_broken() {
        let s = build(
            &[
                unit(
                    "fwupd-refresh.service",
                    "failed",
                    "failed",
                    "Refresh fwupd metadata",
                ),
                unit("zzz.service", "failed", "failed", "Zzz"),
            ],
            &[],
            &HashMap::new(),
        );
        for x in &s {
            assert_eq!(x.state, State::Failed);
            let tip = x.tip.as_deref().unwrap();
            assert!(tip.contains("failed"), "{tip}");
        }
        assert_eq!(
            find(&s, "fwupd-refresh.service").name,
            "Firmware update check"
        );
        // A protected service that failed is shown but no suggestion is made about it.
        let p = build(
            &[unit(
                "NetworkManager.service",
                "failed",
                "failed",
                "Network Manager",
            )],
            &[],
            &HashMap::new(),
        );
        assert!(p[0].protected && p[0].tip.is_none());
    }

    #[test]
    fn names_fall_back_to_the_description_then_the_stem() {
        let units = [
            unit("foo-bar.service", "active", "running", "Foo Bar Daemon"),
            unit("same.service", "active", "running", "same.service"),
        ];
        let s = build(&units, &[], &HashMap::new());
        assert_eq!(find(&s, "foo-bar.service").name, "Foo Bar Daemon");
        assert_eq!(find(&s, "same.service").name, "same");
    }

    #[test]
    fn masked_and_not_found_units_are_not_listed_as_services_to_look_at() {
        let mut gone = unit("gone.service", "inactive", "dead", "gone.service");
        gone.load = "not-found".into();
        let s = build(
            &[gone, unit("ok.service", "active", "running", "Ok")],
            &[],
            &HashMap::new(),
        );
        assert!(s.iter().all(|x| x.unit != "gone.service"));
    }
}
