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
