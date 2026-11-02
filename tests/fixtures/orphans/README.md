# Orphan (unused package) fixtures

- `../pacman/Qdtq-real.txt` and `../pacman/Qi-orphans-real.txt`: REAL. `LC_ALL=C pacman -Qdtq` (25 packages)
  and `LC_ALL=C pacman -Qi -- <those names>` on Arch Linux, captured 2026-11-02 (read-only).
- `apt-autoremove-*-handwritten.txt`: HAND-WRITTEN from the documented output of `apt-get -s autoremove`
  (same layout as `apt-get -s remove`: `Remv name [version]`); `none` = nothing to remove, `de` imitates a
  German run (must be refused, not guessed), `garbage` has hostile names.
- `dnf-autoremove-*-handwritten.txt`: HAND-WRITTEN from the documented dnf 4 table of
  `dnf autoremove --assumeno`; same four cases. dnf 5 prints a different layout and is not covered.
No `apt-get` database or `dnf` exists on the capture machine. Replace the hand-written files with real
captures from Ubuntu / Fedora containers before trusting the parsers.
