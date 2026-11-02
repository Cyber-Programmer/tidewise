//! Clean up screen: what the rows, the summary and the confirm dialog say, from the groups the
//! junk scanner found and the ones the user ticked. Plain data; `view.rs` copies it into Slint.

use tidewise_adapters::cleanup::{format_bytes, Group, GroupId, Outcome, Safety, StepResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowData {
    pub title: String,
    pub description: String,
    /// `Safe` or `Review`.
    pub safe: String,
    pub review: bool,
    pub admin: bool,
    pub size: String,
    pub checked: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    /// `12.0 GB`
    pub total: String,
    /// `of 12.6 GB found · 4 of 6 groups`
    pub found: String,
    /// Selected share of everything found, 0.0 to 1.0.
    pub fraction: f32,
    /// At least one selected group needs admin access.
    pub admin: bool,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmRow {
    pub name: String,
    pub size: String,
    pub admin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    /// `Remove 12.0 GB?`
    pub title: String,
    pub rows: Vec<ConfirmRow>,
    /// `2 of these need your password. You'll be asked once.`
    pub admin_note: Option<String>,
}

pub fn rows(groups: &[Group], selected: &[GroupId]) -> Vec<RowData> {
    groups
        .iter()
        .map(|g| RowData {
            title: g.title.to_string(),
            description: g.description.clone(),
            safe: if g.safety == Safety::Review {
                "Review"
            } else {
                "Safe"
            }
            .to_string(),
            review: g.safety == Safety::Review,
            admin: g.admin,
            size: format_bytes(g.bytes),
            checked: selected.contains(&g.id),
            detail: g.detail.clone(),
        })
        .collect()
}

fn chosen<'a>(groups: &'a [Group], selected: &[GroupId]) -> Vec<&'a Group> {
    groups.iter().filter(|g| selected.contains(&g.id)).collect()
}

pub fn summary(groups: &[Group], selected: &[GroupId]) -> Summary {
    let picked = chosen(groups, selected);
    let found: u64 = groups.iter().map(|g| g.bytes).sum();
    let total: u64 = picked.iter().map(|g| g.bytes).sum();
    Summary {
        total: format_bytes(total),
        found: format!(
            "of {} found · {} of {} groups",
            format_bytes(found),
            picked.len(),
            groups.len()
        ),
        fraction: if found == 0 {
            0.0
        } else {
            (total as f64 / found as f64) as f32
        },
        admin: picked.iter().any(|g| g.admin),
        count: picked.len(),
    }
}

/// The confirm dialog always counts in gigabytes from 100 MB up (`0.9 GB`), as the design does.
fn confirm_size(bytes: u64) -> String {
    if bytes >= 100_000_000 {
        format!("{:.1} GB", bytes as f64 / 1e9)
    } else {
        format_bytes(bytes)
    }
}

pub fn confirm(groups: &[Group], selected: &[GroupId]) -> Confirm {
    let picked = chosen(groups, selected);
    let total: u64 = picked.iter().map(|g| g.bytes).sum();
    let admin = picked.iter().filter(|g| g.admin).count();
    Confirm {
        title: format!("Remove {}?", format_bytes(total)),
        rows: picked
            .iter()
            .map(|g| ConfirmRow {
                name: g.title.to_string(),
                size: confirm_size(g.bytes),
                admin: g.admin,
            })
            .collect(),
        admin_note: match admin {
            0 => None,
            1 => Some("1 of these needs your password. You'll be asked once.".into()),
            n => Some(format!(
                "{n} of these need your password. You'll be asked once."
            )),
        },
    }
}

/// Tick or untick one group.
pub fn toggle(selected: &mut Vec<GroupId>, id: GroupId) {
    if let Some(at) = selected.iter().position(|s| *s == id) {
        selected.remove(at);
    } else {
        selected.push(id);
    }
}

/// What the dialog says when a run is over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finish {
    /// `true` when every step was cleaned (the Done dialog), `false` for the Skipped/Stopped one.
    pub all_done: bool,
    pub title: String,
    pub text: String,
    pub retry_label: String,
    /// Offer to run the rest. Only after a stop: admin steps cannot be retried until the helper exists.
    pub can_retry: bool,
}

/// The page when the scan found nothing worth cleaning (the artboard's "nothing found").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyState {
    pub title: String,
    pub text: String,
    /// The one thing to do next.
    pub action: String,
}

