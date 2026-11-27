# 0009 — Update system: what the review shows, and what blocks it

- Status: **Accepted** (26 Nov 2026), amended 27 Nov 2026 twice after the reviews
  (`docs/security/reviews/6.5-update-system.md`): **replacements are blocked, not skipped.**
- Decided by: Bahauddin Hassan (human), 26 Nov 2026: Arch news (a); rule 10 (a); skip replacements, block conflicts;
  then A1 and B1 after the measurements
- Task: WBS 6.5 (PLAN D9; golden rules 4, 5, 10, 11, 13; threat model 41)

## Measured (`tests/container/tests/pacman_update.rs`, pacman 7.1.0)

| Case | `-Qu` | `-Su --print` | `-Su --noconfirm --ask 66` |
|---|---|---|---|
| Upgrade needs a name two packages provide | the upgrade | lists the first provider, says nothing | installs it, as a dependency |
| Replacement (the old package has no newer version) | **not listed** (empty and exit 1 if it is the only change) | lists the new package | declines; old package stays |
| Replacement, and the old package HAS a newer version | the old package | lists only the new one | declines, then **upgrades the old one** and what it needs (review R-01) |
| Replacement that needs another package | not listed | lists both | installs **neither** |
| `IgnorePkg` | marked `[ignored]` | left out | skips it, warns |
| Upgrade needs a held package | both | **fails**, exit 1, `unable to satisfy dependency` | upgrades **nothing** |
| Conflict with an installed package | the upgrade | **says nothing** | aborts; nothing at all is upgraded |
| Interactive `pacman -Syu`, held-package case | | | asks `:: Do you want to skip the above package for this upgrade? [y/N]`; `y` upgrades the rest |

| Lone provider | the upgrade | lists it | asks nothing |
| An installed package provides the name; another listed package provides it too | the upgrade | lists the other one | asks nothing, keeps both |
| The installed provider's new version **drops** the name in the same update | both | lists a new provider | **asks** (`-T` still says satisfied) |
| `pacman -T -- <dep>` | | | exit 0 satisfied, 127 not (printed); respects versions |
| `-Ss --dbpath <private> -- '^name$'` | | | finds packages that only `Provides` the name |

## Rule 10 as read for UPDATES (human's decision, 26 Nov 2026)

"Providers named explicitly" means: pacman must not make a choice. An update has no way to name one, so two cases with
no choice are let through. **Installs are unchanged** and still name every pick.

1. **One possible provider** in the repositories (`-Ss '^name$'`, confirmed with `-Si`): not a pick.
2. **Satisfied by what is installed, and staying so:** every listed dependency on the name has no version, `pacman -T`
   says it is satisfied now, **and** an installed package is among the repository providers and is either not in the
   update, or upgraded to a version whose listed record still provides the name. (Review R-03, R-04: versioned needs
   never pass this way.)
   "One possible provider" means one repository and name: the same name in two repositories is a choice (R-07).
   The second half is Claude's addition: the human asked for `-T`, and `-T` alone would let the "drops the name" case
   through. It uses only the approved reads.

## No update with a replacement (human's decision, 27 Nov 2026; reviews R-01, S-01, S-02, S-06)

**The v0.1 rule changes from "skip" to "block".** Skipping was not safe: after `--ask 66` declines a replacement,
pacman can upgrade the replaced package and install what that or another upgrade needs, none of it in the dry run
(R-01: the old package has a newer version; S-01: an upgrade needs a name the replacer provides; S-02: the same with
nothing in `-Qu`). `--ask 66` does not change what `-Su --print` lists, and comparing `-Qu` (built, then removed) did
not cover S-01 or S-02 and refused ordinary updates under one `IgnoreGroup` case (S-06).

