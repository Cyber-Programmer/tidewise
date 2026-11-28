//! Tests of the resolver engine. `World` is a small pretend repository that answers like pacman does where it
//! matters here: a dry run takes the first provider and lists dependencies first, an installed package satisfies a
//! dependency by itself, and a target pacman does not know is "not found". What real pacman does is pinned by the
//! fixtures (`Scripted` tests below) and by the container tests; this file tests the logic on top.

use std::cell::RefCell;

use super::*;
use crate::{
    Comparison, Dependency, InstallReason, PackageDetails, VersionRule, parse_print, parse_si,
};

// ---- the pretend repository -----------------------------------------------------------------------------------

#[derive(Clone)]
struct Pkg {
    repo: &'static str,
    name: String,
    version: String,
    provides: Vec<String>,
    depends: Vec<String>,
    groups: Vec<String>,
    replaces: Vec<String>,
}

#[derive(Default)]
struct World {
    pkgs: Vec<Pkg>,
    /// (name, version, provides)
    installed: Vec<(String, String, Vec<String>)>,
    /// (name, ignored)
    pending: Vec<(String, bool)>,
    /// Every dry run asked: (update, targets).
    runs: RefCell<Vec<(bool, Vec<String>)>>,
}

impl World {
    fn add(&mut self, name: &str, provides: &[&str], depends: &[&str]) -> &mut Self {
        self.pkgs.push(Pkg {
            repo: "test",
            name: name.to_string(),
            version: "1.0-1".to_string(),
            provides: provides.iter().map(|s| s.to_string()).collect(),
            depends: depends.iter().map(|s| s.to_string()).collect(),
            groups: Vec::new(),
            replaces: Vec::new(),
        });
        self
    }

    /// The last package added replaces `old`.
    fn replacing(&mut self, old: &str) -> &mut Self {
        if let Some(pkg) = self.pkgs.last_mut() {
            pkg.replaces.push(old.to_string());
        }
        self
    }

    fn group(&mut self, group: &str, members: &[&str]) -> &mut Self {
        for pkg in &mut self.pkgs {
            if members.contains(&pkg.name.as_str()) {
                pkg.groups.push(group.to_string());
            }
        }
        self
    }

    fn install(&mut self, name: &str, version: &str, provides: &[&str]) -> &mut Self {
        self.installed.push((
            name.to_string(),
            version.to_string(),
            provides.iter().map(|s| s.to_string()).collect(),
        ));
        self
    }

    fn pend(&mut self, name: &str, ignored: bool) -> &mut Self {
        self.pending.push((name.to_string(), ignored));
        self
    }

    /// The packages of the container tests (`tests/container/pkgs`), under shorter names.
    fn base() -> World {
        let mut w = World::default();
        w.add("hello", &[], &[])
            .add("lib", &[], &[])
            .add("app", &[], &["lib"])
            .add("prov-a", &["virt"], &[])
            .add("prov-b", &["virt"], &[])
            .add("needs-virt", &[], &["virt"])
            .add("vprov-1", &["vvirt=1"], &[])
            .add("vprov-2", &["vvirt=2"], &[])
            .add("vprov-3", &["vvirt=3"], &[])
            .add("needs-vvirt", &[], &["vvirt>=2"])
            .add("one-prov", &["single"], &[])
            .add("needs-one", &[], &["single"])
            .add("stale-prov", &["stale=1"], &[])
            .add("needs-stale", &[], &["stale>=2"])
            .add("chain-a", &["chain"], &["chain-b"])
            .add("chain-b", &["chain"], &[])
            .add("chain-c", &["chain"], &[])
            .add("needs-chain", &[], &["chain"])
            .add("zlib", &["libz.so=1"], &[])
            .add("needs-soname", &[], &["libz.so=1"]);
        w
    }

    fn satisfies(dependency: &str, name: &str, provides: &[String]) -> bool {
        let (dep_name, rule) = split_dependency(dependency);
        if dep_name == name && rule.is_none() {
            return true;
        }
        if dep_name == name {
            // A rule on a real name: not modelled, the packages here are all version 1.
            return true;
        }
        provides.iter().any(|provided| {
            let (provided_name, version) = provided
                .split_once('=')
                .map_or((provided.as_str(), None), |(n, v)| (n, Some(v)));
            if provided_name != dep_name {
                return false;
            }
            match (rule, version) {
                (None, _) => true,
                (Some(_), None) => false,
                (Some((op, wanted)), Some(have)) => {
                    let (have, wanted): (u32, u32) =
                        (have.parse().unwrap(), wanted.parse().unwrap());
                    match op {
                        "=" => have == wanted,
                        ">=" => have >= wanted,
                        ">" => have > wanted,
                        "<=" => have <= wanted,
                        _ => have < wanted,
                    }
                }
            }
        })
    }

    fn find(&self, target: &str) -> Option<usize> {
        match target.split_once('/') {
            Some((repo, name)) => self
                .pkgs
                .iter()
                .position(|p| p.repo == repo && p.name == name),
            None => self
                .pkgs
                .iter()
                .position(|p| Self::satisfies(target, &p.name, &p.provides)),
        }
    }

