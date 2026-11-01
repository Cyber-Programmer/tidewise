//! Clean up: turns the junk report into the six design groups, builds the exact list the confirm
//! screen shows, and runs the parts that need no admin access (app caches and the Trash).
//! Everything that needs root, and Flatpak runtimes, is reported as skipped, never attempted here.
//!
//! Golden rules: only caches and the Trash are removed directly, only after the confirm screen, and
//! each path is re-checked right before it is removed (still where it was, not a link, not in use).

use std::path::PathBuf;

use tidewise_core::history;

use crate::junk::{JunkItem, JunkReport};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupId {
    PackageDownloads,
    AppCaches,
    SystemLogs,
    FlatpakParts,
    Trash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Safety {
    Safe,
    Review,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub id: GroupId,
    pub title: &'static str,
    pub description: String,
    pub safety: Safety,
    /// Some of it needs admin access.
    pub admin: bool,
    pub bytes: u64,
    pub items: Vec<JunkItem>,
    /// The path or command, shown behind "Show file paths & commands".
    pub detail: String,
}

/// One group the user picked, as it will run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub group: GroupId,
    pub title: &'static str,
    pub admin: bool,
    pub bytes: u64,
    pub items: Vec<JunkItem>,
}

/// What a step did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Cleaned {
        freed: u64,
        removed: Vec<String>,
        /// Items left alone at the last check, with the reason.
        refused: Vec<(String, String)>,
    },
    /// Needs the admin helper, which is not built yet. Nothing was changed.
    NeedsAdmin,
    /// Tidewise cannot do this one yet. Nothing was changed.
    NotYet,
    /// The user stopped before this step. Nothing was changed.
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepResult {
    pub group: GroupId,
    pub title: &'static str,
    pub outcome: Outcome,
}

/// What the last check compares against.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub cache_root: PathBuf,
    pub trash_root: PathBuf,
    /// Names of running programs right now (see `junk::live_sources`).
    pub running: Vec<String>,
}

pub fn groups(report: &JunkReport) -> Vec<Group> {
    let _ = report;
    Vec::new()
}

/// Safe groups on, Review groups off.
pub fn default_selection(groups: &[Group]) -> Vec<GroupId> {
    let _ = groups;
    Vec::new()
}

pub fn plan(groups: &[Group], selected: &[GroupId]) -> Vec<Step> {
    let _ = (groups, selected);
    Vec::new()
}

pub fn run_step(step: &Step, env: &Env) -> StepResult {
    StepResult {
        group: step.group,
        title: step.title,
        outcome: Outcome::Stopped,
    }
}

/// Run the steps in order. `stop` is asked after every step ("stop after this step"); the steps
/// after it come back as `Stopped`. `progress` is called before each step with its index.
pub fn run(
    steps: &[Step],
    env: &Env,
    stop: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(usize, &Step),
) -> Vec<StepResult> {
    let _ = (env, stop, progress);
    steps
        .iter()
        .map(|s| StepResult {
            group: s.group,
            title: s.title,
            outcome: Outcome::Stopped,
        })
        .collect()
}

pub fn freed_bytes(results: &[StepResult]) -> u64 {
    let _ = results;
    0
}

/// The History record for a finished run.
pub fn record(results: &[StepResult], time: u64, id: &str) -> history::Record {
    history::Record {
        version: history::VERSION,
        id: id.into(),
        time,
        kind: history::Kind::CleanUp,
        title: String::new(),
        freed_bytes: 0,
        steps: results.iter().map(|_| unreachable_step()).collect(),
    }
}

