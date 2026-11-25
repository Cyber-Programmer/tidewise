# 0008 — The app refreshes its own private copy of the package lists, as the user

- Status: **Accepted** (6.11, 25 Nov 2026)
- Decided by: Bahauddin Hassan (human), 25 Nov 2026: option A; keep the sandbox on with `setpriv`; hand the folder lock
  to pacman; wait at most 60 s; `fakeroot` a hard dependency
- Task: WBS 6.11 (review 6.10b R-02; threat model question 12 (c); golden rules 1, 3, 4, 9, 11)

## Context

Since 6.10b the helper syncs a private copy of the package lists and, when its list differs from the confirmed one,
refuses and leaves the real lists as old as they were ([0007](0007-update-private-copy.md)). The app built its lists
from the real ones, so the same list was offered and refused again until someone ran a manual `pacman -Sy`. The app
needs fresh lists as the normal user, without a password, without touching `/var/lib/pacman/sync`, with no network but
pacman's own.

## How `checkupdates` does it

`/usr/bin/checkupdates` (`pacman -Q pacman-contrib`: 1.13.1-1): a private database folder
`${TMPDIR:-/tmp}/checkup-db-${UID}/`, `local` **symlinked** to `<DBPath>/local`,
`fakeroot -- pacman -Sy --disable-sandbox-filesystem --dbpath <it> --logfile /dev/null`, then `pacman -Qu --dbpath <it>`;
it removes its own `db.lck` on exit.

## Options

| | Runs | Result |
|---|---|---|
| **A. fakeroot** | `fakeroot -- pacman -Sy` into a private folder | **Chosen.** Works with the sandbox on (below). One new dependency |
| B. user namespace | `unshare --user --map-root-user -- pacman -Sy` | Works **only** with `--disable-sandbox`; with either narrower flag pacman fails with `failed to chown temporary download directory …: Invalid argument` (only uid 0 is mapped, so there is no `alpm`). Not used: it turns off both sandboxes and has no download-user switch, to save a 142 KiB dependency |
| C. the helper's copy | `pacman -Qu --dbpath /run/tidewise-helper-lists/` | Readable by a user once it exists (0711, lists 0644), an error before. Only a helper run, with a password, refreshes it. Not enough |

Also rejected: our own download of the `.db` files (rule 11), libalpm in the app (0004), running `checkupdates` (two
shell scripts, a predictable folder in `/tmp`, output without the repository).

## Decision

### The command

```
/usr/bin/setpriv --no-new-privs /usr/bin/fakeroot -- /usr/bin/pacman -Sy --dbpath <F>/ --logfile /dev/null
```

Started through `pacman_command` (the one place the read-only side starts a program): absolute path, argument list,
empty environment except `LC_ALL=C`. `fakeroot` needs no `PATH` (measured). So proxy variables are not passed on, as in
the helper: behind a proxy set only in the environment, the refresh fails and the real lists are used.

`-Sy` appears only here, always with the private `--dbpath`, and nothing is ever installed from these lists (rule 4).

### The sandbox stays on

As a normal user under fakeroot, pacman believes it is root and applies its Landlock filesystem sandbox without
`no-new-privileges`, which the kernel refuses: `restricting filesystem access failed because the Landlock ruleset could
not be applied: Operation not permitted`. `setpriv --no-new-privs` sets the bit first and then replaces itself with
fakeroot. Measured from a caller without the bit: pacman logs `filesystem access has been restricted to
<F>/sync/download-…/, Landlock ABI is 11` and `successfully restricted 83 syscalls via seccomp`; the same call without
`setpriv` fails. `--disable-sandbox-filesystem` (what `checkupdates` does) is the **recorded fallback, not used**.

### The folder

