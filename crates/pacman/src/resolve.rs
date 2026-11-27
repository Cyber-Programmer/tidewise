//! The install resolver (WBS 6.1): from "install these" to the exact list for the confirm screen, or to the
//! questions pacman would otherwise answer silently (which provider, which group members, reinstall?).
//!
//! The dry run (`pacman -S --print`) never asks anything: it takes the first provider and every group member.
//! So the resolver finds the choices itself, asks pacman as the judge (one extra dry run per candidate provider,
//! with every other target and answer in the same request), and returns a plan only when every provider is named
//! (golden rule 10). Pure logic over `Facts`; the real facts, with `--dbpath`, are in `run.rs`.
//!
//! One call is one *round*. An answer can bring a new question (the chosen provider has a dependency with
//! several providers of its own), so the caller asks again with the answers so far and `round` raised, at most
//! `MAX_ROUNDS` times.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::picks::provides_for;
use crate::{
    ChoiceFacts, Dependency, DetailsLookup, GroupMember, InstallDecision, InstalledPackage,
    PackageDetails, PackageRef, PacmanError, PendingUpdate, PrintFailure, PrintedPackage,
    ProviderPick, SearchResult, install_decision, is_soname, provider_picks, real_choices,
};
use tidewise_names::{is_valid_package_name, is_valid_repo_name};

/// Most rounds of questions one install may take. A sixth round is an error, not a sixth dialog.
pub const MAX_ROUNDS: u32 = 5;

/// What to resolve. `Default` is "nothing", the real package lists and round 0.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolveInput {
    /// Packages the user chose, as `repo/name` (from search results).
    pub targets: Vec<PackageRef>,
    /// Groups the user chose (`plasma`). Their members are asked about, see `Question::ChooseGroupMembers`.
    pub groups: Vec<String>,
    pub group_answers: Vec<GroupAnswer>,
    pub provider_answers: Vec<ProviderAnswer>,
    /// Targets that are installed already and the user said to reinstall.
    pub reinstall: Vec<PackageRef>,
    /// How many earlier calls ended in `NeedsAnswers`. `next_round` raises it.
    pub round: u32,
    /// The folder with pacman's databases (`--dbpath`), or `None` for the real one. Every pacman call of the
    /// resolver uses it, so a private copy of the package lists (WBS 6.11) can be resolved against.
    pub dbpath: Option<PathBuf>,
}

impl ResolveInput {
    /// The same input for the next round, after the caller added the answers.
    pub fn next_round(mut self) -> Self {
        self.round += 1;
        self
    }
}

/// The members of a group the user kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupAnswer {
    pub group: String,
    pub members: Vec<PackageRef>,
}

/// The package the user picked for a name several packages provide (`java-runtime` -> `extra/jre-openjdk`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAnswer {
    pub dependency: String,
    pub chosen: PackageRef,
}

