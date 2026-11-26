# 0009 — Update system: what the review shows, and what blocks it

- Status: **Accepted** (26 Nov 2026). The app and helper logic are built; the window is not (WBS 6.5 step 6).
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

- The pick rule is the install's (`provider_picks`): any new package satisfying a listed dependency through
  `Provides` counts, also when it is the only provider, or when an installed package already provides the name. The
  second is from reading, not measured. It fails closed: the user is sent to a terminal.
- Conflicts and replacements with a version rule: versioned conflicts are not judged (pacman aborts the real run);
  a replacement is matched by name only.
- `-Si` of a very large update can pass the helper's 4 MiB output cap (about 5000 packages); the update is refused.
