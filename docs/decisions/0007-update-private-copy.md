# 0007 — An update syncs a private copy of the package lists first, and only moves it in once the list matches

- Status: **Proposed** (6.10a). It becomes **Accepted** in 6.10b, after the container measurements below agree with the
  source reading. If one disagrees, the record is revised and the human is asked before any code depends on it.
- Date: 2026-11-18
- Decided by: Bahauddin Hassan (human), 2026-11-18, the 6.10 plan: option 1; private folder `/run/tidewise-helper-lists/`
  at 0711; one staged copy and one `rename` per list into `/var/lib/pacman/sync/`; journal field `TIDEWISE_LISTS`
- Task: WBS 6.10 (review 5.8 R-02; threat model questions 12 and 33; golden rules 4, 5, 7, 9)

## Context

Today `update_job` (`crates/helper/src/flows.rs`) runs `pacman -Sy` on the **real** lists, then compares `pacman -Su --print`
with the list the user confirmed. If the two differ (a mirror changed since the list was made, or a request carries a
forged list with the right summary) the helper refuses with `TransactionChanged`, but the real lists are already newer
than the installed packages. A later plain `pacman -S foo`, by the user or any other tool, is then a partial upgrade
(golden rule 4). Three reviewers found this independently (5.8 R-02). Question 12 deferred it to M4 on 13 Nov 2026 and
said it had to be settled before the update flow was built; the flow is built.

### What pacman 7.1.0 does (read in the source, `alpm-sys` 5.0.1 ships pacman 7.1.0, the host runs `7.1.0.r9`)

1. **A sync moves its files one by one, never as a set.** `alpm_db_update` (`lib/libalpm/be_sync.c:139`) downloads into
   `sync/download-XXXXXX/`, then `move_file` (`dload.c:1086`) `rename`s each file into `sync/`. A kill half-way leaves some
   repositories new and some old. This is the baseline any move of ours has to match, not beat.
2. **The download runs as `alpm`.** As root with `DownloadUser = alpm` (this host, `pacman.conf:39`),
   `_alpm_download_dir_setup` (`util.c:961`) makes the temporary folder and `chown`s it to `alpm`, and
   `alpm_sandbox_setup_child` (`sandbox.c:51`) drops to that user. It writes by absolute path, so **every folder above the
   private `sync/` has to be searchable by `alpm`**. A 0700 folder (like `/run/tidewise-helper`) would break it, unless the
   sandbox is switched off, which this design does not do (checkupdates switches it off because it runs under fakeroot, not
   as root).