/// How a resolve call ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Every question is answered and every provider is named: this is the list to confirm.
    Ready(InstallPlan),
    /// The user must answer these, then the caller resolves again (`ResolveInput::next_round`).
    NeedsAnswers(Vec<Question>),
    /// Nothing can be installed as asked, and no answer changes that. Text for the user comes from these.
    Blocked(Vec<Blocker>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    /// Several packages provide `dependency` and fit; pacman would take `candidates[default]`.
    ChooseProvider {
        dependency: String,
        candidates: Vec<Candidate>,
        default: usize,
    },
    /// A group was chosen: which of its members? All are offered; installed ones are marked.
    ChooseGroupMembers {
        group: String,
        members: Vec<GroupChoice>,
    },
    /// The target is installed already. Reinstall it (`ResolveInput::reinstall`), or drop it.
    AlreadyInstalled {
        target: PackageRef,
        installed_version: String,
        repo_version: String,
        /// The installed version is not the repository's and no update is pending: installing would downgrade it.
        differs_without_update: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub package: PackageRef,
    pub version: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupChoice {
    pub package: PackageRef,
    pub version: String,
    /// `Some` when installed: such members are not selected by default.
    pub installed_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocker {
    /// No target and no group was given.
    NothingToInstall,
    /// pacman does not know these names.
    NotFound(Vec<String>),
    UnknownRepo(String),
    UnknownGroup(String),
    /// Nothing in the repositories provides `dependency` (for example the only provider is too old).
    NothingProvides {
        dependency: String,
        required_by: String,
    },
    /// The requested package does not come back under its own name: it is now provided by `by`.
    ReplacedBy {
        requested: PackageRef,
        by: Vec<PackageRef>,
    },
    /// The requested package is not in the list and nothing in it explains why.
    MissingFromList {
        requested: PackageRef,
    },
    /// Two packages of the list need different providers of one name (`java-runtime=17` and `=21`). The request
    /// holds one answer per name, so the helper would refuse it (threat model 42 a).
    TwoVersionsNeeded {
        dependency: String,
    },
    /// Several packages provide this shared library and pacman would choose one by itself. Tidewise asks no question
    /// about a soname (review 6.5 R-02).
    SonameChoice {
        dependency: String,
        /// The repository packages that provide that version: each is offered as an install of its own.
        candidates: Vec<PackageRef>,
    },
}

/// The exact list for the confirm screen, and what to send to the helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    /// `Allowed`: install against the current lists. `MustUpdateSystem`: "Update system and install".
    pub decision: InstallDecision,
    /// What the helper's `targets` hold: the chosen packages, group members expanded.
    pub targets: Vec<PackageRef>,
    /// Every provider choice, including the ones with only one candidate (named, never asked).
    pub providers: Vec<ProviderAnswer>,
    /// The lines of the dry run, in pacman's order: the list the helper compares.
    pub items: Vec<PlanItem>,
    /// Bytes to download (the sum of `PlanItem::download_size`).
    pub download_total: u64,
    /// Bytes on disk once installed (the sum of `PlanItem::installed_size`).
    pub installed_total: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItem {
    pub package: PackageRef,
    pub version: String,
    pub role: Role,
    /// `Some` when another version is installed now.
    pub installed_version: Option<String>,
    pub download_size: u64,
    pub installed_size: u64,
    /// pacman's own words for `installed_size` (`-Si`, like `6.94 MiB`); empty if `-Si` had no record.
    pub installed_size_text: String,
    /// Bytes the version installed now takes (`-Qi`); `None` when the package is not installed.
    pub current_size: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The user chose it (or kept it from a group).
    Target,
    /// A package the user picked, or the only one there is, for a name several packages could provide.
    Provider,
    /// Pulled in because something needs it.
    Dependency,
    /// An update that comes with "Update system and install".
    Update,
}

/// What the resolver needs to know about pacman. The real one runs pacman with `--dbpath`; tests use a fake.
pub(crate) trait Facts {
    /// `pacman -S --print` (`-Su` when `update`) of these targets (`repo/name` or a plain name).
    fn dry_run(&self, update: bool, targets: &[String]) -> Result<DryRun, PacmanError>;
    fn details(&self, refs: &[PackageRef]) -> Result<DetailsLookup, PacmanError>;
    /// `pacman -Ss '^name$'`: the packages named so, or providing that name.
    fn search_exact(&self, name: &str) -> Result<Vec<SearchResult>, PacmanError>;
    /// One search for all these names (`-Ss '^(a|b)$'`).
    fn search_any(&self, names: &[String]) -> Result<Vec<SearchResult>, PacmanError>;
    /// `pacman -T`: is `dependency` satisfied by what is installed?
    fn satisfied(&self, dependency: &Dependency) -> Result<bool, PacmanError>;
    /// `None` when pacman does not know the group.
    fn group_members(&self, group: &str) -> Result<Option<Vec<GroupMember>>, PacmanError>;
    fn installed(&self) -> Result<Vec<InstalledPackage>, PacmanError>;
    fn pending_updates(&self) -> Result<Vec<PendingUpdate>, PacmanError>;
}

#[derive(Clone)]
#[allow(
    dead_code,
    reason = "the red-phase shims in run.rs do not build one yet (6.1)"
)]
pub(crate) enum DryRun {
    Listed(Vec<PrintedPackage>),
    Failed(PrintFailure),
}

/// Most passes of "dry run, find the choices, name the single ones" in one call. Each pass names at least one more
/// provider, so this is only a stop for a pacman that keeps changing its mind.
const MAX_PASSES: usize = 8;

/// Most provider answers the helper accepts in one request (`MAX_PROVIDERS` in `helper-protocol`).
const MAX_PROVIDERS: usize = 100;

/// One resolve call against `facts`.
pub(crate) fn resolve_with(
    input: &ResolveInput,
    facts: &dyn Facts,
) -> Result<Resolution, PacmanError> {
    check_input(input)?;
    if input.targets.is_empty() && input.groups.is_empty() {
        return Ok(Resolution::Blocked(vec![Blocker::NothingToInstall]));
    }
    Engine::new(facts, input.dbpath.clone()).run(input)
}

/// Refuses names that could not come from pacman, before anything runs.
fn check_input(input: &ResolveInput) -> Result<(), PacmanError> {
    let bad = |reason| Err(PacmanError::BadPackageRef { reason });
    let refs = input
        .targets
        .iter()
        .chain(&input.reinstall)
        .chain(input.group_answers.iter().flat_map(|a| &a.members))
        .chain(input.provider_answers.iter().map(|a| &a.chosen));
    for item in refs {
        if !is_valid_repo_name(&item.repo) {
            return bad("the repository name is not valid");
        }
        if !is_valid_package_name(&item.name) {
            return bad("the package name is not valid");
        }
    }
    let names = input
        .groups
        .iter()
        .chain(input.group_answers.iter().map(|a| &a.group))
        .chain(input.provider_answers.iter().map(|a| &a.dependency));
    for name in names {
        if !is_valid_package_name(name) {
            return bad("a group or dependency name is not valid");
        }
    }
    if let Some(path) = &input.dbpath {
        let text = path.to_str().ok_or(PacmanError::BadDbPath {
            reason: "it is not plain text",
        })?;
        if !text.starts_with('/') || text.chars().any(char::is_control) {
            return Err(PacmanError::BadDbPath {
                reason: "it is not an absolute path",
            });
        }
    }
    Ok(())
}

/// What one call remembers so that no dry run, details lookup or search is asked twice. Dropped at the end of the
/// call ("per round"). The key of a dry run holds everything that decides its answer: the package-list folder, the
/// update flag and the full target list in order (targets, group members and answers included).
struct Engine<'a> {
    facts: &'a dyn Facts,
    dbpath: Option<PathBuf>,
    runs: HashMap<RunKey, DryRun>,
    details: HashMap<PackageRef, PackageDetails>,
    unknown: HashSet<PackageRef>,
    searches: HashMap<String, Vec<SearchResult>>,
}

