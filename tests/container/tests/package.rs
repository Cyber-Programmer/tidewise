//! The built Tidewise package, installed in the package image (WBS 5.5; threats T-03, T-01, T-02; golden rules 1, 2, 8).
//! Real pacman, real polkitd and real pkexec, as root, in the test container only. The image holds the package that
//! `makepkg` built from `git archive` of HEAD, in `/srv/tidewise-pkg/` (see `Containerfile.package`), together with
//! the PKGBUILD and .SRCINFO of HEAD and what `makepkg --printsrcinfo` printed in the build.
//!
//! What is checked, and why it is not a unit test:
//!  - the package's files, owners, modes and the helper's real path (T-03: a helper that a user could write is a free
//!    root shell; the policy only finds the helper at exactly `/usr/lib/tidewise/tidewise-helper`);
//!  - no RPATH or RUNPATH in any ELF file, and every library found (T-03);
//!  - namcap on the recipe and on the package: nothing but a short list of accepted lines, each with its reason;
//!  - the committed .SRCINFO is what makepkg prints;
//!  - polkitd, started with the policy the PACKAGE installed (nothing copied by the test), lists our action;
//!  - the real pkexec runs the installed helper through a dummy-repository job, end to end;
//!  - `pacman -R` removes every file the package owns; the helper's work folder is handled (see below).
//!
//! The tests share one container (one test file = one container), run one after the other, and each one installs the
//! package itself and removes it again, so none depends on the order.
//!
//! What these tests do NOT prove (review R-07 of 5.5): pkexec runs as root in the container, and polkitd authorizes root
//! whatever the action, so the real-pkexec tests show that the installed helper runs and is accepted, not that pkexec
//! matched OUR action (that is `exec.path` matching the real path, from polkit's source, plus `pkaction` listing it);
//! and no test starts the window (the container has no display; the manual check is in the plan, for M4).
//!
//! `/run/tidewise-helper` is not owned by the package: the helper creates it when it first runs (root, mode 0700,
//! under the root-owned `/run`, which is a tmpfs). `pacman -R` leaves it, empty; it is gone at the next boot, and
//! nothing in it is readable by anyone else. The human decided (16 Nov 2026) not to add a removal script for it.

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use common::{
    HELPER_ARGV0, POLICY_PATH, Polkit, WORK_DIR, assert_inside_the_test_container, install_hello,
    installed_version, pacman, pkexec, reset_world,
};

const PKG_DIR: &str = "/srv/tidewise-pkg";
const ACTION_ID: &str = "io.github.tidewise.helper";

/// Every regular file the package owns, and its mode. Nothing else may be in the package (but directories).
const EXPECTED_FILES: [(&str, u32); 5] = [
    ("/usr/bin/tidewise", 0o755),
    ("/usr/lib/tidewise/tidewise-helper", 0o755),
    ("/usr/share/applications/io.github.tidewise.desktop", 0o644),
    ("/usr/share/licenses/tidewise/LICENSE", 0o644),
    (POLICY_PATH, 0o644),
];

/// namcap lines that are accepted, each with the reason (the human accepted the first three kinds on 16 Nov 2026).
/// Any other line fails the test. Matched as text inside the line.
const ACCEPTED_NAMCAP: [(&str, &str); 13] = [
    (
        "W: Missing Maintainer tag",
        "the maintainer's name and contact are the human's choice, made when the package goes to the AUR",
    ),
    (
        "W: Unused shared library '/usr/lib64/ld-linux-x86-64.so.2' by file",
        "the dynamic loader is always listed this way; glibc is in depends",
    ),
    (
        "W: Dependency included, but may not be needed ('pacman')",
        "a program the helper runs (golden rule 3), not a library",
    ),
    (
        "W: Dependency included, but may not be needed ('polkit')",
        "pkexec, polkitd and the policy folder, not a library",
    ),
    (
        "W: Dependency included, but may not be needed ('libxkbcommon')",
        "loaded at run time by the windowing code (the name is in the app binary), so namcap cannot see it",
    ),
    (
        "W: Dependency included, but may not be needed ('wayland')",
        "loaded at run time (libwayland-client, libwayland-egl); the human decided it is a depends",
    ),
    (
        "W: Dependency included, but may not be needed ('libx11')",
        "loaded at run time; the human decided it is a depends",
    ),
    (
        "W: Dependency included, but may not be needed ('libxcb')",
        "loaded at run time; the human decided it is a depends",
    ),
    (
        "W: Dependency included, but may not be needed ('libxi')",
        "loaded at run time; the human decided it is a depends",
    ),
    (
        "W: Dependency included, but may not be needed ('libxkbcommon-x11')",
        "loaded at run time; the human decided it is a depends",
    ),
    (
        "W: Dependency included, but may not be needed ('libglvnd')",
        "loaded at run time (libEGL, libGL), found by review R-01 of 5.5; the human decided it is a depends",
    ),
    (
        "W: Dependency included, but may not be needed ('libxcursor')",
        "loaded at run time (libXcursor), found by review R-01 of 5.5; the human decided it is a depends",
    ),
    (
        "W: Dependency included, but may not be needed ('libxrender')",
        "loaded at run time (libXrender), found by review R-01 of 5.5; the human decided it is a depends",
    ),
];

