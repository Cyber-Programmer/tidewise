//! Shared by the two throwaway prototypes: the package list, the common output lines, argument parsing.
//! Both binaries print the SAME line formats so `diff` can compare them.
//!
//! Search line:  S <TAB> repo/name <TAB> version <TAB> installed(0|1) <TAB> description
//! Detail line:  D <TAB> name <TAB> repo <TAB> version <TAB> download <TAB> installed size <TAB> depends <TAB> provides
//! Sizes are printed the way pacman prints them ("2.31 MiB"); with `--bytes` prototype B prints exact bytes.
//! depends / provides are joined with a single space (pacman's own two-space gap is normalised).

pub const SEARCH_TERM: &str = "linux";

pub const PACKAGES: [&str; 20] = [
    "zsh",
    "pacman",
    "linux",
    "linux-headers",
    "base",
    "glibc",
    "bash",
    "git",
    "vim",
    "firefox",
    "qt6-base",
    "gtk4",
    "python",
    "rust",
    "systemd",
    "mesa",
    "ffmpeg",
    "baobab",
    "tzdata",
    "bc",
];

/// `--dbpath DIR` and `--bytes`, nothing else.
pub struct Args {
    pub dbpath: String,
    pub bytes: bool,
}

pub fn parse_args() -> Args {
    let mut args = Args {
        dbpath: "/var/lib/pacman".to_string(),
        bytes: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--dbpath" => args.dbpath = it.next().unwrap_or_default(),
            "--bytes" => args.bytes = true,
            other => eprintln!("ignored argument: {other}"),
        }
    }
    args
}

/// Same rounding as pacman's `humanize_size`: powers of 1024, two decimals, B/KiB/MiB/GiB/TiB.
pub fn human_size(bytes: u64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.2} {}", units[unit])
}

pub fn search_line(repo: &str, name: &str, version: &str, installed: bool, desc: &str) -> String {
    format!(
        "S\t{repo}/{name}\t{version}\t{}\t{desc}",
        u8::from(installed)
    )
}

pub fn detail_line(
    name: &str,
    repo: &str,
    version: &str,
    download: &str,
    installed_size: &str,
    depends: &[String],
    provides: &[String],
) -> String {
    format!(
        "D\t{name}\t{repo}\t{version}\t{download}\t{installed_size}\t{}\t{}",
        depends.join(" "),
        provides.join(" ")
    )
}