#[derive(PartialEq, Eq, Hash)]
struct RunKey {
    dbpath: Option<PathBuf>,
    update: bool,
    targets: Vec<String>,
}

fn spec(item: &PackageRef) -> String {
    format!("{}/{}", item.repo, item.name)
}

fn blockers_of(failure: PrintFailure) -> Vec<Blocker> {
    match failure {
        PrintFailure::NotFound(names) => vec![Blocker::NotFound(names)],
        PrintFailure::UnknownRepo(repo) => vec![Blocker::UnknownRepo(repo)],
        PrintFailure::Unsatisfiable(list) => list
            .into_iter()
            .map(|item| Blocker::NothingProvides {
                dependency: item.dependency,
                required_by: item.required_by,
            })
            .collect(),
    }
}

impl<'a> Engine<'a> {
    fn new(facts: &'a dyn Facts, dbpath: Option<PathBuf>) -> Self {
        Self {
            facts,
            dbpath,
            runs: HashMap::new(),
            details: HashMap::new(),
            unknown: HashSet::new(),
            searches: HashMap::new(),
        }
    }

    fn dry_run(&mut self, update: bool, targets: &[String]) -> Result<DryRun, PacmanError> {
        let key = RunKey {
            dbpath: self.dbpath.clone(),
            update,
            targets: targets.to_vec(),
        };
        if let Some(known) = self.runs.get(&key) {
            return Ok(known.clone());
        }
        let result = self.facts.dry_run(update, targets)?;
        self.runs.insert(key, result.clone());
        Ok(result)
    }

