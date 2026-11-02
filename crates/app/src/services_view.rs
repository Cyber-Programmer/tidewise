//! The Services screen: tabs, rows and the details of the picked service, from what
//! `tidewise_adapters::services` read. Plain data; `view.rs` copies it into Slint. Nothing can be
//! started, stopped or changed here yet: the buttons say they need admin access.

use tidewise_adapters::services::{Boot, Service, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Review,
    Running,
    All,
}

impl Tab {
    pub fn from_index(i: i32) -> Tab {
        match i {
            1 => Tab::Running,
            2 => Tab::All,
            _ => Tab::Review,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabInfo {
    pub label: String,
    pub count: usize,
    pub on: bool,
}

/// 0 running, 1 failed, 2 quiet (off or finished).
pub type StateKind = i32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub unit: String,
    pub name: String,
    pub line: String,
    pub state: String,
    pub kind: StateKind,
    pub protected: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detail {
    pub unit: String,
    pub name: String,
    pub state: String,
    pub kind: StateKind,
    pub desc: String,
    pub tip: String,
    pub boot: String,
    pub memory: String,
    pub protected: bool,
    /// Button words; empty when there is no such action (protected, or nothing to offer).
    pub primary: String,
    pub secondary: String,
    pub technical: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Screen {
    pub tabs: Vec<TabInfo>,
    pub rows: Vec<Row>,
    pub detail: Option<Detail>,
}

pub fn screen(_services: &[Service], _tab: Tab, _selected: Option<&str>) -> Screen {
    Screen::default()
}

#[allow(dead_code)]
fn unused(_: Boot, _: State) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc(
        unit: &str,
        name: &str,
        state: State,
        boot: Boot,
        memory: Option<u64>,
        protected: bool,
        tip: Option<&str>,
    ) -> Service {
        Service {
            unit: unit.into(),
            name: name.into(),
            desc: format!("{name} does things."),
            state,
            boot,
            memory,
            protected,
            tip: tip.map(Into::into),
            technical: "active/running · unit file: enabled".into(),
        }
    }

    fn sample() -> Vec<Service> {
        vec![
            svc(
                "cups.service",
                "Printing",
                State::Running,
                Boot::Yes,
                Some(24_000_000),
                false,
                Some("Turn it off if you have no printer."),
            ),
            svc(
                "docker.service",
                "Docker",
                State::Running,
                Boot::Yes,
                Some(640_000_000),
                false,
                Some("Start it only when you need it."),
            ),
            svc(
                "fwupd-refresh.service",
                "Firmware update check",
                State::Failed,
                Boot::No,
                None,
                false,
                Some("It failed."),
            ),
            svc(
                "sshd.service",
                "Remote login (SSH)",
                State::Off,
                Boot::No,
                None,
                false,
                None,
            ),
            svc(
                "NetworkManager.service",
                "Network",
                State::Running,
                Boot::Yes,
                Some(22_000_000),
                true,
                None,
            ),
            svc(
                "alsa-restore.service",
                "Sound card state",
                State::Finished,
                Boot::WhenNeeded,
                None,
                false,
                None,
            ),
        ]
    }

    #[test]
    fn the_tabs_count_worth_a_look_running_and_all() {
        let s = screen(&sample(), Tab::Review, None);
        let t: Vec<_> = s
            .tabs
            .iter()
            .map(|t| (t.label.as_str(), t.count, t.on))
            .collect();
        assert_eq!(
            t,
            [
                ("Worth a look", 3, true),
                ("Running", 3, false),
                ("All", 6, false)
            ]
        );
        assert!(screen(&sample(), Tab::All, None).tabs[2].on);
    }

    #[test]
    fn worth_a_look_lists_failed_first_then_the_biggest_memory() {
        let s = screen(&sample(), Tab::Review, None);
        let names: Vec<_> = s.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Firmware update check", "Docker", "Printing"]);
    }

    #[test]
    fn running_lists_the_biggest_memory_first_and_all_is_by_name() {
        let running = screen(&sample(), Tab::Running, None);
        assert_eq!(
            running
                .rows
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            ["Docker", "Printing", "Network"]
        );
        let all = screen(&sample(), Tab::All, None);
        let names: Vec<_> = all.rows.iter().map(|r| r.name.to_lowercase()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn a_row_says_the_state_and_whether_it_starts_with_the_pc_and_its_memory() {
        let s = screen(&sample(), Tab::All, None);
        let row = |n: &str| s.rows.iter().find(|r| r.name == n).unwrap().clone();
        let d = row("Docker");
        assert_eq!(
            (d.state.as_str(), d.kind, d.line.as_str()),
            ("Running", 0, "Starts with your PC · 640 MB")
        );
        let f = row("Firmware update check");
        assert_eq!((f.state.as_str(), f.kind), ("Failed", 1));
        assert_eq!(f.line, "Doesn't start with your PC");
        let o = row("Remote login (SSH)");
        assert_eq!((o.state.as_str(), o.kind), ("Off", 2));
        let q = row("Sound card state");
        assert_eq!(
            (q.state.as_str(), q.kind, q.line.as_str()),
            ("Finished", 2, "Starts when needed")
        );
        assert!(row("Network").protected && !d.protected);
    }

    #[test]
    fn the_first_row_is_picked_unless_another_one_of_this_tab_is() {
        let s = screen(&sample(), Tab::Review, None);
        assert!(s.rows[0].selected && !s.rows[1].selected);
        assert_eq!(s.detail.as_ref().unwrap().unit, "fwupd-refresh.service");
        let s = screen(&sample(), Tab::Review, Some("docker.service"));
        assert_eq!(s.detail.unwrap().unit, "docker.service");
        // A service that is not in this tab falls back to the first one.
        let s = screen(&sample(), Tab::Review, Some("sshd.service"));
        assert_eq!(s.detail.unwrap().unit, "fwupd-refresh.service");
        assert!(screen(&[], Tab::All, None).detail.is_none());
    }

    #[test]
    fn the_details_have_a_suggestion_boot_memory_and_buttons_that_need_admin() {
        let d = screen(&sample(), Tab::Review, Some("docker.service"))
            .detail
            .unwrap();
        assert_eq!(d.name, "Docker");
        assert_eq!(
            (d.state.as_str(), d.boot.as_str(), d.memory.as_str()),
            ("Running", "Yes", "640 MB")
        );
        assert_eq!(d.tip, "Start it only when you need it.");
        assert_eq!(
            (d.primary.as_str(), d.secondary.as_str()),
            ("Stop now", "Don't start with PC")
        );
        assert_eq!(d.technical, "active/running · unit file: enabled");
        let f = screen(&sample(), Tab::Review, None).detail.unwrap();
        assert_eq!(f.primary, "Try again");
        assert_eq!(f.memory, "—");
        let off = screen(&sample(), Tab::All, Some("sshd.service"))
            .detail
            .unwrap();
        assert_eq!(
            (
                off.primary.as_str(),
                off.secondary.as_str(),
                off.boot.as_str()
            ),
            ("Start now", "Start with PC", "No")
        );
        let finished = screen(&sample(), Tab::All, Some("alsa-restore.service"))
            .detail
            .unwrap();
        assert_eq!(
            finished.secondary, "",
            "started when needed: no switch to offer"
        );
        assert_eq!(finished.boot, "When needed");
    }

    #[test]
    fn a_protected_service_has_no_action_at_all_and_no_suggestion() {
        let d = screen(&sample(), Tab::All, Some("NetworkManager.service"))
            .detail
            .unwrap();
        assert!(d.protected);
        assert_eq!(
            (d.primary.as_str(), d.secondary.as_str(), d.tip.as_str()),
            ("", "", "")
        );
    }
}
