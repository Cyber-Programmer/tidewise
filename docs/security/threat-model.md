# Threat model v1 (STRIDE)

Status: **v1, 13 Nov 2026** (WBS 5.1); **updated 14 Nov 2026 for 5.2** (protocol crate: rows T-02, T-04 to T-07, T-09 to T-11, T-15, open questions 8, 11, 12). Written before any privileged code exists, so every M3 task starts from this
list. Update it whenever a milestone adds a privileged action, a file write, a parser of outside data or a network call
(ENGINEERING.md §2).

How to read it: section 1 draws the system, 2 names what we protect, 3 compares the two ways to reach root
(**decided 13 Nov 2026: pkexec + checked summary, see `docs/decisions/0005-helper-invocation.md`**), 4 is the threat table, 5 says what we do *not* defend, 6 lists what I could not verify,
7 lists sources. `[n]` is a source in section 7. "Planned" means the defence is designed but no code or test exists yet.

## 0. Base assumption (read this first)

**A1. The helper trusts nothing about its caller.** The polkit policy only decides *who may become root*; it does not
say *who is calling*. Any process running as the user can run `pkexec /usr/lib/tidewise/tidewise-helper` with a JSON
request of its own, not just the Tidewise app. So the app is not a security boundary, and the helper must treat every
request as hostile: full validation, allow-list, re-check of the confirmed list, every time, no matter who asks.
The caller's identity (`PKEXEC_UID` [1], parent process, argv) is only used for logging, never to relax a check.
This is threat T-01.

## 1. System and trust boundaries

Three privilege levels, and two lines you can cross only by being authorized:

```mermaid
flowchart LR
  subgraph USER["Boundary A: the user's session (uid 1000) - not trusted by the helper"]
    APP["tidewise app (Slint)<br/>builds requests, shows the confirm screen"]
    OTHER["any other process of the user<br/>(browser, script, malware)"]
    RO["read-only paths (M1/M2)<br/>pacman -Qi/-Qq/-Qu/-Ss/-Si, pacman-conf"]
  end
  subgraph AUTH["Boundary B: authorization"]
    PK["pkexec (setuid root) + polkitd<br/>+ the user's authentication agent"]
  end
  subgraph ROOT["Boundary C: root"]
    HELP["tidewise-helper<br/>JSON on stdin -> validate -> allow-list<br/>-> re-check -> run"]
    PAC["/usr/bin/pacman<br/>(env cleared, LC_ALL=C)"]
    HOOKS["hooks + install scripts<br/>(from signed packages)"]
  end
  DB[("/var/lib/pacman<br/>db.lck, sync dbs, keyring")]
  FILES[("user's files<br/>(never touched by v0.1)")]
  NET(("mirrors<br/>(pacman only)"))

  APP -- "request (JSON)" --> PK
  OTHER -. "same call, own JSON (T-01)" .-> PK
  PK -- "after admin password" --> HELP
  HELP -- "argv list, no shell" --> PAC
  PAC --> HOOKS
  PAC <--> DB
  PAC <-- "signed packages" --> NET
  RO -- "runs as the user, reads text" --> PAC
  HELP -- "result + coded errors" --> APP
  APP -.- FILES
```

- **Boundary A -> B** is where untrusted input enters. The only data that crosses it is the request JSON (stdin) and
  whatever the helper prints back.
- **Boundary B** decides "may this user become root now?". It does not see the request (T-02).
- **Boundary C** is the only place with root. Inside it the helper turns a validated request into a fixed `pacman`
  command line.
- **Read-only paths** (already built, M1/M2) cross no privilege boundary: the app runs `pacman` as the user and parses
  its text output. The input is *outside data* (package names and descriptions come from repository maintainers), so
  the parsers are treated as untrusted-input code (T-21).

## 2. Assets

| Asset | Why it matters | Who may touch it |
|---|---|---|
| **Root on the machine** | Everything else follows from it | Only the helper, only via the allow-list |
| **Package database and keyring** (`/var/lib/pacman`, `db.lck`, `/etc/pacman.d/gnupg`) | A corrupt or tampered db breaks the system; a half-finished upgrade can leave it unbootable | pacman only |
| **The user's files** (home directory) | Cannot be recreated | v0.1 helper never reads or writes them; later cleanup goes to Trash (rule 7) |
| **System config and installed binaries** (`/etc/pacman.conf`, `/usr/bin/pacman`, hooks) | Code run as root | Root only; not ours to defend (section 5) |
| **Integrity of what the confirm screen shows** | The user's consent is only worth something if it is about the real change | Helper re-checks the list (T-09) |
| **Tidewise's own installed files** (helper binary, polkit policy) | A writable helper is a free root shell | Root only (T-03) |
| **The audit trail** (journal, `pacman.log`) | "What did root do, and for whom?" | Append-only for us (T-14) |

## 3. How the app reaches the helper: pkexec vs a D-Bus service

Decision: **decided 13 Nov 2026, see `docs/decisions/0005-helper-invocation.md`**: pkexec with a human-readable summary
in argv that the helper checks (3.2); D-Bus is the fallback. The analysis below is kept as the reasoning behind it.
Both options end in the same helper logic and the same allow-list; they differ in how the process gets root and in what
the user is shown before saying yes.