    /// Looks up the packages not known yet, in one call.
    fn load_details(&mut self, refs: &[PackageRef]) -> Result<(), PacmanError> {
        let mut missing: Vec<PackageRef> = Vec::new();
        for item in refs {
            if !self.details.contains_key(item)
                && !self.unknown.contains(item)
                && !missing.contains(item)
            {
                missing.push(item.clone());
            }
        }
        if missing.is_empty() {
            return Ok(());
        }
        let lookup = self.facts.details(&missing)?;
        for details in lookup.found {
            let item = PackageRef {
                repo: details.repo.clone(),
                name: details.name.clone(),
            };
            self.details.insert(item, details);
        }
        self.unknown.extend(lookup.not_found);
        Ok(())
    }

    fn details_of(&self, item: &PackageRef) -> Option<&PackageDetails> {
        self.details.get(item)
    }

    fn search(&mut self, name: &str) -> Result<Vec<SearchResult>, PacmanError> {
        if let Some(known) = self.searches.get(name) {
            return Ok(known.clone());
        }
        let found = self.facts.search_exact(name)?;
        self.searches.insert(name.to_string(), found.clone());
        Ok(found)
    }

    fn needs(&self, questions: Vec<Question>, round: u32) -> Result<Resolution, PacmanError> {
        if round >= MAX_ROUNDS {
            return Err(PacmanError::TooManyRounds { limit: MAX_ROUNDS });
        }
        Ok(Resolution::NeedsAnswers(questions))
    }

