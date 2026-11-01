//! Runs the real, read-only pacman on this machine. Skipped where pacman is not installed.

use std::path::Path;

use tidewise_adapters::pacman::Pacman;

#[test]
fn lists_real_installed_packages() {
    if !Path::new("/usr/bin/pacman").exists() {
        eprintln!("skipped: pacman not installed");
        return;
    }
    let pacman = Pacman::system();
    let packages = pacman.installed().unwrap();
    assert!(!packages.is_empty());
    let pacman_pkg = packages.iter().find(|p| p.name == "pacman").unwrap();
    assert!(pacman_pkg.installed_size_bytes > 0);

    let orphans = pacman.orphans().unwrap();
    let names: Vec<_> = packages.iter().map(|p| &p.name).collect();
    assert!(orphans.iter().all(|o| names.contains(&o)));
}

#[test]
fn real_machine_lists_apps_from_every_source_and_plans_without_changing_anything() {
    use tidewise_adapters::appimage::AppImageApps;
    use tidewise_adapters::flatpak::FlatpakApps;
    use tidewise_adapters::pacman::PacmanApps;
    use tidewise_adapters::source::{list_all, AppSource, SourceKind};
    if !Path::new("/usr/bin/pacman").exists() {
        eprintln!("skipped: pacman not installed");
        return;
    }
    let pacman = PacmanApps::system();
    let flatpak = FlatpakApps::system();
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap();
    let appimage = AppImageApps::for_home(&home);
    let listing = list_all(&[&pacman, &flatpak, &appimage]);
    assert!(listing.errors.is_empty(), "{:?}", listing.errors);
    let bash = listing.apps.iter().find(|a| a.id == "bash").unwrap();
    assert_eq!(bash.kind, SourceKind::Pacman);
    let plan = pacman.removal_plan(bash).unwrap();
    assert!(plan.blocked.is_some(), "bash is needed by other packages");
}

#[test]
fn real_machine_turns_packages_into_a_much_shorter_list_of_apps() {
    use std::collections::HashMap;
    use tidewise_adapters::desktop::{apps_from_packages, pacman_owners, parse_desktop_file};
    use tidewise_adapters::pacman::PacmanApps;
    use tidewise_adapters::runner::SystemRunner;
    use tidewise_adapters::source::AppSource;
    if !Path::new("/usr/bin/pacman").exists() {
        eprintln!("skipped: pacman not installed");
        return;
    }
    let packages = PacmanApps::system().list().unwrap();
    let files: Vec<std::path::PathBuf> = std::fs::read_dir("/usr/share/applications")
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "desktop"))
        .collect();
    let entries: HashMap<_, _> = files
        .iter()
        .filter_map(|f| {
            let text = std::fs::read_to_string(f).ok()?;
            Some((f.clone(), parse_desktop_file(&text)?))
        })
        .collect();
    let owned = pacman_owners(&SystemRunner, &files);
    assert!(!owned.is_empty());
    let total = packages.len();
    let apps = apps_from_packages(packages, &owned, &entries);
    eprintln!("{} packages -> {} apps", total, apps.len());
    assert!(!apps.is_empty() && apps.len() < total / 2);
    assert!(apps.iter().all(|a| a.id != "bash" && a.id != "glibc"));
}