### 3.1 The main point: what can the password dialog show? (threat T-02)

The danger: a malicious process calls the helper at the very moment the user expects Tidewise's prompt. The user sees
a password dialog, assumes it is theirs, and approves a *different* request.

- **pkexec.** The dialog text is the `<message>` of the polkit action. pkexec offers only these variables for it:
  `$(program)`, `$(command_line)`, `$(user)`, `$(user.gecos)`, `$(user.display)` [1]. The request travels on **stdin**,
  which polkit never sees. So every request, ours or an attacker's, produces the *same* dialog. The one thing that
  varies is `$(command_line)` (the argv), and pkexec's own man page says of it: "do not use this for any security
  checks, it is not secure" [1], because the caller chooses it freely.
- **D-Bus service.** The service (root) calls polkit's `CheckAuthorization(subject, action_id, details, flags, ...)`
  [3]. `details` is a string dictionary describing the action; a non-empty `polkit.message` key in it overrides the
  message shown to the user and may "include references to other keys that are expanded" [3]. Keys starting with
  `polkit.` are accepted only from a caller that is itself sufficiently privileged (e.g. uid 0) [3], and the service is
  root. So the *service* can write "Install vlc and 3 more packages (212 MB)" from the request **it** validated, and
  the attacker cannot change that text. The dialog tells the truth about the request that will actually run.
  (Whether the policy file's own `<message>` can also reference detail keys: I believe so but did not verify it. The
  `polkit.message` route above is the documented one.)