    fn run(&mut self, input: &ResolveInput) -> Result<Resolution, PacmanError> {
        let installed: HashMap<String, InstalledPackage> = self
            .facts
            .installed()?
            .into_iter()
            .map(|package| (package.name.clone(), package))
            .collect();
        let pending = self.facts.pending_updates()?;
        let decision = install_decision(&pending);
        let update = matches!(decision, InstallDecision::MustUpdateSystem { .. });
        let updating: HashSet<&str> = pending
            .iter()
            .filter(|item| !item.ignored)
            .map(|item| item.name.as_str())
            .collect();

        // Groups and installed targets come first: the providers depend on which packages are wanted.
        let mut open = Vec::new();
        let mut requested: Vec<PackageRef> = Vec::new();
        for item in &input.targets {
            if !requested.contains(item) {
                requested.push(item.clone());
            }
        }
        for group in &input.groups {
            if let Some(answer) = input.group_answers.iter().find(|a| a.group == *group) {
                for member in &answer.members {
                    if !requested.contains(member) {
                        requested.push(member.clone());
                    }
                }
                continue;
            }
            match self.group_question(update, group, &installed)? {
                Ok(question) => open.push(question),
                Err(blocked) => return Ok(blocked),
            }
        }
        for item in &input.targets {
            let Some(current) = installed.get(&item.name) else {
                continue;
            };
            if input.reinstall.contains(item) || updating.contains(item.name.as_str()) {
                continue;
            }
            self.load_details(std::slice::from_ref(item))?;
            if let Some(details) = self.details_of(item) {
                open.push(Question::AlreadyInstalled {
                    target: item.clone(),
                    installed_version: current.version.clone(),
                    repo_version: details.version.clone(),
                    differs_without_update: current.version != details.version,
                });
            }
        }
        if !open.is_empty() {
            return self.needs(open, input.round);
        }

        let base: Vec<String> = requested.iter().map(spec).collect();
        let mut providers =
            self.kept_answers(update, &base, &input.provider_answers, &installed)?;
        for _ in 0..MAX_PASSES {
            let mut names = base.clone();
            for answer in &providers {
                let name = spec(&answer.chosen);
                if !names.contains(&name) {
                    names.push(name);
                }
            }
            let list = match self.dry_run(update, &names)? {
                DryRun::Listed(list) => list,
                DryRun::Failed(failure) => return Ok(Resolution::Blocked(blockers_of(failure))),
            };
            let mut asked: Vec<PackageRef> = requested.clone();
            asked.extend(providers.iter().map(|a| a.chosen.clone()));
            let listed: Vec<PackageRef> = list.iter().map(printed_ref).collect();
            self.load_details(&listed)?;
            if let Some(blocked) = self.requested_missing(&asked, &listed) {
                return Ok(blocked);
            }

            // Shared libraries are never asked about: judged once the other providers are settled.
            let (sonames, picks): (Vec<ProviderPick>, Vec<ProviderPick>) = self
                .picks(&list, &asked, &providers, &installed)
                .into_iter()
                .partition(|pick| is_soname(&pick.dependency));
            let picks: Vec<(String, PackageRef)> = picks
                .into_iter()
                .map(|pick| (pick.dependency, pick.provider))
                .collect();
            if picks.is_empty() {
                if let Some((dependency, candidates)) =
                    self.soname_choice(sonames, &listed, &installed)?
                {
                    return Ok(Resolution::Blocked(vec![Blocker::SonameChoice {
                        dependency,
                        candidates,
                    }]));
                }
                if let Some(dependency) =
                    self.second_provider(update, &providers, &asked, &listed, &installed)?
                {
                    return Ok(Resolution::Blocked(vec![Blocker::TwoVersionsNeeded {
                        dependency,
                    }]));
                }
                let providers = named_once(&requested, providers);
                return Ok(Resolution::Ready(self.plan(
                    decision, &requested, providers, &list, &installed, &updating,
                )));
            }
            let mut questions = Vec::new();
            for (dependency, default) in picks {
                let candidates = self.candidates(&dependency)?;
                let fitting: Vec<SearchResult> = if candidates.len() <= 1 {
                    candidates
                } else {
                    let mut fitting = Vec::new();
                    for candidate in candidates {
                        if self.fits(
                            update,
                            &base,
                            &providers,
                            &dependency,
                            &PackageRef::from(&candidate),
                            &installed,
                        )? {
                            fitting.push(candidate);
                        }
                    }
                    fitting
                };
                match fitting.as_slice() {
                    [] => providers.push(ProviderAnswer {
                        dependency,
                        chosen: default,
                    }),
                    [only] => providers.push(ProviderAnswer {
                        dependency,
                        chosen: PackageRef::from(only),
                    }),
                    _ => {
                        let default_index = fitting
                            .iter()
                            .position(|c| PackageRef::from(c) == default)
                            .unwrap_or(0);
                        questions.push(Question::ChooseProvider {
                            dependency,
                            candidates: fitting
                                .iter()
                                .map(|c| Candidate {
                                    package: PackageRef::from(c),
                                    version: c.version.clone(),
                                    description: c.description.clone(),
                                })
                                .collect(),
                            default: default_index,
                        });
                    }
                }
            }
            if !questions.is_empty() {
                return self.needs(questions, input.round);
            }
            if providers.len() > MAX_PROVIDERS {
                return Err(PacmanError::Inconsistent {
                    reason: "more provider choices than the helper accepts",
                });
            }
        }
        Err(PacmanError::Inconsistent {
            reason: "the provider choices did not settle",
        })
    }

