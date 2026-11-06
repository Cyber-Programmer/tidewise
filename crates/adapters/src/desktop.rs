//! `.desktop` files: what an app is called, how it starts, which package owns the file. Used to turn
//! the thousands of installed packages into the apps a person would recognise.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::source::{is_plain_name, AppEntry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
    /// `NoDisplay=true` or `Hidden=true`: it is not shown in menus, so it is not an app.
    pub hidden: bool,
}

/// The `[Desktop Entry]` group of a `.desktop` file. `None` when there is none, when it is not an
/// application, or when it has no plain `Name=`. Translated `Name[xx]=` lines are ignored.
pub fn parse_desktop_file(text: &str) -> Option<DesktopEntry> {
    let mut in_group = false;
    let mut found_group = false;
    let (mut kind, mut name, mut exec, mut icon) = (None, None, None, None);
    let (mut no_display, mut hidden) = (false, false);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_group = line == "[Desktop Entry]";
            found_group |= in_group;
            continue;
        }
        if !in_group || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Type" => kind = Some(value.to_string()),
            "Name" => name = Some(value.to_string()),
            "Exec" => exec = Some(value.to_string()),
            "Icon" => icon = Some(value.to_string()).filter(|v| !v.is_empty()),
            "NoDisplay" => no_display = value == "true",
            "Hidden" => hidden = value == "true",
            _ => {}
        }
    }
    if !found_group || kind.as_deref() != Some("Application") {
        return None;
    }
    Some(DesktopEntry {
        name: name.filter(|n| !n.is_empty())?,
        exec: exec.unwrap_or_default(),
        icon,
        hidden: no_display || hidden,
    })
}

/// One word of an `Exec=` line, unquoted, and the rest of the line.
fn next_word(text: &str) -> Option<(String, &str)> {
    let text = text.trim_start();
    if let Some(quoted) = text.strip_prefix('"') {
        let end = quoted.find('"')?;
        return Some((quoted[..end].to_string(), &quoted[end + 1..]));
    }
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    (end > 0).then(|| (text[..end].to_string(), &text[end..]))
}

/// The program an `Exec=` line starts: the first word, unquoted, skipping a leading `env VAR=value`.
pub fn exec_program(exec: &str) -> Option<String> {
    let (first, mut rest) = next_word(exec)?;
    if first != "env" {
        return Some(first);
    }
    loop {
        let (word, after) = next_word(rest)?;
        if word.contains('=') && !word.starts_with('/') {
            rest = after;
        } else {
            return Some(word);
        }
    }
}

/// `/path/file.desktop is owned by package 1.2-3` lines of `pacman -Qo`. Error lines are skipped.
pub fn parse_pacman_qo(text: &str) -> Vec<(PathBuf, String)> {
    text.lines()
        .filter_map(|line| {
            let (path, rest) = line.split_once(" is owned by ")?;
            let package = rest.split_whitespace().next()?;
            (path.starts_with('/') && is_plain_name(package))
                .then(|| (PathBuf::from(path), package.to_string()))
        })
        .collect()
}

/// `package[, package]: /path` lines of `dpkg -S`. Lines that are not that are skipped.
pub fn parse_dpkg_s(text: &str) -> Vec<(PathBuf, String)> {
    let mut owned = Vec::new();
    for line in text.lines() {
        let Some((packages, path)) = line.rsplit_once(": ") else {
            continue;
        };
        let packages: Vec<&str> = packages.split(", ").collect();
        if path.starts_with('/') && packages.iter().all(|p| is_plain_name(p)) {
            owned.extend(
                packages
                    .iter()
                    .map(|p| (PathBuf::from(path), p.to_string())),
            );
        }
    }
    owned
}

/// Keep the packages that own a visible `.desktop` file and name them as the file does; everything
/// else (libraries, tools without a menu entry) is not an app.
pub fn apps_from_packages(
    packages: Vec<AppEntry>,
    owned: &[(PathBuf, String)],
    entries: &HashMap<PathBuf, DesktopEntry>,
) -> Vec<AppEntry> {
    packages
        .into_iter()
        .filter_map(|mut package| {
            let shown = owned
                .iter()
                .filter(|(_, owner)| *owner == package.id)
                .find_map(|(path, _)| entries.get(path).filter(|e| !e.hidden))?;
            package.name = shown.name.clone();
            Some(package)
        })
        .collect()
}

