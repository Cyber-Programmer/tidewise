//! View model for the Apps screen: tabs, search, sort, selection and details.
//! Plain Rust. It never runs pacman; it is handed the result of `tidewise_pacman::installed_packages()`.

use tidewise_pacman::{InstallReason, InstalledPackage, PacmanError};

/// Sizes the way people read them: SI units (1 kB = 1 000 bytes), so "1000 kB" never appears.
///
/// Integer maths only. Each unit is rounded first; if that gives 1000 of the unit, the next unit is used.
pub fn format_size(bytes: u64) -> String {
    if bytes < 1_000 {
        return format!("{bytes} B");
    }
    let kilobytes = divide_rounded(bytes, 1_000);
    if kilobytes < 1_000 {
        return format!("{kilobytes} kB");
    }
    let tenths_of_mb = divide_rounded(bytes, 100_000);
    if tenths_of_mb < 10_000 {
        return format!("{}.{} MB", tenths_of_mb / 10, tenths_of_mb % 10);
    }
    let hundredths_of_gb = divide_rounded(bytes, 10_000_000);
    format!(
        "{}.{:02} GB",
        hundredths_of_gb / 100,
        hundredths_of_gb % 100
    )
}

/// `number / unit`, rounded to the nearest whole number (halves go up). `unit` is never 0 here.
fn divide_rounded(number: u64, unit: u64) -> u64 {
    let whole = number / unit;
    if number % unit >= unit / 2 {
        whole + 1
    } else {
        whole
    }
}

/// The three tabs on the Apps screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    /// "Installed by you".
    #[default]
    Explicit,
    /// "Dependencies": installed because another package needs them.
    Dependencies,
    /// "All".
    All,
}

/// How the list is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOrder {
    /// Largest first; equal sizes are ordered by name.
    #[default]
    SizeLargestFirst,
    /// Name A to Z.
    NameAToZ,
}

/// Plain-language text for a failure, plus the technical line behind the "details" switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorText {
    pub title: String,
    pub body: String,
    pub technical: String,
}

/// What the screen is doing: waiting, showing the list, or showing an error.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ListState {
    #[default]
    Loading,
    Loaded,
    Error(ErrorText),
}

/// One visible row of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub version: String,
    pub description: String,
    pub size_text: String,
    /// True for the one row that is currently selected.
    pub selected: bool,
}

/// How many packages each tab would show with the current search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TabCounts {
    pub explicit: usize,
    pub dependencies: usize,
    pub all: usize,
}

/// Everything the details panel shows for the selected package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Details {
    pub name: String,
    pub description: String,
    pub version: String,
    pub size_text: String,
    /// "You installed it" or "Another package needs it".
    pub why_here: String,
    /// The first 5 packages that need this one.
    pub needed_by: Vec<String>,
    /// How many more there are beyond `needed_by` (shown as "and N more").
    pub needed_by_more: usize,
    /// The exact size, for the technical details.
    pub size_bytes: u64,
}

/// What to say when the list is empty. Two different empty states, so the screen can help.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyState {
    /// There are rows (or the screen is loading or showing an error): no empty message.
    NotEmpty,
    /// This tab has no match, but another tab has `count` of them.
    OtherTabHas { tab: Tab, count: usize },
    /// Nothing matches on any tab.
    NothingMatches,
}

/// How many "needed by" names the details panel lists before "and N more".
const NEEDED_BY_SHOWN: usize = 5;

/// The Apps screen's state. Set the inputs with `set_*`, read what to show with the other methods.
#[derive(Debug, Default)]
pub struct AppsModel {
    state: ListState,
    packages: Vec<InstalledPackage>,
    tab: Tab,
    sort: SortOrder,
    /// Already trimmed and lower-cased; empty means no search.
    search: String,
    /// Name of the selected package. Always one of the visible rows, or `None` when there are none.
    selected: Option<String>,
    /// True from `begin_reload` until `set_result`: a worker thread is running.
    load_running: bool,
    /// True once a running load has taken too long (the screen adds a "taking longer" note).
    slow: bool,
}

impl AppsModel {
    /// A new model is `Loading` until `set_result` is called.
    pub fn new() -> Self {
        Self::default()
    }

    /// Hands the model what pacman answered.
    pub fn set_result(&mut self, result: Result<Vec<InstalledPackage>, PacmanError>) {
        self.load_running = false;
        self.slow = false;
        match result {
            Ok(packages) => {
                self.packages = packages;
                self.state = ListState::Loaded;
                self.fix_selection();
            }
            Err(error) => {
                self.packages.clear();
                self.selected = None;
                self.state = ListState::Error(error_text(&error));
            }
        }
    }

