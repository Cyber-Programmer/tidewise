# Junk scanner fixtures

REAL (captured 2026-11-01 on Arch Linux with Flatpak 1.18.4, `LC_ALL=C`, read-only commands):
- `flatpak-runtimes-real.txt`: `flatpak list --runtime --columns=ref,size,installation`
- `flatpak-app-runtimes-real.txt`: `flatpak list --app --columns=application,runtime,installation`
- `journal-real.txt`: `journalctl --disk-usage`

HAND-WRITTEN (from the documented format; this machine cannot produce them):
- `flatpak-runtimes-handwritten.txt`, `flatpak-app-runtimes-handwritten.txt`: an unused GNOME and KDE
  runtime, a KDE SDK, an old freedesktop base, and two extensions.
- `flatpak-runtimes-broken.txt`: a line with too few columns.
- `journal-de.txt`: German wording and a comma decimal; `journal-zero.txt`: `0B`; `journal-broken.txt`: an error message.
