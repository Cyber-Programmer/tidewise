//! Prototype B: read the sync databases through libalpm (the `alpm` crate). Throwaway.
//! Read-only: no transaction, no `trans_*`, no `-Sy`. The repo names are hard-coded for the spike.
use alpm::{Alpm, SigLevel};
use tidewise_spike::{PACKAGES, SEARCH_TERM, detail_line, human_size, parse_args, search_line};

const REPOS: [&str; 3] = ["core", "extra", "multilib"];

fn size(bytes: i64, exact: bool) -> String {
    let bytes = bytes.max(0) as u64;
    if exact {
        bytes.to_string()
    } else {
        human_size(bytes)
    }
}

fn run(dbpath: &str, exact: bool, time: bool) -> Result<Vec<String>, alpm::Error> {
    let t0 = std::time::Instant::now();
    let handle = Alpm::new("/", dbpath)?;
    for repo in REPOS {
        handle.register_syncdb(repo, SigLevel::USE_DEFAULT)?;
    }
    let local = handle.localdb();
    let t_open = t0.elapsed();
    let mut lines = Vec::new();

    for db in handle.syncdbs() {
        for pkg in db.search(std::iter::once(SEARCH_TERM))? {
            let installed = local.pkg(pkg.name()).is_ok();
            let version = pkg.version().to_string();
            lines.push(search_line(
                db.name(),
                pkg.name(),
                &version,
                installed,
                pkg.desc().unwrap_or(""),
            ));
        }
    }

    let t_search = t0.elapsed();
    for name in PACKAGES {
        // First repository that has the package, like `pacman -Si <name>`.
        let found = handle
            .syncdbs()
            .iter()
            .find_map(|db| db.pkg(name).ok().map(|p| (db, p)));
        let Some((db, pkg)) = found else { continue };
        let depends: Vec<String> = pkg.depends().iter().map(|d| d.to_string()).collect();
        let provides: Vec<String> = pkg.provides().iter().map(|d| d.to_string()).collect();
        lines.push(detail_line(
            pkg.name(),
            db.name(),
            pkg.version().as_ref(),
            &size(pkg.download_size(), exact),
            &size(pkg.isize(), exact),
            &depends,
            &provides,
        ));
    }
    lines.sort();
    if time {
        eprintln!(
            "open+register: {t_open:?}; +search: {:?}; +details: {:?}",
            t_search - t_open,
            t0.elapsed() - t_search
        );
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        for line in status
            .lines()
            .filter(|l| l.starts_with("VmRSS") || l.starts_with("VmHWM"))
        {
            eprintln!("{line}");
        }
    }
    Ok(lines)
}

fn main() {
    let args = parse_args();
    match run(&args.dbpath, args.bytes, args.time) {
        Ok(lines) => println!("{}", lines.join("\n")),
        Err(error) => {
            eprintln!("B failed: {error}");
            std::process::exit(1);
        }
    }
}
