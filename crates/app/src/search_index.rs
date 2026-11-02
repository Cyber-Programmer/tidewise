//! What the search box looks through: pages and actions and settings (fixed), the apps that were
//! found and the programs that are running (as known when the box is opened). Home files are not
//! indexed. Each item carries a `key` that says what to open; `parse_key` reads it back and refuses
//! anything that is not one of ours.

use tidewise_core::search::Item;

/// The few things the index needs to know about an app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInfo {
    pub id: String,
    pub name: String,
    /// `System`, `Flatpak`, `Snap`, `AppImage`.
    pub source: String,
    pub size_bytes: Option<u64>,
}

/// One running program (all its processes together).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub name: String,
    pub count: usize,
    pub mem_bytes: u64,
}

/// What a key opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Open {
    /// A page of the sidebar (0 Home … 9 Settings) or 10, "Why your score is N".
    Page(i32),
    /// The Apps page, searched for this app id.
    App(String),
    /// The Processes page, searched for this program.
    Process(String),
}

pub fn parse_key(_key: &str) -> Option<Open> {
    None
}

/// The whole index.
pub fn items(_apps: &[AppInfo], _procs: &[ProcInfo]) -> Vec<Item> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewise_core::search::{search, Group};

    fn app(id: &str, name: &str, source: &str, size: Option<u64>) -> AppInfo {
        AppInfo {
            id: id.into(),
            name: name.into(),
            source: source.into(),
            size_bytes: size,
        }
    }

    fn sample() -> Vec<Item> {
        items(
            &[
                app("firefox", "Firefox", "System", Some(248_000_000)),
                app(
                    "com.valvesoftware.Steam",
                    "Steam",
                    "Flatpak",
                    Some(1_400_000_000),
                ),
                app("odd", "Odd one", "AppImage", None),
            ],
            &[
                ProcInfo {
                    name: "Firefox".into(),
                    count: 14,
                    mem_bytes: 2_100_000_000,
                },
                ProcInfo {
                    name: "Kdenlive".into(),
                    count: 1,
                    mem_bytes: 430_000_000,
                },
            ],
        )
    }

    fn find<'a>(items: &'a [Item], group: Group, title: &str) -> &'a Item {
        items
            .iter()
            .find(|i| i.group == group && i.title == title)
            .unwrap_or_else(|| panic!("no {title}"))
    }

    #[test]
    fn every_page_can_be_opened_by_name_and_has_a_plain_description() {
        let all = sample();
        for (page, title) in [
            (0, "Home"),
            (1, "Clean up"),
            (2, "Apps"),
            (3, "Storage"),
            (4, "Performance"),
            (5, "Processes"),
            (6, "Startup apps"),
            (7, "Services"),
            (8, "History"),
            (9, "Settings"),
        ] {
            let item = find(&all, Group::Actions, &format!("Open {title}"));
            assert_eq!(parse_key(&item.key), Some(Open::Page(page)), "{title}");
            assert!(!item.meta.is_empty());
        }
    }

    #[test]
    fn the_artboards_actions_and_settings_are_there_with_words_people_type() {
        let all = sample();
        let clean = find(&all, Group::Actions, "Clean up junk files");
        assert_eq!(parse_key(&clean.key), Some(Open::Page(1)));
        assert!(clean.keywords.contains("free space") && clean.keywords.contains("cache"));
        assert_eq!(
            parse_key(&find(&all, Group::Actions, "See what's using space").key),
            Some(Open::Page(3))
        );
        assert_eq!(
            parse_key(&find(&all, Group::Actions, "Stop apps opening at login").key),
            Some(Open::Page(6))
        );
        assert_eq!(
            parse_key(&find(&all, Group::Actions, "See what is slowing my PC").key),
            Some(Open::Page(4))
        );
        assert_eq!(
            parse_key(&find(&all, Group::Actions, "Why is my health score what it is?").key),
            Some(Open::Page(10))
        );
        assert_eq!(
            parse_key(&find(&all, Group::Settings, "Theme").key),
            Some(Open::Page(9))
        );
        assert!(find(&all, Group::Settings, "Theme")
            .keywords
            .contains("dark"));
        assert_eq!(
            parse_key(&find(&all, Group::Settings, "Check for updates").key),
            Some(Open::Page(9))
        );
        // Nothing for settings that do not exist (the artboard's "Admin access").
        assert!(!all.iter().any(|i| i.title == "Admin access"));
    }

    #[test]
    fn apps_say_where_they_come_from_and_how_big_they_are() {
        let all = sample();
        let ff = find(&all, Group::Apps, "Firefox");
        assert_eq!(ff.meta, "System app · 248 MB");
        assert_eq!(parse_key(&ff.key), Some(Open::App("firefox".into())));
        assert_eq!(find(&all, Group::Apps, "Steam").meta, "Flatpak · 1.4 GB");
        assert_eq!(
            find(&all, Group::Apps, "Odd one").meta,
            "AppImage",
            "no size known: just the source"
        );
    }

    #[test]
    fn running_programs_say_how_many_processes_and_how_much_memory() {
        let all = sample();
        let ff = find(&all, Group::Processes, "Firefox");
        assert_eq!(ff.meta, "Running · 14 processes · 2.1 GB memory");
        assert_eq!(parse_key(&ff.key), Some(Open::Process("Firefox".into())));
        assert_eq!(
            find(&all, Group::Processes, "Kdenlive").meta,
            "Running · 1 process · 430 MB memory"
        );
    }

    #[test]
    fn keys_are_read_back_strictly() {
        assert_eq!(parse_key("page:0"), Some(Open::Page(0)));
        assert_eq!(parse_key("page:10"), Some(Open::Page(10)));
        for bad in [
            "",
            "page",
            "page:",
            "page:11",
            "page:-1",
            "page:3x",
            "page: 3",
            "app:",
            "proc:",
            "other:1",
            "APP:x",
            "page:3\n",
            &format!("app:{}", "a".repeat(300)),
        ] {
            assert_eq!(parse_key(bad), None, "{bad:?}");
        }
        assert_eq!(
            parse_key("app:org.gimp.GIMP"),
            Some(Open::App("org.gimp.GIMP".into()))
        );
        assert_eq!(
            parse_key("proc:Plasma desktop"),
            Some(Open::Process("Plasma desktop".into()))
        );
    }

    #[test]
    fn typing_finds_things_across_the_groups() {
        let all = sample();
        let hits = search(&all, "fire");
        let got: Vec<_> = hits
            .iter()
            .map(|&i| (all[i].group, all[i].title.as_str()))
            .collect();
        assert_eq!(
            got,
            [(Group::Apps, "Firefox"), (Group::Processes, "Firefox")]
        );
        let free = search(&all, "free space");
        assert!(free.iter().any(|&i| all[i].title == "Clean up junk files"));
        let dark = search(&all, "dark");
        assert_eq!(all[dark[0]].title, "Theme");
        let slow = search(&all, "my pc is slow");
        assert!(!slow.is_empty(), "plain sentences find something");
    }
}
