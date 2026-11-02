# Who owns a .desktop file

- `pacman-Qo-real.txt`: REAL. `pacman -Qo $(ls /usr/share/applications/*.desktop | head -6)`, `LC_ALL=C`, Arch Linux,
  captured 2026-11-02 (read-only).
- `dpkg-S-handwritten.txt`: HAND-WRITTEN from dpkg's documented `dpkg -S` format (`pkg[, pkg2]: /path`; a
  multi-arch package is `name:arch`). `dpkg-S-de-handwritten.txt` imitates a German line mixed in.
Replace the dpkg file with a real capture from an Ubuntu container.
- `rpm-qf-*-handwritten.txt`: HAND-WRITTEN (this machine has no `rpm`) from rpm's documented
  `rpm -qf --qf '%{NAME}\n' FILE` behaviour: one package name per line; a file nobody owns prints
  `file X is not owned by any package` (`…-notowned-…`); `…-de-…` imitates a German run (must be skipped);
  `…-broken-…` mixes real names with a shell-looking line, a `--force` line and a line with spaces (all must
  be skipped). Replace with real captures from a Fedora container.