    /// Asks to start a load. `true` means "start the worker thread now"; `false` means one is already running.
    /// A brand-new model is `Loading` but has no load running, so the first call returns `true`.
    pub fn begin_reload(&mut self) -> bool {
        if self.load_running {
            return false;
        }
        self.load_running = true;
        self.slow = false;
        self.state = ListState::Loading;
        self.packages.clear();
        self.selected = None;
        true
    }

    /// Called when a load has run for too long. Ignored unless a load is running.
    pub fn mark_slow(&mut self) {
        if self.load_running {
            self.slow = true;
        }
    }

    /// Has the running load been marked slow?
    pub fn is_slow(&self) -> bool {
        self.slow
    }

    /// Package counts ignoring the search: what is on this system, whatever is typed.
    pub fn totals(&self) -> TabCounts {
        let count = |tab| self.packages.iter().filter(|p| tab_shows(tab, p)).count();
        TabCounts {
            explicit: count(Tab::Explicit),
            dependencies: count(Tab::Dependencies),
            all: count(Tab::All),
        }
    }

    pub fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.fix_selection();
    }

    pub fn set_sort(&mut self, sort: SortOrder) {
        self.sort = sort;
        self.fix_selection();
    }

    pub fn set_search(&mut self, search: &str) {
        self.search = search.trim().to_lowercase();
        self.fix_selection();
    }

    /// Selects a package by name. Ignored unless that package is in the visible list.
    pub fn select(&mut self, name: &str) {
        if self.visible().iter().any(|package| package.name == name) {
            self.selected = Some(name.to_string());
        }
    }

    /// Selects the next row. On the last row nothing changes.
    pub fn select_next(&mut self) {
        self.select_page(1);
    }

    /// Selects the previous row. On the first row nothing changes.
    pub fn select_previous(&mut self) {
        self.select_page(-1);
    }

    /// Selects the first row.
    pub fn select_first(&mut self) {
        self.select_page(isize::MIN);
    }

    /// Selects the last row.
    pub fn select_last(&mut self) {
        self.select_page(isize::MAX);
    }

    /// Moves the selection by `delta` rows (negative = up) and stops at the first or last row.
    pub fn select_page(&mut self, delta: isize) {
        let visible = self.visible();
        let Some(last) = visible.len().checked_sub(1) else {
            return;
        };
        let current = self
            .selected
            .as_ref()
            .and_then(|name| visible.iter().position(|package| &package.name == name))
            .unwrap_or(0);
        let target = current.saturating_add_signed(delta).min(last);
        if let Some(package) = visible.get(target) {
            self.selected = Some(package.name.clone());
        }
    }

    /// Keeps the selected package if it is still visible; otherwise selects the first visible row, or nothing.
    fn fix_selection(&mut self) {
        let visible = self.visible();
        let still_visible = self
            .selected
            .as_ref()
            .is_some_and(|name| visible.iter().any(|package| &package.name == name));
        if !still_visible {
            self.selected = visible.first().map(|package| package.name.clone());
        }
    }

    pub fn state(&self) -> &ListState {
        &self.state
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    pub fn sort(&self) -> SortOrder {
        self.sort
    }

    /// The rows to show: the current tab's packages.
    pub fn rows(&self) -> Vec<Row> {
        self.visible()
            .into_iter()
            .map(|package| Row {
                name: package.name.clone(),
                version: package.version.clone(),
                description: package.description.clone(),
                size_text: format_size(package.installed_size),
                selected: self.selected.as_deref() == Some(package.name.as_str()),
            })
            .collect()
    }

    /// With no search everything matches; otherwise the name or the description must contain the text.
    fn matches_search(&self, package: &InstalledPackage) -> bool {
        self.search.is_empty()
            || package.name.to_lowercase().contains(&self.search)
            || package.description.to_lowercase().contains(&self.search)
    }

    /// The packages the current tab lists, in the current order.
    fn visible(&self) -> Vec<&InstalledPackage> {
        let mut visible: Vec<&InstalledPackage> = self
            .packages
            .iter()
            .filter(|package| tab_shows(self.tab, package) && self.matches_search(package))
            .collect();
        match self.sort {
            SortOrder::SizeLargestFirst => visible.sort_by(|a, b| {
                b.installed_size
                    .cmp(&a.installed_size)
                    .then_with(|| compare_names(&a.name, &b.name))
            }),
            SortOrder::NameAToZ => visible.sort_by(|a, b| compare_names(&a.name, &b.name)),
        }
        visible
    }

    /// Details of the selected package, or `None` when nothing is selected.
    pub fn details(&self) -> Option<Details> {
        let name = self.selected.as_ref()?;
        let package = self.packages.iter().find(|p| &p.name == name)?;
        let why_here = match package.reason {
            InstallReason::Explicit => "You installed it",
            InstallReason::Dependency => "Another package needs it",
        };
        Some(Details {
            name: package.name.clone(),
            description: package.description.clone(),
            version: package.version.clone(),
            size_text: format_size(package.installed_size),
            why_here: why_here.to_string(),
            needed_by: package
                .required_by
                .iter()
                .take(NEEDED_BY_SHOWN)
                .cloned()
                .collect(),
            needed_by_more: package.required_by.len().saturating_sub(NEEDED_BY_SHOWN),
            size_bytes: package.installed_size,
        })
    }

    pub fn empty_state(&self) -> EmptyState {
        if self.state != ListState::Loaded {
            return EmptyState::NotEmpty;
        }
        let counts = self.tab_counts();
        match self.tab {
            Tab::Explicit if counts.explicit == 0 && counts.dependencies > 0 => {
                EmptyState::OtherTabHas {
                    tab: Tab::Dependencies,
                    count: counts.dependencies,
                }
            }
            Tab::Dependencies if counts.dependencies == 0 && counts.explicit > 0 => {
                EmptyState::OtherTabHas {
                    tab: Tab::Explicit,
                    count: counts.explicit,
                }
            }
            _ if counts.all == 0 => EmptyState::NothingMatches,
            _ => EmptyState::NotEmpty,
        }
    }

    pub fn tab_counts(&self) -> TabCounts {
        let count = |tab| {
            self.packages
                .iter()
                .filter(|p| tab_shows(tab, p) && self.matches_search(p))
                .count()
        };
        TabCounts {
            explicit: count(Tab::Explicit),
            dependencies: count(Tab::Dependencies),
            all: count(Tab::All),
        }
    }
}