3. **Who owns a finished list: root.** The temporary folder belongs to `alpm`, but `finalize_download_file`
   (`dload.c:76-90`) does `chown(filename, 0, 0)` when pacman runs as root and then `chmod(~umask & 0666)` (0644 under the
   helper's umask 022), and only then `move_file` renames it into `sync/`. So a downloaded `sync/*.db` is `root:root 0644`,
   not `alpm`. **This is a source reading; 6.10b measures it** (stat of the private and the real `sync/*.db` after a sync,
   `pacman_private_sync.rs`). The rule for the move is: the owner is **exactly root (uid 0)**. If the measurement shows
   `alpm`, the rule becomes "exactly root or the uid of the configured `DownloadUser`, nothing else", and I stop and ask
   first, because reading `DownloadUser` is either a new pacman command (`pacman-conf`, golden rule 2) or a parse of
   `/etc/passwd`.
4. **The lock is `<dbpath>/db.lck`** (`alpm.c:69`). With `--dbpath <private>` pacman locks a private file and never
   touches the real `db.lck` (golden rule 9). A dry run (`--print`) takes no lock at all (`pacman.c:1296`,
   `ALPM_TRANS_FLAG_NOLOCK`).
5. **The log.** pacman writes "Running '…'" (`src/pacman/pacman.c:1166`) and "synchronizing package lists" (`sync.c:934`)
   to its log file. `--logfile /dev/null` keeps a refused update out of `pacman.log`.
6. **`pacman -Sc` (the user's own, `sync_cleandb`, `src/pacman/sync.c:55`).** After its prompt ("Do you want to remove
   unused repositories?") it `rmrf`s every **directory** in `<dbpath>/sync/` and `unlink`s every file whose name does not
   end in `.db`, `.db.sig`, `.files` or `.files.sig`; a `.db` that matches no configured repository goes too. So a
   leftover `.tidewise-<repo>.db.new` (ends in `.new`) is **removed by pacman itself** the next time the user cleans, and
   nothing in `sync/` is read except by repository name, so a leftover is harmless until then. The private folder is
   under `/run`, outside `DBPath`: `pacman -Sc` never sees it. (This corrects a sentence of the plan that said pacman
   "ignores unknown files in `sync/`": true for a sync, not for `-Sc`.)
7. **checkupdates' recipe**, which this follows: `ln -s <DBPath>/local <private>/`, then `pacman -Sy --dbpath <private>
   --logfile /dev/null`, then `-Qu --dbpath <private>`. The `local` database is linked, never copied.

### The machine

- `/run` is tmpfs and `/var/lib/pacman` is on ext4 (`findmnt`). `rename(2)` across filesystems fails with `EXDEV`, so the
  private lists cannot simply be renamed into place: they are copied into `sync/` first, then renamed within it.
- `/var/lib/pacman/sync` is 8.9 MiB here: `core.db` 129 821 B, `extra.db` 9 079 125 B, `multilib.db` 85 711 B. It also holds
  six leftover `download-*` folders from 9 Sep, owned by `alpm`: killed syncs leave exactly that. A private copy costs
  about 9 MB of tmpfs until the next reboot.

## Options

1. **Private sync, compare, move in, compare again, then update.** `pacman -Sy --dbpath <private> --logfile /dev/null` →
   `-Su --print --dbpath <private>` must equal the confirmed list → move the verified lists into the real `sync/` (only
   while the real `db.lck` is absent) → `-Su --print` on the real lists must equal it again → real `-Su`.
2. **Private check, then a normal real `-Sy` and compare again.**
3. **Back the real lists up, run the real `-Sy`, restore on a mismatch.**

| | 1 (recommended) | 2 | 3 |
|---|---|---|---|
| A refusal (list differs) | Real lists untouched, byte for byte. | Untouched if the private compare fails; if the mirror moved between the two downloads, the R-02 state again. | Lists flip to new and back; `pacman.log` records a sync that was then undone. |
| The mirror changes after the user confirmed | Caught privately; what is moved in is the file that was verified, never a second download. | A window of seconds between the two downloads. | Caught, but only after the real lists were touched. |
| A forged confirmed list | Caught privately. | Caught privately. | Caught after the real `-Sy`. |
| SIGKILL during the private sync | Real lists untouched. A private `db.lck` is left (see below). | Same. | Between `-Sy` and the restore: the R-02 state plus backups lying around. |
| SIGKILL during the move | Some lists new, some old: the same as pacman's own `-Sy` (fact 1). The window is the renames, after every copy is staged and synced. | No move. | The restore is a rename too: the same mix. |
| SIGKILL after the move, before `-Su` | Lists newer than packages: the residual every update flow has (T-11, question 12). | Same. | Same. |
| Cost | One download, a ~9 MB copy, one `rename` exception. | Two downloads, no rename. | A copy and a rename both ways. |

## Decision

**Option 1.** Details:

### Where the private copy lives

`/run/tidewise-helper-lists/` (`--dbpath`, with the trailing slash pacman wants), owned by root, exactly mode **0711**
(`alpm` can walk through, not list), opened like `WorkDir`: a real directory, not a symlink, owner and mode checked,
refused and never repaired. Not under `/run/tidewise-helper` (0700, fact 2). Inside it:
- `local`: a symlink to `/var/lib/pacman/local`, checked on every run with `symlink_metadata` and `read_link`; a wrong link
  is refused, never fixed;
- `sync/`: mode 0711, where the private sync writes.

tmpfs means a reboot clears everything, including a private `db.lck` that a killed private sync left. The helper never
deletes any `db.lck`, the private one included (golden rule 9): a private lock makes the update refuse with the
`DatabaseLocked` text, and the user is told to wait or restart. It is not the real lock, so the real lists and the real
`db.lck` are untouched.

### The move, in this order

For each private `sync/<repo>.db` (and `<repo>.db.sig` if there is one):

1. **Check.** The repository name passes `is_helper_repo_name`; the file is a regular file (`symlink_metadata`, never
   followed); its owner is **exactly root** (fact 3).
2. **Look for the real lock.** The real `db.lck` is absent (looked at, never created, changed or removed). If it is
   present: stop, nothing moved, `DatabaseLocked`.
3. **Stage every copy.** Create `<real sync>/.tidewise-<repo>.db.new` (`create` with `O_NOFOLLOW`, truncating, as root,
   mode 0644, never the private file's owner or mode), write the private file into it, **`fsync` the file**. All lists are
   staged before any is renamed.
4. **Look for the real lock again, immediately before the rename loop** (the staging takes time; another package manager
   may have started). Present: stop, nothing renamed. The staged dotfiles stay (fact 6: `pacman -Sc` removes them, and the
   next run truncates and reuses them).
5. **Rename each staged file over `<repo>.db`.** Same folder, so atomic per file. The one `#[expect(clippy::disallowed_methods)]`
   for `std::fs::rename`, on one function, in a new `lists.rs`.
6. **`fsync` the real `sync/` directory.** `fsync(2)`: syncing a file does not guarantee its directory entry; the
   renames are only durable once the directory is synced. A unit test pins the order: stage all, `fsync` each, rename each,
   `fsync` the directory.
7. **Then** the real `-Su --print` runs on the real lists and must equal the confirmed list again, and only then the real
   `-Su`.

**Why per file, not as a set, is acceptable.** pacman's own `-Sy` gives no better guarantee (fact 1). Staging everything
first, and syncing before renaming, shrinks the window to three `rename` calls. Another root process (PackageKit, a cron
job) can still change the lists between the move and the real dry run; the second compare catches that, and it is the same
residual window as T-09, stated and not hidden.

**Nothing is ever deleted by the helper.** The leftover of a kill is a dotfile that pacman's own `-Sc` removes; `removal.rs`
does not change.

### `--logfile`

`/dev/null` for the private sync and the private dry run, so a refused update never shows up in `pacman.log`. pacman does
not log the move, so the helper's finished journal entry says what happened to the real lists with one fixed field:
`TIDEWISE_LISTS=moved` or `untouched`.

### The rename ban (`clippy.toml`)

`std::fs::rename` stays banned for the whole workspace. It gets exactly one named exception, on `lists::replace_list`, with
a reason: one rename inside the real `sync/` of a staged pacman list (cache-class data, not a user file, golden rule 7).
`clippy.toml` itself does not change; its header comment gains the new allowed place. `crates/helper/tests/file_changes.rs`
gains its new lines on purpose (`DirBuilder` for the private folder, `symlink(`, `OpenOptions`, `fs::rename`, and the lock
look). `removal.rs` does not change, because nothing is deleted.

## What stays out of this decision

- **The app's own unprivileged refresh** (where the confirm list comes from: a fakeroot private sync like checkupdates, or
  not; the app side of question 12) is not part of 6.10. It is a follow-up for 6.1 and 6.6. After 6.10b a stale app list costs
  one refused update and no damage.
- **A `.sig` that matters.** Arch's databases are not signed here (`SigLevel = Required DatabaseOptional`); the move
  carries `.db.sig` if the private sync produced one, with the same checks. The measurement covers both.

## To measure before this becomes Accepted (6.10b, container, `pacman_private_sync.rs`)

1. The download works as `alpm` through a 0711 private folder and fails through a 0700 one (fact 2).
2. `stat` of the private and the real `sync/*.db` after a sync: owner (fact 3), mode.
3. `--logfile /dev/null` keeps `pacman.log` free of lines from the private sync (fact 5).
4. A stale private `db.lck` makes the private sync fail with the lock error, and the real lists are untouched.
5. `.db.sig` handling; `--dbpath` with and without the trailing slash.
6. `pacman -Sc` on a `sync/` that holds a `.tidewise-core.db.new` and a `download-*` folder removes both (fact 6).

## Consequences

- No command can refresh the real lists any more: `PacmanCommand::Sync` is replaced by a private sync, and "`-y` appears
  only with `--dbpath <private>`" becomes a template test. Golden rule 4 gets a stronger proof than today's.
- A refused update has no side effect on the real lists (the new unit and container tests of 6.10b).
- New root file writes (the private folder, the staged copy, one rename) are recorded in the threat model (T-03, T-20, a new
  row) and in `file_changes.rs`; a fresh adversarial review follows each PR.
- The residual that stays: a kill between the move and the real `-Su` leaves the lists newer than the packages. It is the
  same as for pacman's own `-Sy` followed by a kill, it is documented (T-11, question 12), and it cannot be avoided by any
  update flow that has to refresh the lists at all.
