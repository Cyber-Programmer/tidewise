# 0010 — Update-and-install is deferred until after v0.1

- Status: **Accepted** (28 Nov 2026)
- Decided by: Bahauddin Hassan (human)
- Task: WBS 6.6 (golden rules 4, 5, 10; threat model 40)

## Context

WBS 6.6 built "Update system and install {name}": one review, one password, one `pacman -Su {targets}`. Golden rule 10
says pacman must never answer a provider question the request does not name. `pacman --print` answers such questions
silently, so Tidewise has to work out by itself where pacman had a choice, in the app (to ask) and in the root helper
(to refuse).

The work is kept on `spike/6.6-update-and-install` (never merged). Its last commit is `4ccba31`.

## What happened: five review rounds

Each round was an adversarial review of a new version of the rule; each found at least one Medium or worse. Findings
are in `docs/security/reviews/6.6-update-and-install.md` on the spike.

| Round | Rule reviewed (spike commit) | What was found |
|---|---|---|
| 1 | The install's rule, asking what the chosen package brings with `-S --print` (`dac64f1`) | U-01: on a stale system that dry run fails ("breaks dependency"), so good requests are refused |
| 2 | What it brings read from by-name dependencies (`b9c179a`) | V-01: accepts a provider pacman chose itself, when something else already meets the name |
| 3 | The same, only where nothing else meets the name (`e27f2a7`) | W-01: shared-library needs matched by version text, pacman compares versions (also on `main`) |
| 4 | Strict rule: any unnamed provider with another repository package of that name (`ae1254f`) | X-01: refuses about 37 % of installs on a bare system with multilib |
| 5 | A rival must meet the need, judged by `vercmp` (`84515e2`) | Y-01: the app names a provider unasked; Y-02: answering `opengl-driver` with `nvidia-utils` always ends at the terminal |

## Reason

**We were re-creating pacman's dependency resolver from outside it.** pacman's choices depend on the order it resolves
in, on what is installed, on literal names before virtual ones, on version comparison and on what is already in the
transaction. Every rule that approximated this was either looser than pacman (a golden rule 10 breach) or stricter (the
feature does not work), and each fix exposed the next difference. A terminal asks these questions correctly.

## Decision

- v0.1 has the **two-step flow** that `main` already has: "Your system needs updating first", then "Update system", then
  install.
- The **helper refuses** `update_system_and_install` (`ErrorKind::ActionNotSupported`, journal code
  `action_not_supported`) before any command, whoever sends it. The protocol still knows the action.
- Threat model 40 is closed by that refusal.
- Nothing else from the spike is merged.

## Consequences

- One password more and one review more for an install on a system with updates pending.
- A plain update that needs a provider choice still goes to the terminal (decision 0009).
- The older gaps in plain installs and updates stay open (WBS 6.20), among them W-01.
- A future attempt should not start from a rule of our own (below).

## Measured on `main`, for later (28 Nov 2026, pacman 7.1.0; nothing changed)

**A question pacman would not ask (the spike's Y-02), plain installs.** `libglvnd` depends on `mesa` by name and on the
virtual `opengl-driver`, which `mesa` provides: pacman takes `mesa` and asks nothing. The app asks "mesa / mesa-amber /
nvidia-utils" on a bare system (the real resolver against a copy of this machine's lists). In the test container with
dummy packages of that shape: pacman asks nothing; the app asks; answering the other provider gives a plan with
**both**, and the real helper installs it. The user chose that package, so rule 10 holds, but it is an extra package
pacman would not have installed. **Y-01 does not occur on `main`** (it came with a filter that exists only on the
spike): the app asks, as pacman does.

**Can pacman itself check rule 10?** (test container, dummy packages, `LC_ALL=C`, stdin closed)

| Command | Prints "There are N providers available for X" | When the request names the provider |
|---|---|---|
| `-S --print`, `-Su --print` (also as root, with `--noconfirm`, `--confirm`, `--verbose`, private `--dbpath`) | **never** | - |
| `-Sw --noconfirm`, `-Suw --noconfirm` (download only) | **yes**, for virtual, versioned and shared-library needs, and for a choice below a named provider | silent |
| `-Sw` of a dependency written as a package's own name | silent (pacman has no choice) | - |

`--debug` prints "provider found" lines in `--print` mode, but not whether pacman would ask. The download-only form
also works on a private copy of the lists with its own `--cachedir`: nothing was installed and the real `sync/` folder
did not change. It downloads the packages, it takes the first provider, its output would have to be parsed, and it
would be a new helper command. Not measured: group questions, conflicts, a signed repository. **WBS 6.20 should weigh
this before writing another rule.**