/// Turns a pacman failure into the words the screen shows, plus the technical line for the details switch.
fn error_text(error: &PacmanError) -> ErrorText {
    match error {
        PacmanError::NotFound { .. } => ErrorText {
            title: "Tidewise can't find pacman".to_string(),
            body: "Tidewise is made for Arch Linux and reads your apps through pacman, \
                   but there is no pacman at /usr/bin/pacman on this system."
                .to_string(),
            technical: error.to_string(),
        },
        PacmanError::Spawn { .. } => ErrorText {
            title: "Tidewise couldn't start pacman".to_string(),
            body: "pacman is there but wouldn't start. Try again, or check its permissions."
                .to_string(),
            technical: error.to_string(),
        },
        PacmanError::Failed { code, stderr } => {
            let how = match code {
                Some(code) => format!("exit code {code}"),
                None => "stopped by a signal".to_string(),
            };
            let stderr = stderr.trim();
            ErrorText {
                title: "pacman couldn't list your packages".to_string(),
                body: "pacman stopped with an error before it finished. \
                       This is usually temporary; trying again often works."
                    .to_string(),
                technical: if stderr.is_empty() {
                    how
                } else {
                    format!("{how}: {stderr}")
                },
            }
        }
        PacmanError::Parse(parse_error) => ErrorText {
            title: "Tidewise couldn't read pacman's answer".to_string(),
            body: "The list from pacman looked incomplete, so Tidewise shows nothing \
                   rather than a list with packages missing."
                .to_string(),
            technical: parse_error.to_string(),
        },
        // Not reachable from the Apps list yet (it never searches); the search screen (4.6) will use it.
        PacmanError::BadQuery { .. } => ErrorText {
            title: "That search can't be used".to_string(),
            body: "Tidewise didn't search because the text was too long or had characters \
                   it can't use. Try a shorter, simpler search."
                .to_string(),
            technical: error.to_string(),
        },
        // The details screen (4.6) only passes names that pacman itself printed, so this is a bug in Tidewise.
        PacmanError::BadPackageRef { .. } => ErrorText {
            title: "Tidewise asked pacman about an invalid package".to_string(),
            body: "This is a bug in Tidewise, not something you did. Please report it.".to_string(),
            technical: error.to_string(),
        },
        // Placeholder until the green commit; the Home/Apps screens will show it when the lists can't be read.
        PacmanError::ReadLists { .. } => ErrorText {
            title: String::new(),
            body: String::new(),
            technical: error.to_string(),
        },
    }
}