struct Out {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// A program as these tests run them: absolute path, argument list, empty environment except `LC_ALL=C` and `PATH`,
/// no stdin, no shell.
fn run(program: &str, args: &[&str]) -> Out {
    let output = Command::new(program)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|error| panic!("could not start {program}: {error}"));
    Out {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// The built package's file name, read from the recipe in the image: `tidewise-<pkgver>-<pkgrel>-x86_64.pkg.tar.zst`.
/// (`options=('!debug')` means there is no `tidewise-debug` package.)
fn package_file() -> String {
    let recipe =
        fs::read_to_string(format!("{PKG_DIR}/PKGBUILD")).expect("no PKGBUILD in the image");
    let value = |key: &str| {
        recipe
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{key}=")))
            .unwrap_or_else(|| panic!("no {key}= in the PKGBUILD"))
            .to_string()
    };
    format!(
        "{PKG_DIR}/tidewise-{}-{}-x86_64.pkg.tar.zst",
        value("pkgver"),
        value("pkgrel")
    )
}

/// The package installed with `pacman -U` (as root, in the container) for the length of a test, and removed again on
/// drop (when a test already removed it, the removal fails and nobody minds).
struct Installed;

impl Installed {
    fn new() -> Installed {
        assert_inside_the_test_container();
        reset_world();
        let out = pacman(&["-U", "--noconfirm", &package_file()]);
        assert!(
            out.status.success(),
            "pacman -U failed:\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        Installed
    }
}

impl Drop for Installed {
    fn drop(&mut self) {
        let _ = pacman(&["-R", "--noconfirm", "tidewise"]);
    }
}

/// What `pacman -Ql tidewise` lists: files, and folders with a trailing slash.
fn owned_paths() -> Vec<String> {
    let out = pacman(&["-Ql", "tidewise"]);
    assert!(out.status.success(), "tidewise is not installed");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|line| {
            let (name, path) = line.split_once(' ').expect("a `name path` line");
            assert_eq!(name, "tidewise");
            path.to_string()
        })
        .collect()
}

fn is_elf(path: &str) -> bool {
    let mut magic = [0u8; 4];
    fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut magic))
        .is_ok()
        && magic == *b"\x7fELF"
}

fn elf_files() -> Vec<String> {
    owned_paths()
        .into_iter()
        .filter(|path| !path.ends_with('/') && is_elf(path))
        .collect()
}

// ---- the package is what the recipe says ----------------------------------------------------------------

#[test]
#[ignore = "runs only in the package image: tests/container/run.sh package"]
fn package_the_built_package_is_in_the_image_and_says_what_the_recipe_says() {
    assert_inside_the_test_container();
    let file = package_file();
    assert!(
        fs::metadata(&file).is_ok(),
        "the built package is missing: {file}"
    );
    let out = run("/usr/bin/pacman", &["-Qp", &file]);
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    let version = file
        .rsplit_once("tidewise-")
        .unwrap()
        .1
        .trim_end_matches("-x86_64.pkg.tar.zst");
    assert_eq!(out.stdout.trim(), format!("tidewise {version}"));
    // `options=('!debug')`: only the one package was built.
    let built: Vec<String> = fs::read_dir(PKG_DIR)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".pkg.tar.zst"))
        .collect();
    assert_eq!(built.len(), 1, "{built:?}");
}

#[test]
#[ignore = "runs only in the package image: tests/container/run.sh package"]
fn package_srcinfo_is_what_makepkg_prints() {
    assert_inside_the_test_container();
    let committed = fs::read_to_string(format!("{PKG_DIR}/.SRCINFO")).unwrap();
    let printed = fs::read_to_string(format!("{PKG_DIR}/printsrcinfo")).unwrap();
    assert_eq!(
        committed, printed,
        "packaging/aur/.SRCINFO is not what `makepkg --printsrcinfo` prints"
    );
}