- **Evidence (13 Nov 2026, the human's KDE agent):** `pkexec /usr/bin/true install dbeaver jre-openjdk`, cancelled at
  the dialog: the agent **showed the full argv in the dialog title** [14]. That used the stock `exec` action; our own
  action must be re-tested in 5.4 (a custom message may hide argv).
- **What I could not verify:** how GNOME's agent renders these strings; truncation of long summaries, multi-line and
  markup; and what PackageKit's dialog shows. Needs a test on a real desktop.

### 3.2 A middle way for pkexec (my idea, untested)

Make argv carry a short human summary, and make the helper refuse unless it matches:

`pkexec /usr/lib/tidewise/tidewise-helper --summary "install vlc +3 (212 MB)"` with the policy message containing
`$(command_line)`. After authentication the helper recomputes the summary **from the validated JSON** and exits with
"summary mismatch" if the argv text differs. Then a lying caller does not get a harmful action; it only wastes one
approval. The dialog is truthful whenever the action succeeds. Costs: the summary is visible in `ps`; the summary
format becomes part of the protocol; whether agents show `$(command_line)` clearly is unverified (above).

### 3.3 Comparison

| | **pkexec, one process per job** | **D-Bus system service + polkit** |
|---|---|---|
| Root code running when idle | None. Root exists only during a transaction | A daemon (activated on demand by D-Bus/systemd, may exit when idle). More moving parts |
| Attack surface | pkexec is a **setuid-root** binary we do not control, plus polkitd | polkitd + the D-Bus daemon + our service parsing D-Bus messages; no setuid binary of ours, but the service must authenticate callers by bus name correctly |
| Dialog shows the request (T-02) | **No**, only program path / argv [1]. With 3.2: a checked summary | **Yes**, the service passes `details` [3] |
| Password prompts | One per `pkexec` call. Use `auth_admin` (not `_keep`) so a retained authorization can't be reused by another process [1] | Same policy choices. Service can also rate-limit before asking polkit (helps T-16) |
| Complexity / new dependencies | Smallest: a binary, a `.policy` file. No new crate for transport | Larger: a D-Bus crate (e.g. `zbus`), a service file, a D-Bus policy file, a systemd unit; dependency review needed |
| Caller identity | `PKEXEC_UID` (set by pkexec [1]); still only "some process of this user" | Bus name -> pid/uid via the bus; still only "some process of this user" (T-01 remains) |
| Known history | **PwnKit CVE-2021-4034**: memory corruption in pkexec's argument handling, present since pkexec was created in 2009, local root for any user, disclosed 25 Jan 2022 [4]. **CVE-2011-1485**: race in pkexec/polkitd about the caller's identity, polkit 0.96 [6] | **CVE-2021-3560**: polkit mishandled a D-Bus caller that disconnected mid-request and treated it as uid 0, giving an unprivileged user authorization (GHSL-2021-074) [5]. Affected-version ranges differ between sources (0.113-0.118 vs "seven years old"), so I give no exact range |
| Which parts are still trusted either way | polkit (policy + agent) | polkit (policy + agent) |

Two honest observations: (1) **Neither choice removes polkit from our trusted base**; both of the big historical bugs
above were in polkit code that both options use or sit next to. Arch ships patched polkit (128-1 on the machine I
checked); the exact fixed versions: I don't know. (2) The allow-list is what bounds the damage in both cases: a wrongly
approved request can only do what an allow-listed action does (e.g. install an official-repo package), never "run
anything as root".

### 3.4 Recommendation (not a decision)

**pkexec, one process per confirmed transaction, request as JSON on stdin, with the argv-summary check from 3.2.**
Why: no root process stays alive, it fits golden rules 1 and 6 as written, it adds no dependency, and the helper's
checks (the part that matters, A1) are identical in both designs. The cost is the weaker dialog; 3.2 narrows that gap,
and the allow-list bounds what is left.

What would flip me to D-Bus: if a manual test on your KDE (and GNOME, if you care) agent shows that `$(command_line)`
is truncated or unreadable, so the summary does not reach the user. Then T-02 can only be closed properly by the
service passing `details`. D-Bus would also change the wording of golden rule 1 ("started with `pkexec`").

**Decision: made (human, 13 Nov 2026): pkexec + checked summary; see `docs/decisions/0005-helper-invocation.md`.**
The KDE evidence above supports it; the 5.4 re-test with our own action can reopen it.

## 4. Threats

STRIDE letters: **S**poofing, **T**ampering, **R**epudiation, **I**nformation disclosure, **D**enial of service,
**E**levation of privilege. "Proof" names the planned test; IDs of WBS tasks I do not know are written as
"M3 helper task (ID to fill)". Test names marked *exists* are real tests in the repo today.

### 4.1 Your three additions first

| ID | STRIDE | Threat | Defence | Proof | Status |
|---|---|---|---|---|---|
| **T-01** | S, E | **Any process in the user's session can run `pkexec tidewise-helper` with its own JSON**, not just our app. Also covers a compromised app. | Base assumption A1: the helper trusts nothing about its caller. Every request gets the full pipeline: size cap -> strict JSON -> allow-list of actions -> name validation -> re-check of the confirmed list. `PKEXEC_UID` is for the log only. Policy requires admin authentication on every call (`auth_admin`, no `_keep`, `allow_any`/`allow_inactive` = `no`) so no retained authorization can be ridden [1][2] | Container test: run the helper directly (no app) with each row of the hostile-input table (T-04..T-07) and with a valid request from a different uid; assert the same checks fire. Unit test: no code path reads `PKEXEC_UID` to allow anything | planned |
| **T-02** | S, E | **Authorization confusion.** A malicious process starts a helper request at the moment the user expects Tidewise's prompt; with pkexec the dialog cannot show the request (stdin is invisible to polkit), so the user approves the wrong one. | See section 3. If pkexec: summary in argv, helper recomputes it from the validated JSON and refuses on mismatch; `auth_admin` every time; the allow-list bounds what a wrongly approved request can do. If D-Bus: service passes `details`/`polkit.message` built from its own validated request [3]. Residual: user fatigue (T-16) and a fake password dialog drawn by malware (section 5) | Unit test: argv summary != summary derived from JSON -> refused (planned with the helper, 5.3). **Protocol side done (5.2):** `summary()` in `crates/helper-protocol`, one fixed function, printable ASCII, at most 200 bytes, targets then chosen providers (when all fit) then the count; tests *exist*: `the_sample_requests_read_like_this`, `a_single_target_is_cut_exactly_at_the_cap`, `and_n_more_counts_the_targets_that_did_not_fit`, `chosen_providers_are_listed_when_all_of_them_fit_and_left_out_when_not`, `requests_that_differ_in_what_the_user_sees_read_differently`, `whatever_the_request_holds_the_text_is_printable_ascii_and_capped`; fuzz target `request`. Known limit: when the list is cut, requests differing only past the cut read the same (the confirmed list, T-09, is what is re-checked) Dialog: the human's KDE agent showed the full argv in the title for the stock action (13 Nov 2026, [14]); **re-test with our own polkit action in 5.4**, and on GNOME if wanted, recorded in progress.md as a bent rule | protocol: done (5.2); helper compare: planned (5.3); dialog re-test pending (5.4) |
| **T-03** | T, E | **Tampering with installed files:** the helper binary, the polkit policy, any config or library the helper loads. A user-writable helper is a free root shell. | The package installs everything as `root:root`, directories `0755`, files not group/other-writable, under `/usr/lib/tidewise/` and `/usr/share/polkit-1/actions/`; nothing under `$HOME`, `/tmp` or `/opt`. The helper reads **no** config from user-controlled places (no env, no home). No RPATH/RUNPATH in the binary (`namcap` flags insecure RPATH/RUNPATH [9]); `cargo` builds without `-C rpath`. The policy's `exec.path` annotation is the absolute helper path [1] | **WBS 5.5**: `namcap` on the built package (`insecure-rpath` / `insecure-runpath` clean) + a permission test that walks the package file list and every parent directory and asserts owner root and no group/other write bit; plus `readelf -d` check for RPATH/RUNPATH | planned |

### 4.2 Input to the helper and to pacman

| ID | STRIDE | Threat | Defence | Proof | Status |
|---|---|---|---|---|---|
| T-04 | T, E | **Option injection into pacman.** Any option that moves pacman's root, config, hooks, keys or checks turns the helper into a root shell. All of these exist in pacman 7.1's man page [7]: `--root/-r`, `--dbpath/-b`, `--config`, `--hookdir`, `--gpgdir`, `--cachedir`, `--sysroot`, `--logfile`, `--overwrite`, `-d/-dd/--nodeps`, `--assume-installed`, `--noconfirm`, `--noscriptlet`, `--dbonly`, `--disable-sandbox`, `--print-format` | The request has **no free-form option field**. The helper builds argv from fixed templates (one per allow-listed action; the helper itself adds `--noconfirm` only as golden rule 10 allows: a verified transaction, never a caller's choice); the only caller-supplied values are validated `repo/name` pairs placed after `--`. A test lists every option above and shows no request can produce it. Never a shell (rule 3), absolute `/usr/bin/pacman` | Unit tests on the argv builder (one per forbidden option, plus "every argv equals template + names"); container test with real pacman for `-S -- --root=/x`-style names. Read-only side, *exists*: `option_lookalikes_stay_search_words_after_double_dash`, `args_are_si_then_double_dash_then_repo_slash_name`, `command_is_absolute_pacman_with_arg_list_and_c_locale`. **Protocol side (5.2), *exists*:** the request types have no field for any option, path or URL, and every struct refuses unknown fields: `an_unknown_field_is_refused_at_every_level`, `flags_paths_and_urls_have_no_field_to_live_in` (noconfirm, overwrite, nodeps, root, config, dbpath, hookdir, print_format, file, url, sysupgrade, refresh at two levels), `actions_that_do_not_exist_are_refused`. The argv builder itself is 5.3 | read-only: done; protocol: done (5.2); argv builder: planned (5.3) |
| T-05 | T, E | **`-U`, local file paths, URLs.** `pacman -U` installs an arbitrary file or URL as root. | The protocol has **no path or URL field**; the helper only ever runs `-S` (sync) forms with repo package names, never `-U`. Names with `/`, `:`, `.` first etc. are refused by the allow-list (T-06) | Unit test: argv never contains `-U`/`--upgrade`; request with `file:///x`, `/tmp/a.pkg.tar.zst`, `http://..` as names is refused. **Protocol side (5.2), *exists*:** `hostile_names_are_refused_as_names` (paths, `file://`, `http://`, `../x`, `*`), `flags_paths_and_urls_have_no_field_to_live_in` (`file`, `url`) | protocol: done (5.2); argv: planned (5.3) |
| T-06 | T, E | **Hostile names:** leading `-`, NUL, `../`, whitespace, shell characters, unicode look-alikes (Cyrillic "a", full-width characters), empty string, 10 000 names, one 1 MB name | Allow-list (not block-list) from `man PKGBUILD`: ASCII letters, digits and `@ . _ + -`, not starting with `-` or `.`; repo names likewise (no `/`, not `local`, no control chars). Refuse the whole request on the first bad name; never "clean" a name. Cap on name length and on the number of names per request | *exists*: `accepts_real_package_names`, `rejects_everything_else` (includes `café`), `rejects_repo_names_pacman_or_we_refuse`, `bad_package_names_are_refused`, `one_bad_ref_among_good_ones_refuses_everything`. Helper: same table plus NUL, 10 000 names, look-alikes, over-length. **Protocol side (5.2), *exists*:** one copy of the rules in `tidewise-names`; the helper's repo rule is ASCII only and at most 64 bytes (`helper_accepts_plain_ascii_repo_names`, `helper_refuses_everything_that_is_not_plain_ascii_or_is_reserved`); `hostile_package_names_are_refused_everywhere_a_name_can_go`, `hostile_repo_names_are_refused_everywhere_a_repo_can_go`, `bad_versions_are_refused`, `too_many_of_anything_is_refused` (100 targets, 100 provider answers, 5 000 confirmed lines), `duplicates_are_refused`, `hostile_names_are_refused_as_names` (26 rows through `parse_request`, including a 1 MB name). Refuse, never clean: the first problem fails the whole request | read-only: done; protocol: done (5.2) |
| T-07 | D, T | **Huge or deeply nested JSON**, trailing garbage, duplicate keys, unknown fields | Read stdin with a hard byte cap before parsing; parse into typed structs with unknown fields denied; exactly one JSON value, nothing after it; bounded nesting (serde_json has a default recursion limit; I believe 128, to be confirmed by a test, not assumed) | Hostile-input tests (oversize, 10 000 levels, trailing data, NUL in strings, `-1`/huge numbers) + a `cargo fuzz` target for the request parser (ENGINEERING.md §2). **Protocol side (5.2), *exists*:** `a_request_over_the_byte_cap_is_refused_before_parsing` (2 MiB, checked before parsing), `anything_after_the_value_is_malformed`, `a_repeated_or_unknown_key_is_malformed`, `another_protocol_version_is_a_version_mismatch`, `a_version_that_is_not_a_plain_number_is_malformed`, `deep_nesting_is_refused_and_does_not_overflow_the_stack`, `broken_json_is_refused`, `a_request_over_two_mebibytes_is_refused_before_it_is_read`; fuzz target `request` (run result in progress.md, 5.2). **Correction:** serde_json has no 128-level limit here, see open question 8 | protocol: done (5.2) |

### 4.3 Process environment

| ID | STRIDE | Threat | Defence | Proof | Status |
|---|---|---|---|---|---|
| T-08 | E, T | **Inherited environment:** env vars, `PATH`, `LD_PRELOAD`/`LD_LIBRARY_PATH`, working directory, umask, open file descriptors, signals | pkexec already starts the program with "a minimal known and safe environment" [1] and in the target user's (root's) home directory unless `--keep-cwd` is used [1]; the helper does not rely on that: clear the environment for every child (`env_clear` + `LC_ALL=C`), absolute paths only (no `PATH` lookup), `chdir("/")`, set `umask 022`, stdin of children = `/dev/null`, check there are no unexpected open fds. **Signals:** after the transaction starts, a dying app or `SIGHUP`/`SIGINT` must not kill pacman half-way (T-17). `#![forbid(unsafe_code)]` limits what I can do for fds in-process: open question (section 6) | Container test with a fake pacman that dumps its env, cwd, umask, fds and signal mask; helper started with hostile env (`LD_PRELOAD`, `PATH=/tmp/evil`, `IFS`, `PACMAN_*`, inherited fd 5). Read-only side, *exists*: `command_is_absolute_pacman_with_arg_list_and_c_locale` | read-only: done; helper: planned |

### 4.4 Races, locks, concurrency

| ID | STRIDE | Threat | Defence | Proof | Status |
|---|---|---|---|---|---|
| T-09 | T | **TOCTOU between review and run.** The user confirms list L; before the helper acts, the sync databases or installed packages change, so the real change is L'. | The confirm request carries the exact list L the user saw. The helper redoes the dry-run (`pacman -S --print ...`) itself and refuses if the result differs from L. Residual window: between the helper's re-check and pacman's own resolution (pacman re-resolves). We cannot close it by taking `db.lck` ourselves (T-10), so it is accepted and small. Targets are passed explicitly; nothing is expanded by glob | Container test: change the sync db (second repo state) between "review" and "run"; helper must refuse. Container test: list differs by one package -> refuse. **Protocol side (5.2), *exists*:** every install and update request carries the confirmed list (`transaction`: repo, name, version per line) and `validate` requires every target and chosen provider to be in it from the same repository (`targets_and_choices_must_be_in_the_confirmed_list_from_the_same_repo`); the re-check is 5.3. **Side effect of a refusal: open question 12** | protocol: done (5.2); helper re-check: planned (5.3) |
| T-10 | D, T | **Lock file** `/var/lib/pacman/db.lck`: deleting it while another pacman runs corrupts the database | Golden rule 9: never delete, create, or "fix" the lock. If pacman reports it, the helper returns the coded error `DatabaseLocked` and the app explains "another package manager is running, try again". Never a retry loop that sleeps forever | Container test: hold a real lock, run the helper, assert coded error and that the file's inode and mtime are unchanged. Test that the source contains no removal of `db.lck`. Protocol side (5.2): the coded error `DatabaseLocked` and its fixed text exist (`a_locked_database_says_to_wait_and_try_again`) | protocol: done (5.2); helper: planned |
| T-11 | T, E | **Partial upgrade** (rule 4): `-Sy` then `-S`, or installing against a stale db | The helper has only two sync templates: `-S <names>` (against the current db, no `-y`) and `-Syu [<names>]`. No `-Sy` or `-y` alone exists in the protocol. The app decides which one with `install_decision`; the helper re-checks pending updates and refuses `-S` when the system is behind (T-09 style) | App side, *exists*: `nothing_pending_allows_the_install`, `one_pending_update_means_the_system_must_be_updated`, `only_ignored_updates_still_allow_the_install_and_are_counted`, `the_real_sample_blocks_with_eight_updates`. Helper: unit test "no argv contains `-y` except in `-Syu`"; container test: stale db + `-S` request is refused. Protocol side (5.2): the only actions are install (`-S`), update system (`-Syu`) and both (`-Syu <names>`); no refresh field exists (`flags_paths_and_urls_have_no_field_to_live_in`: `refresh`, `sysupgrade`); the coded error `SystemOutOfDate` exists | app: done; protocol: done (5.2); helper: planned |
| T-12 | D, T | **Two helpers at once** (two apps, a double-click, an attacker, or pamac/yay in a terminal) | pacman's own `db.lck` serialises writers; the second helper gets `DatabaseLocked` (T-10) and does not retry silently. Dry-runs are read-only and may overlap. The helper creates **no lock file of its own**, so there is nothing to go stale. Cost: a second prompt for the second request | Container test: start two helpers on the real pacman; exactly one runs, the other returns `DatabaseLocked`, db consistent afterwards | planned |

### 4.5 What runs as root underneath

| ID | STRIDE | Threat | Defence | Proof | Status |
|---|---|---|---|---|---|
| T-13 | E | **pacman hooks and install scripts** run arbitrary commands as root (`Exec =` in `.hook` files from `/usr/share/libalpm/hooks` and `/etc/pacman.d/hooks` [8]; scriptlets inside packages) | Same trust as running `pacman -S` in a terminal: packages are signature-checked by pacman. We never pass `--hookdir` (T-04) and never skip scripts (`--noscriptlet` is refused, as it breaks packages). Hook directories and `pacman.conf` are root-owned system config (section 5). The hooks run under the helper's cleared environment and `chdir /` | Container test: install a package that ships a hook which dumps its env/cwd; assert both clean. Honest limit: a malicious package from the official repos is **out of scope** (section 5) | accepted risk + planned test |
| T-14 | R | **Repudiation:** "what did root do, and on whose request?" | pacman already writes `/var/log/pacman.log` (default `LogFile` [8]). The helper additionally logs to the **journal** one line per request: time, action, validated targets, `PKEXEC_UID` [1] (informational), outcome. It logs validated values only, with newlines and control characters stripped (no log injection), never the raw JSON | Container test with a fake journal sink: one line per request, with the right fields, none for rejected-before-parse input beyond a fixed "rejected" line; injection test with `\n` in a name | planned. How to write to the journal without a new dependency: open question (section 6) |
| T-15 | I | **Error messages leaking paths or content** the user couldn't read (the helper is root) | The helper takes no path, so errors can't be steered to files of the attacker's choosing. Results are coded errors with fixed plain-language texts; pacman's raw stderr is returned only for failures of allow-listed actions, capped in length and stripped of control characters, and the app shows it only under "Technical details" | Unit test: every error variant has a fixed text; test with a stderr full of control characters and 1 MB. **Protocol side (5.2), *exists*:** `every_error_has_a_fixed_plain_text`, `the_text_does_not_depend_on_what_the_request_or_pacman_said`, `an_error_line_holds_no_path_or_text_from_the_request`, `no_refusal_repeats_what_was_sent`; the only free text is `Details`: `details_lose_control_characters_and_invisible_marks`, `details_are_cut_at_the_cap_on_a_character_boundary`, `a_megabyte_of_details_is_cut_not_copied`, `details_read_back_from_a_line_are_cleaned_again`. A new line is kept in `Details` (readability under Technical details); every other control character is removed | protocol: done (5.2); helper: planned |
| T-18 | E | **Protected packages** (rule 8): `base`, kernels, bootloader, desktop session, audio, network, `pacman`, `systemd` removed through the helper | Refused **in the helper** (a protected-set check on removal requests), not only in the UI. Mechanism designed in M3; the removal action itself arrives in M5 | Unit tests in the helper per protected family; container test removing `pacman` is refused | planned (M5 action) |
| T-19 | S, E | **Polkit action defined too loosely** (`allow_any=yes`, `_keep`, `exec.allow_gui`) | Policy: `allow_any=no`, `allow_inactive=no`, `allow_active=auth_admin`; `exec.path` = absolute helper path; no `exec.allow_gui` [1][2]. On Arch the default admin rule makes members of `wheel` administrators [10][11]; we don't change that | **WBS 5.5**: a test that parses the `.policy` XML and asserts these values; `pkaction --verbose --action-id ...` in the container | planned |

### 4.6 Denial of service

| ID | STRIDE | Threat | Defence | Proof | Status |
|---|---|---|---|---|---|
| T-16 | D, S | **Prompt spam:** a process fires `pkexec` in a loop, burying the user in password dialogs; fatigue makes a wrong approval likelier (feeds T-02) | The prompt appears *before* the helper runs, so a pkexec-launched helper cannot rate-limit it. What we do: `auth_admin` every time, and the dialog is the user's to cancel. A D-Bus service could rate-limit before asking polkit: a point for D-Bus. How polkit/agents queue concurrent requests: I don't know | Manual test: loop `pkexec` in a terminal and observe the agent; record in progress.md | open; depends on section 3 |
| T-17 | D | **Long or hanging runs:** a stuck download holds `db.lck` for hours; killing pacman half-way can corrupt the db | No timeout that kills pacman during the install phase (an interrupted transaction is worse than a slow one). Timeouts only on dry-runs and reads. Output captured with a size cap. The UI shows progress and says "this can't be stopped safely once started". Helper ignores `SIGHUP` when the app disappears and lets pacman finish | Container test: kill the app/caller mid-run -> pacman still finishes, db consistent; test with a fake pacman that prints 1 GB | planned |

### 4.7 Later actions and the read-only side

| ID | STRIDE | Threat | Defence | Proof | Status |
|---|---|---|---|---|---|
| T-20 | T, E | **Symlinks and paths in cleaner actions** (M6): a user-writable cache directory replaced by a symlink to `/etc` | Not in M3: the helper takes no paths. When M6 adds cleaning, it works on fixed directories, refuses symlinks, never follows them, and user files go to Trash (rule 7) | Privilege test "refuses symlinked cache paths" (ENGINEERING.md §2) in M6 | later (M6) |
| T-21 | T, D | **Malicious or malformed pacman output** reaches the app's parsers: control characters, invalid UTF-8, huge descriptions, wrong record counts, a record for a package we didn't ask for (the descriptions come from repository maintainers) | Parsers are total: they return `ParseError`, never panic; names validated by the same allow-list (T-06); records not asked for, duplicated, or in the wrong repo are refused; non-UTF-8 becomes a replacement character in text but is rejected in names. Fuzz targets `qi qq ss si qu` | *exists*: `invalid_utf8_in_a_name_is_still_rejected`, `invalid_utf8_becomes_replacement_char`, `a_record_that_was_not_asked_for_is_refused`, `a_record_printed_twice_is_refused`, `the_right_name_in_the_wrong_repo_is_refused`, `unparseable_text_is_wrapped_parse_error`. Fuzz: 10-minute runs clean for qi, qq, ss, si (progress.md 3.6, 4.3, 4.4); `qu` target exists (4.5): see progress.md for its run | done (see note on `qu`) |

## 5. Out of scope (stated plainly)

- **AUR** (rule 12; out of v1.0). Building and installing AUR packages runs unreviewed code; it needs its own model.
- **A malicious mirror, beyond pacman's signature checks.** We rely on pacman verifying signatures with the keyring;
  we add nothing and do not claim more. A compromised signing key or a malicious package inside the official
  repositories is outside what Tidewise can defend (it would equally hit a terminal `pacman -S`).
- **An attacker who already has root.** They can do everything the helper can, without it.
- **A fully compromised user session** (keylogger, window overlay, fake password dialog, a debugger attached to the
  app). What Tidewise limits is what a merely *running* hostile process gets (T-01, T-02), not what a fully
  controlling one gets.
- **Root-owned system configuration and binaries:** `/etc/pacman.conf` (including `XferCommand`, `Include`),
  `/etc/pacman.d/hooks`, `/usr/bin/pacman`, polkit's own rules. If those are writable by an attacker, root is lost
  already.
- **Kernel, hardware, firmware, and the Arch build infrastructure.**
- **Other distros**, and Flatpak/Snap (not in v1.0).
- **Privacy/telemetry:** none exists (rule 11); not a threat modelled here.

## 6. Open questions and "I don't know"

1. ~~**Decision: pkexec vs D-Bus** (section 3).~~ **Answered 13 Nov 2026:** pkexec + checked summary, D-Bus as
   fallback (`docs/decisions/0005-helper-invocation.md`).
2. **How agents render `$(command_line)`** or `polkit.message`. KDE (stock `exec` action): full argv in the dialog
   title, tested by the human 13 Nov 2026 [14]. **Still open:** our own action (re-test in 5.4), GNOME, long summaries.
3. **What PackageKit's dialog shows.** The searches did not settle it. A KDE bug report mentions PackageKit's own
   polkit actions (e.g. `org.freedesktop.packagekit.package-remove`) [12], nothing about dialog contents.
4. **Fixed versions of polkit** for CVE-2021-4034 and CVE-2021-3560 on Arch: I didn't verify. The machine I looked at
   has polkit 128-1.
5. ~~**Running pacman non-interactively without `--noconfirm`.**~~ **Answered 13 Nov 2026 (human, option A):** golden
   rule 10 now allows `--noconfirm` only for a transaction the helper has verified: providers named explicitly, a
   `--print` dry run equal to the confirmed list, no removals. Never `--overwrite` or `-d`/`-dd` with user-chosen
   input. The helper tests for this (T-04, T-09) are written against that wording.
6. **Closing inherited file descriptors under `#![forbid(unsafe_code)]`**: Rust's standard library doesn't expose a
   safe "close all fds" call. Options: check `/proc/self/fd` and refuse to run if unexpected fds exist (no new
   dependency), or a small safe-wrapper crate (needs your OK as a dependency).
7. **Writing to the journal** without adding a dependency (write to stderr and let systemd capture it? pkexec's
   stderr goes to the caller, not the journal, so probably not), vs `libsystemd`/a journald crate (dependency, your
   OK). Unresolved.
8. ~~**serde_json's nesting limit** is believed to be 128.~~ **Answered 14 Nov 2026 (5.2): the belief was wrong.** Measured with serde_json 1.0.154: a deep value under an unknown key is skipped in a loop, with no stack use and no limit (about 150 ms at the 1 million levels a 2 MiB request can hold); the typed parse never recurses beyond the request's own shape. What protects us is the byte cap (2 MiB, before parsing) plus the shape, not a depth limit. Pinned by `serde_json_does_not_stop_at_128_levels_but_skips_deep_values_without_recursion`. The T-07 text above that says "bounded nesting" means this.
9. **Whether `$(key)` substitution works in the policy file's `<message>`** for keys supplied in `details`.
10. **WBS IDs** for the helper tasks (protocol, validation, polkit policy, logging): I haven't read the sheet; the
    "Proof" column uses names, and only 5.5 and 5.7 are cited by ID because you gave them.
11. **Chosen providers become "explicitly installed" (requirement for 5.3, recorded 14 Nov 2026).** When the user picks a
    provider (`java-runtime` -> `jre-openjdk`) and the helper passes it to `pacman -S` as a target, pacman records it
    as explicitly installed, so it would later look like something the user chose and never show as an orphan. The
    helper must follow `pacman -S` with `pacman -D --asdeps <chosen providers>` **in the same helper run** (one
    password prompt, golden rule 6), and a container test must show `pacman -Qi` printing "Install Reason :
    Installed as a dependency" for them (and "Explicitly installed" for the real targets). `-D --asdeps` is a new
    pacman invocation in the helper's allow-list: needs the human's explicit OK in 5.3 (golden rule 2), and a new row
    in the T-04 template list. The protocol already carries what it needs (`providers`, separate from `targets`).
12. **A refused update leaves the real lists refreshed (recorded 14 Nov 2026, for M4).** The helper's dry run for
    `-Syu` (`pacman -Syu --print`) first syncs the **real** package lists. If the helper then refuses with
    `TransactionChanged` (T-09), nothing was installed, but the lists are now newer than the installed packages
    ("lists newer than packages": partial-upgrade-prone if the user then installs something with a plain `-S`).
    Today's mitigation is wording: the fixed text of `TransactionChanged` tells the user nothing was changed and to
    finish with a full system update (`a_changed_transaction_tells_the_user_to_finish_with_a_full_update`). It is
    tied to the **private-copy decision**: running the dry run against a private copy of the sync databases would
    leave the real lists alone until the user confirms. **Not decided; I could not find that decision written down
    in the repo, so this entry states only my understanding of it. The human should correct it and the M4 plan
    should settle it before the update flow is built.** Also decides whether `SystemOutOfDate` (T-11) can ever be
    shown after a refused update.

## 7. Sources

1. `pkexec(1)`, polkit. Read from the installed man page on this machine (polkit 128-1) and
   <https://polkit.pages.freedesktop.org/polkit/pkexec.1.html>. Used for: dialog shows the program's full path;
   variables `program`, `command_line` ("do not use this for any security checks, it is not secure"), `user*`;
   minimal environment; `PKEXEC_UID`; "pkexec does no validation of the ARGUMENTS"; retained authorizations are a
   hazard; `exec.path` and `exec.allow_gui` annotations; exit codes 126/127.
2. `polkit(8)`, <https://polkit.pages.freedesktop.org/polkit/polkit.8.html>. Used for: `allow_any`, `allow_inactive`,
   `allow_active`, `auth_admin_keep` ("kept for a brief period"), the `<message>` element.
3. polkit D-Bus API, `org.freedesktop.PolicyKit1.Authority`,
   <https://polkit.pages.freedesktop.org/polkit/eggdbus-interface-org.freedesktop.PolicyKit1.Authority.html>.
   Used for: `CheckAuthorization(subject, action_id, details, flags, cancellation_id)`; `details` dictionary;
   `polkit.message`, `polkit.gettext_domain`, `polkit.icon_name`; `$(key)` expansion; `polkit.` keys only from a
   sufficiently privileged caller.
4. Qualys, "pwnkit: Local Privilege Escalation in polkit's pkexec (CVE-2021-4034)", 25 Jan 2022,
   <https://www.qualys.com/2022/01/25/cve-2021-4034/pwnkit.txt>. Used for: out-of-bounds write via argument/environment
   handling, vulnerable since pkexec's creation in May 2009, any unprivileged user to root, mitigation `chmod 0755`.
5. GitHub Security Lab, GHSL-2021-074 / CVE-2021-3560, <https://securitylab.github.com/advisories/GHSL-2021-074-polkit>
   and <https://github.blog/security/vulnerability-research/privilege-escalation-polkit-root-on-linux-with-bug/>.
   Used for: authentication bypass for an unprivileged D-Bus caller via `polkit_system_bus_name_get_creds_sync`.
   Affected-version statements differ between secondary sources; not repeated here as fact.
6. CVE-2011-1485 (polkit 0.96 pkexec race), <https://ubuntu.com/security/CVE-2011-1485>. The `/proc` detail of the
   race is my inference from a patch title ("stat race"); not verified.
7. `pacman(8)`, pacman 7.1.0 / libalpm 16.0.1, the installed man page on this machine. Used for the option list in T-04.
8. `pacman.conf(5)` and `alpm-hooks(5)`, same installation. Used for `HookDir` (default `/etc/pacman.d/hooks`), the
   system hook directory `/usr/share/libalpm/hooks`, `LogFile` (`/var/log/pacman.log` via `pacman-conf LogFile`),
   `XferCommand`, hook `Exec =`.
9. namcap, Arch Wiki / bug tracker: `insecure-rpath` check, <https://bugs.archlinux.org/task/14049>; manual
   <https://man.archlinux.org/man/namcap.1>; an `insecure-runpath` rule was added later (arch-projects patch). I did
   not run namcap yet; to be done in 5.5.
10. Arch Wiki, Polkit, <https://wiki.archlinux.org/title/Polkit>. Only a fragment reached me (wheel members are
    administrators by default); the local file in 11 is the primary evidence.