**The rule** (`listed_replacements`, one function for app and helper): any package in the dry run, **installed ones
too** (an installed replacer is listed again, measured), whose `Replaces` names an installed package, **its own name
included** (human's decision of 27 Nov, review T-01: a self-replacement that was declined installed unlisted packages).
A versioned entry counts only when `pacman -T` says the installed version fits (47 name-only matches on the
development machine, none fitting their version). The app shows "This update includes a replacement Tidewise can't do
yet. Run sudo pacman -Syu once in a terminal; pacman will ask you about it." before the password; the helper refuses
(`transaction_changed`, lists untouched) for both update actions. No new helper command; the helper now reads the
private `-Si` for every update. **Replacements themselves are WBS 7.5.**

**A consequence of T-01's fix, measured:** a package that replaces its own name is listed by **every** `-Su`, also
when it is up to date and also after the replacement was accepted in a terminal. While such a package is installed,
Tidewise shows the replacement card every time and never "up to date" or a review. Running `-Syu` once does not clear
it, so whenever any listed replacement replaces its own name the card says instead (human's words, 27 Nov): "{pkg}
marks itself as a replacement, so Tidewise can't update while it's installed. Update with sudo pacman -Syu in a
terminal instead." `{pkg}` is the first such package in the list; any other replaced names follow on their own line,
"Also replaced: {names}.", so nothing listed is hidden. Without a self-replacement the card keeps the general words. `extra/python-pylint-pydantic` and `extra/python-pylint-plugin-utils` replace their own
names. It errs on the safe side; WBS 7.5 (replacements) is where it can be lifted.

Consequence: while a replacement is pending, nothing can be updated in Tidewise. A replacement the user always
declines blocks every update until they accept it, remove one of the two, or ignore it in `pacman.conf`.

## Shared libraries (review R-02, 27 Nov 2026)

A soname need counts only providers with the identical version. A real choice (several, and none installed that stays
and satisfies `-T`) refuses an update (the terminal card) and blocks an install. The install card names the choice,
"<package> needs a <library> library, and more than one package can provide it. Install one of them first, then try
again:", with one "Install <candidate>" button each, a normal install of that package; no default (human, 27 Nov). In the repositories of
27 Nov 2026 only six soname entries have two providers (`zlib`/`zlib-ng-compat`, 32 and 64 bit; `jack2`/`pipewire-jack`,
two entries; mariadb; iptables). **Consequence:** on a system with neither `jack2` nor `pipewire-jack`, installing
plasma or gnome is blocked (they need `libjack.so`).

Providers are counted with one search for all names, `-Ss -- ^(a|b|c)$` (approved: private lists for updates, real
lists for installs), 100 names per search, none dropped. Measured in the test container (full core and extra lists):
one search per name took 73 s (plasma, 85 names), 88 s (gnome, 108) and 104 s (all three groups, 127); one batched
search took under 1 s and found the identical set of packages.

Two new fixed reads in the helper (approved): `-Ss --dbpath /run/tidewise-helper-lists/ -- ^name$` and `-T -- <dep>`
(against the real local database). The rule is `update_picks` in `tidewise-pacman`, used by the app and the helper.

## Decisions

1. **Arch news (rule 13 against rule 11):** for v0.1 the review shows the address as text. An opt-in fetch
   (`/usr/bin/curl`, `roxmltree`) is its own task after v0.1. **Rule 13 is only partly met in v0.1.**
2. **Rule 10:** a pick refuses the update. The app finds it on its review list (before the password); the helper checks
   again on the private copy, before the lists move (new fixed read `pacman -Si --dbpath /run/tidewise-helper-lists/
   -- repo/name ...`), and answers `provider_not_chosen`. The helper always runs `pacman -Q`; `-Si` and the pick rule
   are skipped when everything listed is installed already. (The plan said the `-Q` could be skipped too; it cannot:
   `-Q` is how that is known.) The protocol is unchanged.
3. **Replacements block** (see above; this replaces "skip", and with it A1's "nothing but replacements").
5. **A held package that an upgrade needs** blocks the update with its own card, naming both and the terminal way
   (B1). **A conflict** with an installed package blocks it before the password.
6. **Restart line** after a successful update that installed an official kernel, `systemd`, `mesa` or `nvidia*`.

## Limits (accepted for now)

- Fails closed (sent to a terminal) when what satisfies a name is in no repository (an AUR or local package), and for
  every versioned need that is not met by a lone provider.
- A **versioned** provider count: all providers of the name count, also ones whose version could not fit.
- From reading, not reproduced: an installed provider held by `IgnorePkg` is judged by its repository version's
  `Provides`, and `-T` may be satisfied by a different package.
- False replacement block (T-02): with `IgnorePkg` on the replaced package, or a replacer from a later repository,
  pacman asks nothing but Tidewise blocks. Errs on the safe side.
- A package that replaces its own name blocks every update while it is installed (T-01's fix, above).
- A repository whose `Usage` lacks `Search` is not seen by `-Ss`: a provider there is not counted (R-06, accepted).
- A soname need spelled differently from the `Provides` (`>=`, or without the release part) is not judged, so pacman
  may choose a provider unasked (S-03, accepted; the second review found no such need in the official repositories).
- False refusal: when a package names the provider before the soname, pacman asks nothing but Tidewise refuses or
  blocks (S-04, accepted; for example `mariadb-libs` before `libmariadb.so`).
- Conflicts and replacements with a version rule: versioned conflicts are not judged (pacman aborts the real run);
  a replacement is matched by name only.
- `-Si` of a very large update can pass the helper's 4 MiB output cap (about 5000 packages); the update is refused.
