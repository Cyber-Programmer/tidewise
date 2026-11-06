//! Which kind of cache a folder in `~/.cache` is: one an app rebuilds by itself in seconds, or one
//! that costs minutes and bandwidth to download again. Decided by name from a table, and by size for
//! names the table does not know (an unknown cache over 2 GB is treated as the slow kind: when in
//! doubt, do not tick it by default).

/// How long a cache takes to come back after it is removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheKind {
    /// Thumbnails, shader and font caches, browser and desktop caches: the app rebuilds it.
    Rebuilds,
    /// ML models, browser binaries, language packages, build tools, containers, game launchers,
    /// streaming caches: coming back means downloading it again.
    SlowToDownload,
}

impl CacheKind {
    /// The plain-language label shown to the user.
    pub fn label(self) -> &'static str {
        match self {
            CacheKind::Rebuilds => "Rebuilds itself",
            CacheKind::SlowToDownload => "Takes a while to download again",
        }
    }
}

/// An unknown cache bigger than this (decimal bytes, like the rest of the app) is the slow kind.
pub const UNKNOWN_SLOW_ABOVE: u64 = 2_000_000_000;

/// Names (lower case, `.` and `_` and spaces turned into `-`) that are the slow kind, whole name.
/// Short names are matched whole on purpose: `pip` must not catch `pipewire`.
const SLOW_EXACT: &[&str] = &[
    // ML models and their tooling
    "torch",
    "tensorflow",
    "keras",
    "ollama",
    "whisper",
    "transformers",
    // browser binaries and browser-driven test tools
    "playwright",
    "puppeteer",
    "cypress",
    "selenium",
    "electron",
    "chrome-devtools-mcp",
    // language packages and their tools
    "uv",
    "pip",
    "pipx",
    "poetry",
    "pypoetry",
    "npm",
    "pnpm",
    "yarn",
    "bun",
    "deno",
    "cargo",
    "go-build",
    "node-gyp",
    "pre-commit",
    // build tools and AUR helpers
    "paru",
    "yay",
    "bazel",
    "gradle",
    "ccache",
    "sccache",
    "eas-cli",
    "maestro",
    "codex-runtimes",
    // containers
    "docker",
    "docker-desktop",
    "podman",
    "containers",
    "buildah",
    // game launchers
    "legendary",
    "lutris",
    "heroic",
    "steam",
    "itch",
    "bottles",
    "winetricks",
    // streaming
    "spotify",
    "spotify-launcher",
];

/// Names that start like these are the slow kind.
const SLOW_PREFIX: &[&str] = &["huggingface", "ms-playwright", "docker-"];

/// Names that are the quick kind, whole name.
const REBUILD_EXACT: &[&str] = &[
    "thumbnails",
    "fontconfig",
    "mozilla",
    "kde",
    "krunner",
    "appstream",
    "drkonqi",
    "mpv",
    "spectacle",
];

/// Names that start like these are the quick kind: shader caches, browser caches, KDE and GNOME
/// caches, crash metadata.
const REBUILD_PREFIX: &[&str] = &[
    "mesa",
    "qtshadercache",
    "nvidia",
    "chromium",
    "google-chrome",
    "brave",
    "vivaldi",
    "opera",
    "microsoft-edge",
    "plasma",
    "kwin",
    "ksycoca",
    "kscreen",
    "kcmshell",
    "gtk-",
    "gnome-",
];

fn normal(name: &str) -> String {
    name.trim_start_matches('.')
        .to_lowercase()
        .replace([' ', '_'], "-")
}

/// The kind of the cache folder called `name` that weighs `bytes`: the table first (slow names, then
/// quick names), and for a name in neither, the size decides.
pub fn classify(name: &str, bytes: u64) -> CacheKind {
    let n = normal(name);
    let slow = SLOW_EXACT.contains(&n.as_str()) || SLOW_PREFIX.iter().any(|p| n.starts_with(p));
    if slow {
        return CacheKind::SlowToDownload;
    }
    let quick =
        REBUILD_EXACT.contains(&n.as_str()) || REBUILD_PREFIX.iter().any(|p| n.starts_with(p));
    if quick || bytes <= UNKNOWN_SLOW_ABOVE {
        CacheKind::Rebuilds
    } else {
        CacheKind::SlowToDownload
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: u64 = 10_000_000;

    #[test]
    fn quick_to_rebuild_caches_are_the_first_kind_at_any_size() {
        for name in [
            "thumbnails",
            "mesa_shader_cache",
            "qtshadercache-x86_64-little_endian-lp64",
            "fontconfig",
            "mozilla",
            "chromium",
            "google-chrome",
            "BraveSoftware",
            "KDE",
            "kwin",
            "plasma_theme_Sweet.kcache",
            "ksycoca6_en_LcTGOV1EJFwJ5enOzXQVoLoYAzs=",
            "krunner",
            "appstream",
            "drkonqi",
            "gtk-4.0",
        ] {
            assert_eq!(classify(name, SMALL), CacheKind::Rebuilds, "{name}");
            // Known to rebuild: size alone never turns it into the slow kind.
            assert_eq!(
                classify(name, 9_000_000_000),
                CacheKind::Rebuilds,
                "{name} big"
            );
        }
    }

    #[test]
    fn caches_that_take_a_long_download_are_the_second_kind_even_when_small() {
        for name in [
            "huggingface",
            "ms-playwright",
            "ms-playwright-go",
            "uv",
            "pip",
            "paru",
            "yay",
            "Docker Desktop",
            "codex-runtimes",
            "chrome-devtools-mcp",
            "eas-cli",
            "maestro",
            "spotify",
            "torch",
            "npm",
            "pnpm",
            "go-build",
            "legendary",
            "lutris",
            "ollama",
        ] {
            assert_eq!(classify(name, SMALL), CacheKind::SlowToDownload, "{name}");
        }
    }

    #[test]
    fn matching_ignores_case_dots_and_separators() {
        assert_eq!(classify("HuggingFace", SMALL), CacheKind::SlowToDownload);
        assert_eq!(classify("docker_desktop", SMALL), CacheKind::SlowToDownload);
        assert_eq!(classify(".Spotify", SMALL), CacheKind::SlowToDownload);
        assert_eq!(classify("THUMBNAILS", SMALL), CacheKind::Rebuilds);
    }

    #[test]
    fn an_unknown_cache_is_slow_only_above_2_gb() {
        assert_eq!(classify("mystery", 1_999_999_999), CacheKind::Rebuilds);
        assert_eq!(classify("mystery", 2_000_000_000), CacheKind::Rebuilds);
        assert_eq!(
            classify("mystery", 2_000_000_001),
            CacheKind::SlowToDownload
        );
        assert_eq!(
            classify("mystery", 40_000_000_000),
            CacheKind::SlowToDownload
        );
    }

    #[test]
    fn a_name_that_only_contains_a_slow_name_is_not_slow() {
        // "pip" must not catch "pipewire", "uv" must not catch "uvc-thing"; no prefix guessing
        // for short names.
        assert_eq!(classify("pipewire", SMALL), CacheKind::Rebuilds);
        assert_eq!(classify("uvc-thing", SMALL), CacheKind::Rebuilds);
        assert_eq!(classify("yaylike", SMALL), CacheKind::Rebuilds);
    }

    #[test]
    fn the_labels_are_the_plain_words() {
        assert_eq!(CacheKind::Rebuilds.label(), "Rebuilds itself");
        assert_eq!(
            CacheKind::SlowToDownload.label(),
            "Takes a while to download again"
        );
    }
}