pub fn empty_state() -> EmptyState {
    EmptyState {
        title: String::new(),
        text: String::new(),
        action: String::new(),
    }
}

pub fn finish(results: &[StepResult]) -> Finish {
    let freed = tidewise_adapters::cleanup::freed_bytes(results);
    let mut left_alone = 0;
    let mut why_left: Vec<String> = Vec::new();
    let mut all_folders = true;
    let (mut admin, mut not_yet) = (Vec::new(), Vec::new());
    let mut stopped = false;
    for r in results {
        match &r.outcome {
            Outcome::Cleaned {
                refused, entries, ..
            } => {
                left_alone += refused.len();
                let left: Vec<_> = entries.iter().filter(|e| e.left).collect();
                all_folders &= left.len() == refused.len() && left.iter().all(|e| e.is_dir);
                if left.is_empty() {
                    why_left.extend(refused.iter().map(|(what, why)| format!("{what}: {why}")));
                } else {
                    why_left.extend(left.iter().filter_map(|e| e.reason.clone()));
                }
            }
            Outcome::NeedsAdmin => admin.push(r.title),
            Outcome::NotYet => not_yet.push(r.title),
            Outcome::Stopped => stopped = true,
        }
    }
    let skipped = admin.len() + not_yet.len();
    if results.is_empty() {
        return Finish {
            all_done: false,
            title: "Nothing was cleaned".into(),
            text: String::new(),
            retry_label: String::new(),
            can_retry: false,
        };
    }
    if stopped {
        return Finish {
            all_done: false,
            title: "Stopped early".into(),
            text: "Tidewise finished the group it was working on and stopped there. Everything removed so far is listed in History; the rest is untouched.".into(),
            retry_label: "Clean the rest".into(),
            can_retry: true,
        };
    }
    if left_alone == 0 && skipped == 0 {
        let text = if results.len() == 1 {
            "The group was cleaned.".to_string()
        } else {
            format!("All {} groups were cleaned.", results.len())
        };
        return Finish {
            all_done: true,
            title: format!("{} freed", format_bytes(freed)),
            text,
            retry_label: String::new(),
            can_retry: false,
        };
    }
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut title = format!("{} freed", format_bytes(freed));
    if left_alone > 0 {
        title += &format!(
            " · {} left",
            tidewise_adapters::cleanup::left_words(left_alone, all_folders)
        );
    }
    if skipped > 0 {
        title += &format!(", {} skipped", plural(skipped, "group", "groups"));
    }
    let mut text = Vec::new();
    if left_alone > 0 {
        let shown: Vec<_> = why_left.iter().take(3).cloned().collect();
        let more = why_left.len().saturating_sub(shown.len());
        text.push(format!(
            "Tidewise could not clean {}: {}{}. A folder that was left may be partly emptied; History lists exactly what went and what stayed.",
            tidewise_adapters::cleanup::left_words(left_alone, all_folders),
            shown.join("; "),
            if more > 0 { format!("; and {more} more") } else { String::new() },
        ));
    }
    if !admin.is_empty() {
        text.push(format!(
            "{} {} skipped because {} admin access, which Tidewise does not have yet (coming soon).",
            join_names(&admin),
            if admin.len() == 1 { "was" } else { "were" },
            if admin.len() == 1 { "it needs" } else { "they need" },
        ));
    }
    if !not_yet.is_empty() {
        text.push(format!(
            "Tidewise cannot remove {} yet.",
            join_names(&not_yet)
        ));
    }
    if left_alone == 0 {
        text.push("Nothing went wrong, and nothing was half-removed.".into());
    }
    Finish {
        all_done: false,
        title,
        text: text.join(" "),
        retry_label: String::new(),
        can_retry: false,
    }
}

