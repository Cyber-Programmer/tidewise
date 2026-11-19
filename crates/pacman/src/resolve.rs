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

#![allow(
    dead_code,
    reason = "the engine and its wiring to pacman come in the next commits (6.1)"
)]

use std::path::PathBuf;

use crate::{
    DetailsLookup, GroupMember, InstallDecision, InstalledPackage, PackageRef, PacmanError,
    PendingUpdate, PrintFailure, PrintedPackage, SearchResult,
};

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
    /// `None` when pacman does not know the group.
    fn group_members(&self, group: &str) -> Result<Option<Vec<GroupMember>>, PacmanError>;
    fn installed(&self) -> Result<Vec<InstalledPackage>, PacmanError>;
    fn pending_updates(&self) -> Result<Vec<PendingUpdate>, PacmanError>;
}

pub(crate) enum DryRun {
    Listed(Vec<PrintedPackage>),
    Failed(PrintFailure),
}

/// One resolve call against `facts`.
pub(crate) fn resolve_with(
    input: &ResolveInput,
    facts: &dyn Facts,
) -> Result<Resolution, PacmanError> {
    // Red-phase shim (6.1): does nothing. The engine comes in the next commit.
    let _ = (input, facts);
    Err(PacmanError::Inconsistent {
        reason: "the resolver is not built yet",
    })
}

#[cfg(test)]
mod tests;
