# 0005 — How the app starts the helper: pkexec with a checked summary, or a D-Bus service

- Status: **Accepted** (option B, pkexec + human-readable summary in argv; D-Bus kept as the fallback)
- Date: 2026-11-13
- Decided by: Bahauddin Hassan (human), 2026-11-13
- Task: WBS 5.1 (threat model, section 3 and threat T-02); recorded while starting 5.7.

## Context

Tidewise's helper runs as root and does one confirmed transaction. `docs/security/threat-model.md` (T-02,
"authorization confusion") names the problem: a malicious process of the same user can start a helper request at the
moment the user expects Tidewise's password prompt, and the user approves the wrong request. The helper request
travels on **stdin**, which polkit never sees, so with plain `pkexec` every request produces the same dialog. The pkexec
man page offers only `$(program)`, `$(command_line)` and `$(user…)` for the dialog text and says of `command_line`
"do not use this for any security checks, it is not secure" (threat model, source 1).

## Options

| Option | In one line |
|---|---|
| **A. pkexec, nothing in argv** | `pkexec /usr/lib/tidewise/tidewise-helper`, request on stdin. Every dialog looks the same. |
| **B. pkexec + summary in argv** | `pkexec …/tidewise-helper install dbeaver jre-openjdk`-style: a short human-readable summary in argv, shown by the agent through `$(command_line)`. After authentication the helper recomputes the summary **from the validated JSON** and refuses if the argv text differs. |
| **C. D-Bus system service + polkit** | The service passes `details` / `polkit.message` to `CheckAuthorization`, so the dialog shows the request the service validated. Costs a daemon, a D-Bus crate, a service file and a D-Bus policy file. |

## Decision

**Option B.** Reasons: no root process stays running, no new dependency, golden rules 1 and 6 hold as written, and a
lying caller gains nothing: if argv and JSON disagree the helper refuses, so the worst a malicious caller can do is
waste one approval. The helper's own checks (threat model A1: it trusts nothing about its caller) are the same in B
and C.

The helper must: build the summary text from the **validated** request with one fixed function; compare it to the argv
text byte for byte; refuse with a coded error (`SummaryMismatch`) on any difference; never use the argv text for
anything else. The summary format becomes part of the protocol (`crates/helper-protocol`). The polkit action keeps
`auth_admin` (no `_keep`) so no retained authorization can be reused.

## Evidence

Tested by the human on 13 Nov 2026 on their KDE Plasma agent: `pkexec /usr/bin/true install dbeaver jre-openjdk`
(cancelled at the dialog). The agent **showed the full argv in the dialog title**, so a summary in argv does reach the
user there. Not tested: GNOME's agent, other KDE versions, long summaries (truncation), non-ASCII text.

## Caveats

- **Re-test in 5.4 with our own polkit action.** The test above used the stock `org.freedesktop.policykit.exec`
  action. A custom action with its own `<message>` may hide argv; if our dialog does not show the summary, this
  decision is reopened.
- argv is visible to every user in `ps`. The summary must contain nothing secret (package names only).
- The summary shown can be long (many packages); the summary function must have a length rule decided with the
  protocol (for example "install dbeaver and 11 more"), tested against the dialog's truncation.

## Consequences

- **Fallback: option C.** If the 5.4 re-test shows the summary is hidden or truncated, or if prompt spam (T-16)
  proves a problem we can only fix before polkit is asked, switch to a D-Bus service. That would change the wording of
  golden rule 1 ("started with `pkexec`") and add a dependency that needs its own approval.
- Threat T-02's proof becomes: the unit test "argv summary ≠ summary derived from JSON → refused" plus the 5.4 dialog
  check with our action.
- Threat model §3 is updated to say "decided, see this record".
