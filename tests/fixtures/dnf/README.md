# dnf / rpm fixtures

ALL HAND-WRITTEN. The capture machine runs Arch Linux and has neither `rpm` nor `dnf`.

- `rpm -qa --qf '%{NAME}\t%{SIZE}\n'` (size in bytes; several installed kernels appear as several lines;
  `gpg-pubkey` entries are keys, not apps): `rpm-qa-handwritten.txt`, `…-badsize-…`, `…-broken-…`.
- `dnf remove --assumeno -- NAME` with `LC_ALL=C` (dnf 4 table layout; a long package name wraps onto its own
  line): `dnf-remove-{ok,protected,notfound,busy}-handwritten.txt`. `…-de-…` imitates a German run (the parser
  must refuse it); `…-garbage-…` is a broken case. dnf 5 prints a different layout and is not covered.

Replace these with real captures from Fedora containers (dnf 4 and dnf 5) before trusting the parsers.
