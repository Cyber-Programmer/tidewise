#!/usr/bin/env bash
# Checks the CI set-up without GitHub: tools/ci-local.sh mirrors ci.yml, the workflow has the
# guard rails the project wants, and the dependency advisories are clean. Exit 0 = all good.
set -u
cd "$(dirname "$0")/.."
fail=0
check() { if "$@" >/dev/null 2>&1; then echo "ok   $DESC"; else echo "FAIL $DESC"; fail=1; fi; }

DESC="tools/ci-local.sh exists and is executable"; check test -x tools/ci-local.sh

# The cargo commands of ci.yml (except installs), in order, must equal those in ci-local.sh.
yml_cmds=$(python3 -I - <<'PY'
import yaml
wf = yaml.safe_load(open(".github/workflows/ci.yml"))
for job in wf["jobs"].values():
    for step in job["steps"]:
        run = step.get("run", "")
        if run.startswith("cargo ") and not run.startswith("cargo install"):
            print(run)
PY
)
local_cmds=$(grep -E '^run "cargo ' tools/ci-local.sh 2>/dev/null | sed -E 's/^run "(.*)"$/\1/')
DESC="ci-local.sh runs the same cargo commands as ci.yml, in order"
[ -n "$yml_cmds" ] && [ "$yml_cmds" = "$local_cmds" ] && echo "ok   $DESC" || { echo "FAIL $DESC"; fail=1; }

DESC="ci.yml has concurrency, a timeout on every job, a release-build job, no unlocked cargo"
python3 -I - >/dev/null 2>&1 <<'PY' && echo "ok   $DESC" || { echo "FAIL $DESC"; fail=1; }
import yaml, sys
wf = yaml.safe_load(open(".github/workflows/ci.yml"))
assert "concurrency" in wf and wf["concurrency"].get("cancel-in-progress")
assert "release-build" in wf["jobs"]
for name, job in wf["jobs"].items():
    assert "timeout-minutes" in job, name
    for s in job["steps"]:
        r = s.get("run", "")
        if r.startswith(("cargo clippy", "cargo test", "cargo build")):
            assert "--locked" in r, r
PY

DESC="ci.yml and ci-local.sh run tools/check-secrets.sh"
grep -q 'run: tools/check-secrets.sh' .github/workflows/ci.yml && grep -q '^run "tools/check-secrets.sh"' tools/ci-local.sh && echo "ok   $DESC" || { echo "FAIL $DESC"; fail=1; }
DESC="ci.yml runs tools/test-check-secrets.sh"
grep -q 'run: tools/test-check-secrets.sh' .github/workflows/ci.yml && echo "ok   $DESC" || { echo "FAIL $DESC"; fail=1; }

DESC="release.yml parses"; check python3 -I -c 'import yaml;yaml.safe_load(open(".github/workflows/release.yml"))'
DESC="dependabot covers cargo and github-actions weekly"
python3 -I - >/dev/null 2>&1 <<'PY' && echo "ok   $DESC" || { echo "FAIL $DESC"; fail=1; }
import yaml
d = yaml.safe_load(open(".github/dependabot.yml"))
eco = {u["package-ecosystem"]: u["schedule"]["interval"] for u in d["updates"]}
assert eco == {"cargo": "weekly", "github-actions": "weekly"}, eco
PY

DESC="cargo audit exits 0"; check cargo audit
DESC="cargo deny check exits 0"; check cargo deny check
exit $fail
