//! Runs the real, read-only pacman on this machine. Skipped where pacman is not installed.

use std::path::Path;

use tidewise_adapters::pacman::Pacman;
use tidewise_adapters::PackageSource;

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
