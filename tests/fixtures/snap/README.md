# snap fixtures

ALL HAND-WRITTEN. The capture machine has no `snap`. They imitate `snap list --all` run with `LC_ALL=C`:
space-aligned columns `Name Version Rev Tracking Publisher Notes`; Notes may be `-`, `base`, `snapd`,
`classic` or `disabled` (an old revision kept for rollback), joined with commas. Local installs have a
revision like `x1`. `…-de-…` imitates a German header (the parser must refuse it); `…-broken-…` has a short
row; `…-garbage-…` has a hostile name and control bytes; `…-none-…` is what `snap list` prints when nothing
is installed (it goes to stderr in real life, so both streams must be passed in).

Sizes are not in `snap list`; the adapter reads them from the size of `/var/lib/snapd/snaps/<name>_<rev>.snap`
(world-readable). Replace these fixtures with real captures from an Ubuntu container before trusting them.