/// The files that can safely be handed to an owner lookup: absolute, no newline, no NUL.
fn valid_files(files: &[PathBuf]) -> Vec<&str> {
    files
        .iter()
        .filter_map(|f| f.to_str())
        .filter(|f| f.starts_with('/') && !f.contains(['\n', '\0']))
        .collect()
}

/// Ask pacman who owns these files (`pacman -Qo`, read-only). Files that nobody owns are simply
/// missing from the answer.
pub fn pacman_owners(
    runner: &dyn crate::runner::CommandRunner,
    files: &[PathBuf],
) -> Vec<(PathBuf, String)> {
    let valid = valid_files(files);
    if valid.is_empty() {
        return Vec::new();
    }
    let mut args = vec!["-Qo", "--"];
    args.extend(valid);
    match runner.run("/usr/bin/pacman", &args) {
        Ok(out) => parse_pacman_qo(&out.stdout),
        Err(_) => Vec::new(),
    }
}

/// Ask dpkg who owns these files (`dpkg -S`, read-only). It exits 1 when some file is unknown but
/// still prints the ones it found, so the exit code is not looked at.
pub fn dpkg_owners(
    runner: &dyn crate::runner::CommandRunner,
    files: &[PathBuf],
) -> Vec<(PathBuf, String)> {
    let valid = valid_files(files);
    if valid.is_empty() {
        return Vec::new();
    }
    let mut args = vec!["-S", "--"];
    args.extend(valid);
    match runner.run("/usr/bin/dpkg", &args) {
        Ok(out) => parse_dpkg_s(&out.stdout),
        Err(_) => Vec::new(),
    }
}

/// The package names `rpm -qf --qf '%{NAME}\n' FILE` printed for `file`. Lines that are not a plain
/// package name (the "not owned" sentence in any language, an error, anything odd) are skipped.
pub fn parse_rpm_qf(file: &std::path::Path, text: &str) -> Vec<(PathBuf, String)> {
    text.lines()
        .map(str::trim)
        .filter(|line| is_plain_name(line))
        .map(|line| (file.to_path_buf(), line.to_string()))
        .collect()
}

/// Ask rpm who owns these files, one `rpm -qf` per file (its output does not say which file a name
/// belongs to when several are asked at once). The files are absolute, so none can look like an
/// option. A machine with hundreds of launchers means hundreds of short runs; this is meant for a
/// worker thread.
pub fn rpm_owners(
    runner: &dyn crate::runner::CommandRunner,
    files: &[PathBuf],
) -> Vec<(PathBuf, String)> {
    let mut owned = Vec::new();
    for file in valid_files(files) {
        let args = ["-qf", "--qf", "%{NAME}\\n", file];
        if let Ok(out) = runner.run("/usr/bin/rpm", &args) {
            owned.extend(parse_rpm_qf(std::path::Path::new(file), &out.stdout));
        }
    }
    owned
}

