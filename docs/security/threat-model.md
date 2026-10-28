# Threat model

Status: initial version, run 04 (2026-10-28). Updated in every milestone that adds a privileged action, a
file write, a parser of outside data or a network call (ENGINEERING.md §2). Method: STRIDE per trust boundary.

## Assets

| Asset | Why it matters |
| --- | --- |
| The user's own files (Documents, Downloads, …) | Irreplaceable; golden rule 4 — never deleted, only moved to Trash |
| The installed system (packages, desktop session, audio, network, journal) | Breaking it strands a beginner; golden rule 7 |
| Root privilege | What the helper holds; the biggest prize for an attacker |
| Package-manager state and locks | Corrupting it breaks updates |
| The user's privacy (what is installed, what is large, file names) | Never leaves the machine; golden rule 6 |
| The release artefact (AppImage) | What users run; its integrity is the supply chain |

## Actors

- **Local unprivileged process or user** (malware running as the same user, or another user on the machine).
- **Malicious file content**: crafted package metadata, `.desktop` files, `/proc` text, file names, symlinks.
- **Network attacker** on the path to GitHub (only the update check talks to the network).
- **Malicious or compromised dependency / build input.**
- The user themself, by mistake (a confirm screen is a control against this actor too).

## Trust boundaries and data flows

```
 [ user ]──clicks──▶ [ app (user) ] ──read-only──▶ /proc, /sys, package DBs, home folder
                           │  │
                           │  └─ one JSON request ─▶ pkexec ─▶ [ tidewise-helper (root) ] ─▶ package manager, cache dirs
                           │                           (B2: polkit)              (allow-list only; M3)
                           └─ HTTPS GET, once a day ─▶ GitHub Releases API (B3; toggle in Settings)
 other programs' output (pacman, apt, flatpak, snap, dnf) ─▶ parsers in app/adapters (B4)
 files on disk (.desktop, os-release, /proc text) ─▶ parsers (B4)
 Trash (freedesktop spec, in the user's home) ◀─ app moves user files here (B5)
```

| ID | Boundary | Crossing |
| --- | --- | --- |
| B1 | User ↔ app | Clicks and keys; the confirm screen is the control |
| B2 | App (user) ↔ helper (root) | One JSON request through polkit — **M3, not built yet** |
| B3 | App ↔ network | Daily update check to GitHub Releases — **not built yet** |
| B4 | Outside data ↔ parsers | Command output, `/proc`, config files, file names |
| B5 | App ↔ user's files | Scan (read-only), Trash (move) |
| B6 | Build ↔ release | CI, dependencies, AppImage |

## STRIDE per boundary

"Test" names the proof. **TODO M3** means the control is designed but the code does not exist yet.

### B1 User ↔ app

| STRIDE | Threat | Defence | Test |
| --- | --- | --- | --- |
| T | A misclick removes something wanted | Dry-run list on the confirm screen; action re-checks the list; user files only go to Trash with Undo | R4-T4 Trash round-trip tests; TODO M3 for the re-check |
| E | User is nudged into removing a protected item | Protected items get no destructive button, refused in code | TODO M3 (protected list in the helper and adapters) |
| R | "I did not remove that" | Local History screen (what was done, when) | TODO M4 |

### B2 App ↔ helper (M3)

| STRIDE | Threat | Defence | Test |
| --- | --- | --- | --- |
| S | Something other than the app starts the helper | Polkit action authenticates the user for each run; helper refuses when not started through polkit | TODO M3 container test |
| T | Request edited to add flags (`--force`, `--`) or paths (`../`, `*`) | Allow-list of actions in `helper-protocol`; typed fields, strict validation; no shell; argument list only | TODO M3 hostile-input table |
| T | Confirmed list changed between confirm and act (TOCTOU) | Helper re-checks the list right before acting | TODO M3 |
| T | Cache path swapped for a symlink to `/etc` | Helper opens with no-follow checks and refuses symlinks | TODO M3 container symlink test |
| I | Helper output leaks data | Helper returns only the result enum and a short log line | TODO M3 |
| D | Huge or deeply nested request | Size limit, depth limit, name-count limit | TODO M3 + fuzz target for the request |
| E | Environment tricks (`PATH`, `LD_PRELOAD`) | Helper clears the environment, sets `LC_ALL=C`, runs absolute paths | TODO M3 |
| E | Helper binary replaced after install (`/usr/local/libexec`) | Root-owned, mode 0755, installed once; polkit action pins the path | TODO M3 |

### B3 App ↔ network (update check)

| STRIDE | Threat | Defence | Test |
| --- | --- | --- | --- |
| S/T | Fake "new version" response | HTTPS with certificate validation; the app only shows a link, never downloads or runs anything by itself | TODO when built |
| I | Telemetry or fingerprinting | One GET, no identifiers, no other host; toggle in Settings | TODO when built (test: no other network calls — golden rule 6) |
| D | Hang or huge response | Timeout, response size cap | TODO when built |

### B4 Outside data ↔ parsers

| STRIDE | Threat | Defence | Test |
| --- | --- | --- | --- |
| T/D | Malformed or hostile text crashes or hangs a parser | Parsers return `Result`/`Option`, never `unwrap`; `forbid(unsafe_code)` in `core` and `adapters` | Fixture tests incl. broken and non-English input; `cargo fuzz` targets in `fuzz/` (docs/security/fuzzing.md) |
| E | Memory-safety bug in a parser | Safe Rust only (`forbid(unsafe_code)`); `deny(unsafe_code)` in the app | Compiler-enforced |
| T | Locale changes the output format | `LC_ALL=C` and machine-readable flags | Adapter tests with saved outputs |
| E | A file name like `-rf` or `;rm` becomes a command argument | No shell; names passed as list items after `--` where the tool supports it | TODO M3 |
| I | `/proc` text of other users' processes read | Only what the kernel already allows the user to read | n/a |

### B5 App ↔ user's files

| STRIDE | Threat | Defence | Test |
| --- | --- | --- | --- |
| T | Scanner follows a symlink out of the folder | `symlink_metadata`; symlinks count as themselves; other filesystems skipped | `core::scan` tests |
| T | "Trash" silently deletes (other filesystem) | Refuse with a clear error when Trash is not on the same mount and cannot be used; never fall back to delete | R4-T4 tests |
| T | Trashing a symlink removes its target | The link itself is moved | R4-T4 test |
| I | Scan results leave the machine | No network code in `core`; golden rule 6 | Review; TODO a test that `core` has no network dependency |
| D | Scan of a huge tree stalls the UI | Background thread, progress, cancel | `core::scan` cancel test |

### B6 Build ↔ release

| STRIDE | Threat | Defence | Test |
| --- | --- | --- | --- |
| T | Compromised dependency | `cargo audit`, `cargo deny` on every CI run; ask before adding dependencies | CI `supply-chain` job |
| T | Compromised GitHub Action | Actions pinned to full commit SHAs; `permissions: contents: read` | R4-T0 workflow; checked by eye / Scorecard |
| T | Tampered release | SHA-256 file next to the AppImage; signing later (human holds the key) | TODO signing |
| E | CI token abuse | Least-privilege workflow tokens; release workflow gets `contents: write` only for the release job | `release.yml` |

## Open items

1. M3 builds the helper: every `TODO M3` row becomes code and a test; an adversarial review by a fresh
   subagent goes to `docs/security/reviews/`.
2. Fuzzing is run by hand (fuzzing.md); running it in CI is a follow-up.
3. Human pen-test checklist (`docs/security/pentest-checklist.md`) is written before v1.0.