    /// The question for a group, or the reason it cannot be asked.
    fn group_question(
        &mut self,
        update: bool,
        group: &str,
        installed: &HashMap<String, InstalledPackage>,
    ) -> Result<Result<Question, Resolution>, PacmanError> {
        let Some(members) = self.facts.group_members(group)? else {
            return Ok(Err(Resolution::Blocked(vec![Blocker::UnknownGroup(
                group.to_string(),
            )])));
        };
        // `-Sg` gives names only; the repository and version come from a dry run of the names.
        let names: Vec<String> = members.iter().map(|m| m.name.clone()).collect();
        let list = match self.dry_run(update, &names)? {
            DryRun::Listed(list) => list,
            DryRun::Failed(failure) => {
                return Ok(Err(Resolution::Blocked(blockers_of(failure))));
            }
        };
        let choices = members
            .iter()
            .filter_map(|member| {
                let printed = list.iter().find(|p| p.name == member.name)?;
                Some(GroupChoice {
                    package: printed_ref(printed),
                    version: printed.version.clone(),
                    installed_version: installed.get(&member.name).map(|p| p.version.clone()),
                })
            })
            .collect();
        Ok(Ok(Question::ChooseGroupMembers {
            group: group.to_string(),
            members: choices,
        }))
    }

    /// The answers that really satisfy their dependency; the others are dropped, so the question is asked again.
    fn kept_answers(
        &mut self,
        update: bool,
        base: &[String],
        answers: &[ProviderAnswer],
        installed: &HashMap<String, InstalledPackage>,
    ) -> Result<Vec<ProviderAnswer>, PacmanError> {
        let mut kept = Vec::new();
        for (index, answer) in answers.iter().enumerate() {
            let others: Vec<ProviderAnswer> = answers
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != index)
                .map(|(_, other)| other.clone())
                .collect();
            if self.fits(
                update,
                base,
                &others,
                &answer.dependency,
                &answer.chosen,
                installed,
            )? {
                kept.push(answer.clone());
            }
        }
        Ok(kept)
    }

    /// The packages that could answer a question about `dependency`: those that provide the name.
    fn candidates(&mut self, dependency: &str) -> Result<Vec<SearchResult>, PacmanError> {
        let found = self.search(dependency)?;
        let refs: Vec<PackageRef> = found.iter().map(PackageRef::from).collect();
        self.load_details(&refs)?;
        Ok(found
            .into_iter()
            .filter(|result| {
                self.details_of(&PackageRef::from(result))
                    .is_some_and(|details| details.provides.iter().any(|p| p.name == dependency))
            })
            .collect())
    }

    /// Whether naming `candidate` for `dependency` works, with pacman as the judge: in a dry run of everything else
    /// asked plus the candidate, every package that provides `dependency` is the candidate or something the candidate
    /// brings along by itself (its own dry run). Installed packages never count, and are never a fit.
    fn fits(
        &mut self,
        update: bool,
        base: &[String],
        others: &[ProviderAnswer],
        dependency: &str,
        candidate: &PackageRef,
        installed: &HashMap<String, InstalledPackage>,
    ) -> Result<bool, PacmanError> {
        // An installed package is never named as a provider: pacman would list it again, as a reinstall.
        if installed.contains_key(&candidate.name) {
            return Ok(false);
        }
        let mut names = base.to_vec();
        names.extend(others.iter().map(|a| spec(&a.chosen)));
        names.push(spec(candidate));
        let DryRun::Listed(with_all) = self.dry_run(update, &names)? else {
            return Ok(false);
        };
        let DryRun::Listed(alone) = self.dry_run(update, &[spec(candidate)])? else {
            return Ok(false);
        };
        let refs: Vec<PackageRef> = with_all.iter().chain(&alone).map(printed_ref).collect();
        self.load_details(&refs)?;
        let providers_in = |engine: &Self, list: &[PrintedPackage]| -> Vec<String> {
            list.iter()
                .filter(|p| !installed.contains_key(&p.name))
                .filter(|p| engine.provides(&printed_ref(p), dependency))
                .map(|p| p.name.clone())
                .collect()
        };
        let mut allowed = providers_in(self, &alone);
        allowed.push(candidate.name.clone());
        Ok(providers_in(self, &with_all)
            .iter()
            .all(|name| allowed.contains(name)))
    }

    /// An answered name that the list provides a second time, with a package nobody named and the chosen one does
    /// not bring by itself: the helper's third refusal (`check_providers`), looked for here so the app never sends a
    /// request that will be refused. In practice two packages need two versions of one name.
    fn second_provider(
        &mut self,
        update: bool,
        answers: &[ProviderAnswer],
        asked: &[PackageRef],
        listed: &[PackageRef],
        installed: &HashMap<String, InstalledPackage>,
    ) -> Result<Option<String>, PacmanError> {
        for answer in answers {
            let others: Vec<&PackageRef> = listed
                .iter()
                .filter(|p| !asked.contains(p) && !installed.contains_key(&p.name))
                .filter(|p| self.provides(p, &answer.dependency))
                .collect();
            if others.is_empty() {
                continue;
            }
            let DryRun::Listed(brought) = self.dry_run(update, &[spec(&answer.chosen)])? else {
                return Ok(Some(answer.dependency.clone()));
            };
            if others
                .iter()
                .any(|other| !brought.iter().any(|item| item.name == other.name))
            {
                return Ok(Some(answer.dependency.clone()));
            }
        }
        Ok(None)
    }

    fn provides(&self, item: &PackageRef, dependency: &str) -> bool {
        self.details_of(item)
            .is_some_and(|d| d.provides.iter().any(|p| p.name == dependency))
    }

    /// The names a dry run satisfied through `Provides` with a package nobody named, where the answer is not given
    /// yet: `(dependency, the provider pacman took)`. The rule is `provider_picks`, which the helper uses too.
    fn picks(
        &self,
        list: &[PrintedPackage],
        asked: &[PackageRef],
        answers: &[ProviderAnswer],
        installed: &HashMap<String, InstalledPackage>,
    ) -> Vec<ProviderPick> {
        let refs: Vec<PackageRef> = list.iter().map(printed_ref).collect();
        let answered: Vec<&str> = answers.iter().map(|a| a.dependency.as_str()).collect();
        provider_picks(
            &refs,
            |item| self.details_of(item),
            asked,
            &answered,
            |name| installed.contains_key(name),
        )
    }

    /// The first shared library pacman would choose a provider for by itself, if any: more than one package provides
    /// that version and nothing installed answers it (`real_choices`, the helper's rule too). With it, the packages
    /// that could be installed first, by name.
    fn soname_choice(
        &mut self,
        picks: Vec<ProviderPick>,
        listed: &[PackageRef],
        installed: &HashMap<String, InstalledPackage>,
    ) -> Result<Option<(String, Vec<PackageRef>)>, PacmanError> {
        if picks.is_empty() {
            return Ok(None);
        }
        let details: Vec<PackageDetails> = listed
            .iter()
            .filter_map(|item| self.details_of(item).cloned())
            .collect();
        let mut facts = EngineFacts {
            engine: self,
            found: Vec::new(),
        };
        let choices = real_choices(
            picks,
            &details.iter().collect::<Vec<_>>(),
            |name| installed.contains_key(name),
            &mut facts,
        )?;
        let Some(pick) = choices.into_iter().next() else {
            return Ok(None);
        };
        let mut candidates: Vec<PackageRef> = facts
            .found
            .iter()
            .filter(|d| provides_for(d, &pick.dependency, pick.version.as_ref()))
            .map(|d| PackageRef {
                repo: d.repo.clone(),
                name: d.name.clone(),
            })
            .collect();
        candidates.sort_by(|a, b| (&a.name, &a.repo).cmp(&(&b.name, &b.repo)));
        candidates.dedup();
        Ok(Some((pick.dependency, candidates)))
    }

    /// A requested package that is not in the list under its own name is never silently dropped.
    fn requested_missing(&self, asked: &[PackageRef], listed: &[PackageRef]) -> Option<Resolution> {
        let mut blockers = Vec::new();
        for item in asked.iter().filter(|item| !listed.contains(item)) {
            let by: Vec<PackageRef> = listed
                .iter()
                .filter(|other| !asked.contains(other))
                .filter(|other| {
                    self.details_of(other).is_some_and(|d| {
                        d.provides
                            .iter()
                            .chain(&d.replaces)
                            .any(|p| p.name == item.name)
                    })
                })
                .cloned()
                .collect();
            blockers.push(if by.is_empty() {
                Blocker::MissingFromList {
                    requested: item.clone(),
                }
            } else {
                Blocker::ReplacedBy {
                    requested: item.clone(),
                    by,
                }
            });
        }
        (!blockers.is_empty()).then_some(Resolution::Blocked(blockers))
    }

    fn plan(
        &self,
        decision: InstallDecision,
        requested: &[PackageRef],
        providers: Vec<ProviderAnswer>,
        list: &[PrintedPackage],
        installed: &HashMap<String, InstalledPackage>,
        updating: &HashSet<&str>,
    ) -> InstallPlan {
        let mut items = Vec::new();
        for printed in list {
            let package = printed_ref(printed);
            let role = if requested.contains(&package) {
                Role::Target
            } else if providers.iter().any(|a| a.chosen == package) {
                Role::Provider
            } else if updating.contains(printed.name.as_str()) {
                Role::Update
            } else {
                Role::Dependency
            };
            let current = installed.get(&printed.name);
            let details = self.details_of(&package);
            items.push(PlanItem {
                installed_version: current.map(|p| p.version.clone()),
                download_size: printed.download_size,
                installed_size: details.map_or(0, |d| d.installed_size),
                installed_size_text: details
                    .map(|d| d.installed_size_text.clone())
                    .unwrap_or_default(),
                current_size: current.map(|p| p.installed_size),
                version: printed.version.clone(),
                package,
                role,
            });
        }
        InstallPlan {
            decision,
            targets: requested.to_vec(),
            providers,
            download_total: items.iter().map(|i| i.download_size).sum(),
            installed_total: items.iter().map(|i| i.installed_size).sum(),
            items,
        }
    }
}