/// The packages that own a visible `.desktop` file in `apps_dir`, as apps, for any package source
/// that has an owner lookup (pacman, apt, dnf). Other kinds give nothing.
pub fn package_apps(
    packages: Vec<AppEntry>,
    kind: crate::source::SourceKind,
    runner: &dyn crate::runner::CommandRunner,
    apps_dir: &std::path::Path,
) -> Vec<AppEntry> {
    use crate::source::SourceKind;
    if !matches!(kind, SourceKind::Pacman | SourceKind::Apt | SourceKind::Dnf) {
        return Vec::new();
    }
    let files: Vec<PathBuf> = std::fs::read_dir(apps_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "desktop"))
        .collect();
    if files.is_empty() {
        return Vec::new();
    }
    let entries: HashMap<PathBuf, DesktopEntry> = files
        .iter()
        .filter_map(|f| {
            let text = std::fs::read_to_string(f).ok()?;
            Some((f.clone(), parse_desktop_file(&text)?))
        })
        .collect();
    let owned = match kind {
        SourceKind::Pacman => pacman_owners(runner, &files),
        SourceKind::Apt => dpkg_owners(runner, &files),
        _ => rpm_owners(runner, &files),
    };
    apps_from_packages(packages, &owned, &entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(rel: &str) -> String {
        let path = format!("{}/../../tests/fixtures/{rel}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn real_desktop_files_give_name_exec_and_icon() {
        let ff = parse_desktop_file(&fx("desktop/firefox.desktop")).unwrap();
        assert_eq!(ff.name, "Firefox");
        assert_eq!(ff.exec, "/usr/lib/firefox/firefox %u");
        assert_eq!(ff.icon.as_deref(), Some("firefox"));
        assert!(!ff.hidden);
        let code = parse_desktop_file(&fx("desktop/code.desktop")).unwrap();
        assert_eq!(
            (code.name.as_str(), code.icon.as_deref()),
            ("Visual Studio Code", Some("vscode"))
        );
    }

    #[test]
    fn a_broken_file_and_other_types_are_not_apps() {
        assert_eq!(parse_desktop_file(&fx("desktop/broken.desktop")), None);
        assert_eq!(parse_desktop_file(""), None);
        assert_eq!(
            parse_desktop_file("[Desktop Entry]\nType=Link\nName=x\nURL=y\n"),
            None
        );
        assert_eq!(
            parse_desktop_file("[Desktop Entry]\nType=Application\nExec=x\n"),
            None,
            "no Name"
        );
        assert_eq!(
            parse_desktop_file("[Desktop Entry]\nType=Application\nName[de]=Nur\nExec=x\n"),
            None
        );
    }

    #[test]
    fn only_the_desktop_entry_group_counts_and_hidden_is_flagged() {
        let text = "[Desktop Action new]\nName=New window\nExec=x --new\n\n[Desktop Entry]\nType=Application\nName=Tool\nExec=tool\nNoDisplay=true\n";
        let e = parse_desktop_file(text).unwrap();
        assert_eq!(
            (e.name.as_str(), e.exec.as_str(), e.hidden),
            ("Tool", "tool", true)
        );
        let hidden = "[Desktop Entry]\nType=Application\nName=T\nExec=t\nHidden=true\n";
        assert!(parse_desktop_file(hidden).unwrap().hidden);
        let shown = "[Desktop Entry]\nType=Application\nName=T\nExec=t\nNoDisplay=false\n";
        assert!(!parse_desktop_file(shown).unwrap().hidden);
    }

    #[test]
    fn exec_lines_give_the_program() {
        assert_eq!(exec_program("code %F").as_deref(), Some("code"));
        assert_eq!(
            exec_program("\"/home/u/Applications/Krita 5.2.AppImage\" %U").as_deref(),
            Some("/home/u/Applications/Krita 5.2.AppImage")
        );
        assert_eq!(
            exec_program("env GDK_BACKEND=x11 /opt/app/app --flag").as_deref(),
            Some("/opt/app/app")
        );
        assert_eq!(exec_program("  /usr/bin/x").as_deref(), Some("/usr/bin/x"));
        assert_eq!(exec_program(""), None);
        assert_eq!(exec_program("\"unterminated"), None);
        assert_eq!(exec_program("env A=b"), None);
    }

    #[test]
    fn real_pacman_qo_output_is_read_and_errors_are_skipped() {
        let owned = parse_pacman_qo(&fx("desktop-owners/pacman-Qo-real.txt"));
        assert_eq!(owned.len(), 6);
        assert_eq!(
            owned[0],
            (
                PathBuf::from("/usr/share/applications/antigravity-ide-url-handler.desktop"),
                "antigravity-ide".to_string()
            )
        );
        assert_eq!(owned[4].1, "avahi");
        assert_eq!(parse_pacman_qo("error: No package owns /x\n"), vec![]);
        assert_eq!(parse_pacman_qo(""), vec![]);
    }

    #[test]
    fn dpkg_s_lines_with_several_packages_and_arch_suffixes() {
        let owned = parse_dpkg_s(&fx("desktop-owners/dpkg-S-handwritten.txt"));
        let got: Vec<_> = owned
            .iter()
            .map(|(p, n)| (p.to_str().unwrap(), n.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("/usr/share/applications/firefox.desktop", "firefox"),
                ("/usr/share/doc/libc6", "libc6:amd64"),
                ("/usr/share/applications/vlc.desktop", "vlc"),
                ("/usr/share/applications/vlc.desktop", "vlc-bin"),
            ]
        );
        assert_eq!(
            parse_dpkg_s(&fx("desktop-owners/dpkg-S-de-handwritten.txt")).len(),
            1
        );
    }

    use crate::runner::{CommandRunner, Output};
    use crate::source::SourceKind;
    use std::cell::RefCell;

    type Answer = Box<dyn Fn(&str, &[&str]) -> Result<Output, String>>;

    /// Answers each call from a closure and remembers what was asked.
    struct Fake {
        calls: RefCell<Vec<(String, Vec<String>)>>,
        answer: Answer,
    }

    impl Fake {
        fn new(answer: impl Fn(&str, &[&str]) -> Result<Output, String> + 'static) -> Self {
            Fake {
                calls: RefCell::new(Vec::new()),
                answer: Box::new(answer),
            }
        }
    }

    impl CommandRunner for Fake {
        fn run(&self, program: &str, args: &[&str]) -> Result<Output, String> {
            self.calls.borrow_mut().push((
                program.to_string(),
                args.iter().map(|a| a.to_string()).collect(),
            ));
            (self.answer)(program, args)
        }
    }

    fn out(success: bool, stdout: &str) -> Result<Output, String> {
        Ok(Output {
            success,
            stdout: stdout.into(),
            stderr: String::new(),
        })
    }

    #[test]
    fn dpkg_owners_asks_once_after_a_double_dash_and_only_with_absolute_plain_paths() {
        let text = fx("desktop-owners/dpkg-S-handwritten.txt");
        let runner = Fake::new(move |_, _| out(false, &text)); // exit 1: some file unknown
        let files = [
            PathBuf::from("/usr/share/applications/firefox.desktop"),
            PathBuf::from("relative.desktop"),
            PathBuf::from("/usr/share/applications/new\nline.desktop"),
            PathBuf::from("/usr/share/applications/vlc.desktop"),
        ];
        let owned = dpkg_owners(&runner, &files);
        assert_eq!(owned.len(), 4, "{owned:?}");
        let calls = runner.calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "/usr/bin/dpkg");
        assert_eq!(
            calls[0].1,
            [
                "-S",
                "--",
                "/usr/share/applications/firefox.desktop",
                "/usr/share/applications/vlc.desktop"
            ]
        );
    }

    #[test]
    fn dpkg_owners_with_nothing_valid_or_a_failing_runner_is_empty() {
        let never = Fake::new(|_, _| panic!("must not run"));
        assert!(dpkg_owners(&never, &[]).is_empty());
        assert!(dpkg_owners(&never, &[PathBuf::from("x.desktop")]).is_empty());
        let broken = Fake::new(|_, _| Err("no dpkg".into()));
        assert!(dpkg_owners(&broken, &[PathBuf::from("/a.desktop")]).is_empty());
    }

    #[test]
    fn rpm_qf_output_gives_the_owner_and_skips_sentences_errors_and_odd_lines() {
        let f = PathBuf::from("/usr/share/applications/firefox.desktop");
        assert_eq!(
            parse_rpm_qf(&f, &fx("desktop-owners/rpm-qf-handwritten.txt")),
            [(f.clone(), "firefox".to_string())]
        );
        for skipped in [
            "rpm-qf-notowned-handwritten.txt",
            "rpm-qf-de-handwritten.txt",
        ] {
            assert!(parse_rpm_qf(&f, &fx(&format!("desktop-owners/{skipped}"))).is_empty());
        }
        let broken = parse_rpm_qf(&f, &fx("desktop-owners/rpm-qf-broken-handwritten.txt"));
        let names: Vec<_> = broken.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(
            names,
            ["vlc", "vlc-libs"],
            "no $(id), no --force, no spaces"
        );
        assert!(parse_rpm_qf(&f, "").is_empty());
    }

    #[test]
    fn rpm_owners_asks_once_per_file_with_the_name_format_and_no_shell() {
        let yes = fx("desktop-owners/rpm-qf-handwritten.txt");
        let no = fx("desktop-owners/rpm-qf-notowned-handwritten.txt");
        let runner = Fake::new(move |_, args| {
            if args.last() == Some(&"/usr/share/applications/firefox.desktop") {
                out(true, &yes)
            } else {
                out(false, &no)
            }
        });
        let files = [
            PathBuf::from("/usr/share/applications/firefox.desktop"),
            PathBuf::from("/usr/share/applications/nope.desktop"),
            PathBuf::from("not-absolute.desktop"),
        ];
        let owned = rpm_owners(&runner, &files);
        assert_eq!(
            owned,
            [(files[0].clone(), "firefox".to_string())],
            "the unowned file is simply missing"
        );
        let calls = runner.calls.borrow();
        assert_eq!(calls.len(), 2, "one per valid file");
        assert_eq!(calls[0].0, "/usr/bin/rpm");
        assert_eq!(
            calls[0].1,
            [
                "-qf",
                "--qf",
                "%{NAME}\\n",
                "/usr/share/applications/firefox.desktop"
            ]
        );
    }

    /// A temp `applications` folder with a visible launcher, a hidden one and a broken file.
    fn apps_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let put = |name: &str, text: &str| std::fs::write(dir.path().join(name), text).unwrap();
        put("firefox.desktop", &fx("desktop/firefox.desktop"));
        put(
            "helper.desktop",
            "[Desktop Entry]\nType=Application\nName=Helper\nExec=h\nNoDisplay=true\n",
        );
        put("broken.desktop", &fx("desktop/broken.desktop"));
        dir
    }

    fn pkg_of(name: &str, kind: SourceKind) -> AppEntry {
        AppEntry { kind, ..pkg(name) }
    }

    fn owner_answer(dir: &std::path::Path, tool_is_dpkg: bool) -> Fake {
        let d = dir.display().to_string();
        Fake::new(move |_, args| {
            if tool_is_dpkg {
                out(
                    true,
                    &format!("firefox: {d}/firefox.desktop\nlibfoo: {d}/helper.desktop\n"),
                )
            } else {
                match args.last().copied().unwrap_or_default() {
                    f if f.ends_with("/firefox.desktop") => out(true, "firefox\n"),
                    f if f.ends_with("/helper.desktop") => out(true, "libfoo\n"),
                    _ => out(false, "file x is not owned by any package\n"),
                }
            }
        })
    }

    #[test]
    fn apt_packages_with_a_visible_launcher_become_apps_through_dpkg() {
        let dir = apps_dir();
        let runner = owner_answer(dir.path(), true);
        let packages = ["bash", "firefox", "libfoo"]
            .map(|n| pkg_of(n, SourceKind::Apt))
            .to_vec();
        let apps = package_apps(packages, SourceKind::Apt, &runner, dir.path());
        let got: Vec<_> = apps
            .iter()
            .map(|a| (a.id.as_str(), a.name.as_str()))
            .collect();
        assert_eq!(got, [("firefox", "Firefox")], "no bash, no hidden helper");
        assert_eq!(runner.calls.borrow()[0].0, "/usr/bin/dpkg");
    }

    #[test]
    fn dnf_packages_with_a_visible_launcher_become_apps_through_rpm() {
        let dir = apps_dir();
        let runner = owner_answer(dir.path(), false);
        let packages = ["bash", "firefox", "libfoo"]
            .map(|n| pkg_of(n, SourceKind::Dnf))
            .to_vec();
        let apps = package_apps(packages, SourceKind::Dnf, &runner, dir.path());
        let got: Vec<_> = apps
            .iter()
            .map(|a| (a.id.as_str(), a.name.as_str()))
            .collect();
        assert_eq!(got, [("firefox", "Firefox")]);
        assert!(runner.calls.borrow().iter().all(|c| c.0 == "/usr/bin/rpm"));
    }

    #[test]
    fn pacman_goes_through_the_same_door_and_other_kinds_ask_nobody() {
        let dir = apps_dir();
        let d = dir.path().display().to_string();
        let runner = Fake::new(move |_, _| {
            out(
                true,
                &format!("{d}/firefox.desktop is owned by firefox 130-1\n"),
            )
        });
        let apps = package_apps(
            vec![pkg("firefox"), pkg("bash")],
            SourceKind::Pacman,
            &runner,
            dir.path(),
        );
        assert_eq!(apps.len(), 1);
        assert_eq!(runner.calls.borrow()[0].0, "/usr/bin/pacman");
        let never = Fake::new(|_, _| panic!("must not run"));
        assert!(package_apps(vec![pkg("x")], SourceKind::Flatpak, &never, dir.path()).is_empty());
    }

    #[test]
    fn a_missing_applications_folder_gives_no_apps() {
        let never = Fake::new(|_, _| panic!("must not run"));
        let apps = package_apps(
            vec![pkg_of("firefox", SourceKind::Apt)],
            SourceKind::Apt,
            &never,
            std::path::Path::new("/definitely/not/here"),
        );
        assert!(apps.is_empty());
    }

    fn pkg(name: &str) -> AppEntry {
        AppEntry {
            id: name.into(),
            name: name.into(),
            kind: crate::source::SourceKind::Pacman,
            scope: crate::source::Scope::System,
            version: None,
            size_bytes: Some(1),
            required_by: vec![],
        }
    }

    fn entry(name: &str, hidden: bool) -> DesktopEntry {
        DesktopEntry {
            name: name.into(),
            exec: "x".into(),
            icon: None,
            hidden,
        }
    }

    #[test]
    fn packages_with_a_visible_desktop_file_become_apps_named_like_the_file() {
        let owned = vec![
            (
                "/usr/share/applications/firefox.desktop".into(),
                "firefox".to_string(),
            ),
            (
                "/usr/share/applications/helper.desktop".into(),
                "libfoo".to_string(),
            ),
            (
                "/usr/share/applications/a.desktop".into(),
                "multi".to_string(),
            ),
            (
                "/usr/share/applications/b.desktop".into(),
                "multi".to_string(),
            ),
        ];
        let entries: HashMap<PathBuf, DesktopEntry> = [
            (
                "/usr/share/applications/firefox.desktop",
                entry("Firefox", false),
            ),
            (
                "/usr/share/applications/helper.desktop",
                entry("Helper", true),
            ),
            ("/usr/share/applications/a.desktop", entry("Multi A", false)),
            ("/usr/share/applications/b.desktop", entry("Multi B", false)),
        ]
        .into_iter()
        .map(|(p, e)| (PathBuf::from(p), e))
        .collect();
        let apps = apps_from_packages(
            vec![pkg("bash"), pkg("firefox"), pkg("libfoo"), pkg("multi")],
            &owned,
            &entries,
        );
        let got: Vec<_> = apps
            .iter()
            .map(|a| (a.id.as_str(), a.name.as_str()))
            .collect();
        assert_eq!(got, [("firefox", "Firefox"), ("multi", "Multi A")]);
    }

    #[test]
    fn a_desktop_file_nobody_listed_or_owned_by_an_unknown_package_changes_nothing() {
        let owned = vec![("/x.desktop".into(), "ghost".to_string())];
        let entries: HashMap<PathBuf, DesktopEntry> =
            [(PathBuf::from("/x.desktop"), entry("Ghost", false))].into();
        assert!(apps_from_packages(vec![pkg("bash")], &owned, &entries).is_empty());
        assert!(apps_from_packages(vec![pkg("bash")], &[], &HashMap::new()).is_empty());
    }

    struct Canned(
        std::cell::RefCell<Vec<Vec<String>>>,
        Result<crate::runner::Output, String>,
    );

    impl crate::runner::CommandRunner for Canned {
        fn run(&self, program: &str, args: &[&str]) -> Result<crate::runner::Output, String> {
            assert_eq!(program, "/usr/bin/pacman");
            self.0
                .borrow_mut()
                .push(args.iter().map(|a| a.to_string()).collect());
            self.1.clone()
        }
    }

    #[test]
    fn pacman_owners_asks_once_with_the_files_after_a_double_dash_and_reads_the_answer() {
        let reply = crate::runner::Output {
            success: false,
            stdout: fx("desktop-owners/pacman-Qo-real.txt"),
            stderr: "error: No package owns /x".into(),
        };
        let runner = Canned(Default::default(), Ok(reply));
        let files: Vec<PathBuf> = vec![
            "/usr/share/applications/a.desktop".into(),
            "/usr/share/applications/-b.desktop".into(),
        ];
        let owned = pacman_owners(&runner, &files);
        assert_eq!(
            owned.len(),
            6,
            "a non-zero exit (some file unowned) still gives the answers"
        );
        let calls = runner.0.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0],
            [
                "-Qo",
                "--",
                "/usr/share/applications/a.desktop",
                "/usr/share/applications/-b.desktop"
            ]
        );
    }

    #[test]
    fn pacman_owners_does_nothing_for_no_files_relative_paths_or_a_failing_runner() {
        let runner = Canned(Default::default(), Err("boom".into()));
        assert!(pacman_owners(&runner, &[]).is_empty());
        assert!(pacman_owners(&runner, &["relative.desktop".into(), "/a\nb".into()]).is_empty());
        assert!(runner.0.borrow().is_empty(), "nothing valid to ask about");
        assert!(pacman_owners(&runner, &["/usr/share/applications/a.desktop".into()]).is_empty());
    }
}
