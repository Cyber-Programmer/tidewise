//! Command-line flags: `--demo`, `--screenshot <screen> [--state <variant>] --out <file.png>`.

#[derive(Debug, PartialEq, Eq)]
pub struct Screenshot {
    pub screen: String,
    pub state: Option<String>,
    pub out: String,
    /// Window height in pixels. The width is always 1360; the default height is 960. Artboards
    /// that grow with their content (e.g. Performance with details) are taller than that.
    pub height: u32,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Args {
    pub demo: bool,
    pub screenshot: Option<Screenshot>,
    /// `--page <screen>`: the page the window opens on (used to measure each screen).
    pub page: Option<String>,
}

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Args, String> {
    let mut args = args.into_iter();
    let mut out = Args::default();
    let mut screen = None;
    let mut state = None;
    let mut file = None;
    let mut height = 960;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--demo" => out.demo = true,
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
        });
    } else if state.is_some() || file.is_some() {
        return Err("--state and --out only work with --screenshot".into());
    }
    Ok(out)
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
                height: 960
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
}
