# 0009 — Update system: what the review shows, and what blocks it

- Status: **Accepted** (26 Nov 2026). Built, window included; the adversarial review is still to come.
- Decided by: Bahauddin Hassan (human), 26 Nov 2026: Arch news (a); rule 10 (a); skip replacements, block conflicts;
  then A1 and B1 after the measurements
- Task: WBS 6.5 (PLAN D9; golden rules 4, 5, 10, 11, 13; threat model 41)

## Measured (`tests/container/tests/pacman_update.rs`, pacman 7.1.0)

| Case | `-Qu` | `-Su --print` | `-Su --noconfirm --ask 66` |
|---|---|---|---|
| Upgrade needs a name two packages provide | the upgrade | lists the first provider, says nothing | installs it, as a dependency |
| Replacement | **not listed** (empty and exit 1 if it is the only change) | lists the new package | declines; old package stays |
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
2. **Satisfied by what is installed, and staying so:** for every listed dependency on the name, `pacman -T` says it
   is satisfied now, **and** an installed package is among the repository providers and is either not in the update, or
   upgraded for a need without a version (its new version still provides the name, by `-Si`).
   The second half is Claude's addition: the human asked for `-T`, and `-T` alone would let the "drops the name" case
   through. It uses only the approved reads.

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

- Fails closed (sent to a terminal) when what satisfies a name is in no repository (an AUR or local package), or when
  an upgraded provider meets a versioned need (versions of the new `Provides` are not compared).
- A **versioned** provider count: all providers of the name count, also ones whose version could not fit, so a name
  with one fitting and one unfitting provider is refused.
- An installed provider that is held by `IgnorePkg` is judged by its repository version's `Provides`; `-T` still has
  to agree about the installed one.
- Conflicts and replacements with a version rule: versioned conflicts are not judged (pacman aborts the real run);
  a replacement is matched by name only.
- `-Si` of a very large update can pass the helper's 4 MiB output cap (about 5000 packages); the update is refused.
