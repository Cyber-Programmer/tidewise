//! Command-line flags: `--demo`, `--screenshot <screen> [--state <variant>] --out <file.png>`.

#[derive(Debug, PartialEq, Eq)]
pub struct Screenshot {
    pub screen: String,
    pub state: Option<String>,
    pub out: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Args {
    pub demo: bool,
    pub screenshot: Option<Screenshot>,
}

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Args, String> {
    let mut args = args.into_iter();
    let mut out = Args::default();
    let mut screen = None;
    let mut state = None;
    let mut file = None;
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
            "--state" => state = Some(args.next().ok_or("--state needs a value")?),
            "--out" => file = Some(args.next().ok_or("--out needs a file name")?),
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
                out: "a.png".into()
            })
        );
        assert!(a.demo, "a screenshot always uses demo data");
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
    fn unknown_flag_is_an_error() {
        assert!(args(&["--force"]).unwrap_err().contains("--force"));
    }
}