11. `/usr/share/polkit-1/rules.d/50-default.rules` on this machine: `polkit.addAdminRule(... ["unix-group:wheel"])`.
12. KDE bug 442383, <https://bugs.kde.org/show_bug.cgi?id=442383>. Weak source; only for the existence of PackageKit
    polkit actions.
13. In-repo evidence: `crates/pacman/src/run.rs` (absolute `/usr/bin/pacman`, `env_clear`, `LC_ALL=C`, null stdin,
    `--` before terms), `crates/pacman/src/partial.rs`, `crates/names/src/lib.rs` (moved from `crates/pacman/src/name.rs` in 5.2), `fuzz/`, `docs/progress.md`.
14. The human's own test, 13 Nov 2026, KDE Plasma authentication agent: `pkexec /usr/bin/true install dbeaver
    jre-openjdk`, cancelled at the dialog; the dialog title showed the full argv. First-hand report, not reproduced
    by me; stock `org.freedesktop.policykit.exec` action only. Recorded in `docs/decisions/0005-helper-invocation.md`.
15. In-repo evidence for the 5.2 protocol rows: `crates/names`, `crates/helper-protocol` (`src/{request,validate,parse,
    summary,event}.rs`, `tests/hostile_input.rs`), `tests/fixtures/helper-protocol/`, `fuzz/fuzz_targets/request.rs`.
    The serde_json depth measurement of open question 8 was done with a throwaway probe (deleted); its result is
    pinned in a test.