`$XDG_RUNTIME_DIR/tidewise/lists`, only when `XDG_RUNTIME_DIR` is absolute, a real folder, ours and exactly 0700.
`tidewise/` and `lists/` are made 0700 and checked; `local` must be a link to `<DBPath>/local`; `sync/` and the
`refreshed` file, when there, must be ours and no links. Anything else is refused, never repaired. Not `/tmp`: another
user could make the folder first. It lives in tmpfs (about 9 MB) and goes at logout.

### The lock, and the one deletion (why rule 9 still holds)

pacman **outlives** the process that started it: after killing fakeroot (TERM or KILL) pacman went on and kept the
private `db.lck`. So the app takes an `flock` on the folder and gives pacman that open folder as stdin. Measured: the
lock stays held until pacman ends, however fakeroot or the app ended; a finished refresh releases it. (After a hard kill
an idle `faked` stays behind; it does not hold the lock.)

Under that `flock`, and only there, a left-over `<F>/db.lck` is removed (`remove_stale_lock`, the app's one deletion,
with its own `#[expect]`). It takes only exactly that path, and only a plain file. Rule 9 protects
`/var/lib/pacman/db.lck`, and 0007 extends it to the helper's private lock: neither can be named here. What the rule is
for, not pulling a lock from under a running pacman, is guaranteed by the `flock` our own pacman holds. Nobody else is
meant to run pacman on this folder.

A second caller waits at most **60 s**, then shows the real lists with their age and says another refresh is still
running.

### When, and what reads them

Refresh when a screen with an update list asks and the last successful refresh is older than 10 minutes, and always
after the helper refused. Never on a timer. The time of the last success is the mtime of `<F>/refreshed` (the lists' own
times are the mirror's); it is set to 1970 before pacman starts, so a failed or killed refresh is tried again.

The private copy answers "are updates waiting?" and builds the update and update+install lists. **A plain install keeps
resolving against the real lists**, which are what the helper installs from.

Without `fakeroot`, `setpriv` or a usable folder the app uses the real lists and says how old they are. No error.

### Dependency

`fakeroot` (core, `2:1.37.2-1`, 141.69 KiB installed; Debian's fakeroot) is a hard `depends` of the package.
`setpriv` is `util-linux`, which `fakeroot` depends on.

**`/usr/bin/fakeroot` is a POSIX shell script.** Rule 3 says no shell. We start no shell and pass no `-c`, and nothing a
user typed is an argument (the only variable one is the checked folder), but a shell does run. This is a bent rule,
accepted by choosing option A.

## Measured

`tests/container/tests/app_private_lists.rs` (the measurements) and `app_private_refresh.rs` (the app's own code as
`nobody`), `--network=none`, a local `file://` repository. `run.sh` starts these two without `no-new-privileges`.
Measurements also agreed on a throwaway image at the Arch Archive snapshot of 23 Nov (pacman 7.1.0.r9.g54d9411-4,
fakeroot 2:1.37.2-1, this machine's versions); the committed image has -3 and fakeroot 1:2.1.4.

Also seen: a plain `pacman -Sy` as a user is refused; the real lists, `pacman.log`, the local database and the absence of
the real `db.lck` are unchanged by a refresh; a failed download keeps the old private list; a killed or refused refresh
leaves an empty `download-*` folder in `<F>/sync`.

## Not shown, and what is left

- **Never run on a real machine.** Container only. Real mirrors, signed databases (`DatabaseOptional`) read by a user's
  pacman, and a desktop session's `XDG_RUNTIME_DIR` need the manual check.
- Under fakeroot the switch to `alpm` is only pretended (the files are the user's), so the download is as unprivileged
  as the user and no more separated than that. The two sandboxes apply.
- The private lists can be written by anything running as the user. Only the app reads them; the helper never does, and
  re-checks the confirmed list itself (T-09).
- The first caller has no time limit of its own (pacman's download timeout applies), and Cancel does not stop it.
- No caller yet for `AfterRefusal` (6.5) or for update lists with `ResolveInput::dbpath` (6.6).
