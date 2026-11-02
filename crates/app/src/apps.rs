//! Apps screen: what the list, the filter chips and the uninstall panel say, from the apps the
//! sources found and the removal plan of the one picked. Plain data; `view.rs` copies it into Slint.

use tidewise_adapters::cleanup::format_bytes;
use tidewise_adapters::source::{AppEntry, Blocked, RemovalPlan, SourceKind};
use tidewise_adapters::uninstall::Outcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    System,
    Flatpak,
    AppImage,
    Snap,
}

impl Filter {
    pub fn from_index(i: i32) -> Filter {
        match i {
            1 => Filter::System,
            2 => Filter::Flatpak,
            3 => Filter::AppImage,
            4 => Filter::Snap,
            _ => Filter::All,
        }
    }
}

/// What the user sees for where an app comes from.
pub fn source_label(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Pacman | SourceKind::Apt | SourceKind::Dnf => "System",
        SourceKind::Flatpak => "Flatpak",
        SourceKind::Snap => "Snap",
        SourceKind::AppImage => "AppImage",
    }
}

fn filter_of(kind: SourceKind) -> Filter {
    match kind {
        SourceKind::Pacman | SourceKind::Apt | SourceKind::Dnf => Filter::System,
        SourceKind::Flatpak => Filter::Flatpak,
        SourceKind::Snap => Filter::Snap,
        SourceKind::AppImage => Filter::AppImage,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    pub label: String,
    pub count: usize,
    pub filter: Filter,
}

/// `All` first, then each source that has apps. System packages (pacman, apt, dnf) share one chip.
pub fn chips(apps: &[AppEntry]) -> Vec<Chip> {
    let count = |f: Filter| apps.iter().filter(|a| filter_of(a.kind) == f).count();
    let mut chips = vec![Chip {
        label: "All".into(),
        count: apps.len(),
        filter: Filter::All,
    }];
    for (label, filter) in [
        ("System", Filter::System),
        ("Flatpak", Filter::Flatpak),
        ("AppImage", Filter::AppImage),
        ("Snap", Filter::Snap),
    ] {
        let n = count(filter);
        if n > 0 {
            chips.push(Chip {
                label: label.into(),
                count: n,
                filter,
            });
        }
    }
    chips
}

/// `143 apps from 4 sources. Remove any of them the same way.`
pub fn subtitle(apps: &[AppEntry]) -> String {
    if apps.is_empty() {
        return "No apps found.".into();
    }
    let sources = chips(apps).len() - 1;
    format!(
        "{} app{} from {} source{}. Remove any of them the same way.",
        apps.len(),
        if apps.len() == 1 { "" } else { "s" },
        sources,
        if sources == 1 { "" } else { "s" }
    )
}

/// Indices into `apps` of what the list shows: the filter, then the search (case-insensitive, name
/// or id), largest first, unknown sizes last, ties by name.
pub fn visible(apps: &[AppEntry], filter: Filter, query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    let mut shown: Vec<usize> = (0..apps.len())
        .filter(|&i| filter == Filter::All || filter_of(apps[i].kind) == filter)
        .filter(|&i| {
            query.is_empty()
                || apps[i].name.to_lowercase().contains(&query)
                || apps[i].id.to_lowercase().contains(&query)
        })
        .collect();
    shown.sort_by_key(|&i| {
        (
            apps[i].size_bytes.is_none(),
            std::cmp::Reverse(apps[i].size_bytes),
            apps[i].name.to_lowercase(),
        )
    });
    shown
}

/// `Showing 8 of 143`
pub fn footer(shown: usize, total: usize) -> String {
    format!("Showing {shown} of {total}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub source: String,
    /// The version, or empty.
    pub detail: String,
    pub size: String,
    /// First letter, upper case, for the round icon.
    pub letter: String,
    /// 0..=5, picks the icon colour; the same name always gets the same one.
    pub hue: i32,
}

pub fn row(app: &AppEntry) -> Row {
    Row {
        name: app.name.clone(),
        source: source_label(app.kind).into(),
        detail: app.version.clone().unwrap_or_default(),
        size: app.size_bytes.map_or("—".into(), format_bytes),
        letter: app
            .name
            .chars()
            .next()
            .map_or("?".into(), |c| c.to_uppercase().collect()),
        hue: (app.name.to_lowercase().bytes().map(u32::from).sum::<u32>() % 6) as i32,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Button {
    Enabled,
    /// Why it cannot be pressed, in plain words.
    Disabled(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Panel {
    pub name: String,
    /// `from Flatpak`
    pub subtitle: String,
    pub rows: Vec<(String, String)>,
    /// `732 MB`
    pub total: String,
    /// `Uninstall GIMP`
    pub button_label: String,
    pub button: Button,
    /// The command, for "Technical details". Display only.
    pub technical: String,
    /// Show the "Also delete my settings & data" choice (only where the folder is known).
    pub settings_offer: bool,
    /// The choice is on.
    pub delete_settings: bool,
}

/// The settings folder Tidewise knows for the app, and whether the user ticked it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsChoice {
    pub bytes: u64,
    pub delete: bool,
}

pub fn panel(app: &AppEntry, plan: &RemovalPlan) -> Panel {
    panel_with_settings(app, plan, None)
}

pub fn panel_with_settings(
    app: &AppEntry,
    plan: &RemovalPlan,
    settings: Option<SettingsChoice>,
) -> Panel {
    let known = |items: &[tidewise_adapters::source::PlanItem]| -> Option<u64> {
        let sizes: Vec<u64> = items.iter().filter_map(|i| i.bytes).collect();
        (!sizes.is_empty()).then(|| sizes.iter().sum())
    };
    let size_text = |bytes: Option<u64>| bytes.map_or("—".into(), format_bytes);
    let mut rows = Vec::new();
    if let Some(first) = plan.items.first() {
        rows.push(("The app itself".to_string(), size_text(first.bytes)));
    }
    let rest = plan.items.get(1..).unwrap_or_default();
    if !rest.is_empty() {
        let label = if plan.kind == SourceKind::AppImage {
            "Its menu entry and icon".to_string()
        } else {
            "Shared parts only it uses".to_string()
        };
        rows.push((label, size_text(known(rest))));
    }
    if !plan.items.is_empty() {
        rows.push(("Your settings and data".to_string(), "kept".to_string()));
    }
    let mut total = known(&plan.items);
    let deleting = settings.filter(|s| s.delete);
    if let Some(s) = deleting {
        if let Some(row) = rows.iter_mut().find(|r| r.0 == "Your settings and data") {
            row.1 = format!("{} to the Trash", format_bytes(s.bytes));
        }
        total = total.map(|t| t + s.bytes);
    }
    let button = match &plan.blocked {
        Some(Blocked::RequiredBy(who)) => Button::Disabled(format!(
            "{} other app{} {} it, so Tidewise won't remove it",
            who.len(),
            if who.len() == 1 { "" } else { "s" },
            if who.len() == 1 { "needs" } else { "need" }
        )),
        Some(Blocked::Protected(_)) => {
            Button::Disabled("Your system needs it, so Tidewise won't remove it".into())
        }
        Some(Blocked::Busy) => {
            Button::Disabled("Another installer is running. Try again when it is done".into())
        }
        None if plan.needs_admin => Button::Disabled("Needs admin access (coming soon)".into()),
        None => Button::Enabled,
    };
    Panel {
        name: app.name.clone(),
        subtitle: match app.version.as_deref() {
            Some(v) if !v.is_empty() => format!("{v} · from {}", source_label(app.kind)),
            _ => format!("from {}", source_label(app.kind)),
        },
        rows,
        total: size_text(total),
        button_label: format!("Uninstall {}", app.name),
        button,
        technical: if plan.command.is_empty() {
            "Moves the file to the Trash".into()
        } else {
            plan.command.join(" ")
        },
        settings_offer: settings.is_some(),
        delete_settings: deleting.is_some(),
    }
}

/// What the dialog says when an uninstall is over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    /// The app is gone (the "uninstalled" dialog); otherwise a message dialog.
    pub removed: bool,
    pub title: String,
    pub text: String,
}

pub fn done(name: &str, to_trash: bool, outcome: &Outcome) -> Done {
    done_with_settings(name, to_trash, false, outcome)
}

/// Like `done`, for an uninstall that also moved the settings folder to the Trash (`deleted`).
pub fn done_with_settings(name: &str, to_trash: bool, deleted: bool, outcome: &Outcome) -> Done {
    let say = |removed: bool, title: String, text: String| Done {
        removed,
        title,
        text,
    };
    match outcome {
        Outcome::Removed { freed, .. } => say(
            true,
            format!("{name} is uninstalled"),
            if to_trash {
                format!("{} freed. The files are in your Trash, so you can still put them back from there.", format_bytes(*freed))
            } else if deleted {
                format!("{} freed. Your settings and data are in your Trash, so you can still put them back from there.", format_bytes(*freed))
            } else {
                format!("{} freed. Your settings were kept, so reinstalling later brings everything back.", format_bytes(*freed))
            },
        ),
        Outcome::NeedsAdmin => say(
            false,
            format!("{name} needs admin access"),
            "Removing this kind of app needs your system's password prompt, which Tidewise does not use yet (coming soon). Nothing was changed.".into(),
        ),
        Outcome::Blocked(Blocked::Busy) => say(
            false,
            "Waiting for another installer".into(),
            "Something else is installing or updating software right now, and only one program can do that at a time. Try again when it has finished. Nothing was changed.".into(),
        ),
        Outcome::Blocked(Blocked::RequiredBy(who)) => say(
            false,
            format!("{name} can't be removed on its own"),
            format!(
                "{} installed app{} {} it. Removing it would break {}, so Tidewise won't. Remove {} first if you really don't need {}.",
                who.len(),
                if who.len() == 1 { "" } else { "s" },
                if who.len() == 1 { "needs" } else { "need" },
                if who.len() == 1 { "it" } else { "them" },
                if who.len() == 1 { "that app" } else { "those apps" },
                if who.len() == 1 { "it" } else { "them" },
            ),
        ),
        Outcome::Blocked(Blocked::Protected(_)) => say(
            false,
            format!("{name} can't be removed"),
            "Your system needs it to run, so Tidewise never removes it.".into(),
        ),
        Outcome::Failed { reason, removed } if removed.is_empty() => {
            say(false, format!("{name} was not removed"), reason.clone())
        }
        Outcome::Failed { reason, removed } => say(
            false,
            format!("{name} was only partly removed"),
            format!("{reason} {} items were already removed.", removed.len()),
        ),
    }
}

/// Everything the screen shows at once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    pub subtitle: String,
    /// Each chip and whether it is the selected filter.
    pub chips: Vec<(Chip, bool)>,
    /// Each visible row and whether it is the picked one.
    pub rows: Vec<(Row, bool)>,
    pub footer: String,
    /// The picked app's id, when there is one on the list.
    pub picked: Option<String>,
}

/// `picked` is the id the user clicked; when it is not in the visible list the first row is used.
pub fn screen(apps: &[AppEntry], filter: Filter, query: &str, picked: Option<&str>) -> Screen {
    let shown = visible(apps, filter, query);
    let picked = picked
        .filter(|id| shown.iter().any(|&i| apps[i].id == *id))
        .map(str::to_string)
        .or_else(|| shown.first().map(|&i| apps[i].id.clone()));
    Screen {
        subtitle: subtitle(apps),
        chips: chips(apps)
            .into_iter()
            .map(|c| {
                let on = c.filter == filter;
                (c, on)
            })
            .collect(),
        rows: shown
            .iter()
            .map(|&i| {
                (
                    row(&apps[i]),
                    picked.as_deref() == Some(apps[i].id.as_str()),
                )
            })
            .collect(),
        footer: footer(shown.len(), apps.len()),
        picked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewise_adapters::source::{PlanItem, Scope};

    fn app(name: &str, kind: SourceKind, size: Option<u64>) -> AppEntry {
        AppEntry {
            id: name.to_lowercase(),
            name: name.into(),
            kind,
            scope: Scope::System,
            version: Some("1.0-1".into()),
            size_bytes: size,
            required_by: vec![],
        }
    }

    fn sample() -> Vec<AppEntry> {
        vec![
            app("VLC", SourceKind::Pacman, Some(68_000_000)),
            app("Steam", SourceKind::Flatpak, Some(1_400_000_000)),
            app("Obsidian", SourceKind::AppImage, Some(132_000_000)),
            app("Firefox", SourceKind::Pacman, Some(248_000_000)),
            app("Spotify", SourceKind::Snap, Some(290_000_000)),
            app("Mystery", SourceKind::Apt, None),
            app("Zed", SourceKind::Dnf, Some(68_000_000)),
        ]
    }

    #[test]
    fn source_names_are_plain() {
        let got: Vec<_> = [
            SourceKind::Pacman,
            SourceKind::Apt,
            SourceKind::Dnf,
            SourceKind::Flatpak,
            SourceKind::Snap,
            SourceKind::AppImage,
        ]
        .map(source_label)
        .to_vec();
        assert_eq!(
            got,
            ["System", "System", "System", "Flatpak", "Snap", "AppImage"]
        );
    }

    #[test]
    fn chips_count_each_source_and_system_packages_together() {
        let got: Vec<_> = chips(&sample())
            .iter()
            .map(|c| (c.label.clone(), c.count))
            .collect();
        assert_eq!(
            got,
            [
                ("All".into(), 7),
                ("System".into(), 4),
                ("Flatpak".into(), 1),
                ("AppImage".into(), 1),
                ("Snap".into(), 1)
            ]
        );
        let none: Vec<_> = chips(&[])
            .iter()
            .map(|c| (c.label.clone(), c.count))
            .collect();
        assert_eq!(none, [("All".to_string(), 0)]);
    }

    #[test]
    fn the_subtitle_counts_apps_and_sources() {
        assert_eq!(
            subtitle(&sample()),
            "7 apps from 4 sources. Remove any of them the same way."
        );
        assert_eq!(
            subtitle(&sample()[..1]),
            "1 app from 1 source. Remove any of them the same way."
        );
        assert_eq!(subtitle(&[]), "No apps found.");
    }

    #[test]
    fn the_list_is_largest_first_with_unknown_sizes_last_and_ties_by_name() {
        let apps = sample();
        let names: Vec<_> = visible(&apps, Filter::All, "")
            .iter()
            .map(|&i| apps[i].name.as_str())
            .collect();
        assert_eq!(
            names,
            ["Steam", "Spotify", "Firefox", "Obsidian", "VLC", "Zed", "Mystery"]
        );
    }

    #[test]
    fn filters_and_search_combine() {
        let apps = sample();
        let names = |f, q: &str| -> Vec<String> {
            visible(&apps, f, q)
                .iter()
                .map(|&i| apps[i].name.clone())
                .collect()
        };
        assert_eq!(
            names(Filter::System, ""),
            ["Firefox", "VLC", "Zed", "Mystery"]
        );
        assert_eq!(names(Filter::Snap, ""), ["Spotify"]);
        assert_eq!(names(Filter::All, "FIRE"), ["Firefox"]);
        assert_eq!(
            names(Filter::All, "  vlc "),
            ["VLC"],
            "the search is trimmed"
        );
        assert_eq!(names(Filter::Flatpak, "vlc"), Vec::<String>::new());
        assert_eq!(names(Filter::All, "zzz"), Vec::<String>::new());
        assert_eq!(visible(&apps, Filter::All, "").len(), 7);
    }

    #[test]
    fn the_footer_and_filter_index() {
        assert_eq!(footer(8, 143), "Showing 8 of 143");
        assert_eq!(footer(0, 0), "Showing 0 of 0");
        assert_eq!(Filter::from_index(3), Filter::AppImage);
        assert_eq!(Filter::from_index(99), Filter::All);
        assert_eq!(Filter::from_index(-1), Filter::All);
    }

    #[test]
    fn a_row_shows_name_source_version_size_and_a_stable_icon() {
        let r = row(&app("gimp", SourceKind::Flatpak, Some(412_000_000)));
        assert_eq!(
            (
                r.source.as_str(),
                r.detail.as_str(),
                r.size.as_str(),
                r.letter.as_str()
            ),
            ("Flatpak", "1.0-1", "412 MB", "G")
        );
        assert_eq!(row(&app("gimp", SourceKind::Flatpak, None)).size, "—");
        assert_eq!(row(&app("gimp", SourceKind::Flatpak, None)).hue, r.hue);
        assert!((0..6).contains(&r.hue));
        assert_eq!(row(&app("élan", SourceKind::Pacman, None)).letter, "É");
        assert_eq!(row(&app("", SourceKind::Pacman, None)).letter, "?");
    }

    fn plan(
        items: &[(&str, Option<u64>)],
        blocked: Option<Blocked>,
        admin: bool,
        kind: SourceKind,
    ) -> RemovalPlan {
        RemovalPlan {
            id: "gimp".into(),
            kind,
            items: items
                .iter()
                .map(|(l, b)| PlanItem {
                    label: l.to_string(),
                    bytes: *b,
                })
                .collect(),
            blocked,
            needs_admin: admin,
            to_trash: kind == SourceKind::AppImage,
            command: vec!["flatpak".into(), "uninstall".into(), "org.gimp.GIMP".into()],
        }
    }

    #[test]
    fn a_removable_app_shows_what_goes_and_the_space_back() {
        let a = app("GIMP", SourceKind::Flatpak, Some(412_000_000));
        let p = panel(
            &a,
            &plan(
                &[
                    ("org.gimp.GIMP", Some(412_000_000)),
                    ("org.gnome.Platform", Some(320_000_000)),
                ],
                None,
                false,
                SourceKind::Flatpak,
            ),
        );
        assert_eq!(p.name, "GIMP");
        assert_eq!(p.subtitle, "1.0-1 · from Flatpak");
        assert_eq!(
            p.rows,
            [
                ("The app itself".to_string(), "412 MB".to_string()),
                (
                    "Shared parts only it uses".to_string(),
                    "320 MB".to_string()
                ),
                ("Your settings and data".to_string(), "kept".to_string()),
            ]
        );
        assert_eq!(p.total, "732 MB");
        assert_eq!(
            (p.button_label.as_str(), &p.button),
            ("Uninstall GIMP", &Button::Enabled)
        );
        assert_eq!(p.technical, "flatpak uninstall org.gimp.GIMP");
    }

    fn flatpak_panel(settings: Option<SettingsChoice>) -> Panel {
        let a = app("GIMP", SourceKind::Flatpak, Some(412_000_000));
        panel_with_settings(
            &a,
            &plan(
                &[
                    ("org.gimp.GIMP", Some(412_000_000)),
                    ("org.gnome.Platform", Some(320_000_000)),
                ],
                None,
                false,
                SourceKind::Flatpak,
            ),
            settings,
        )
    }

    #[test]
    fn without_a_known_settings_folder_nothing_is_offered() {
        let p = flatpak_panel(None);
        assert!(!p.settings_offer && !p.delete_settings);
        assert_eq!(
            p,
            panel(
                &app("GIMP", SourceKind::Flatpak, Some(412_000_000)),
                &plan(
                    &[
                        ("org.gimp.GIMP", Some(412_000_000)),
                        ("org.gnome.Platform", Some(320_000_000)),
                    ],
                    None,
                    false,
                    SourceKind::Flatpak,
                ),
            )
        );
    }

    #[test]
    fn a_known_settings_folder_is_offered_and_kept_until_ticked() {
        let p = flatpak_panel(Some(SettingsChoice {
            bytes: 2_000_000,
            delete: false,
        }));
        assert!(p.settings_offer && !p.delete_settings);
        assert_eq!(
            p.rows[2],
            ("Your settings and data".to_string(), "kept".to_string())
        );
        assert_eq!(p.total, "732 MB");
    }

    #[test]
    fn ticking_it_shows_the_size_that_goes_to_the_trash_and_adds_it_to_the_space_back() {
        let p = flatpak_panel(Some(SettingsChoice {
            bytes: 2_000_000,
            delete: true,
        }));
        assert!(p.settings_offer && p.delete_settings);
        assert_eq!(
            p.rows[2],
            (
                "Your settings and data".to_string(),
                "2 MB to the Trash".to_string()
            )
        );
        assert_eq!(p.total, "734 MB");
    }

    #[test]
    fn the_done_text_says_where_the_settings_went() {
        let o = Outcome::Removed {
            items: vec!["x".into()],
            freed: 734_000_000,
        };
        let d = done_with_settings("GIMP", false, true, &o);
        assert_eq!(
            d.text,
            "734 MB freed. Your settings and data are in your Trash, so you can still put them back from there."
        );
        assert_eq!(
            done_with_settings("GIMP", false, false, &o),
            done("GIMP", false, &o)
        );
    }

    #[test]
    fn an_appimage_lists_its_menu_entry_and_icon() {
        let a = app("Krita", SourceKind::AppImage, Some(100));
        let p = panel(
            &a,
            &plan(
                &[
                    ("/h/Krita.AppImage", Some(100)),
                    ("/h/krita.desktop", Some(80)),
                    ("/h/krita.png", None),
                ],
                None,
                false,
                SourceKind::AppImage,
            ),
        );
        assert_eq!(p.rows[1].0, "Its menu entry and icon");
        assert_eq!(p.rows[1].1, "80 B");
        assert_eq!(p.total, "180 B");
    }

    #[test]
    fn the_button_says_why_it_cannot_be_pressed() {
        let a = app("Python", SourceKind::Pacman, Some(1));
        let why = |blocked, admin| match panel(
            &a,
            &plan(&[("python", Some(1))], blocked, admin, SourceKind::Pacman),
        )
        .button
        {
            Button::Disabled(w) => w,
            Button::Enabled => "enabled".into(),
        };
        assert_eq!(
            why(
                Some(Blocked::RequiredBy(vec!["a".into(), "b".into()])),
                true
            ),
            "2 other apps need it, so Tidewise won't remove it"
        );
        assert_eq!(
            why(Some(Blocked::RequiredBy(vec!["a".into()])), true),
            "1 other app needs it, so Tidewise won't remove it"
        );
        assert_eq!(
            why(Some(Blocked::Protected(vec!["python".into()])), true),
            "Your system needs it, so Tidewise won't remove it"
        );
        assert_eq!(
            why(Some(Blocked::Busy), true),
            "Another installer is running. Try again when it is done"
        );
        assert_eq!(why(None, true), "Needs admin access (coming soon)");
    }

    #[test]
    fn a_removed_app_says_how_much_came_back_and_where_it_went() {
        let o = Outcome::Removed {
            items: vec!["x".into()],
            freed: 732_000_000,
        };
        let d = done("GIMP", false, &o);
        assert!(d.removed);
        assert_eq!(d.title, "GIMP is uninstalled");
        assert_eq!(
            d.text,
            "732 MB freed. Your settings were kept, so reinstalling later brings everything back."
        );
        let t = done("Krita", true, &o);
        assert_eq!(
            t.text,
            "732 MB freed. The files are in your Trash, so you can still put them back from there."
        );
    }

    #[test]
    fn other_endings_never_claim_success() {
        let admin = done("VLC", false, &Outcome::NeedsAdmin);
        assert!(!admin.removed);
        assert_eq!(admin.title, "VLC needs admin access");
        assert!(admin.text.ends_with("Nothing was changed."));
        let busy = done("VLC", false, &Outcome::Blocked(Blocked::Busy));
        assert_eq!(busy.title, "Waiting for another installer");
        let req = done(
            "Python",
            false,
            &Outcome::Blocked(Blocked::RequiredBy(vec!["a".into(); 38])),
        );
        assert_eq!(req.title, "Python can't be removed on its own");
        assert!(req.text.starts_with("38 installed apps need it"));
        let prot = done(
            "systemd",
            false,
            &Outcome::Blocked(Blocked::Protected(vec![])),
        );
        assert_eq!(prot.title, "systemd can't be removed");
        let none = done(
            "GIMP",
            false,
            &Outcome::Failed {
                reason: "It changed.".into(),
                removed: vec![],
            },
        );
        assert_eq!(
            (none.removed, none.title.as_str(), none.text.as_str()),
            (false, "GIMP was not removed", "It changed.")
        );
        let some = done(
            "GIMP",
            false,
            &Outcome::Failed {
                reason: "Disk error.".into(),
                removed: vec!["a".into(), "b".into()],
            },
        );
        assert_eq!(some.title, "GIMP was only partly removed");
        assert_eq!(some.text, "Disk error. 2 items were already removed.");
    }

    #[test]
    fn the_screen_marks_the_filter_and_the_picked_row() {
        let apps = sample();
        let s = screen(&apps, Filter::All, "", Some("firefox"));
        assert_eq!(s.picked.as_deref(), Some("firefox"));
        let marked: Vec<_> = s
            .rows
            .iter()
            .filter(|(_, on)| *on)
            .map(|(r, _)| r.name.as_str())
            .collect();
        assert_eq!(marked, ["Firefox"]);
        assert_eq!(s.rows.len(), 7);
        assert_eq!(s.footer, "Showing 7 of 7");
        let selected: Vec<_> = s
            .chips
            .iter()
            .filter(|(_, on)| *on)
            .map(|(c, _)| c.label.as_str())
            .collect();
        assert_eq!(selected, ["All"]);
    }

    #[test]
    fn a_pick_that_is_filtered_out_or_missing_falls_back_to_the_first_row() {
        let apps = sample();
        let s = screen(&apps, Filter::Snap, "", Some("firefox"));
        assert_eq!(s.picked.as_deref(), Some("spotify"));
        assert_eq!(
            screen(&apps, Filter::All, "", None).picked.as_deref(),
            Some("steam")
        );
        let none = screen(&apps, Filter::All, "zzz", Some("steam"));
        assert_eq!(
            (none.picked, none.rows.len(), none.footer.as_str()),
            (None, 0, "Showing 0 of 7")
        );
    }
}