/// `A`, `A and B`, `A, B and C`.
fn join_names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_string(),
        [first @ .., last] => format!("{} and {last}", first.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewise_adapters::cleanup::{groups, Safety};
    use tidewise_adapters::junk::{JunkItem, JunkKind, JunkReport};

    fn item(kind: JunkKind, label: &str, bytes: u64, admin: bool) -> JunkItem {
        JunkItem {
            kind,
            label: label.into(),
            path: Some(format!("/x/{label}").into()),
            bytes,
            needs_admin: admin,
        }
    }

    /// The numbers of the Cleaner artboard.
    fn design() -> Vec<Group> {
        groups(&JunkReport {
            items: vec![
                item(
                    JunkKind::PackageCache,
                    "Package cache (pacman)",
                    6_800_000_000,
                    true,
                ),
                item(JunkKind::AppCache, "a", 3_100_000_000, false),
                item(
                    JunkKind::Journal,
                    "System log (journal)",
                    1_200_000_000,
                    true,
                ),
                item(JunkKind::FlatpakRuntime, "r", 900_000_000, false),
                item(JunkKind::Trash, "Trash", 400_000_000, false),
            ],
            skipped: vec![],
        })
    }

    fn safe_ids(g: &[Group]) -> Vec<GroupId> {
        g.iter()
            .filter(|g| g.safety == Safety::Safe)
            .map(|g| g.id)
            .collect()
    }

    #[test]
    fn rows_say_what_the_design_says() {
        let g = design();
        let r = rows(&g, &safe_ids(&g));
        let got: Vec<_> = r
            .iter()
            .map(|r| {
                (
                    r.title.as_str(),
                    r.safe.as_str(),
                    r.admin,
                    r.size.as_str(),
                    r.checked,
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("Old package downloads", "Safe", true, "6.8 GB", true),
                ("App caches", "Safe", false, "3.1 GB", true),
                ("Old system logs", "Safe", true, "1.2 GB", true),
                ("Unused Flatpak parts", "Safe", false, "900 MB", true),
                ("Trash", "Review", false, "400 MB", false),
            ]
        );
        assert!(r[4].review && !r[0].review);
        assert!(r[1]
            .description
            .starts_with("Temporary data like thumbnails"));
    }

    #[test]
    fn summary_adds_up_the_ticked_groups() {
        let g = design();
        let s = summary(&g, &safe_ids(&g));
        assert_eq!(s.total, "12.0 GB");
        assert_eq!(s.found, "of 12.4 GB found · 4 of 5 groups");
        assert_eq!(s.count, 4);
        assert!(s.admin);
        assert!((s.fraction - 12.0 / 12.4).abs() < 0.001, "{}", s.fraction);
    }

    #[test]
    fn nothing_ticked_is_zero_and_asks_for_no_password() {
        let g = design();
        let s = summary(&g, &[]);
        assert_eq!(
            (s.total.as_str(), s.count, s.admin, s.fraction),
            ("0 B", 0, false, 0.0)
        );
    }

    #[test]
    fn only_user_groups_ticked_means_no_password_note() {
        let g = design();
        let s = summary(&g, &[GroupId::AppCaches, GroupId::Trash]);
        assert!(!s.admin);
        assert_eq!(s.total, "3.5 GB");
    }

    #[test]
    fn summary_of_nothing_found() {
        let s = summary(&[], &[]);
        assert_eq!(
            (s.found.as_str(), s.fraction),
            ("of 0 B found · 0 of 0 groups", 0.0)
        );
    }

    #[test]
    fn the_confirm_dialog_lists_exactly_the_ticked_groups() {
        let g = design();
        let c = confirm(&g, &safe_ids(&g));
        assert_eq!(c.title, "Remove 12.0 GB?");
        let rows: Vec<_> = c
            .rows
            .iter()
            .map(|r| (r.name.as_str(), r.size.as_str(), r.admin))
            .collect();
        assert_eq!(
            rows,
            [
                ("Old package downloads", "6.8 GB", true),
                ("App caches", "3.1 GB", false),
                ("Old system logs", "1.2 GB", true),
                ("Unused Flatpak parts", "0.9 GB", false),
            ]
        );
        assert_eq!(
            c.admin_note.as_deref(),
            Some("2 of these need your password. You'll be asked once.")
        );
    }

    #[test]
    fn one_admin_group_reads_in_the_singular() {
        let g = design();
        let c = confirm(&g, &[GroupId::SystemLogs, GroupId::AppCaches]);
        assert_eq!(
            c.admin_note.as_deref(),
            Some("1 of these needs your password. You'll be asked once.")
        );
        assert_eq!(confirm(&g, &[GroupId::AppCaches]).admin_note, None);
    }

    #[test]
    fn toggling_ticks_and_unticks() {
        let mut sel = vec![GroupId::AppCaches];
        toggle(&mut sel, GroupId::Trash);
        assert_eq!(sel, [GroupId::AppCaches, GroupId::Trash]);
        toggle(&mut sel, GroupId::AppCaches);
        assert_eq!(sel, [GroupId::Trash]);
    }

    fn result(title: &'static str, outcome: Outcome) -> StepResult {
        StepResult {
            group: GroupId::AppCaches,
            title,
            outcome,
        }
    }

    fn cleaned(freed: u64) -> Outcome {
        Outcome::Cleaned {
            freed,
            removed: vec!["x".into()],
            refused: vec![],
            entries: vec![],
        }
    }

    #[test]
    fn everything_cleaned_is_the_done_dialog() {
        let f = finish(&[
            result("App caches", cleaned(3_100_000_000)),
            result("Trash", cleaned(400_000_000)),
        ]);
        assert!(f.all_done);
        assert_eq!(f.title, "3.5 GB freed");
        assert_eq!(f.text, "All 2 groups were cleaned.");
    }

    #[test]
    fn one_group_reads_in_the_singular() {
        let f = finish(&[result("Trash", cleaned(400_000_000))]);
        assert_eq!(f.text, "The group was cleaned.");
    }

    #[test]
    fn admin_steps_that_could_not_run_are_named_and_not_retryable_yet() {
        let f = finish(&[
            result("Old package downloads", Outcome::NeedsAdmin),
            result("App caches", cleaned(3_100_000_000)),
            result("Old system logs", Outcome::NeedsAdmin),
        ]);
        assert!(!f.all_done);
        assert_eq!(f.title, "3.1 GB freed, 2 groups skipped");
        assert!(f
            .text
            .starts_with("Old package downloads and Old system logs were skipped"));
        assert!(f.text.contains("admin access"));
        assert!(f
            .text
            .ends_with("Nothing went wrong, and nothing was half-removed."));
        assert!(!f.can_retry);
    }

    #[test]
    fn a_stopped_run_offers_to_clean_the_rest() {
        let f = finish(&[
            result("App caches", cleaned(3_100_000_000)),
            result("Trash", Outcome::Stopped),
        ]);
        assert!(!f.all_done);
        assert_eq!(f.title, "Stopped early");
        assert!(f
            .text
            .starts_with("Tidewise finished the group it was working on and stopped there."));
        assert_eq!(f.retry_label, "Clean the rest");
        assert!(f.can_retry);
    }

    #[test]
    fn items_left_alone_at_the_last_check_stop_it_from_saying_all_cleaned() {
        let f = finish(&[result(
            "App caches",
            Outcome::Cleaned {
                freed: 1_000_000,
                removed: vec!["a".into()],
                refused: vec![("b".into(), "The app is running right now".into())],
                entries: vec![],
            },
        )]);
        assert!(!f.all_done);
        assert_eq!(f.title, "1 MB freed · 1 item left");
        assert!(
            f.text.contains("b: The app is running right now"),
            "{}",
            f.text
        );
    }

    #[test]
    fn a_folder_that_could_not_be_removed_is_named_with_its_reason() {
        let why = "read-only folder: ~/.cache/dotslash";
        let f = finish(&[result(
            "App caches",
            Outcome::Cleaned {
                freed: 19_500_000_000,
                removed: vec!["a".into()],
                refused: vec![("dotslash".into(), why.into())],
                entries: vec![tidewise_adapters::cleanup::Entry {
                    path: "/home/u/.cache/dotslash".into(),
                    bytes: 0,
                    left: true,
                    reason: Some(why.into()),
                    is_dir: true,
                }],
            },
        )]);
        assert!(!f.all_done);
        assert_eq!(f.title, "19.5 GB freed · 1 folder left");
        assert!(f.text.contains(why), "{}", f.text);
        assert!(!f.text.contains("changed after the scan"), "{}", f.text);
    }

    #[test]
    fn nothing_found_says_so_calmly_and_offers_to_look_again() {
        let e = empty_state();
        assert_eq!(e.title, "Already tidy");
        assert_eq!(
            e.text,
            "Nothing worth cleaning right now. Junk builds up slowly; check back in a few weeks."
        );
        assert_eq!(e.action, "Check again");
    }

    #[test]
    fn nothing_selected_or_nothing_removed_does_not_claim_success() {
        assert!(!finish(&[]).all_done);
        let f = finish(&[result("Old system logs", Outcome::NeedsAdmin)]);
        assert_eq!(f.title, "0 B freed, 1 group skipped");
    }
}