#[test]
#[ignore = "runs only in the package image: tests/container/run.sh package"]
fn package_has_no_install_script_and_declares_no_provides_replaces_conflicts_or_backup() {
    // An install script (`.INSTALL`) would run as root at `pacman -U`, outside the helper's allow-list, and a
    // `provides`, `replaces` or `conflict` could change what pacman does with other packages (review R-02 of 5.5).
    assert_inside_the_test_container();
    let file = package_file();
    let listing = run("/usr/bin/bsdtar", &["-tf", &file]);
    assert_eq!(listing.code, Some(0), "{}", listing.stderr);
    for name in listing.stdout.lines() {
        assert!(
            matches!(name, ".PKGINFO" | ".BUILDINFO" | ".MTREE") || name.starts_with("usr/"),
            "an unexpected entry in the package: {name}"
        );
    }
    let info = run("/usr/bin/bsdtar", &["-xOf", &file, ".PKGINFO"]);
    assert_eq!(info.code, Some(0), "{}", info.stderr);
    assert!(
        info.stdout.contains("pkgname = tidewise"),
        "{}",
        info.stdout
    );
    for line in info.stdout.lines() {
        for forbidden in ["provides", "replaces", "conflict", "backup", "install"] {
            assert!(
                !line.starts_with(forbidden),
                "the package declares `{forbidden}`: {line}"
            );
        }
    }
}

#[test]
#[ignore = "runs only in the package image: tests/container/run.sh package"]
fn package_namcap_prints_only_the_accepted_lines() {
    assert_inside_the_test_container();
    let recipe = run("/usr/bin/namcap", &[&format!("{PKG_DIR}/PKGBUILD")]);
    let package = run("/usr/bin/namcap", &[&package_file()]);
    let mut surprises = Vec::new();
    for out in [&recipe, &package] {
        assert_eq!(out.stderr, "", "namcap wrote to stderr");
        for line in out.stdout.lines().filter(|line| !line.trim().is_empty()) {
            assert!(!line.contains(" E: "), "namcap found an error: {line}");
            if !ACCEPTED_NAMCAP.iter().any(|(text, _)| line.contains(text)) {
                surprises.push(line.to_string());
            }
        }
    }
    assert!(
        surprises.is_empty(),
        "namcap lines that are not on the accepted list:\n{}",
        surprises.join("\n")
    );
}

// ---- what is installed, who owns it, with which mode ----------------------------------------------------

