# Saved real pacman output (WBS 5.3b)

Captured on 15 Nov 2026 inside the Tidewise test container (pacman 7.1.0, libalpm 16.0.1, `LC_ALL` unset = the
container's C locale, stdout and stderr redirected to files, stdin `/dev/null`, no network), against a local test
repository `[spike]` of dummy packages. One `<scenario>.out` (stdout) and `<scenario>.err` (stderr) per run; the exit
code of each is in `exit-codes.txt`. Used by the helper's parser tests. Not edited by hand.

`print-*`: `pacman -S|-Su --print --print-format '%r/%n %v'`. `real-*`: `--noconfirm --ask 66`. `lock-*`: run while
`/var/lib/pacman/db.lck` existed (created by the capture inside the throwaway container). `verify-partial`:
`pacman -Q` with one missing name. `conflict`: the real run that aborts. `real-update-declined`: a pending replacement
that `--ask 66` declined.
