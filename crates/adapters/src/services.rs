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

pub fn parse_units(_json: &str) -> Result<Vec<Unit>, ParseError> {
    Ok(Vec::new())
}

pub fn parse_unit_files(_json: &str) -> Result<Vec<UnitFile>, ParseError> {
    Ok(Vec::new())
}

/// `Id=x.service` / `MemoryCurrent=123` blocks of `systemctl show`: bytes per unit. Units that do
/// not count memory (`[not set]`, or the "unlimited" all-ones value) are left out.
pub fn parse_memory(_text: &str) -> Result<HashMap<String, u64>, ParseError> {
    Ok(HashMap::new())
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
pub fn is_protected(_unit: &str) -> bool {
    false
}

/// Every installed service: the loaded ones from `units` plus unit files that are not loaded
/// (listed as Off), without templates (`name@.service`) and aliases.
pub fn build(_units: &[Unit], _files: &[UnitFile], _memory: &HashMap<String, u64>) -> Vec<Service> {
    Vec::new()
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
