# 0005 — How the app starts the helper: pkexec with a checked summary, or a D-Bus service

- Status: **Accepted** (option B, pkexec + human-readable summary in argv; D-Bus kept as the fallback)
- Date: 2026-11-13
- Decided by: Bahauddin Hassan (human), 2026-11-13
- Task: WBS 5.1 (threat model, section 3 and threat T-02); recorded while starting 5.7.
- Updated: 2026-11-16, WBS 5.4: the re-test with our own policy action passed on KDE (section "5.4 re-test"). The decision stands.
- Updated: 2026-11-16, WBS 5.4b: the helper now refuses unless `argv[0]` is exactly the installed path (caveat below).

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

### 5.4 re-test (15 Nov 2026, the human, KDE Plasma's polkit agent)

Our policy action, `packaging/polkit/io.github.tidewise.helper.policy`, has the message
"Tidewise needs an administrator password to make this change: $(command_line)". Its program path is the helper's, so
the test used a TEST action with the same defaults and message and `exec.path` `/usr/bin/true`, installed by hand and
removed again. A custom message **replaces** pkexec's text (pkexec sets `polkit.message` only for the stock action),
and shows argv only because the message says `$(command_line)` (polkit 128, source in threat model 25).

- `pkexec /usr/bin/true "install extra/dbeaver (2 packages)"`: the title read "Tidewise needs an administrator
  password to make this change: /usr/bin/true install extra/dbeaver (2 packages)", **whole**.
- A 192-byte summary: **whole**, wrapped over four lines, ending "(40 packages)", nothing cut. (The stock action's
  text uses `cmdline_short`, which pkexec cuts to 80 characters; ours does not.)
- Details pane: the action description, the action id, the vendor. `pkaction`: `implicit active: auth_admin` and the
  `exec.path` annotation. The quotes around the argument are not shown.
- Cancelled each time. Not tried: a second prompt after authenticating (no `_keep`), GNOME's agent, a summary of the
  full 200 bytes, non-ASCII or hostile text, this action at its real path (5.5).

## Caveats

- ~~**Re-test in 5.4 with our own polkit action.**~~ **Done 15 Nov 2026 (above): the summary is shown whole.** Still
  open: GNOME's agent, and the real action at its real path (5.5).
- **argv is chosen by the caller and polkit does not sanitise it** (`command_line` is not escaped or cut). Any process of
  the user can show a Tidewise-branded prompt with text of its own after the fixed sentence; the helper then refuses
  (the summary does not match), but the user has been asked for a password. This is the spoofing and fatigue risk of
  T-02 and T-16, not a privilege gain. Whether the agents render markup, and what a very long or multi-line argv does,
  is untested (threat model, open question 21).
- **argv[0] is part of `command_line` and the caller can dress it up** with a symlink to the helper: pkexec gives the
  program `argv[0]` as typed (only a name found through `PATH` becomes absolute; measured in the test container and
  from polkit 128's source). **Built in 5.4b (the human's decision, 16 Nov 2026):** the helper starts only when
  `argv[0]` is exactly `/usr/lib/tidewise/tidewise-helper`, byte for byte, so the whole dialog of an accepted request
  is fixed-sentence + fixed-path + recomputed-summary. It runs after authentication, so it cannot stop a dressed-up
  prompt (`argv[1]`, question 21), only a real transaction under one; and `argv[0]` is no identity check.
- argv is visible to every user in `ps`. The summary must contain nothing secret (package names only).
- The summary shown can be long (many packages); the summary function must have a length rule decided with the
  protocol (for example "install dbeaver and 11 more"), tested against the dialog's truncation.

## Consequences

- **Fallback: option C.** If the 5.4 re-test shows the summary is hidden or truncated, or if prompt spam (T-16)
  proves a problem we can only fix before polkit is asked, switch to a D-Bus service. That would change the wording of
  golden rule 1 ("started with `pkexec`") and add a dependency that needs its own approval.
- Threat T-02's proof becomes: the unit test "argv summary ≠ summary derived from JSON → refused" plus the 5.4 dialog
  check with our action (done on KDE, see above).
- The policy must keep `$(command_line)` in its message and must not use `cmdline_short`; `polkit_policy.rs` checks it.
- The installed path is fixed in three places that a test ties together: the policy's `exec.path`, the helper's
  `HELPER_PATH`, and (5.5) the package. **M4's launcher must call pkexec with exactly
  `/usr/lib/tidewise/tidewise-helper`**; a `/lib/...` or `/usr/local/...` spelling is refused by design, and a helper
  run from `target/` has no pkexec route (the container tests pass the name with `arg0`).
- Threat model §3 is updated to say "decided, see this record".