/// The reads `real_choices` needs, through the engine's own caches.
struct EngineFacts<'e, 'a> {
    engine: &'e mut Engine<'a>,
    /// Everything the searches turned up, to name the candidates of a choice.
    found: Vec<PackageDetails>,
}

impl ChoiceFacts for EngineFacts<'_, '_> {
    type Error = PacmanError;

    fn candidates(&mut self, names: &[String]) -> Result<Vec<PackageDetails>, PacmanError> {
        let found = self.engine.facts.search_any(names)?;
        let refs: Vec<PackageRef> = found.iter().map(PackageRef::from).collect();
        self.engine.load_details(&refs)?;
        let details: Vec<PackageDetails> = refs
            .iter()
            .filter_map(|item| self.engine.details_of(item).cloned())
            .collect();
        self.found.extend(details.clone());
        Ok(details)
    }

    fn satisfied(&mut self, dependency: &Dependency) -> Result<bool, PacmanError> {
        self.engine.facts.satisfied(dependency)
    }
}

/// One package may answer several names, or be a target as well. The helper takes each package once (`validate`
/// refuses a duplicate), and a package already named covers every name it provides (the helper's own check skips
/// whatever was asked for), so the later answers are dropped.
fn named_once(requested: &[PackageRef], answers: Vec<ProviderAnswer>) -> Vec<ProviderAnswer> {
    let mut kept: Vec<ProviderAnswer> = Vec::new();
    for answer in answers {
        let named = requested.contains(&answer.chosen)
            || kept.iter().any(|other| other.chosen == answer.chosen);
        if !named {
            kept.push(answer);
        }
    }
    kept
}

fn printed_ref(printed: &PrintedPackage) -> PackageRef {
    PackageRef {
        repo: printed.repo.clone(),
        name: printed.name.clone(),
    }
}

#[cfg(test)]
mod tests;