fn unreachable_step() -> history::Step {
    history::Step {
        title: String::new(),
        status: history::Status::NotRun,
        bytes: 0,
        items: Vec::new(),
        note: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::junk::JunkKind;
    use std::fs;
    use std::path::Path;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![1u8; bytes]).unwrap();
    }

    fn item(kind: JunkKind, label: &str, path: Option<&Path>, bytes: u64, admin: bool) -> JunkItem {
        JunkItem {
            kind,
            label: label.into(),
            path: path.map(Path::to_path_buf),
            bytes,
            needs_admin: admin,
        }
    }

    /// A temp home with two caches, a Trash with one file, and the report that lists them.
    struct World {
        _tmp: tempfile::TempDir,
        env: Env,
        report: JunkReport,
    }

    fn world() -> World {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("home/.cache");
        let trash = tmp.path().join("home/.local/share/Trash");
        write(&cache.join("alpha/a.bin"), 2000);
        write(&cache.join("beta/b.bin"), 3000);
        write(&cache.join("keep-me/k.bin"), 4000);
        write(&trash.join("files/old.iso"), 5000);
        write(&trash.join("info/old.iso.trashinfo"), 50);
        let pkgs = tmp.path().join("var/pacman");
        write(&pkgs.join("x.pkg.tar.zst"), 6000);
        let report = JunkReport {
            items: vec![
                item(
                    JunkKind::PackageCache,
                    "Package cache (pacman)",
                    Some(&pkgs),
                    6_800_000_000,
                    true,
                ),
                item(
                    JunkKind::AppCache,
                    "alpha",
                    Some(&cache.join("alpha")),
                    2000,
                    false,
                ),
                item(
                    JunkKind::AppCache,
                    "beta",
                    Some(&cache.join("beta")),
                    3000,
                    false,
                ),
                item(
                    JunkKind::Journal,
                    "System log (journal)",
                    None,
                    1_200_000_000,
                    true,
                ),
                item(
                    JunkKind::FlatpakRuntime,
                    "org.kde.Platform 6.7",
                    None,
                    900_000_000,
                    false,
                ),
                item(JunkKind::Trash, "Trash", Some(&trash), 5050, false),
            ],
            skipped: vec![],
        };
        World {
            env: Env {
                cache_root: cache,
                trash_root: trash,
                running: vec![],
            },
            report,
            _tmp: tmp,
        }
    }

    fn select(w: &World, ids: &[GroupId]) -> Vec<Step> {
        plan(&groups(&w.report), ids)
    }

    fn cleaned(r: &StepResult) -> (u64, &Vec<String>, &Vec<(String, String)>) {
        match &r.outcome {
            Outcome::Cleaned {
                freed,
                removed,
                refused,
            } => (*freed, removed, refused),
            other => panic!("{} was {other:?}", r.title),
        }
    }

    #[test]
    fn groups_follow_the_design_order_with_sums_and_flags() {
        let w = world();
        let g = groups(&w.report);
        let got: Vec<_> = g
            .iter()
            .map(|g| (g.id, g.title, g.bytes, g.admin, g.safety))
            .collect();
        assert_eq!(
            got,
            [
                (
                    GroupId::PackageDownloads,
                    "Old package downloads",
                    6_800_000_000,
                    true,
                    Safety::Safe
                ),
                (GroupId::AppCaches, "App caches", 5000, false, Safety::Safe),
                (
                    GroupId::SystemLogs,
                    "Old system logs",
                    1_200_000_000,
                    true,
                    Safety::Safe
                ),
                (
                    GroupId::FlatpakParts,
                    "Unused Flatpak parts",
                    900_000_000,
                    false,
                    Safety::Safe
                ),
                (GroupId::Trash, "Trash", 5050, false, Safety::Review),
            ]
        );
        assert_eq!(g[1].items.len(), 2);
        assert!(g[0].description.contains("2 newest"));
        assert!(g[1].detail.contains("~/.cache") || g[1].detail.contains(".cache"));
        assert_eq!(g[2].detail, "journalctl --vacuum-time=2weeks");
        assert_eq!(g[3].detail, "flatpak uninstall --unused");
    }

    #[test]
    fn groups_with_nothing_in_them_are_left_out() {
        let g = groups(&JunkReport::default());
        assert!(g.is_empty());
    }

    #[test]
    fn safe_groups_start_selected_and_review_groups_do_not() {
        let w = world();
        assert_eq!(
            default_selection(&groups(&w.report)),
            [
                GroupId::PackageDownloads,
                GroupId::AppCaches,
                GroupId::SystemLogs,
                GroupId::FlatpakParts
            ]
        );
    }

    #[test]
    fn the_plan_is_the_selected_groups_in_design_order() {
        let w = world();
        let steps = select(&w, &[GroupId::Trash, GroupId::AppCaches]);
        let ids: Vec<_> = steps.iter().map(|s| s.group).collect();
        assert_eq!(ids, [GroupId::AppCaches, GroupId::Trash]);
        assert_eq!(steps[0].bytes, 5000);
        assert_eq!(steps[0].items.len(), 2);
        assert!(select(&w, &[]).is_empty());
    }

    #[test]
    fn app_caches_are_removed_and_nothing_else() {
        let w = world();
        let results = run(
            &select(&w, &[GroupId::AppCaches]),
            &w.env,
            &|| false,
            &mut |_, _| {},
        );
        let (freed, removed, refused) = cleaned(&results[0]);
        assert_eq!(freed, 5000);
        assert_eq!(removed.len(), 2);
        assert!(refused.is_empty());
        assert!(!w.env.cache_root.join("alpha").exists());
        assert!(!w.env.cache_root.join("beta").exists());
        assert!(
            w.env.cache_root.join("keep-me/k.bin").exists(),
            "not on the list, not touched"
        );
        assert!(w.env.cache_root.exists());
        assert!(w.env.trash_root.join("files/old.iso").exists());
    }

    #[test]
    fn a_cache_swapped_for_a_link_after_the_scan_is_refused() {
        let w = world();
        let outside = w.env.cache_root.parent().unwrap().join("precious");
        write(&outside.join("doc.txt"), 100);
        fs::remove_dir_all(w.env.cache_root.join("alpha")).unwrap();
        std::os::unix::fs::symlink(&outside, w.env.cache_root.join("alpha")).unwrap();
        let results = run(
            &select(&w, &[GroupId::AppCaches]),
            &w.env,
            &|| false,
            &mut |_, _| {},
        );
        let (freed, removed, refused) = cleaned(&results[0]);
        assert_eq!(removed.len(), 1);
        assert_eq!(freed, 3000);
        assert_eq!(refused.len(), 1);
        assert!(refused[0].1.contains("link"));
        assert!(outside.join("doc.txt").exists());
    }

    #[test]
    fn a_cache_whose_app_started_after_the_scan_is_left_alone() {
        let mut w = world();
        w.env.running = vec!["beta".into()];
        let results = run(
            &select(&w, &[GroupId::AppCaches]),
            &w.env,
            &|| false,
            &mut |_, _| {},
        );
        let (_, removed, refused) = cleaned(&results[0]);
        assert_eq!(removed.len(), 1);
        assert!(refused[0].1.contains("running"));
        assert!(w.env.cache_root.join("beta/b.bin").exists());
    }

    #[test]
    fn paths_outside_the_cache_folder_are_refused_even_if_listed() {
        let w = world();
        let victim = w._tmp.path().join("home/Documents");
        write(&victim.join("thesis.odt"), 100);
        let hostile = [
            victim.clone(),
            w.env.cache_root.clone(),
            w.env.cache_root.join(".."),
            w.env.cache_root.join("alpha/a.bin"),
            w.env.cache_root.join("../Documents"),
            PathBuf::from("/"),
            PathBuf::new(),
        ];
        for path in hostile {
            let step = Step {
                group: GroupId::AppCaches,
                title: "App caches",
                admin: false,
                bytes: 1,
                items: vec![item(JunkKind::AppCache, "x", Some(&path), 1, false)],
            };
            let r = run_step(&step, &w.env);
            let (freed, removed, refused) = cleaned(&r);
            assert_eq!((freed, removed.len(), refused.len()), (0, 0, 1), "{path:?}");
        }
        assert!(victim.join("thesis.odt").exists());
        assert!(w.env.cache_root.join("alpha/a.bin").exists());
    }

    #[test]
    fn the_trash_is_emptied_but_its_folders_stay() {
        let w = world();
        let results = run(
            &select(&w, &[GroupId::Trash]),
            &w.env,
            &|| false,
            &mut |_, _| {},
        );
        let (freed, removed, _) = cleaned(&results[0]);
        assert_eq!(freed, 5050);
        assert!(!removed.is_empty());
        assert!(w.env.trash_root.join("files").is_dir());
        assert!(w.env.trash_root.join("info").is_dir());
        assert_eq!(
            fs::read_dir(w.env.trash_root.join("files"))
                .unwrap()
                .count(),
            0
        );
        assert_eq!(
            fs::read_dir(w.env.trash_root.join("info")).unwrap().count(),
            0
        );
    }

    #[test]
    fn a_trash_folder_that_is_not_the_users_trash_is_refused() {
        let w = world();
        let other = w._tmp.path().join("home/Documents");
        write(&other.join("files/thesis.odt"), 100);
        let step = Step {
            group: GroupId::Trash,
            title: "Trash",
            admin: false,
            bytes: 1,
            items: vec![item(JunkKind::Trash, "Trash", Some(&other), 1, false)],
        };
        let (freed, removed, refused) = {
            let r = run_step(&step, &w.env);
            let c = cleaned(&r);
            (c.0, c.1.len(), c.2.len())
        };
        assert_eq!((freed, removed, refused), (0, 0, 1));
        assert!(other.join("files/thesis.odt").exists());
    }

    #[test]
    fn admin_and_flatpak_steps_change_nothing() {
        let w = world();
        let pkgs = w.report.items[0].path.clone().unwrap();
        let results = run(
            &select(
                &w,
                &[
                    GroupId::PackageDownloads,
                    GroupId::SystemLogs,
                    GroupId::FlatpakParts,
                ],
            ),
            &w.env,
            &|| false,
            &mut |_, _| {},
        );
        let outcomes: Vec<_> = results.iter().map(|r| r.outcome.clone()).collect();
        assert_eq!(
            outcomes,
            [Outcome::NeedsAdmin, Outcome::NeedsAdmin, Outcome::NotYet]
        );
        assert!(pkgs.join("x.pkg.tar.zst").exists());
    }

    #[test]
    fn stopping_lets_the_current_step_finish_and_skips_the_rest() {
        let w = world();
        let steps = select(&w, &[GroupId::AppCaches, GroupId::Trash]);
        let asked = std::cell::Cell::new(0);
        let mut seen = Vec::new();
        let results = run(
            &steps,
            &w.env,
            &|| {
                asked.set(asked.get() + 1);
                true
            },
            &mut |i, s| seen.push((i, s.group)),
        );
        assert_eq!(cleaned(&results[0]).0, 5000);
        assert_eq!(results[1].outcome, Outcome::Stopped);
        assert_eq!(
            seen,
            [(0, GroupId::AppCaches)],
            "the second step never started"
        );
        assert!(w.env.trash_root.join("files/old.iso").exists());
    }

    #[test]
    fn the_record_lists_what_was_removed_and_what_was_not() {
        let w = world();
        let steps = select(
            &w,
            &[GroupId::AppCaches, GroupId::SystemLogs, GroupId::Trash],
        );
        let results = run(&steps, &w.env, &|| false, &mut |_, _| {});
        assert_eq!(freed_bytes(&results), 5000 + 5050);
        let rec = record(&results, 1_700_000_000, "1700000000-clean_up");
        assert_eq!(rec.kind, history::Kind::CleanUp);
        assert_eq!(rec.freed_bytes, 10_050);
        assert_eq!(rec.time, 1_700_000_000);
        let statuses: Vec<_> = rec.steps.iter().map(|s| s.status).collect();
        assert_eq!(
            statuses,
            [
                history::Status::Done,
                history::Status::Skipped,
                history::Status::Done
            ]
        );
        assert_eq!(rec.steps[0].title, "App caches");
        assert_eq!(rec.steps[0].bytes, 5000);
        assert_eq!(rec.steps[0].items.len(), 2);
        assert!(rec.steps[0].items[0].contains(".cache"));
        assert!(rec.steps[1].note.as_deref().unwrap().contains("admin"));
        assert!(
            rec.title.contains("10.1 GB")
                || rec.title.contains("10 KB")
                || rec.title.contains("freed")
        );
    }

    #[test]
    fn a_stopped_run_is_recorded_as_not_run() {
        let w = world();
        let steps = select(&w, &[GroupId::AppCaches, GroupId::Trash]);
        let results = run(&steps, &w.env, &|| true, &mut |_, _| {});
        let rec = record(&results, 5, "5-clean_up");
        assert_eq!(rec.steps[1].status, history::Status::NotRun);
        assert_eq!(rec.steps[1].bytes, 0);
    }
}
