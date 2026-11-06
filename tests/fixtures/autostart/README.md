# Autostart fixtures

- `baloo_file.desktop`, `localsearch-3.desktop`, `xapp-sn-watcher.desktop`, `org.kde.discover.notifier.desktop`,
  `org.kde.plasmashell.desktop`: REAL, copied read-only from `/etc/xdg/autostart` on Arch Linux (KDE Plasma 6),
  2026-11-02. System package files, no personal data.
- `*-handwritten.desktop`: HAND-WRITTEN: a user entry written by an app (with a localized name, comments and an
  action group), a GNOME-disabled entry, a `Hidden=true` entry, an `OnlyShowIn=GNOME` entry and a broken file.
  This machine's `~/.config/autostart` is empty, so there is no real user entry to capture.
