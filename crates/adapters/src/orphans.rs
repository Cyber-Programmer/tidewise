//! Packages nothing needs any more ("orphans"), read from what the package manager says without
//! changing anything: `pacman -Qdtq` (+ `pacman -Qi` for sizes), `apt-get -s autoremove`, and
//! `dnf autoremove --assumeno`. Removing them needs admin access, so this module only lists.
//!
//! Anything that is not C-locale output in the expected shape is an error, never a guess.

/// Which package manager printed the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manager {
    Pacman,
    Apt,
    Dnf,
}

/// What a scan is given: the raw text of the two read-only commands of one package manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanInput {
    pub manager: Manager,
    /// `pacman -Qdtq` / `apt-get -s autoremove` / `dnf autoremove --assumeno` (stdout and stderr).
    pub list: String,
    /// `pacman -Qi -- NAMES` / `dpkg-query -W -f=…` / `rpm -qa --qf '%{NAME}\t%{SIZE}\n'`, for sizes.
    pub sizes: String,
}

/// One package nothing needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Orphan {
    pub name: String,
    /// Installed size in bytes; 0 when the package manager did not say.
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("this is not the expected output of the package manager in the C locale")]
pub struct Unrecognized;

/// Read one manager's input.
pub fn read(input: &OrphanInput) -> Result<Vec<Orphan>, Unrecognized> {
    match input.manager {
        Manager::Pacman => from_pacman(&input.list, &input.sizes),
        Manager::Apt => from_apt(&input.list, &input.sizes),
        Manager::Dnf => from_dnf(&input.list, &input.sizes),
    }
}

/// `pacman -Qdtq` prints one name per line (nothing at all, and exit 1, when there are none).
pub fn from_pacman(_names: &str, _info: &str) -> Result<Vec<Orphan>, Unrecognized> {
    Ok(Vec::new())
}

/// `apt-get -s autoremove`: the `Remv name [version]` lines; sizes from `dpkg-query`.
pub fn from_apt(_autoremove: &str, _dpkg_query: &str) -> Result<Vec<Orphan>, Unrecognized> {
    Ok(Vec::new())
}

/// `dnf autoremove --assumeno`: the packages under `Removing:`; sizes from `rpm -qa`.
pub fn from_dnf(_autoremove: &str, _rpm_qa: &str) -> Result<Vec<Orphan>, Unrecognized> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(rel: &str) -> String {
        let path = format!("{}/../../tests/fixtures/{rel}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(path).unwrap()
    }

    fn names(o: &[Orphan]) -> Vec<&str> {
        o.iter().map(|o| o.name.as_str()).collect()
    }

    #[test]
    fn real_pacman_orphans_are_the_listed_names_with_their_installed_sizes() {
        let (list, info) = (fx("pacman/Qdtq-real.txt"), fx("pacman/Qi-orphans-real.txt"));
        let o = from_pacman(&list, &info).unwrap();
        assert_eq!(o.len(), 25);
        assert_eq!(o[0].name, "asar");
        assert!(o.iter().all(|x| x.bytes > 0), "every size was found");
        let qi = crate::pacman::parse_qi(&info).unwrap();
        let total: u64 = qi.iter().map(|p| p.installed_size_bytes).sum();
        assert_eq!(o.iter().map(|x| x.bytes).sum::<u64>(), total);
    }

    #[test]
    fn no_pacman_orphans_is_an_empty_list_not_an_error() {
        assert_eq!(from_pacman("", ""), Ok(vec![]));
        assert_eq!(from_pacman("\n", ""), Ok(vec![]));
    }

    #[test]
    fn pacman_lines_that_are_not_package_names_are_refused() {
        for bad in [
            "--force\n",
            "two words\n",
            "error: no packages\n",
            "$(id)\n",
            "../x\n",
        ] {
            assert_eq!(from_pacman(bad, ""), Err(Unrecognized), "{bad:?}");
        }
    }

    #[test]
    fn apt_autoremove_gives_the_remv_lines_with_sizes_from_dpkg() {
        let list = fx("orphans/apt-autoremove-ok-handwritten.txt");
        let dpkg = fx("apt/dpkg-query-handwritten.txt");
        let o = from_apt(&list, &dpkg).unwrap();
        assert_eq!(names(&o), ["linux-image-5.15.0-91-generic", "vlc-bin"]);
        let known = crate::apt::parse_dpkg_query(&dpkg).unwrap();
        for orphan in &o {
            let want = known
                .iter()
                .find(|p| p.name == orphan.name)
                .and_then(|p| p.installed_size_bytes)
                .unwrap();
            assert_eq!(orphan.bytes, want, "{}", orphan.name);
        }
    }

    #[test]
    fn apt_with_nothing_to_remove_is_empty_and_other_text_is_refused() {
        let dpkg = fx("apt/dpkg-query-handwritten.txt");
        assert_eq!(
            from_apt(&fx("orphans/apt-autoremove-none-handwritten.txt"), &dpkg),
            Ok(vec![])
        );
        for refused in [
            "apt-autoremove-de-handwritten.txt",
            "apt-autoremove-garbage-handwritten.txt",
        ] {
            assert_eq!(
                from_apt(&fx(&format!("orphans/{refused}")), &dpkg),
                Err(Unrecognized),
                "{refused}"
            );
        }
        assert_eq!(from_apt("", &dpkg), Err(Unrecognized));
    }

    #[test]
    fn dnf_autoremove_gives_the_removing_rows_with_sizes_from_rpm() {
        let list = fx("orphans/dnf-autoremove-ok-handwritten.txt");
        let rpm = fx("dnf/rpm-qa-handwritten.txt");
        let o = from_dnf(&list, &rpm).unwrap();
        assert_eq!(names(&o), ["kernel-core", "vlc-libs"]);
        assert_eq!(o[0].bytes, 70_000_000);
        assert_eq!(o[1].bytes, 51_234);
    }

    #[test]
    fn dnf_with_nothing_to_do_is_empty_and_other_text_is_refused() {
        let rpm = fx("dnf/rpm-qa-handwritten.txt");
        assert_eq!(
            from_dnf(&fx("orphans/dnf-autoremove-none-handwritten.txt"), &rpm),
            Ok(vec![])
        );
        for refused in [
            "dnf-autoremove-de-handwritten.txt",
            "dnf-autoremove-garbage-handwritten.txt",
        ] {
            assert_eq!(
                from_dnf(&fx(&format!("orphans/{refused}")), &rpm),
                Err(Unrecognized),
                "{refused}"
            );
        }
        assert_eq!(from_dnf("", &rpm), Err(Unrecognized));
    }

    #[test]
    fn a_package_whose_size_is_unknown_counts_as_zero() {
        let list = fx("orphans/apt-autoremove-ok-handwritten.txt");
        let o = from_apt(&list, "").unwrap();
        assert_eq!(o.iter().map(|x| x.bytes).sum::<u64>(), 0);
        assert_eq!(o.len(), 2);
    }

    #[test]
    fn read_picks_the_parser_of_the_manager() {
        let input = OrphanInput {
            manager: Manager::Dnf,
            list: fx("orphans/dnf-autoremove-none-handwritten.txt"),
            sizes: String::new(),
        };
        assert_eq!(read(&input), Ok(vec![]));
    }
}
