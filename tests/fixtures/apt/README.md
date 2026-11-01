# apt / dpkg fixtures

ALL HAND-WRITTEN. The capture machine runs Arch Linux and has neither `dpkg-query` nor `apt-get`, so no real
output exists here. They imitate, from the documented formats:

- `dpkg-query -W -f='${Package}\t${Installed-Size}\t${Status}\n'` (Installed-Size in KiB; the status of a
  removed package with config left is `deinstall ok config-files`; virtual/meta packages may have an empty size):
  `dpkg-query-handwritten.txt`, `…-badsize-…`, `…-broken-…`.
- `apt-get -s remove` run with `LC_ALL=C`: `apt-sim-remove-*-handwritten.txt` (ok, essential warning, unknown
  package, not installed, lock held). `…-de-…` imitates a German run (the parser must refuse it); `…-garbage-…`
  is a broken case.

Replace these with real captures from an Ubuntu 22.04 and a Debian 12 container before trusting the parsers.
