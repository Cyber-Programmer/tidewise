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

/// The Apps screen's state. Set the inputs with `set_*`, read what to show with the other methods.
#[derive(Debug, Default)]
pub struct AppsModel {
    state: ListState,
    packages: Vec<InstalledPackage>,
    tab: Tab,
}

impl AppsModel {
    /// A new model is `Loading` until `set_result` is called.
    pub fn new() -> Self {
        Self::default()
    }

    /// Hands the model what pacman answered.
    pub fn set_result(&mut self, result: Result<Vec<InstalledPackage>, PacmanError>) {
        match result {
            Ok(packages) => {
                self.packages = packages;
                self.state = ListState::Loaded;
            }
            Err(_error) => {}
        }
    }

    pub fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
    }

    pub fn state(&self) -> &ListState {
        &self.state
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    /// The rows to show: the current tab's packages.
    pub fn rows(&self) -> Vec<Row> {
        self.packages
            .iter()
            .filter(|package| tab_shows(self.tab, package))
            .map(|package| Row {
                name: package.name.clone(),
                version: package.version.clone(),
                description: package.description.clone(),
                size_text: format_size(package.installed_size),
                selected: false,
            })
            .collect()
    }

    pub fn tab_counts(&self) -> TabCounts {
        let count = |tab| self.packages.iter().filter(|p| tab_shows(tab, p)).count();
        TabCounts {
            explicit: count(Tab::Explicit),
            dependencies: count(Tab::Dependencies),
            all: count(Tab::All),
        }
    }
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
}
