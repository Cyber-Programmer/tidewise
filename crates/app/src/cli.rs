//! Command-line flags: `--add-to-menu`, `--remove-from-menu`, `--demo`, `--screenshot <screen> [--state <variant>] --out <file.png>`.

#[derive(Debug, PartialEq, Eq)]
pub struct Screenshot {
    pub screen: String,
    pub state: Option<String>,
    pub out: String,
    /// Window height in pixels. The width is always 1360; the default height is 960. Artboards
    /// that grow with their content (e.g. Performance with details) are taller than that.
    pub height: u32,
    /// Window width in pixels (default 1360).
    pub width: u32,
    /// `--open <relative/path>`: the folder of the live Storage screen to show.
    pub open: Option<String>,
}

/// `--add-to-menu` / `--remove-from-menu`: put Tidewise in (or take it out of) the app menu, then exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Add,
    Remove,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Args {
    pub menu: Option<MenuAction>,
    pub demo: bool,
    pub screenshot: Option<Screenshot>,
    /// `--page <screen>`: the page the window opens on (used to measure each screen).
    pub page: Option<String>,
    /// `--page storage --open <relative/path>`: start inside that folder of the scanned one.
    pub open: Option<String>,
}

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Args, String> {
    let mut args = args.into_iter();
    let mut out = Args::default();
    let mut screen = None;
    let mut state = None;
    let mut file = None;
    let mut height = 960;
    let mut width = 1360;
    let mut open = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--demo" => out.demo = true,
            "--add-to-menu" => out.menu = Some(MenuAction::Add),
            "--remove-from-menu" => out.menu = Some(MenuAction::Remove),
            "--screenshot" => {
                screen = Some(
                    args.next()
                        .filter(|v| !v.starts_with("--"))
                        .ok_or("--screenshot needs a screen name")?,
                );
            }
            "--page" => out.page = Some(args.next().ok_or("--page needs a screen name")?),
            "--state" => state = Some(args.next().ok_or("--state needs a value")?),
            "--out" => file = Some(args.next().ok_or("--out needs a file name")?),
            "--width" => {
                width = args
                    .next()
                    .and_then(|v| v.parse::<u32>().ok())
                    .filter(|w| (800..=4000).contains(w))
                    .ok_or("--width needs a number of pixels between 800 and 4000")?;
            }
            "--open" => {
                open = Some(args.next().filter(|v| relative_inside(v)).ok_or(
                    "--open needs a folder path inside the scanned folder (no .. and not absolute)",
                )?);
            }
            "--height" => {
                height = args
                    .next()
                    .and_then(|v| v.parse::<u32>().ok())
                    .filter(|h| (400..=4000).contains(h))
                    .ok_or("--height needs a number of pixels between 400 and 4000")?;
            }
            other => return Err(format!("unknown option {other}")),
        }
    }
    if let Some(screen) = screen {
        let out_file = file.ok_or("--screenshot needs --out <file.png>")?;
        out.demo = true;
        out.screenshot = Some(Screenshot {
            screen,
            state,
            out: out_file,
            height,
            width,
            open,
        });
    } else if state.is_some() || file.is_some() || open.is_some() {
        return Err("--state, --out and --open only work with --screenshot".into());
    }
    Ok(out)
}

/// The folder the live Storage scan starts from, when `TIDEWISE_STORAGE_ROOT` asks for one. It is only
/// honoured together with `--screenshot`, so it cannot surprise someone running the app.
pub fn storage_root(screenshot: bool, env: Option<&std::ffi::OsStr>) -> Option<std::path::PathBuf> {
    let dir = std::path::PathBuf::from(env.filter(|_| screenshot)?);
    (dir.is_absolute()).then_some(dir)
}

