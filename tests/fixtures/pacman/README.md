# pacman fixtures

Real pacman output saved for the parser tests (WBS 3.3). This is a small hand-picked sample, not a machine's
full package list. Every package named here, including names in `Required By`, `Optional For` and
`Optional Deps`, comes from the official Arch repos (checked against `pacman -Slq`).

- Captured: 2026-11-11, on Arch Linux
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

## Refreshing

Run the commands above again from this folder. Before committing, check that `Required By`, `Optional For` and
`Optional Deps` still name only official-repo packages (compare against `pacman -Slq`). The repo is public.
Expect tests to need updating: versions, sizes and dates change with every upgrade.