/// Names compare ignoring case; if only the case differs, the exact text decides, so the order never wobbles.
fn compare_names(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

/// Does this tab list this package?
fn tab_shows(tab: Tab, package: &InstalledPackage) -> bool {
    match tab {
        Tab::Explicit => package.reason == InstallReason::Explicit,
        Tab::Dependencies => package.reason == InstallReason::Dependency,
        Tab::All => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(
        name: &str,
        description: &str,
        size: u64,
        reason: InstallReason,
        required_by: &[&str],
    ) -> InstalledPackage {
        InstalledPackage {
            name: name.to_string(),
            version: "1.0-1".to_string(),
            description: description.to_string(),
            installed_size: size,
            reason,
            required_by: required_by.iter().map(|n| n.to_string()).collect(),
        }
    }

    /// 3 explicit (firefox, git, vim; git and vim have equal sizes) and 2 dependencies (glibc, zlib).
    fn sample() -> Vec<InstalledPackage> {
        use InstallReason::{Dependency, Explicit};
        vec![
            pkg("firefox", "Web browser", 250_000_000, Explicit, &[]),
            pkg("git", "Version control", 40_000_000, Explicit, &[]),
            pkg(
                "glibc",
                "The C library",
                50_000_000,
                Dependency,
                &["firefox", "git"],
            ),
            pkg("vim", "Text editor", 40_000_000, Explicit, &[]),
            pkg("zlib", "Compression library", 300_000, Dependency, &["git"]),
        ]
    }

    fn loaded(packages: Vec<InstalledPackage>) -> AppsModel {
        let mut model = AppsModel::new();
        model.set_result(Ok(packages));
        model
    }

    fn names(model: &AppsModel) -> Vec<String> {
        model.rows().into_iter().map(|row| row.name).collect()
    }

    /// Names of the rows marked as selected.
    fn selected(model: &AppsModel) -> Vec<String> {
        model
            .rows()
            .into_iter()
            .filter(|row| row.selected)
            .map(|row| row.name)
            .collect()
    }

    #[test]
    fn small_sizes_are_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(999), "999 B");
    }

    #[test]
    fn kilobytes_are_whole_numbers_rounded_to_nearest() {
        assert_eq!(format_size(1000), "1 kB");
        assert_eq!(format_size(1499), "1 kB");
        assert_eq!(format_size(1500), "2 kB");
        assert_eq!(format_size(999_499), "999 kB");
    }

    #[test]
    fn rounding_up_moves_to_the_next_unit() {
        // 999 500 bytes would round to "1000 kB": it must become "1.0 MB".
        assert_eq!(format_size(999_500), "1.0 MB");
        assert_eq!(format_size(999_949_999), "999.9 MB");
        // 999 950 000 bytes would round to "1000.0 MB": it must become "1.00 GB".
        assert_eq!(format_size(999_950_000), "1.00 GB");
    }

    #[test]
    fn megabytes_have_one_decimal_and_gigabytes_two() {
        assert_eq!(format_size(1_250_000), "1.3 MB");
        assert_eq!(format_size(12_345_678_901), "12.35 GB");
    }

    #[test]
    fn the_largest_number_does_not_panic() {
        assert_eq!(format_size(u64::MAX), "18446744073.71 GB");
    }

    #[test]
    fn a_new_model_is_loading_with_nothing_to_show() {
        let model = AppsModel::new();
        assert_eq!(model.state(), &ListState::Loading);
        assert!(model.rows().is_empty());
    }

    #[test]
    fn after_loading_the_default_tab_shows_only_what_you_installed() {
        let model = loaded(sample());
        assert_eq!(model.state(), &ListState::Loaded);
        assert_eq!(model.tab(), Tab::Explicit);
        let mut shown = names(&model);
        shown.sort();
        assert_eq!(shown, ["firefox", "git", "vim"]);
    }

    #[test]
    fn without_a_search_the_counts_are_the_full_totals() {
        let counts = loaded(sample()).tab_counts();
        assert_eq!(
            counts,
            TabCounts {
                explicit: 3,
                dependencies: 2,
                all: 5
            }
        );
    }

    #[test]
    fn the_dependencies_tab_shows_only_dependencies() {
        let mut model = loaded(sample());
        model.set_tab(Tab::Dependencies);
        assert_eq!(model.tab(), Tab::Dependencies);
        let mut shown = names(&model);
        shown.sort();
        assert_eq!(shown, ["glibc", "zlib"]);
    }

    #[test]
    fn the_all_tab_shows_everything() {
        let mut model = loaded(sample());
        model.set_tab(Tab::All);
        assert_eq!(names(&model).len(), 5);
    }

    #[test]
    fn the_default_order_is_largest_first_and_ties_go_by_name() {
        let mut model = loaded(sample());
        model.set_tab(Tab::All);
        // git and vim are both 40 MB: git comes first because of its name.
        assert_eq!(names(&model), ["firefox", "glibc", "git", "vim", "zlib"]);
    }

    #[test]
    fn name_order_is_a_to_z() {
        let mut model = loaded(sample());
        model.set_tab(Tab::All);
        model.set_sort(SortOrder::NameAToZ);
        assert_eq!(names(&model), ["firefox", "git", "glibc", "vim", "zlib"]);
    }

    #[test]
    fn name_order_ignores_case_and_is_still_stable() {
        use InstallReason::Explicit;
        let mut model = loaded(vec![
            pkg("b", "", 1, Explicit, &[]),
            pkg("a", "", 1, Explicit, &[]),
            pkg("A", "", 1, Explicit, &[]),
        ]);
        model.set_sort(SortOrder::NameAToZ);
        assert_eq!(names(&model), ["A", "a", "b"]);
    }

    #[test]
    fn search_matches_the_name_or_the_description() {
        let mut model = loaded(sample());
        model.set_tab(Tab::All);
        model.set_search("git"); // only the name of git matches
        assert_eq!(names(&model), ["git"]);
        model.set_search("library"); // glibc and zlib have it in the description
        assert_eq!(names(&model), ["glibc", "zlib"]);
    }

    #[test]
    fn search_ignores_case_and_surrounding_spaces() {
        let mut model = loaded(sample());
        model.set_tab(Tab::All);
        model.set_search("  FireFOX \t");
        assert_eq!(names(&model), ["firefox"]);
    }

    #[test]
    fn the_counts_follow_the_search_and_come_back_when_it_is_cleared() {
        let mut model = loaded(sample());
        model.set_search("library");
        assert_eq!(
            model.tab_counts(),
            TabCounts {
                explicit: 0,
                dependencies: 2,
                all: 2
            }
        );
        model.set_search("   ");
        assert_eq!(model.tab_counts().all, 5);
    }

    #[test]
    fn an_empty_tab_points_to_the_other_tab_that_has_matches() {
        let mut model = loaded(sample());
        model.set_search("library"); // only dependencies match; the default tab is Explicit
        assert_eq!(
            model.empty_state(),
            EmptyState::OtherTabHas {
                tab: Tab::Dependencies,
                count: 2
            }
        );
        model.set_tab(Tab::Dependencies);
        model.set_search("firefox");
        assert_eq!(
            model.empty_state(),
            EmptyState::OtherTabHas {
                tab: Tab::Explicit,
                count: 1
            }
        );
    }

    #[test]
    fn when_nothing_matches_anywhere_that_is_a_different_empty_state() {
        let mut model = loaded(sample());
        model.set_search("no such app");
        assert_eq!(model.empty_state(), EmptyState::NothingMatches);
        model.set_tab(Tab::All);
        assert_eq!(model.empty_state(), EmptyState::NothingMatches);
    }

    #[test]
    fn there_is_no_empty_message_when_there_are_rows_or_while_loading() {
        assert_eq!(AppsModel::new().empty_state(), EmptyState::NotEmpty);
        assert_eq!(loaded(sample()).empty_state(), EmptyState::NotEmpty);
    }

    #[test]
    fn after_loading_exactly_one_visible_row_is_selected_the_first() {
        let model = loaded(sample());
        assert_eq!(selected(&model), ["firefox"]);
    }

    #[test]
    fn the_selection_stays_while_the_package_is_still_visible() {
        let mut model = loaded(sample());
        model.select("vim");
        assert_eq!(selected(&model), ["vim"]);
        model.set_sort(SortOrder::NameAToZ);
        assert_eq!(selected(&model), ["vim"]);
        model.set_search("e"); // firefox, vim, ... still contain an "e" somewhere
        assert_eq!(selected(&model), ["vim"]);
        model.set_search("");
        assert_eq!(selected(&model), ["vim"]);
    }

    #[test]
    fn the_selection_moves_to_the_first_row_when_its_package_is_hidden() {
        let mut model = loaded(sample());
        model.select("vim");
        model.set_search("firefox");
        assert_eq!(selected(&model), ["firefox"]);

        let mut model = loaded(sample());
        model.select("vim");
        model.set_tab(Tab::Dependencies); // largest first: glibc
        assert_eq!(selected(&model), ["glibc"]);
    }

    #[test]
    fn nothing_is_selected_when_the_list_is_empty() {
        let mut model = loaded(sample());
        model.set_search("no such app");
        assert!(selected(&model).is_empty());
        assert!(selected(&AppsModel::new()).is_empty());
    }

    #[test]
    fn selecting_a_package_that_is_not_visible_is_ignored() {
        let mut model = loaded(sample());
        model.select("glibc"); // a dependency; the Explicit tab does not show it
        model.select("no-such-package");
        assert_eq!(selected(&model), ["firefox"]);
    }

    #[test]
    fn details_describe_the_selected_package() {
        let mut model = loaded(sample());
        model.set_tab(Tab::All);
        model.select("glibc");
        let details = model.details().unwrap();
        assert_eq!(details.name, "glibc");
        assert_eq!(details.description, "The C library");
        assert_eq!(details.version, "1.0-1");
        assert_eq!(details.size_text, "50.0 MB");
        assert_eq!(details.size_bytes, 50_000_000);
        assert_eq!(details.why_here, "Another package needs it");
        assert_eq!(details.needed_by, ["firefox", "git"]);
        assert_eq!(details.needed_by_more, 0);
    }

    #[test]
    fn details_say_when_you_installed_it() {
        let model = loaded(sample());
        let details = model.details().unwrap();
        assert_eq!(details.name, "firefox");
        assert_eq!(details.why_here, "You installed it");
        assert!(details.needed_by.is_empty());
        assert_eq!(details.needed_by_more, 0);
    }

    #[test]
    fn needed_by_shows_five_names_and_counts_the_rest() {
        use InstallReason::Explicit;
        let five = ["a", "b", "c", "d", "e"];
        let seven = ["a", "b", "c", "d", "e", "f", "g"];
        let model = loaded(vec![pkg("five", "", 1, Explicit, &five)]);
        let details = model.details().unwrap();
        assert_eq!(details.needed_by, five);
        assert_eq!(details.needed_by_more, 0);

        let model = loaded(vec![pkg("seven", "", 1, Explicit, &seven)]);
        let details = model.details().unwrap();
        assert_eq!(details.needed_by, ["a", "b", "c", "d", "e"]);
        assert_eq!(details.needed_by_more, 2);
    }

    #[test]
    fn there_are_no_details_when_nothing_is_selected() {
        assert_eq!(AppsModel::new().details(), None);
        let mut model = loaded(sample());
        model.set_search("no such app");
        assert_eq!(model.details(), None);
    }

    fn error_of(error: PacmanError) -> ErrorText {
        let mut model = AppsModel::new();
        model.set_result(Err(error));
        match model.state() {
            ListState::Error(text) => text.clone(),
            other => panic!("expected the Error state, got {other:?}"),
        }
    }

    #[test]
    fn pacman_not_found_is_explained_in_plain_words() {
        let text = error_of(PacmanError::NotFound {
            program: "/usr/bin/pacman".into(),
        });
        assert_eq!(text.title, "Tidewise can't find pacman");
        assert_eq!(
            text.body,
            "Tidewise is made for Arch Linux and reads your apps through pacman, \
             but there is no pacman at /usr/bin/pacman on this system."
        );
        assert_eq!(text.technical, "pacman not found at /usr/bin/pacman");
    }

    #[test]
    fn pacman_that_will_not_start_is_explained() {
        let text = error_of(PacmanError::Spawn {
            program: "/usr/bin/pacman".into(),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        });
        assert_eq!(text.title, "Tidewise couldn't start pacman");
        assert_eq!(
            text.body,
            "pacman is there but wouldn't start. Try again, or check its permissions."
        );
        assert_eq!(text.technical, "could not start /usr/bin/pacman: denied");
    }

    #[test]
    fn a_pacman_failure_shows_its_stderr_and_exit_code() {
        let text = error_of(PacmanError::Failed {
            code: Some(1),
            stderr: "error: unable to lock database\n".to_string(),
        });
        assert_eq!(text.title, "pacman couldn't list your packages");
        assert_eq!(
            text.body,
            "pacman stopped with an error before it finished. \
             This is usually temporary; trying again often works."
        );
        assert_eq!(
            text.technical,
            "exit code 1: error: unable to lock database"
        );

        let text = error_of(PacmanError::Failed {
            code: None,
            stderr: String::new(),
        });
        assert_eq!(text.technical, "stopped by a signal");
    }

    #[test]
    fn a_parse_failure_shows_the_parse_error() {
        let parse_error = tidewise_pacman::ParseError::MissingField {
            package: Some("zsh".to_string()),
            field: "Installed Size",
        };
        let technical = parse_error.to_string();
        let text = error_of(PacmanError::Parse(parse_error));
        assert_eq!(text.title, "Tidewise couldn't read pacman's answer");
        assert_eq!(
            text.body,
            "The list from pacman looked incomplete, so Tidewise shows nothing \
             rather than a list with packages missing."
        );
        assert_eq!(text.technical, technical);
        assert!(technical.contains("zsh"));
    }

    #[test]
    fn a_refused_search_text_is_explained_in_plain_words() {
        let error = PacmanError::BadQuery {
            reason: "the search text is too long",
        };
        let technical = error.to_string();
        let text = error_of(error);
        assert_eq!(text.title, "That search can't be used");
        assert_eq!(
            text.body,
            "Tidewise didn't search because the text was too long or had characters \
             it can't use. Try a shorter, simpler search."
        );
        assert_eq!(
            technical,
            "search text not accepted: the search text is too long"
        );
        assert_eq!(text.technical, technical);
    }

    #[test]
    fn a_refused_package_reference_is_reported_as_a_bug() {
        let error = PacmanError::BadPackageRef {
            reason: "the package name is not valid",
        };
        let technical = error.to_string();
        let text = error_of(error);
        assert_eq!(text.title, "Tidewise asked pacman about an invalid package");
        assert_eq!(
            text.body,
            "This is a bug in Tidewise, not something you did. Please report it."
        );
        assert_eq!(
            technical,
            "package reference not accepted: the package name is not valid"
        );
        assert_eq!(text.technical, technical);
    }

    #[test]
    fn unreadable_package_lists_are_reported_in_plain_words() {
        let error = PacmanError::ReadLists {
            path: std::path::PathBuf::from("/var/lib/pacman/sync/core.db"),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        };
        let technical = error.to_string();
        let text = error_of(error);
        assert_eq!(text.title, "Tidewise couldn't read your package lists");
        assert_eq!(
            text.body,
            "Tidewise couldn't check how up to date your package lists are, because one of \
             pacman's database files couldn't be read. Try again, or check the folder's permissions."
        );
        assert_eq!(
            technical,
            "could not read the package list /var/lib/pacman/sync/core.db: denied"
        );
        assert_eq!(text.technical, technical);
    }

    #[test]
    fn an_error_replaces_a_loaded_list() {
        let mut model = loaded(sample());
        model.set_result(Err(PacmanError::Failed {
            code: Some(1),
            stderr: String::new(),
        }));
        assert!(matches!(model.state(), ListState::Error(_)));
        assert!(model.rows().is_empty());
        assert_eq!(model.tab_counts(), TabCounts::default());
        assert_eq!(model.details(), None);
        assert_eq!(model.empty_state(), EmptyState::NotEmpty);
    }

    #[test]
    fn a_later_good_answer_recovers_from_an_error() {
        let mut model = AppsModel::new();
        model.set_result(Err(PacmanError::Failed {
            code: Some(1),
            stderr: String::new(),
        }));
        model.set_result(Ok(sample()));
        assert_eq!(model.state(), &ListState::Loaded);
        assert_eq!(selected(&model), ["firefox"]);
    }

    #[test]
    fn real_parsed_pacman_output_fits_the_model() {
        let text = include_str!("../../../tests/fixtures/pacman/qi-multi.txt");
        let mut model = loaded(tidewise_pacman::parse_qi(text).unwrap());

        // base: installed by you, 0 bytes.
        let rows = model.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "base");
        assert_eq!(rows[0].size_text, "0 B");

        // Dependencies, largest first: 7 MiB then 1701.73 KiB.
        model.set_tab(Tab::Dependencies);
        let rows = model.rows();
        let shown: Vec<(&str, &str, &str)> = rows
            .iter()
            .map(|r| (r.name.as_str(), r.version.as_str(), r.size_text.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                ("linux-api-headers", "7.2-1", "7.3 MB"),
                ("tzdata", "2026e-1", "1.7 MB")
            ]
        );

        let details = model.details().unwrap();
        assert_eq!(details.name, "linux-api-headers");
        assert_eq!(details.needed_by, ["glibc", "libbpf", "libxdp"]);
        assert_eq!(details.size_bytes, 7 * 1024 * 1024);
    }
    #[test]
    fn the_first_load_may_start_on_a_new_model() {
        let mut model = AppsModel::new();
        assert!(model.begin_reload());
    }

    #[test]
    fn a_second_load_is_refused_while_one_is_running() {
        let mut model = AppsModel::new();
        assert!(model.begin_reload());
        assert!(!model.begin_reload());
        assert_eq!(model.state(), &ListState::Loading);
    }

    #[test]
    fn a_new_load_may_start_after_the_answer_arrived() {
        let mut model = AppsModel::new();
        assert!(model.begin_reload());
        model.set_result(Err(PacmanError::Failed {
            code: Some(1),
            stderr: String::new(),
        }));
        assert!(model.begin_reload());
        model.set_result(Ok(sample()));
        assert!(model.begin_reload());
    }

    #[test]
    fn starting_a_reload_clears_the_list_and_goes_back_to_loading() {
        let mut model = loaded(sample());
        assert!(model.begin_reload());
        assert_eq!(model.state(), &ListState::Loading);
        assert!(model.rows().is_empty());
        assert_eq!(model.totals(), TabCounts::default());
        assert_eq!(model.details(), None);
    }

    #[test]
    fn a_reload_keeps_the_tab_and_the_sort_but_not_the_old_selection() {
        let mut model = loaded(sample());
        model.set_tab(Tab::All);
        model.set_sort(SortOrder::NameAToZ);
        model.select("zlib");
        assert!(model.begin_reload());
        model.set_result(Ok(sample()));
        assert_eq!(model.tab(), Tab::All);
        assert_eq!(names(&model), ["firefox", "git", "glibc", "vim", "zlib"]);
        assert_eq!(selected(&model), ["firefox"]);
    }

    #[test]
    fn a_load_is_slow_only_after_it_is_marked_and_only_while_running() {
        let mut model = AppsModel::new();
        model.mark_slow(); // no load running yet: ignored
        assert!(!model.is_slow());
        assert!(model.begin_reload());
        assert!(!model.is_slow());
        model.mark_slow();
        assert!(model.is_slow());
        model.set_result(Ok(sample()));
        assert!(!model.is_slow());
        model.mark_slow(); // a late timer after the answer: ignored
        assert!(!model.is_slow());
    }

    #[test]
    fn a_new_load_forgets_that_the_last_one_was_slow() {
        let mut model = AppsModel::new();
        assert!(model.begin_reload());
        model.mark_slow();
        model.set_result(Ok(sample()));
        assert!(model.begin_reload());
        assert!(!model.is_slow());
    }

    #[test]
    fn totals_ignore_the_search() {
        let mut model = loaded(sample());
        model.set_search("library");
        assert_eq!(model.tab_counts().all, 2);
        assert_eq!(
            model.totals(),
            TabCounts {
                explicit: 3,
                dependencies: 2,
                all: 5
            }
        );
    }
    /// Tab All, largest first: firefox, glibc, git, vim, zlib.
    fn all_tab() -> AppsModel {
        let mut model = loaded(sample());
        model.set_tab(Tab::All);
        model
    }

    #[test]
    fn next_and_previous_move_one_row() {
        let mut model = all_tab();
        model.select_next();
        assert_eq!(selected(&model), ["glibc"]);
        model.select_next();
        assert_eq!(selected(&model), ["git"]);
        model.select_previous();
        assert_eq!(selected(&model), ["glibc"]);
    }

    #[test]
    fn at_the_ends_next_and_previous_change_nothing() {
        let mut model = all_tab();
        model.select_previous();
        assert_eq!(selected(&model), ["firefox"]);
        model.select_last();
        model.select_next();
        assert_eq!(selected(&model), ["zlib"]);
    }

    #[test]
    fn first_and_last_jump_to_the_ends() {
        let mut model = all_tab();
        model.select_last();
        assert_eq!(selected(&model), ["zlib"]);
        model.select_first();
        assert_eq!(selected(&model), ["firefox"]);
    }

    #[test]
    fn a_page_moves_by_the_given_number_of_rows() {
        let mut model = all_tab();
        model.select_page(3);
        assert_eq!(selected(&model), ["vim"]);
        model.select_page(-2);
        assert_eq!(selected(&model), ["glibc"]);
        model.select_page(0);
        assert_eq!(selected(&model), ["glibc"]);
    }

    #[test]
    fn a_page_past_the_end_stops_at_the_first_or_last_row() {
        let mut model = all_tab();
        model.select_page(100);
        assert_eq!(selected(&model), ["zlib"]);
        model.select_page(isize::MAX);
        assert_eq!(selected(&model), ["zlib"]);
        model.select_page(-100);
        assert_eq!(selected(&model), ["firefox"]);
        model.select_page(isize::MIN);
        assert_eq!(selected(&model), ["firefox"]);
    }

    #[test]
    fn moving_follows_the_current_order_and_tab() {
        let mut model = all_tab();
        model.set_sort(SortOrder::NameAToZ); // firefox, git, glibc, vim, zlib
        model.select_next();
        assert_eq!(selected(&model), ["git"]);
        model.set_tab(Tab::Dependencies); // glibc, zlib by name; git is hidden so glibc is selected
        model.select_next();
        assert_eq!(selected(&model), ["zlib"]);
    }

    #[test]
    fn moving_with_no_rows_does_nothing() {
        let mut model = loaded(sample());
        model.set_search("no such app");
        model.select_next();
        model.select_previous();
        model.select_first();
        model.select_last();
        model.select_page(5);
        assert!(selected(&model).is_empty());
        let mut loading = AppsModel::new();
        loading.select_next();
        assert!(selected(&loading).is_empty());
    }

    #[test]
    fn moving_keeps_exactly_one_row_selected_and_the_details_follow() {
        let mut model = all_tab();
        model.select_page(2);
        assert_eq!(selected(&model).len(), 1);
        assert_eq!(model.details().unwrap().name, "git");
    }
}