/// Owner root:root, and none of setuid, setgid, sticky, group-write, other-write. Looks at the path itself, not at
/// where a symlink points.
fn assert_root_owned_and_closed(path: &Path) {
    let meta = fs::symlink_metadata(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!((meta.uid(), meta.gid()), (0, 0), "{}", path.display());
    let mode = meta.mode() & 0o7777;
    assert_eq!(
        mode & 0o7022,
        0,
        "{} has mode {mode:o}: a special bit, or writable by group or others",
        path.display()
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the package image (tests/container/run.sh package)"]
fn package_files_are_root_owned_with_the_exact_modes_and_nothing_is_writable_or_setuid() {
    let _installed = Installed::new();
    let owned = owned_paths();

    // Nothing outside /usr: no /opt, /usr/local, /run, /tmp, /home, /srv.
    for path in &owned {
        assert!(path.starts_with("/usr/"), "outside /usr: {path}");
        assert!(!path.starts_with("/usr/local"), "in /usr/local: {path}");
    }
    // Exactly the five files, no more.
    let files: BTreeSet<&str> = owned
        .iter()
        .filter(|path| !path.ends_with('/'))
        .map(String::as_str)
        .collect();
    let expected: BTreeSet<&str> = EXPECTED_FILES.iter().map(|(path, _)| *path).collect();
    assert_eq!(files, expected);

    // Every owned path and every folder above it, up to `/`: root, closed.
    for path in &owned {
        for ancestor in Path::new(path).ancestors() {
            assert_root_owned_and_closed(ancestor);
        }
    }
    // The exact modes, and they are regular files (not links).
    for (path, mode) in EXPECTED_FILES {
        let meta = fs::symlink_metadata(path).unwrap();
        assert!(meta.file_type().is_file(), "{path} is not a regular file");
        assert_eq!(meta.mode() & 0o7777, mode, "{path}");
    }

    // The helper's real path is exactly the one the policy and `HELPER_PATH` name, no symlink anywhere in it.
    assert_eq!(
        fs::canonicalize(HELPER_ARGV0).unwrap(),
        PathBuf::from(HELPER_ARGV0)
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the package image (tests/container/run.sh package)"]
fn package_no_elf_file_has_an_rpath_or_runpath() {
    let _installed = Installed::new();
    let elves = elf_files();
    // Not vacuous: the app and the helper are both ELF files.
    assert!(
        elves.contains(&"/usr/bin/tidewise".to_string()),
        "{elves:?}"
    );
    assert!(elves.contains(&HELPER_ARGV0.to_string()), "{elves:?}");
    for path in &elves {
        let out = run("/usr/bin/readelf", &["-d", path]);
        assert_eq!(out.code, Some(0), "{path}: {}", out.stderr);
        assert!(
            !out.stdout.contains("RPATH") && !out.stdout.contains("RUNPATH"),
            "{path} has an RPATH or RUNPATH:\n{}",
            out.stdout
        );
    }
}

#[test]
#[ignore = "runs real pacman as root: only inside the package image (tests/container/run.sh package)"]
fn package_every_library_is_found_and_the_desktop_entry_is_valid() {
    let _installed = Installed::new();
    for path in elf_files() {
        let out = run("/usr/bin/ldd", &[&path]);
        assert_eq!(out.code, Some(0), "{path}: {} {}", out.stdout, out.stderr);
        assert!(!out.stdout.contains("not found"), "{path}:\n{}", out.stdout);
    }
    let out = run(
        "/usr/bin/desktop-file-validate",
        &["/usr/share/applications/io.github.tidewise.desktop"],
    );
    assert_eq!(
        (out.code, out.stdout.as_str(), out.stderr.as_str()),
        (Some(0), "", "")
    );
}

/// Every `lib<name>.so.<digits>` text in `bytes`, as groups of candidate file names. Found WITHOUT a word boundary: in
/// a Rust binary the strings are packed against other text (`...codelibEGL.so.1`), and a regex with `\b` missed five
/// names that way (review R-01 of 5.5). For each `.so.N` the run of name characters before it is taken, and every place
/// in it where `lib` starts is one candidate; the occurrence is satisfied when any candidate exists.
fn library_names(bytes: &[u8]) -> BTreeSet<Vec<String>> {
    let is_name = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'+' | b'-');
    let mut found = BTreeSet::new();
    let mut at = 0;
    while let Some(offset) = bytes[at..].windows(4).position(|window| window == b".so.") {
        let dot = at + offset;
        at = dot + 4;
        // The version: digits separated by single dots.
        let mut end = at;
        while end < bytes.len()
            && (bytes[end].is_ascii_digit()
                || (bytes[end] == b'.' && bytes.get(end + 1).is_some_and(u8::is_ascii_digit)))
        {
            end += 1;
        }
        if end == at {
            continue;
        }
        let version = String::from_utf8_lossy(&bytes[dot..end]).into_owned();
        let mut start = dot;
        while start > 0 && is_name(bytes[start - 1]) {
            start -= 1;
        }
        let run = &bytes[start..dot];
        let candidates: Vec<String> = (0..run.len().saturating_sub(3))
            .filter(|&place| run[place..].starts_with(b"lib"))
            .map(|place| format!("{}{version}", String::from_utf8_lossy(&run[place..])))
            .collect();
        if !candidates.is_empty() {
            found.insert(candidates);
        }
    }
    found
}

#[test]
#[ignore = "runs real pacman as root: only inside the package image (tests/container/run.sh package)"]
fn package_every_library_name_in_the_binaries_exists_with_only_the_declared_depends() {
    // Review R-01 of 5.5, and the human's decision to add this test: `ldd` and namcap see only the libraries that are
    // linked in. The window-system libraries are loaded by name at run time, so this takes every versioned library name
    // out of the two binaries and checks the file exists. The image holds exactly the package's declared depends (read
    // from its .PKGINFO) next to the test tools, so a name with no file is a dependency the recipe does not declare.
    let _installed = Installed::new();
    let mut missing = Vec::new();
    let mut checked = 0;
    for path in elf_files() {
        let bytes = fs::read(&path).unwrap();
        for candidates in library_names(&bytes) {
            checked += 1;
            let exists = candidates
                .iter()
                .any(|name| Path::new("/usr/lib").join(name).exists());
            if !exists {
                missing.push(format!("{path}: {}", candidates.join(" | ")));
            }
        }
    }
    // Not vacuous: the two binaries name at least libc, libgcc_s, libfontconfig and the window-system libraries.
    assert!(checked >= 10, "only {checked} library names found");
    assert!(
        missing.is_empty(),
        "library names with no file after installing only the declared depends:\n{}",
        missing.join("\n")
    );
}

// ---- polkit and the real pkexec, with what the package installed ----------------------------------------

/// The value after `label` in a `pkaction --verbose` line such as `  implicit active:   auth_admin`.
fn field<'a>(listing: &'a str, label: &str) -> Option<&'a str> {
    listing
        .lines()
        .find_map(|line| line.trim_start().strip_prefix(label))
        .map(str::trim)
}

#[test]
#[ignore = "runs real pacman as root: only inside the package image (tests/container/run.sh package)"]
fn package_polkitd_lists_our_action_from_the_policy_the_package_installed() {
    let _installed = Installed::new();
    // Nothing is copied by the test: polkitd reads the file the package put in /usr/share/polkit-1/actions/.
    let _polkit = Polkit::start_installed();
    let shown = run(
        "/usr/bin/pkaction",
        &["--action-id", ACTION_ID, "--verbose"],
    );
    assert_eq!(shown.code, Some(0), "{} {}", shown.stdout, shown.stderr);
    let text = shown.stdout;
    assert_eq!(field(&text, "implicit any:"), Some("no"), "{text}");
    assert_eq!(field(&text, "implicit inactive:"), Some("no"), "{text}");
    assert_eq!(
        field(&text, "implicit active:"),
        Some("auth_admin"),
        "{text}"
    );
    // The annotation is the helper's real path (threat model Q23): pkexec matches it exactly.
    assert!(
        text.contains(&format!(
            "org.freedesktop.policykit.exec.path -> {HELPER_ARGV0}"
        )),
        "{text}"
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the package image (tests/container/run.sh package)"]
fn package_real_pkexec_runs_the_installed_helper_end_to_end() {
    let (request, summary) = install_hello();
    let _installed = Installed::new();
    let _polkit = Polkit::start_installed();

    let ran = pkexec(HELPER_ARGV0, &[&summary], None, request.as_bytes());
    assert_eq!(ran.code, Some(0), "{}\n{}", ran.stdout, ran.stderr);
    assert_eq!(
        ran.stdout.lines().last(),
        Some("{\"finished\":\"ok\"}"),
        "{}",
        ran.stdout
    );
    assert_eq!(ran.stderr, "");
    assert!(installed_version("tidewise-dummy-hello").is_some());

    // The helper's work folder: root, 0700, and empty again after the job.
    let meta = fs::metadata(WORK_DIR).unwrap();
    assert_eq!((meta.uid(), meta.gid()), (0, 0));
    assert_eq!(meta.mode() & 0o7777, 0o700);
    assert_eq!(fs::read_dir(WORK_DIR).unwrap().count(), 0);

    let _ = pacman(&["-R", "--noconfirm", "tidewise-dummy-hello"]);
}

// ---- removal --------------------------------------------------------------------------------------------

#[test]
#[ignore = "runs real pacman as root: only inside the package image (tests/container/run.sh package)"]
fn package_removal_leaves_nothing_the_package_owned_and_handles_the_work_folder() {
    let (request, summary) = install_hello();
    let _installed = Installed::new();
    let owned = owned_paths();
    // Run one job first, so the helper has made its work folder: that is the leftover to handle.
    {
        let _polkit = Polkit::start_installed();
        let ran = pkexec(HELPER_ARGV0, &[&summary], None, request.as_bytes());
        assert_eq!(ran.code, Some(0), "{}\n{}", ran.stdout, ran.stderr);
    }
    let _ = pacman(&["-R", "--noconfirm", "tidewise-dummy-hello"]);
    assert!(Path::new(WORK_DIR).is_dir());

    let removed = pacman(&["-R", "--noconfirm", "tidewise"]);
    assert!(
        removed.status.success(),
        "pacman -R failed: {}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert_eq!(installed_version("tidewise"), None);

    for path in &owned {
        let left = fs::symlink_metadata(path).is_ok();
        if path.ends_with('/') {
            // A folder may stay only when another package owns it (`/usr/bin`, `/usr/share/polkit-1/actions`, ...).
            if left {
                let owner = pacman(&["-Qo", "--", path]);
                assert!(
                    owner.status.success(),
                    "left behind, owned by nobody: {path}"
                );
            }
        } else {
            assert!(!left, "a file of the package is still there: {path}");
        }
    }
    assert!(!Path::new("/usr/lib/tidewise").exists());
    assert!(!Path::new("/usr/share/licenses/tidewise").exists());

    // The work folder is not the package's. It stays: tmpfs under /run, root only, empty; gone at the next boot.
    let meta = fs::metadata(WORK_DIR).unwrap();
    assert_eq!((meta.uid(), meta.gid()), (0, 0));
    assert_eq!(meta.mode() & 0o7777, 0o700);
    assert_eq!(fs::read_dir(WORK_DIR).unwrap().count(), 0);
}