/// A path inside the scanned folder: relative, no `..`, not empty.
fn relative_inside(path: &str) -> bool {
    let p = std::path::Path::new(path);
    !path.is_empty()
        && p.is_relative()
        && p.components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Result<Args, String> {
        parse(list.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_flags_is_live_mode() {
        assert_eq!(args(&[]), Ok(Args::default()));
    }

    #[test]
    fn menu_flags() {
        assert_eq!(
            args(&["--add-to-menu"]).unwrap().menu,
            Some(MenuAction::Add)
        );
        assert_eq!(
            args(&["--remove-from-menu"]).unwrap().menu,
            Some(MenuAction::Remove)
        );
        assert_eq!(args(&[]).unwrap().menu, None);
    }

    #[test]
    fn demo_flag() {
        assert!(args(&["--demo"]).unwrap().demo);
    }

    #[test]
    fn screenshot_with_state() {
        let a = args(&[
            "--screenshot",
            "performance",
            "--state",
            "details",
            "--out",
            "a.png",
        ])
        .unwrap();
        assert_eq!(
            a.screenshot,
            Some(Screenshot {
                screen: "performance".into(),
                state: Some("details".into()),
                out: "a.png".into(),
                height: 960,
                width: 1360,
                open: None
            })
        );
        assert!(a.demo, "a screenshot always uses demo data");
    }

    #[test]
    fn screenshot_height_can_be_set() {
        let a = args(&[
            "--screenshot",
            "performance",
            "--out",
            "a.png",
            "--height",
            "1005",
        ])
        .unwrap();
        assert_eq!(a.screenshot.unwrap().height, 1005);
    }

    #[test]
    fn height_must_be_a_sane_number() {
        for bad in ["abc", "0", "100", "20000"] {
            assert!(
                args(&[
                    "--screenshot",
                    "performance",
                    "--out",
                    "a.png",
                    "--height",
                    bad
                ])
                .is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn screenshot_needs_out() {
        assert!(args(&["--screenshot", "performance"]).is_err());
    }

    #[test]
    fn screenshot_needs_a_screen_name() {
        assert!(args(&["--screenshot", "--out", "a.png"]).is_err());
    }

    #[test]
    fn page_flag_picks_the_start_page() {
        assert_eq!(
            args(&["--page", "processes"]).unwrap().page.as_deref(),
            Some("processes")
        );
        assert!(args(&["--page"]).is_err());
    }

    #[test]
    fn unknown_flag_is_an_error() {
        assert!(args(&["--force"]).unwrap_err().contains("--force"));
    }

    fn shot(extra: &[&str]) -> Result<Args, String> {
        let mut v = vec!["--screenshot", "storage", "--out", "a.png"];
        v.extend_from_slice(extra);
        args(&v)
    }

    #[test]
    fn open_takes_a_relative_path() {
        let a = shot(&["--open", "Documents/Creative Chaos"]).unwrap();
        assert_eq!(
            a.screenshot.unwrap().open.as_deref(),
            Some("Documents/Creative Chaos")
        );
    }

    #[test]
    fn open_refuses_absolute_and_parent_paths() {
        for bad in ["/etc", "../x", "a/../../b", "a/..", ""] {
            assert!(shot(&["--open", bad]).is_err(), "{bad:?}");
        }
        assert!(shot(&["--open"]).is_err());
    }

    #[test]
    fn open_needs_a_screenshot_or_the_storage_page() {
        assert!(args(&["--open", "Documents"]).is_err());
        assert!(args(&["--page", "home", "--open", "Documents"]).is_err());
    }

    #[test]
    fn the_live_storage_page_can_start_inside_a_folder() {
        let a = args(&["--page", "storage", "--open", "Documents/Photos"]).unwrap();
        assert_eq!(a.open.as_deref(), Some("Documents/Photos"));
        assert!(args(&["--page", "storage", "--open", "../x"]).is_err());
        assert_eq!(args(&["--page", "storage"]).unwrap().open, None);
    }

    #[test]
    fn the_storage_root_variable_needs_a_screenshot() {
        let dir = std::ffi::OsStr::new("/data/fixture");
        assert_eq!(
            storage_root(true, Some(dir)),
            Some(std::path::PathBuf::from("/data/fixture"))
        );
        assert_eq!(storage_root(false, Some(dir)), None);
        assert_eq!(storage_root(true, None), None);
        assert_eq!(storage_root(true, Some(std::ffi::OsStr::new(""))), None);
        assert_eq!(
            storage_root(true, Some(std::ffi::OsStr::new("relative"))),
            None
        );
    }

    #[test]
    fn width_can_be_set_and_must_be_sane() {
        assert_eq!(
            shot(&["--width", "1920"])
                .unwrap()
                .screenshot
                .unwrap()
                .width,
            1920
        );
        assert_eq!(shot(&[]).unwrap().screenshot.unwrap().width, 1360);
        for bad in ["abc", "0", "300", "99999"] {
            assert!(shot(&["--width", bad]).is_err(), "{bad}");
        }
    }
}
