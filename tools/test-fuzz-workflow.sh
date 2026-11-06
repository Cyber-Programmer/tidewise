#!/usr/bin/env bash
# Checks the weekly fuzz smoke set-up without GitHub. Exit 0 = all good.
set -u
cd "$(dirname "$0")/.."
fail=0
ok()  { echo "ok   $1"; }
bad() { echo "FAIL $1"; fail=1; }
WF=.github/workflows/fuzz.yml

[ -x tools/fuzz-smoke.sh ] && ok "tools/fuzz-smoke.sh exists and is executable" || bad "tools/fuzz-smoke.sh exists and is executable"

want=$(ls fuzz/fuzz_targets/*.rs 2>/dev/null | xargs -n1 basename | sed 's/\.rs$//' | sort)
got=$(tools/fuzz-smoke.sh --list 2>/dev/null | sort)
[ -n "$want" ] && [ "$want" = "$got" ] && ok "--list equals the files in fuzz/fuzz_targets ($(echo "$want" | wc -l) targets)" || bad "--list equals the files in fuzz/fuzz_targets"

[ -f "$WF" ] || { bad "$WF exists"; exit 1; }
ok "$WF exists"

python3 -I - "$WF" >/dev/null 2>&1 <<'PY' && ok "triggers: schedule and workflow_dispatch only; read-only token; nightly; timeouts" || bad "triggers: schedule and workflow_dispatch only; read-only token; nightly; timeouts"
import sys, yaml
wf = yaml.safe_load(open(sys.argv[1]))
on = wf.get("on", wf.get(True))
assert set(on) == {"schedule", "workflow_dispatch"}, on
assert wf["permissions"] == {"contents": "read"}
for job in wf["jobs"].values():
    assert "timeout-minutes" in job
    assert any(str(s.get("with", {}).get("toolchain")) == "nightly" for s in job["steps"])
PY

python3 -I - "$WF" >/dev/null 2>&1 <<'PY' && ok "every action is pinned to a full commit SHA" || bad "every action is pinned to a full commit SHA"
import re, sys, yaml
wf = yaml.safe_load(open(sys.argv[1]))
for job in wf["jobs"].values():
    for s in job["steps"]:
        if "uses" in s:
            assert re.fullmatch(r"[\w./-]+@[0-9a-f]{40}", s["uses"]), s["uses"]
PY

grep -q 'tools/fuzz-smoke.sh 60' "$WF" && ok "runs the script with 60 s per target" || bad "runs the script with 60 s per target"
typed=""
for t in $want; do grep -q "\b$t\b" "$WF" && typed="$typed $t"; done
[ -z "$typed" ] && ok "no target name is typed in the workflow" || bad "target names typed in the workflow:$typed"
grep -q 'if: failure()' "$WF" && grep -q 'fuzz/artifacts' "$WF" && ok "crash inputs are printed when a target fails" || bad "crash inputs are printed when a target fails"
exit $fail
