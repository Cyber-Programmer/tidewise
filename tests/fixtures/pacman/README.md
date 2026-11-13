# pacman fixtures

Real pacman output saved for the parser tests (WBS 3.3, 4.3, 4.4). This is a small hand-picked sample, not a machine's
full package list. Every package named here, including names in `Required By`, `Optional For` and
`Optional Deps`, comes from the official Arch repos (checked against `pacman -Slq`).

- Captured: 2026-11-11 (`qi`, `qq`), 2026-11-12 (`ss`), 2026-11-13 (`si`), on Arch Linux
- pacman: **v7.1.0**, libalpm v16.0.1
- Environment: `LC_ALL=C` (English field names, `.` decimal point)
- Output went **to a file, not a terminal**, so pacman did **not** wrap long lines (base's `Depends On` is one
  261-character line). The app runs pacman through `std::process::Command`, which also isn't a terminal, so it
  sees the same layout. In a terminal you would see wrapped lines.
- Each `pacman -Qi` record ends with a blank line, so every `qi-*.txt` ends in `\n\n` (except the broken file).

| File | Command | Why it's in the sample |
|---|---|---|
| `qi-base.txt` | `LC_ALL=C pacman -Qi base` | Explicitly installed; meta package with many deps on one long line; `Installed Size : 0.00 B`; `Required By` / `Optional For` are `None` |
| `qi-zsh.txt` | `LC_ALL=C pacman -Qi zsh` | Explicitly installed; 8 optional deps on continuation lines, one marked `[installed]`, text with `'`, `(` `)` and a trailing `.`; `Install Script : Yes`; MiB size |
| `qi-linux-api-headers.txt` | `LC_ALL=C pacman -Qi linux-api-headers` | Installed as a dependency; almost every list field is `None` (`Provides`, `Depends On`, `Optional Deps`, …) |
| `qi-tzdata.txt` | `LC_ALL=C pacman -Qi tzdata` | Dependency; optional deps over 2 lines, both `[installed]`, one with a comma in its text; KiB size; version with a letter (`2026e-1`) |
| `qi-bc.txt` | `LC_ALL=C pacman -Qi bc` | Explicitly installed; `Conflicts With` and `Replaces` are not `None` |
| `qi-baobab.txt` | `LC_ALL=C pacman -Qi baobab` | Explicitly installed; `Groups` is not `None` (`gnome`) |
| `qi-qt6-imageformats.txt` | `LC_ALL=C pacman -Qi qt6-imageformats` | The `Description` value itself contains `: ` — splitting each line on every colon breaks here |
| `qi-multi.txt` | `LC_ALL=C pacman -Qi base tzdata linux-api-headers` | Several records in one output, separated by blank lines |
| `qi-broken-truncated.txt` | *hand-made*: `head -n 13 qi-zsh.txt` plus the first 63 bytes of line 14 | **Deliberately broken.** Ends half-way through a word in `Optional Deps`, no final newline, and `Required By`, `Install Reason` etc. are missing. The parser must report an error, not panic or invent values |
| `qq-sample.txt` | `LC_ALL=C pacman -Qq \| grep -xE 'base\|zsh\|linux-api-headers\|tzdata\|bc\|baobab\|qt6-imageformats'` | A short `-Qq` list (one name per line, sorted as pacman prints it), filtered so the full list isn't published |
| `ss-sample.txt` | `LC_ALL=C pacman -Ss -- '^(baobab\|7zip\|alsa-card-profiles\|gnome-calculator\|zsh\|tzdata\|aalib)$'` (WBS 4.3) | Real `-Ss` output, official repos only, in pacman's own order. Has `[installed]`, `[installed: other-version]` (sync DB newer than the installed package on the capture day), a group `(gnome)` with and without `[installed]`, an epoch version (`1:1.6.11-1`), a plain not-installed package, and repos `core` and `extra`. Header line + one 4-space-indented description line per package |
| `ss-third-party.txt` | *hand-made* (not captured from pacman) | Made-up third-party repos `chaotic-aur` and `my repo.x` (a name with a space and a dot, which pacman's rules allow), a package with two groups and `[installed: x]`, mixed with official `extra` and `core` lines **in non-alphabetical order**, to prove the parser keeps pacman's order. The line layout copies the real output above; the names are invented |
| `ss-broken-truncated.txt` | *hand-made*: first 4 lines of `ss-sample.txt` plus the first 43 bytes of the next header | **Deliberately broken.** Ends inside `[installed: …]` with no final newline and no description. The parser must report an error, not panic |
| `ss-bad-byte.bin` | *hand-made*: `ss-sample.txt` with one byte of the `baobab` description changed to `0xFF` | A stray non-UTF-8 byte. `.bin`, not `.txt`, because it is not valid text. Tests read it with `include_bytes!` and decode lossily, as `run.rs` does |
| `si-zsh.txt` | `LC_ALL=C pacman -Si -- extra/zsh` (WBS 4.4) | Real `-Si` output, official repo. 8 Optional Deps lines on continuation lines; **no `[installed]` marker** even though `zsh-completions` is installed (`-Qi` marks it, `-Si` never does); `Groups`/`Provides`/`Conflicts With`/`Replaces` are `None`; MiB sizes |
| `si-dmd.txt` | `LC_ALL=C pacman -Si -- extra/dmd` | Epoch version `1:2.113.0-2`; `Provides : d-compiler=2.113.0` (version rule with `=`); two groups; 3 Optional Deps lines |
| `si-xf86-video-fbdev.txt` | `LC_ALL=C pacman -Si -- extra/xf86-video-fbdev` | `Conflicts With : xorg-server<21.1.1  X-ABI-VIDEODRV_VERSION<25  X-ABI-VIDEODRV_VERSION>=26`: the `<` and `>=` rules, three entries separated by two spaces |
| `si-iana-etc.txt` | `LC_ALL=C pacman -Si -- core/iana-etc` | Every list field is `None`; `Licenses : custom:none` (a colon inside a value); `4114.47 KiB` |
| `si-multi.txt` | `LC_ALL=C pacman -Si -- core/man-db core/less` | Two records, blank line between; man-db has a bare Optional Dep (`gzip`, no reason) and `GPL-2.0-or-later AND GPL-3.0-or-later`; less has an epoch and `GPL-3.0-or-later OR LicenseRef-Less` (a licence with single spaces) |
| `si-not-found.stderr.txt` | stderr of `LC_ALL=C pacman -Si -- extra/zsh nosuchpkg` (exit code 1) | What pacman says about an unknown name: `error: package 'nosuchpkg' was not found`, quoted exactly as we passed it. stdout still has the zsh record |
| `si-broken-truncated.txt` | *hand-made*: first 11 lines of `si-dmd.txt` plus the first 30 bytes of line 12 | **Deliberately broken.** Ends half-way through the `gcc-multilib` Optional Deps line, no final newline, and `Conflicts With` ... `Installed Size` are missing. The parser must report an error, not panic |
| `si-bad-byte.bin` | *hand-made*: `si-zsh.txt` with the `p` of `programmable` in the description changed to `0xFF` | A stray non-UTF-8 byte. `.bin` because it is not valid text. Tests read it with `include_bytes!` and decode lossily, as `run.rs` does |

## Refreshing

Run the commands above again from this folder. Before committing, check that `Required By`, `Optional For` and
`Optional Deps` still name only official-repo packages (compare against `pacman -Slq`). The repo is public.
Expect tests to need updating: versions, sizes and dates change with every upgrade.