    /// Adds `index` after its dependencies. All targets are in the transaction before any dependency is resolved
    /// (the container fixtures `print-b-vprov-*-with-hello` show it): a dependency that a target satisfies is
    /// satisfied, and that target is added first.
    fn add_to_list(
        &self,
        index: usize,
        list: &mut Vec<usize>,
        targets: &[usize],
        unsatisfied: &mut Vec<(String, String)>,
    ) {
        if list.contains(&index) {
            return;
        }
        let pkg = &self.pkgs[index];
        for dependency in &pkg.depends {
            let satisfied_by = |i: &usize| {
                Self::satisfies(dependency, &self.pkgs[*i].name, &self.pkgs[*i].provides)
            };
            if list.iter().any(satisfied_by) {
                continue;
            }
            let installed = self
                .installed
                .iter()
                .any(|(name, _, provides)| Self::satisfies(dependency, name, provides));
            if installed {
                continue;
            }
            if let Some(target) = targets
                .iter()
                .copied()
                .find(|i| *i != index && satisfied_by(i))
            {
                self.add_to_list(target, list, targets, unsatisfied);
                continue;
            }
            match self
                .pkgs
                .iter()
                .position(|p| Self::satisfies(dependency, &p.name, &p.provides))
            {
                Some(provider) => self.add_to_list(provider, list, targets, unsatisfied),
                None => unsatisfied.push((dependency.clone(), pkg.name.clone())),
            }
        }
        list.push(index);
    }
}

fn split_dependency(dependency: &str) -> (&str, Option<(&str, &str)>) {
    match dependency.find(['<', '>', '=']) {
        None => (dependency, None),
        Some(at) => {
            let (name, rule) = dependency.split_at(at);
            let version_at = rule.find(|c: char| !"<>=".contains(c)).unwrap();
            let (op, version) = rule.split_at(version_at);
            (name, Some((op, version)))
        }
    }
}

fn to_dependency(text: &str) -> Dependency {
    let (name, rule) = split_dependency(text);
    Dependency {
        name: name.to_string(),
        rule: rule.map(|(op, version)| VersionRule {
            comparison: match op {
                "=" => Comparison::Equal,
                ">=" => Comparison::GreaterOrEqual,
                ">" => Comparison::Greater,
                "<=" => Comparison::LessOrEqual,
                _ => Comparison::Less,
            },
            version: version.to_string(),
        }),
    }
}

impl Facts for World {
    fn dry_run(&self, update: bool, targets: &[String]) -> Result<DryRun, PacmanError> {
        self.runs.borrow_mut().push((update, targets.to_vec()));
        let mut list = Vec::new();
        let mut unsatisfied = Vec::new();
        let mut not_found = Vec::new();
        let mut wanted: Vec<String> = Vec::new();
        if update {
            wanted.extend(
                self.pending
                    .iter()
                    .filter(|(_, ignored)| !ignored)
                    .map(|(name, _)| name.clone()),
            );
        }
        wanted.extend(targets.iter().cloned());
        let mut indexes = Vec::new();
        for target in &wanted {
            match self.find(target) {
                Some(index) => indexes.push(index),
                None => not_found.push(target.rsplit('/').next().unwrap().to_string()),
            }
        }
        for &index in &indexes {
            self.add_to_list(index, &mut list, &indexes, &mut unsatisfied);
        }
        if !not_found.is_empty() {
            return Ok(DryRun::Failed(PrintFailure::NotFound(not_found)));
        }
        if !unsatisfied.is_empty() {
            return Ok(DryRun::Failed(PrintFailure::Unsatisfiable(
                unsatisfied
                    .into_iter()
                    .map(|(dependency, required_by)| crate::UnsatisfiedDependency {
                        dependency,
                        required_by,
                    })
                    .collect(),
            )));
        }
        Ok(DryRun::Listed(
            list.into_iter()
                .map(|i| PrintedPackage {
                    repo: self.pkgs[i].repo.to_string(),
                    name: self.pkgs[i].name.clone(),
                    version: if self.pending.iter().any(|(n, _)| *n == self.pkgs[i].name) {
                        "9-1".to_string()
                    } else {
                        self.pkgs[i].version.clone()
                    },
                    download_size: 1000,
                })
                .collect(),
        ))
    }

    fn details(&self, refs: &[PackageRef]) -> Result<DetailsLookup, PacmanError> {
        let mut found = Vec::new();
        let mut not_found = Vec::new();
        for item in refs {
            match self
                .pkgs
                .iter()
                .find(|p| p.repo == item.repo && p.name == item.name)
            {
                Some(p) => found.push(PackageDetails {
                    repo: p.repo.to_string(),
                    name: p.name.clone(),
                    version: p.version.clone(),
                    description: format!("{} for tests", p.name),
                    url: String::new(),
                    licenses: Vec::new(),
                    groups: p.groups.clone(),
                    provides: p.provides.iter().map(|t| to_dependency(t)).collect(),
                    depends_on: p.depends.iter().map(|t| to_dependency(t)).collect(),
                    optional_deps: Vec::new(),
                    conflicts_with: Vec::new(),
                    replaces: p.replaces.iter().map(|t| to_dependency(t)).collect(),
                    download_size: 1000,
                    download_size_text: "1000 B".to_string(),
                    installed_size: 2000,
                    installed_size_text: "2000 B".to_string(),
                }),
                None => not_found.push(item.clone()),
            }
        }
        Ok(DetailsLookup { found, not_found })
    }

    fn search_exact(&self, name: &str) -> Result<Vec<SearchResult>, PacmanError> {
        Ok(self
            .pkgs
            .iter()
            .filter(|p| Self::satisfies(name, &p.name, &p.provides) || p.name == name)
            .map(|p| SearchResult {
                repo: p.repo.to_string(),
                name: p.name.clone(),
                version: p.version.clone(),
                groups: p.groups.clone(),
                installed_version: None,
                description: format!("{} for tests", p.name),
            })
            .collect())
    }

