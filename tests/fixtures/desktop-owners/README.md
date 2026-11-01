# Who owns a .desktop file

- `pacman-Qo-real.txt`: REAL. `pacman -Qo $(ls /usr/share/applications/*.desktop | head -6)`, `LC_ALL=C`, Arch Linux,
  captured 2026-11-02 (read-only).
- `dpkg-S-handwritten.txt`: HAND-WRITTEN from dpkg's documented `dpkg -S` format (`pkg[, pkg2]: /path`; a
  multi-arch package is `name:arch`). `dpkg-S-de-handwritten.txt` imitates a German line mixed in.
Replace the dpkg file with a real capture from an Ubuntu container.
