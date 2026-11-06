//! Putting text on the clipboard without a new dependency: through `wl-copy`, `xclip` or `xsel`
//! when one is installed (by absolute path, text on stdin, no shell). If none is, the caller saves
//! the text to a file instead and says where.

use std::io::Write;
use std::process::{Command, Stdio};

/// The clipboard programs in the order they are tried: program, arguments.
const TOOLS: [(&str, &[&str]); 3] = [
    ("/usr/bin/wl-copy", &[]),
    ("/usr/bin/xclip", &["-selection", "clipboard"]),
    ("/usr/bin/xsel", &["--clipboard", "--input"]),
];

/// The first tool that `exists`.
pub fn pick_tool(exists: &dyn Fn(&str) -> bool) -> Option<(&'static str, &'static [&'static str])> {
    TOOLS.iter().copied().find(|(program, _)| exists(program))
}

/// Copy `text`; the name of the program used, or why it did not work.
pub fn copy(text: &str) -> Result<&'static str, String> {
    let (program, args) =
        pick_tool(&|p| std::path::Path::new(p).exists()).ok_or("no clipboard program found")?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not start {program}: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("could not write to {program}: {e}"))?;
    }
    let status = child
        .wait()
        .map_err(|e| format!("{program} did not finish: {e}"))?;
    if status.success() {
        Ok(program)
    } else {
        Err(format!("{program} failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_installed_tool_is_used_wayland_first() {
        assert_eq!(pick_tool(&|_| true), Some(("/usr/bin/wl-copy", &[][..])));
        assert_eq!(
            pick_tool(&|p| p.ends_with("xclip") || p.ends_with("xsel")),
            Some(("/usr/bin/xclip", &["-selection", "clipboard"][..]))
        );
        assert_eq!(
            pick_tool(&|p| p.ends_with("xsel")),
            Some(("/usr/bin/xsel", &["--clipboard", "--input"][..]))
        );
        assert_eq!(pick_tool(&|_| false), None);
    }

    #[test]
    fn every_tool_is_an_absolute_path() {
        assert!(TOOLS.iter().all(|(p, _)| p.starts_with('/')));
    }
}
