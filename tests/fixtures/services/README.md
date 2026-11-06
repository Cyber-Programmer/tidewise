# Services fixtures

- `list-units-real.json`, `list-unit-files-real.json`, `show-memory-real.txt`: REAL. `LC_ALL=C systemctl
  list-units --type=service --all --output=json --no-pager`, `… list-unit-files --type=service --output=json
  --no-pager` and `… show --type=service --property=Id,MemoryCurrent --no-pager` on Arch Linux (systemd 262),
  captured 2026-11-02 (read-only). Checked: no user name or host name inside. This machine has **no failed
  service**, so failed units are covered by hand-written JSON inside the tests.
