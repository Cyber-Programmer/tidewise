//! Clean up screen: what the rows, the summary and the confirm dialog say, from the groups the
//! junk scanner found and the ones the user ticked. Plain data; `view.rs` copies it into Slint.

use tidewise_adapters::cleanup::{format_bytes, Group, GroupId, Safety};

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
}