    fn search_any(&self, names: &[String]) -> Result<Vec<SearchResult>, PacmanError> {
        let mut found = Vec::new();
        for name in names {
            found.extend(self.search_exact(name)?);
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        found.dedup_by(|a, b| a.name == b.name && a.repo == b.repo);
        Ok(found)
    }

    fn satisfied(&self, dependency: &Dependency) -> Result<bool, PacmanError> {
        Ok(self.installed.iter().any(|(name, _, provides)| {
            *name == dependency.name
                || provides
                    .iter()
                    .any(|p| to_dependency(p).name == dependency.name)
        }))
    }

    fn group_members(&self, group: &str) -> Result<Option<Vec<GroupMember>>, PacmanError> {
        let members: Vec<GroupMember> = self
            .pkgs
            .iter()
            .filter(|p| p.groups.iter().any(|g| g == group))
            .map(|p| GroupMember {
                group: group.to_string(),
                name: p.name.clone(),
            })
            .collect();
        Ok((!members.is_empty()).then_some(members))
    }

    fn installed(&self) -> Result<Vec<InstalledPackage>, PacmanError> {
        Ok(self
            .installed
            .iter()
            .map(|(name, version, _)| InstalledPackage {
                name: name.clone(),
                version: version.clone(),
                description: String::new(),
                installed_size: 500,
                reason: InstallReason::Explicit,
                required_by: Vec::new(),
            })
            .collect())
    }

    fn pending_updates(&self) -> Result<Vec<PendingUpdate>, PacmanError> {
        Ok(self
            .pending
            .iter()
            .map(|(name, ignored)| PendingUpdate {
                name: name.clone(),
                installed_version: "1.0-1".to_string(),
                new_version: "9-1".to_string(),
                ignored: *ignored,
            })
            .collect())
    }
}

// ---- helpers --------------------------------------------------------------------------------------------------

fn pr(name: &str) -> PackageRef {
    PackageRef {
        repo: "test".to_string(),
        name: name.to_string(),
    }
}

fn want(names: &[&str]) -> ResolveInput {
    ResolveInput {
        targets: names.iter().map(|n| pr(n)).collect(),
        ..ResolveInput::default()
    }
}

fn answer(dependency: &str, chosen: &str) -> ProviderAnswer {
    ProviderAnswer {
        dependency: dependency.to_string(),
        chosen: pr(chosen),
    }
}

fn plan(resolution: Result<Resolution, PacmanError>) -> InstallPlan {
    match resolution {
        Ok(Resolution::Ready(plan)) => plan,
        other => panic!("expected Ready, got {other:?}"),
    }
}

fn questions(resolution: Result<Resolution, PacmanError>) -> Vec<Question> {
    match resolution {
        Ok(Resolution::NeedsAnswers(questions)) => questions,
        other => panic!("expected NeedsAnswers, got {other:?}"),
    }
}

fn blockers(resolution: Result<Resolution, PacmanError>) -> Vec<Blocker> {
    match resolution {
        Ok(Resolution::Blocked(blockers)) => blockers,
        other => panic!("expected Blocked, got {other:?}"),
    }
}

fn item_names(plan: &InstallPlan) -> Vec<(&str, Role)> {
    plan.items
        .iter()
        .map(|item| (item.package.name.as_str(), item.role))
        .collect()
}

/// The dependency and the candidates' names of the only question.
fn the_provider_question(questions: &[Question]) -> (&str, Vec<&str>, usize) {
    match questions {
        [
            Question::ChooseProvider {
                dependency,
                candidates,
                default,
            },
        ] => (
            dependency.as_str(),
            candidates.iter().map(|c| c.package.name.as_str()).collect(),
            *default,
        ),
        other => panic!("expected one provider question, got {other:?}"),
    }
}

// ---- plain installs -------------------------------------------------------------------------------------------

#[test]
fn a_plain_package_is_ready_with_its_sizes() {
    let world = World::base();
    let plan = plan(resolve_with(&want(&["hello"]), &world));
    assert_eq!(item_names(&plan), [("hello", Role::Target)]);
    assert_eq!(plan.decision, InstallDecision::Allowed { ignored: 0 });
    assert_eq!(plan.targets, [pr("hello")]);
    assert!(plan.providers.is_empty());
    assert_eq!((plan.download_total, plan.installed_total), (1000, 2000));
}

#[test]
fn a_dependency_comes_first_and_is_marked_as_one() {
    let world = World::base();
    let plan = plan(resolve_with(&want(&["app"]), &world));
    assert_eq!(
        item_names(&plan),
        [("lib", Role::Dependency), ("app", Role::Target)]
    );
    assert_eq!((plan.download_total, plan.installed_total), (2000, 4000));
}

#[test]
fn nothing_to_install_is_blocked() {
    let world = World::base();
    assert_eq!(
        blockers(resolve_with(&ResolveInput::default(), &world)),
        [Blocker::NothingToInstall]
    );
}

#[test]
fn a_name_that_is_not_valid_is_refused_before_pacman_runs() {
    let world = World::base();
    let mut input = want(&["hello"]);
    input.targets[0].name = "--force".to_string();
    assert!(matches!(
        resolve_with(&input, &world),
        Err(PacmanError::BadPackageRef { .. })
    ));
    assert!(world.runs.borrow().is_empty());
}

#[test]
fn an_unknown_package_is_not_found() {
    let world = World::base();
    assert_eq!(
        blockers(resolve_with(&want(&["nosuch"]), &world)),
        [Blocker::NotFound(vec!["nosuch".to_string()])]
    );
}

// ---- stale lists: "Update system and install" -----------------------------------------------------------------

#[test]
fn pending_updates_turn_the_install_into_update_and_install() {
    let mut world = World::base();
    world.install("lib", "0.5-1", &[]).pend("lib", false);
    let plan = plan(resolve_with(&want(&["hello"]), &world));
    assert_eq!(
        plan.decision,
        InstallDecision::MustUpdateSystem {
            count: 1,
            ignored: 0
        }
    );
    assert_eq!(
        item_names(&plan),
        [("lib", Role::Update), ("hello", Role::Target)]
    );
    assert!(world.runs.borrow().iter().all(|(update, _)| *update));
}

#[test]
fn updates_held_back_by_ignorepkg_do_not_block_and_no_dry_run_updates() {
    let mut world = World::base();
    world.install("lib", "0.5-1", &[]).pend("lib", true);
    let plan = plan(resolve_with(&want(&["hello"]), &world));
    assert_eq!(plan.decision, InstallDecision::Allowed { ignored: 1 });
    assert!(world.runs.borrow().iter().all(|(update, _)| !*update));
}

#[test]
fn a_provider_question_during_update_and_install_runs_every_candidate_as_an_update_too() {
    let mut world = World::base();
    world.install("lib", "0.5-1", &[]).pend("lib", false);
    let found = questions(resolve_with(&want(&["needs-virt"]), &world));
    assert_eq!(
        the_provider_question(&found),
        ("virt", vec!["prov-a", "prov-b"], 0)
    );
    assert!(world.runs.borrow().iter().all(|(update, _)| *update));
}

#[test]
fn an_installed_package_that_is_being_updated_is_not_a_provider_choice() {
    // `bash`-like: installed, provides what the target needs, and has an update in the list.
    let mut world = World::base();
    world
        .add("bash-like", &["virt"], &[])
        .install("bash-like", "0.5-1", &["virt"])
        .pend("bash-like", false);
    let plan = plan(resolve_with(&want(&["needs-virt"]), &world));
    assert!(plan.providers.is_empty());
    assert!(
        item_names(&plan).contains(&("bash-like", Role::Update)),
        "{:?}",
        item_names(&plan)
    );
}

// ---- providers ------------------------------------------------------------------------------------------------

#[test]
fn a_single_provider_is_named_and_never_asked() {
    let world = World::base();
    let plan = plan(resolve_with(&want(&["needs-one"]), &world));
    assert_eq!(plan.providers, [answer("single", "one-prov")]);
    assert_eq!(
        item_names(&plan),
        [("one-prov", Role::Provider), ("needs-one", Role::Target)]
    );
}

#[test]
fn several_providers_are_a_question_with_pacmans_default_marked() {
    let world = World::base();
    let found = questions(resolve_with(&want(&["needs-virt"]), &world));
    assert_eq!(
        the_provider_question(&found),
        ("virt", vec!["prov-a", "prov-b"], 0)
    );
}

#[test]
fn an_answered_provider_is_named_after_the_targets_in_the_dry_run() {
    let world = World::base();
    let mut input = want(&["needs-virt"]);
    input.provider_answers = vec![answer("virt", "prov-b")];
    let plan = plan(resolve_with(&input, &world));
    assert_eq!(
        item_names(&plan),
        [("prov-b", Role::Provider), ("needs-virt", Role::Target)]
    );
    assert_eq!(plan.providers, [answer("virt", "prov-b")]);
    // The list was made by the dry run the helper will redo: targets first, then the chosen providers.
    assert!(
        world
            .runs
            .borrow()
            .iter()
            .any(|(_, names)| names == &["test/needs-virt", "test/prov-b"]),
        "{:?}",
        world.runs.borrow()
    );
}

#[test]
fn a_provider_that_is_installed_already_is_no_question() {
    let mut world = World::base();
    world.install("somebody-else", "1-1", &["virt"]);
    let plan = plan(resolve_with(&want(&["needs-virt"]), &world));
    assert!(plan.providers.is_empty());
    assert_eq!(item_names(&plan), [("needs-virt", Role::Target)]);
}

#[test]
fn an_answer_naming_an_installed_package_is_never_part_of_the_plan() {
    // The user answered, then installed that provider by hand before the next round. The dependency is satisfied by
    // what is installed, so there is nothing to name; keeping the answer would reinstall it.
    let mut world = World::base();
    world.install("prov-b", "1.0-1", &["virt"]);
    let mut input = want(&["needs-virt"]);
    input.provider_answers = vec![answer("virt", "prov-b")];
    let plan = plan(resolve_with(&input, &world));
    assert_eq!(plan.providers, []);
    assert_eq!(item_names(&plan), [("needs-virt", Role::Target)]);
}

#[test]
fn a_versioned_dependency_offers_only_the_providers_pacman_accepts_with_the_other_targets_in_every_run()
 {
    let world = World::base();
    let found = questions(resolve_with(&want(&["needs-vvirt", "hello"]), &world));
    // vprov-1 provides vvirt=1, which does not satisfy >=2: naming it makes pacman add another provider.
    assert_eq!(
        the_provider_question(&found),
        ("vvirt", vec!["vprov-2", "vprov-3"], 0)
    );
    // Every candidate is judged by a dry run of ALL the user's targets plus the candidate. (A second run of the
    // candidate alone tells what it brings along by itself; it is the only run that omits the other targets.)
    let runs: Vec<Vec<String>> = world.runs.borrow().iter().map(|(_, n)| n.clone()).collect();
    for candidate in ["vprov-1", "vprov-2", "vprov-3"] {
        let spec = format!("test/{candidate}");
        let judged = runs.iter().any(|names| {
            names.contains(&spec)
                && names.contains(&"test/hello".to_string())
                && names.contains(&"test/needs-vvirt".to_string())
        });
        assert!(
            judged,
            "{candidate} was never judged with the other targets: {runs:?}"
        );
        for names in runs.iter().filter(|names| names.contains(&spec)) {
            let alone = names.len() == 1;
            let with_everything = names.contains(&"test/hello".to_string())
                && names.contains(&"test/needs-vvirt".to_string());
            assert!(
                alone || with_everything,
                "a candidate run without the other targets: {names:?}"
            );
        }
    }
}

#[test]
fn an_answer_that_does_not_satisfy_the_dependency_is_dropped_and_asked_again() {
    let world = World::base();
    let mut input = want(&["needs-vvirt"]);
    input.provider_answers = vec![answer("vvirt", "vprov-1")];
    let found = questions(resolve_with(&input, &world));
    assert_eq!(
        the_provider_question(&found),
        ("vvirt", vec!["vprov-2", "vprov-3"], 0)
    );
}

#[test]
fn nothing_providing_the_dependency_is_its_own_state() {
    let world = World::base();
    assert_eq!(
        blockers(resolve_with(&want(&["needs-stale"]), &world)),
        [Blocker::NothingProvides {
            dependency: "stale>=2".to_string(),
            required_by: "needs-stale".to_string(),
        }]
    );
}

#[test]
fn a_provider_whose_own_dependency_provides_the_same_name_is_still_a_choice() {
    // chain-a provides `chain` and needs chain-b, which provides it too: the list then holds two providers of one
    // name. "Exactly one provider in the list" would wrongly reject chain-a.
    let world = World::base();
    let found = questions(resolve_with(&want(&["needs-chain"]), &world));
    assert_eq!(
        the_provider_question(&found),
        ("chain", vec!["chain-a", "chain-b", "chain-c"], 0)
    );
    let mut input = want(&["needs-chain"]);
    input.provider_answers = vec![answer("chain", "chain-a")];
    let plan = plan(resolve_with(&input, &world));
    assert_eq!(
        item_names(&plan),
        [
            ("chain-b", Role::Dependency),
            ("chain-a", Role::Provider),
            ("needs-chain", Role::Target)
        ]
    );
}

/// Review 6.5 R-02: two providers of a soname is a choice pacman makes unasked. No question: the install is blocked.
#[test]
fn a_shared_library_two_packages_provide_blocks_the_install() {
    let mut world = World::base();
    world.add("zlib-ng", &["libz.so=1"], &[]);
    assert_eq!(
        resolve_with(&want(&["needs-soname"]), &world).unwrap(),
        Resolution::Blocked(vec![Blocker::SonameChoice {
            dependency: "libz.so".to_string(),
            // Every provider of that version, none preferred.
            candidates: ["zlib", "zlib-ng"]
                .map(|name| PackageRef {
                    repo: "test".to_string(),
                    name: name.to_string(),
                })
                .to_vec(),
        }])
    );
}

#[test]
fn a_shared_library_with_one_provider_of_its_version_is_no_choice() {
    let mut world = World::base();
    world.add("lib32-zlib", &["libz.so=2"], &[]);
    let plan = plan(resolve_with(&want(&["needs-soname"]), &world));
    assert!(plan.providers.is_empty());
}

#[test]
fn a_shared_library_provided_by_soname_is_never_a_provider_choice() {
    let world = World::base();
    let plan = plan(resolve_with(&want(&["needs-soname"]), &world));
    assert!(plan.providers.is_empty());
    assert_eq!(
        item_names(&plan),
        [("zlib", Role::Dependency), ("needs-soname", Role::Target)]
    );
}

#[test]
fn the_same_dry_run_is_never_asked_twice_in_one_call() {
    // With an answer, the judging run of the chosen provider and the final run are the same dry run.
    let world = World::base();
    let mut input = want(&["needs-virt", "hello"]);
    input.provider_answers = vec![answer("virt", "prov-b")];
    let _ = plan(resolve_with(&input, &world));
    let runs = world.runs.borrow();
    let mut seen = std::collections::HashSet::new();
    for run in runs.iter() {
        assert!(seen.insert(run.clone()), "asked twice: {run:?}");
    }
    assert!(runs.len() >= 2, "{runs:?}");
}

#[test]
fn the_package_list_folder_is_part_of_what_a_cached_dry_run_is_for() {
    let world = World::base();
    let mut engine = Engine::new(&world, Some(PathBuf::from("/var/lib/pacman/")));
    let targets = ["test/hello".to_string()];
    engine.dry_run(false, &targets).unwrap();
    engine.dry_run(false, &targets).unwrap();
    assert_eq!(world.runs.borrow().len(), 1, "same folder: one pacman run");
    engine.dbpath = Some(PathBuf::from("/run/tidewise-helper-lists/"));
    engine.dry_run(false, &targets).unwrap();
    assert_eq!(
        world.runs.borrow().len(),
        2,
        "another folder: a new pacman run"
    );
}

// ---- rounds ---------------------------------------------------------------------------------------------------

/// `app2` needs `v1` (provided by p1a and p1b); p1b needs `v2` (provided by p2a and p2b).
fn two_level_world() -> World {
    let mut w = World::default();
    w.add("app2", &[], &["v1"])
        .add("p1a", &["v1"], &[])
        .add("p1b", &["v1"], &["v2"])
        .add("p2a", &["v2"], &[])
        .add("p2b", &["v2"], &[]);
    w
}

#[test]
fn an_answer_can_bring_a_second_round_question() {
    let world = two_level_world();
    let mut input = want(&["app2"]);
    let first = questions(resolve_with(&input, &world));
    assert_eq!(the_provider_question(&first), ("v1", vec!["p1a", "p1b"], 0));

    input.provider_answers = vec![answer("v1", "p1b")];
    input = input.next_round();
    assert_eq!(input.round, 1);
    let second = questions(resolve_with(&input, &world));
    assert_eq!(
        the_provider_question(&second),
        ("v2", vec!["p2a", "p2b"], 0),
        "v1 is answered and must not be asked again"
    );

    input.provider_answers.push(answer("v2", "p2b"));
    input = input.next_round();
    let plan = plan(resolve_with(&input, &world));
    assert_eq!(plan.providers, [answer("v1", "p1b"), answer("v2", "p2b")]);
    assert_eq!(
        item_names(&plan),
        [
            ("p2b", Role::Provider),
            ("p1b", Role::Provider),
            ("app2", Role::Target)
        ]
    );
}

#[test]
fn the_last_allowed_round_may_still_ask_and_the_next_one_is_an_error() {
    let world = two_level_world();
    let mut input = want(&["app2"]);
    input.round = MAX_ROUNDS - 1;
    assert!(matches!(
        resolve_with(&input, &world),
        Ok(Resolution::NeedsAnswers(_))
    ));
    input.round = MAX_ROUNDS;
    assert!(matches!(
        resolve_with(&input, &world),
        Err(PacmanError::TooManyRounds { limit }) if limit == MAX_ROUNDS
    ));
}

#[test]
fn a_round_past_the_cap_that_has_nothing_to_ask_is_still_fine() {
    let world = World::base();
    let mut input = want(&["hello"]);
    input.round = MAX_ROUNDS;
    assert!(matches!(
        resolve_with(&input, &world),
        Ok(Resolution::Ready(_))
    ));
}

// ---- groups ---------------------------------------------------------------------------------------------------

fn group_world() -> World {
    let mut w = World::base();
    w.group("grp", &["hello", "lib"])
        .install("lib", "1.0-1", &[]);
    w
}

#[test]
fn a_group_is_a_question_with_installed_members_marked() {
    let world = group_world();
    let input = ResolveInput {
        groups: vec!["grp".to_string()],
        ..ResolveInput::default()
    };
    let found = questions(resolve_with(&input, &world));
    let [Question::ChooseGroupMembers { group, members }] = found.as_slice() else {
        panic!("{found:?}");
    };
    assert_eq!(group, "grp");
    assert_eq!(
        members
            .iter()
            .map(|m| (m.package.name.as_str(), m.installed_version.as_deref()))
            .collect::<Vec<_>>(),
        [("hello", None), ("lib", Some("1.0-1"))]
    );
    assert_eq!(members[0].package, pr("hello"));
}

#[test]
fn the_kept_members_become_targets() {
    let world = group_world();
    let input = ResolveInput {
        groups: vec!["grp".to_string()],
        group_answers: vec![GroupAnswer {
            group: "grp".to_string(),
            members: vec![pr("hello")],
        }],
        ..ResolveInput::default()
    };
    let plan = plan(resolve_with(&input, &world));
    assert_eq!(plan.targets, [pr("hello")]);
    assert_eq!(item_names(&plan), [("hello", Role::Target)]);
}

#[test]
fn an_unknown_group_is_blocked() {
    let world = group_world();
    let input = ResolveInput {
        groups: vec!["nogroup".to_string()],
        ..ResolveInput::default()
    };
    assert_eq!(
        blockers(resolve_with(&input, &world)),
        [Blocker::UnknownGroup("nogroup".to_string())]
    );
}

#[test]
fn a_plan_item_keeps_pacmans_words_for_its_installed_size() {
    let world = World::base();
    let plan = plan(resolve_with(&want(&["hello"]), &world));
    assert_eq!(plan.items[0].installed_size_text, "2000 B");
    assert_eq!(plan.items[0].current_size, None);
}

#[test]
fn a_plan_item_that_is_installed_now_says_how_much_it_takes() {
    let mut world = World::base();
    world.install("hello", "1.0-1", &[]);
    let mut input = want(&["hello", "app"]);
    input.reinstall = vec![pr("hello")];
    let plan = plan(resolve_with(&input, &world));
    let sizes: Vec<(&str, Option<u64>)> = plan
        .items
        .iter()
        .map(|item| (item.package.name.as_str(), item.current_size))
        .collect();
    assert_eq!(sizes, [("hello", Some(500)), ("lib", None), ("app", None)]);
}

// ---- installed targets ----------------------------------------------------------------------------------------

#[test]
fn a_target_that_is_installed_at_the_repository_version_asks_before_reinstalling() {
    let mut world = World::base();
    world.install("hello", "1.0-1", &[]);
    let found = questions(resolve_with(&want(&["hello"]), &world));
    assert_eq!(
        found,
        [Question::AlreadyInstalled {
            target: pr("hello"),
            installed_version: "1.0-1".to_string(),
            repo_version: "1.0-1".to_string(),
            differs_without_update: false,
        }]
    );
    let mut input = want(&["hello"]);
    input.reinstall = vec![pr("hello")];
    let plan = plan(resolve_with(&input, &world));
    assert_eq!(plan.items[0].installed_version.as_deref(), Some("1.0-1"));
}

#[test]
fn an_installed_version_that_differs_with_no_update_pending_would_be_a_downgrade() {
    let mut world = World::base();
    world.install("hello", "2.0-1", &[]);
    let found = questions(resolve_with(&want(&["hello"]), &world));
    assert!(matches!(
        found.as_slice(),
        [Question::AlreadyInstalled {
            differs_without_update: true,
            ..
        }]
    ));
}

#[test]
fn an_installed_target_with_an_update_pending_is_simply_updated() {
    let mut world = World::base();
    world.install("hello", "0.9-1", &[]).pend("hello", false);
    let plan = plan(resolve_with(&want(&["hello"]), &world));
    assert!(matches!(
        plan.decision,
        InstallDecision::MustUpdateSystem { count: 1, .. }
    ));
    assert_eq!(item_names(&plan), [("hello", Role::Target)]);
    assert_eq!(plan.items[0].installed_version.as_deref(), Some("0.9-1"));
}

// ---- a request that does not come back under its own name -----------------------------------------------------

/// Answers every dry run with fixed text, for the fixture tests.
struct Scripted {
    print: &'static str,
    si: &'static str,
}

impl Facts for Scripted {
    fn dry_run(&self, _update: bool, _targets: &[String]) -> Result<DryRun, PacmanError> {
        Ok(DryRun::Listed(parse_print(self.print).unwrap()))
    }
    fn details(&self, refs: &[PackageRef]) -> Result<DetailsLookup, PacmanError> {
        let all = parse_si(self.si).unwrap();
        let (found, missing): (Vec<_>, Vec<_>) = refs
            .iter()
            .partition(|r| all.iter().any(|d| d.repo == r.repo && d.name == r.name));
        Ok(DetailsLookup {
            found: all
                .into_iter()
                .filter(|d| found.iter().any(|r| r.repo == d.repo && r.name == d.name))
                .collect(),
            not_found: missing.into_iter().cloned().collect(),
        })
    }
    fn search_exact(&self, _name: &str) -> Result<Vec<SearchResult>, PacmanError> {
        Ok(Vec::new())
    }
    fn search_any(&self, _names: &[String]) -> Result<Vec<SearchResult>, PacmanError> {
        Ok(Vec::new())
    }
    fn satisfied(&self, _dependency: &Dependency) -> Result<bool, PacmanError> {
        Ok(false)
    }
    fn group_members(&self, _group: &str) -> Result<Option<Vec<GroupMember>>, PacmanError> {
        Ok(None)
    }
    fn installed(&self) -> Result<Vec<InstalledPackage>, PacmanError> {
        Ok(Vec::new())
    }
    fn pending_updates(&self) -> Result<Vec<PendingUpdate>, PacmanError> {
        Ok(Vec::new())
    }
}

const RENAMED: &str =
    include_str!("../../../../tests/fixtures/pacman/print-renamed-iptables-nft.txt");
const SI_IPTABLES: &str = include_str!("../../../../tests/fixtures/pacman/si-iptables.txt");
const SI_ZSH: &str = include_str!("../../../../tests/fixtures/pacman/si-zsh.txt");

fn ref_in(repo: &str, name: &str) -> PackageRef {
    PackageRef {
        repo: repo.to_string(),
        name: name.to_string(),
    }
}

#[test]
fn a_requested_package_that_is_now_provided_by_another_is_never_ready() {
    // Real output: `pacman -S iptables-nft` answers with core/iptables, which provides and replaces that name.
    let facts = Scripted {
        print: RENAMED,
        si: SI_IPTABLES,
    };
    let input = ResolveInput {
        targets: vec![ref_in("core", "iptables-nft")],
        ..ResolveInput::default()
    };
    assert_eq!(
        blockers(resolve_with(&input, &facts)),
        [Blocker::ReplacedBy {
            requested: ref_in("core", "iptables-nft"),
            by: vec![ref_in("core", "iptables")],
        }]
    );
}

#[test]
fn a_requested_package_missing_with_nothing_to_explain_it_is_blocked_too() {
    let facts = Scripted {
        print: RENAMED,
        si: SI_ZSH,
    };
    let input = ResolveInput {
        targets: vec![ref_in("extra", "zsh")],
        ..ResolveInput::default()
    };
    assert_eq!(
        blockers(resolve_with(&input, &facts)),
        [Blocker::MissingFromList {
            requested: ref_in("extra", "zsh")
        }]
    );
}

// ---- one package for two names (6.4b) --------------------------------------------------------------------------

#[test]
fn a_package_that_provides_two_needed_names_is_named_once() {
    // The helper refuses a request that chooses one package twice, so the plan must name it once.
    let mut world = World::base();
    world
        .add("both", &["v1", "v2"], &[])
        .add("needs-both", &[], &["v1", "v2"]);
    let plan = plan(resolve_with(&want(&["needs-both"]), &world));
    assert_eq!(plan.providers, [answer("v1", "both")]);
    assert_eq!(
        item_names(&plan),
        [("both", Role::Provider), ("needs-both", Role::Target)]
    );
    // The dry runs never name it twice either.
    assert!(
        world.runs.borrow().iter().all(|(_, names)| names
            .iter()
            .filter(|n| *n == "test/both")
            .count()
            <= 1),
        "{:?}",
        world.runs.borrow()
    );
}

#[test]
fn an_answer_that_chose_a_target_is_not_named_as_a_provider_again() {
    // `validate` refuses a chosen provider that is also a target.
    let world = World::base();
    let mut input = want(&["needs-virt", "prov-a"]);
    input.provider_answers = vec![answer("virt", "prov-a")];
    let plan = plan(resolve_with(&input, &world));
    assert!(plan.providers.is_empty(), "{:?}", plan.providers);
}

// ---- one name needed at two versions (6.4b, threat model 42 a) --------------------------------------------------

fn two_versions_world() -> World {
    let mut world = World::base();
    world
        .add("jv-8", &["jv=8"], &[])
        .add("jv-9", &["jv=9"], &[])
        .add("need8", &[], &["jv=8"])
        .add("need9", &[], &["jv=9"]);
    world
}

#[test]
fn two_versions_of_one_name_block_the_plan() {
    // pacman lists both providers and asks nothing; a request can name only one, and the helper refuses it.
    let world = two_versions_world();
    let found = blockers(resolve_with(&want(&["need8", "need9"]), &world));
    assert_eq!(
        found,
        [Blocker::TwoVersionsNeeded {
            dependency: "jv".to_string()
        }]
    );
}

#[test]
fn one_version_of_a_name_with_two_providers_is_still_ready() {
    let world = two_versions_world();
    let plan = plan(resolve_with(&want(&["need9"]), &world));
    assert_eq!(plan.providers, [answer("jv", "jv-9")]);
}

// ---- update and install (WBS 6.6) -------------------------------------------------------------------------------

/// A world where an update is waiting: `lib` is installed at an older version.
fn stale() -> World {
    let mut world = World::base();
    world.install("lib", "0.5-1", &[]).pend("lib", false);
    world
}

#[test]
fn update_and_install_is_blocked_by_a_listed_replacement() {
    let mut world = stale();
    world.install("oldfoo", "1.0-1", &[]);
    world.add("newfoo", &[], &[]).replacing("oldfoo");
    assert_eq!(
        blockers(resolve_with(&want(&["newfoo"]), &world)),
        [Blocker::Replacement(vec![crate::Replacement {
            new: pr("newfoo"),
            old: "oldfoo".to_string(),
        }])]
    );

    // Nothing installed to replace: nothing to block.
    let mut world = stale();
    world.add("newfoo", &[], &[]).replacing("oldfoo");
    plan(resolve_with(&want(&["newfoo"]), &world));
}

#[test]
fn update_and_install_never_dry_runs_a_package_without_the_update() {
    // On a stale system `-S` of one package can fail by itself (review 6.6 U-01).
    let world = stale();
    let mut input = want(&["needs-virt"]);
    input.provider_answers = vec![answer("virt", "prov-b")];
    plan(resolve_with(&input, &world));
    let runs = world.runs.borrow();
    assert!(runs.iter().all(|(update, _)| *update), "{runs:?}");
}

/// The one blocker "pacman would add a package nobody named": `(name, package)`.
fn unnamed(found: Vec<Blocker>) -> Vec<(String, String)> {
    match found.as_slice() {
        [Blocker::UnnamedChoice(picks)] => picks
            .iter()
            .map(|pick| (pick.dependency.clone(), pick.provider.name.clone()))
            .collect(),
        other => panic!("expected the unnamed choice, got {other:?}"),
    }
}

#[test]
fn update_and_install_sends_an_unnamed_second_provider_to_the_terminal() {
    // M1 and M4: `chain-a` is chosen and needs `chain-b` by name; `chain-b` provides `chain` too, and so does
    // `chain-c`.
    let mut input = want(&["needs-chain"]);
    input.provider_answers = vec![answer("chain", "chain-a")];
    assert_eq!(
        unnamed(blockers(resolve_with(&input, &stale()))),
        [("chain".to_string(), "chain-b".to_string())]
    );
    // A plain install is judged as before.
    plan(resolve_with(&input, &World::base()));
}

#[test]
fn a_provider_the_system_update_brings_is_asked() {
    // Decision 0009: a plain update sends the user to a terminal; update and install can name it, so it asks.
    let mut world = stale();
    world
        .add("upg", &[], &["virt"])
        .install("upg", "0.5-1", &[])
        .pend("upg", false);
    let questions = questions(resolve_with(&want(&["hello"]), &world));
    let (dependency, candidates, _) = the_provider_question(&questions);
    assert_eq!(dependency, "virt");
    assert_eq!(candidates, ["prov-a", "prov-b"]);
}

#[test]
fn installing_a_shared_library_candidate_ends_the_choice_in_update_and_install() {
    // An upgraded package needs a library two packages provide. The card offers each; installing one must resolve,
    // not show the same card again.
    let mut world = stale();
    world
        .add("zlib-ng", &["libz.so=1"], &[])
        .add("upg", &[], &["libz.so=1"])
        .install("upg", "0.5-1", &[])
        .pend("upg", false);
    let found = blockers(resolve_with(&want(&["hello"]), &world));
    assert!(
        matches!(&found[..], [Blocker::SonameChoice { candidates, .. }] if candidates == &[pr("zlib"), pr("zlib-ng")]),
        "{found:?}"
    );
    for candidate in ["zlib", "zlib-ng"] {
        let plan = plan(resolve_with(&want(&[candidate]), &world));
        assert!(
            plan.items.iter().any(|item| item.package == pr(candidate)),
            "{plan:?}"
        );
    }
}

/// Review 6.6 V-01: `jre25` needs `jre-other` by name, but something else already meets that name, so pacman does not
/// pull `jre-other` for it: it is listed as pacman's own pick for `jvm=17`. `jre25` must not be offered.
fn jvm_world(target_provides: &[&str]) -> World {
    let mut world = stale();
    world
        .add("need17", target_provides, &["jvm=17"])
        .add("jre25", &["jvm=25"], &["jre-other"])
        .add("jre-other", &["jvm=17"], &[])
        .add("jre-third", &["jvm=17"], &[]);
    world
}

fn assert_jre25_is_not_accepted(world: &World) {
    let mut input = want(&["need17"]);
    input.provider_answers = vec![answer("jvm", "jre25")];
    assert_eq!(
        unnamed(blockers(resolve_with(&input, world))),
        [("jvm".to_string(), "jre-other".to_string())]
    );
}

#[test]
fn a_candidate_whose_by_name_dependency_is_met_by_something_installed_does_not_fit() {
    let mut world = jvm_world(&[]);
    world.install("shadow", "1.0-1", &["jre-other"]);
    assert_jre25_is_not_accepted(&world);
}

#[test]
fn a_candidate_whose_by_name_dependency_another_listed_package_provides_does_not_fit() {
    assert_jre25_is_not_accepted(&jvm_world(&["jre-other"]));
}
