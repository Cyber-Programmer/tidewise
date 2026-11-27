#![no_main]

use libfuzzer_sys::fuzz_target;
use tidewise_pacman::{ChoiceFacts, Dependency, PackageDetails, PackageRef};

// The provider rules over outside data (WBS 6.5, reviews R-02): any `-Si` text that parses is a list, and the same
// records are what a search finds. `provider_picks` and `real_choices` must not panic, and the search pattern built
// from their names must only ever join names with `|` inside one anchored group.
struct Repo(Vec<PackageDetails>);

impl ChoiceFacts for Repo {
    type Error = ();
    fn candidates(&mut self, names: &[String]) -> Result<Vec<PackageDetails>, ()> {
        assert!(names.len() <= tidewise_pacman::SEARCH_BATCH);
        let pattern = tidewise_pacman::exact_pattern_any(names);
        assert!(pattern.starts_with("^(") && pattern.ends_with(")$"));
        Ok(self.0.clone())
    }
    fn satisfied(&mut self, dependency: &Dependency) -> Result<bool, ()> {
        Ok(tidewise_pacman::dependency_text(dependency).is_some())
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(records) = tidewise_pacman::parse_si(text) else {
        return;
    };
    let list: Vec<PackageRef> = records
        .iter()
        .map(|d| PackageRef {
            repo: d.repo.clone(),
            name: d.name.clone(),
        })
        .collect();
    let picks = tidewise_pacman::provider_picks(
        &list,
        |item| {
            records
                .iter()
                .find(|d| d.repo == item.repo && d.name == item.name)
        },
        &list[..list.len().min(1)],
        &[],
        |name| name.len() % 3 == 0,
    );
    let listed: Vec<&PackageDetails> = records.iter().collect();
    let before = picks.len();
    let left = tidewise_pacman::real_choices(
        picks,
        &listed,
        |name| name.len() % 3 == 0,
        &mut Repo(records.clone()),
    );
    assert!(left.is_ok_and(|kept| kept.len() <= before));
});
