# 0009 — Update system: what the review shows, and what blocks it

- Status: **Accepted** (26 Nov 2026), amended 27 Nov 2026 after the review
  (`docs/security/reviews/6.5-update-system.md`). **Not safe to release:** the second review found that R-01 is only
  partly closed (S-01, S-02, open). R-02 is fixed except two smaller gaps (S-03, S-04).
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

## Nothing the dry run leaves out (review R-01, 27 Nov 2026)

`--ask 66` (or any other flag) does not change what `-Su --print` lists, measured. So the preview cannot be made equal
to the real run that way. Instead: the helper runs `-Qu --dbpath /run/tidewise-helper-lists/` (approved) and refuses
when a non-`[ignored]` upgrade it names is not on the confirmed list; the app blocks the same case from its own `-Qu`
before the password. Such an update has to be done in a terminal.

**This is not enough (second review).** A declined replacement can still install packages nobody listed when an upgrade
needs a name the replacer provides and a package is literally called that (S-01), and `-Qu` shows nothing when an
earlier repository's `Usage` lacks `Upgrade` (S-02). It can also refuse an ordinary update when only a new version
joins an `IgnoreGroup` (S-06). Proposed, not built (a new command): a second private dry run with
`--ignore <the replacers>`, whose every line must be on the confirmed list; it predicted the real run in the measured
cases and could replace the `-Qu` comparison.

## Shared libraries (review R-02, 27 Nov 2026)

A soname need counts only providers with the identical version. A real choice (several, and none installed that stays
and satisfies `-T`) refuses an update and blocks an install; Tidewise never asks about a soname. In the repositories of
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
3. **Replacements are skipped,** with "<new> replaces <old>. Tidewise doesn't do replacements yet; run sudo pacman
   -Syu in a terminal to accept it." The replacer, and what only it needs, are not counted. The helper is still sent
   the whole dry run, so the password window can count more than the button; the review says so.
4. **Nothing but replacements:** no Update button; "Your system is up to date" with the replacement line (A1).
5. **A held package that an upgrade needs** blocks the update with its own card, naming both and the terminal way
   (B1). **A conflict** with an installed package blocks it before the password.
6. **Restart line** after a successful update that installed an official kernel, `systemd`, `mesa` or `nvidia*`.

## Limits (accepted for now)

- Fails closed (sent to a terminal) when what satisfies a name is in no repository (an AUR or local package), and for
  every versioned need that is not met by a lone provider.
- A **versioned** provider count: all providers of the name count, also ones whose version could not fit.
- From reading, not reproduced: an installed provider held by `IgnorePkg` is judged by its repository version's
  `Provides`, and `-T` may be satisfied by a different package.
- A repository whose `Usage` lacks `Search` is not seen by `-Ss`: a provider there is not counted (R-06, accepted).
- Conflicts and replacements with a version rule: versioned conflicts are not judged (pacman aborts the real run);
  a replacement is matched by name only.
- `-Si` of a very large update can pass the helper's 4 MiB output cap (about 5000 packages); the update is refused.
