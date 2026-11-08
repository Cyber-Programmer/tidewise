#!/usr/bin/env bash
# Checks CONTRIBUTING.md, SECURITY.md and the issue templates. Exit 0 = all good.
set -u
cd "$(dirname "$0")/.."
fail=0
ok()  { echo "ok   $1"; }
bad() { echo "FAIL $1"; fail=1; }
C=CONTRIBUTING.md; S=SECURITY.md; T=.github/ISSUE_TEMPLATE

for f in $C $S $T/bug_report.yml $T/feature_request.yml $T/config.yml; do [ -f "$f" ] && ok "exists: $f" || bad "exists: $f"; done
[ -f $C ] || exit 1

for h in "Build" "Check before you push" "Tests first" "Design rules" "Commit messages" "Talk to us first"; do
  grep -qE "^## $h\b" $C && ok "CONTRIBUTING section: $h" || bad "CONTRIBUTING section: $h"
done
grep -q 'tools/ci-local.sh' $C && ok "CONTRIBUTING names tools/ci-local.sh" || bad "CONTRIBUTING names tools/ci-local.sh"
grep -qi 'red' $C && grep -qi 'green' $C && grep -q '(red)' $C && ok "CONTRIBUTING explains red then green and the (red) commit" || bad "CONTRIBUTING explains red then green and the (red) commit"
grep -q 'docs/design/tokens.md' $C && ok "CONTRIBUTING points to the design tokens" || bad "CONTRIBUTING points to the design tokens"

# Every conventional-commit type used in the git log is documented.
types=$(git log --format=%s | grep -oE '^[a-z]+(\([a-z0-9-]+\))?:' | sed -E 's/[(:].*//' | sort -u)
miss=""; for t in $types; do grep -qE "\`$t\`" $C || miss="$miss $t"; done
[ -n "$types" ] && [ -z "$miss" ] && ok "commit types in the log are documented ($(echo $types))" || bad "commit types missing from CONTRIBUTING:$miss"

# Every relative link target exists.
for f in $C $S; do
  m=$(grep -oE '\]\(([^)#]+)(#[^)]*)?\)' $f | sed -E 's/^\]\(([^)#]+).*/\1/' | grep -vE '^(https?:|mailto:)' | sort -u | while read -r p; do [ -e "$p" ] || echo "$p"; done)
  [ -z "$m" ] && ok "links resolve in $f" || bad "missing link targets in $f: $m"
done

# No invented contact details.
if grep -rEq '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}' $C $S $T; then bad "no e-mail address in CONTRIBUTING, SECURITY, templates"; else ok "no e-mail address in CONTRIBUTING, SECURITY, templates"; fi

grep -qi 'private vulnerability reporting' $S && ok "SECURITY: private vulnerability reporting" || bad "SECURITY: private vulnerability reporting"
grep -qi 'no version has been released' $S && ok "SECURITY: supported versions say none released yet" || bad "SECURITY: supported versions say none released yet"

python3 -I - <<'PY' >/dev/null 2>&1 && ok "templates parse; bug asks for distro, version, what you did, privacy; config has no blank issues" || bad "templates parse; bug asks for distro, version, what you did, privacy; config has no blank issues"
import yaml
d = ".github/ISSUE_TEMPLATE/"
bug = yaml.safe_load(open(d + "bug_report.yml"))
feat = yaml.safe_load(open(d + "feature_request.yml"))
cfg = yaml.safe_load(open(d + "config.yml"))
for t in (bug, feat):
    assert t["name"] and t["description"] and isinstance(t["body"], list) and t["body"]
ids = {b.get("id") for b in bug["body"]}
assert {"distro", "version", "steps", "expected", "actual"} <= ids, ids
text = open(d + "bug_report.yml").read()
assert "Settings" in text and "About" in text            # where the version is
assert "Copy for a help forum" in text                   # scrubbed History export
assert "personal" in text.lower()                        # no personal paths
assert cfg["blank_issues_enabled"] is False
assert all("url" in c for c in cfg["contact_links"])
PY

grep -q '(CONTRIBUTING.md)' README.md && ! grep -qi 'still to come' README.md && ok "README links CONTRIBUTING.md" || bad "README links CONTRIBUTING.md"
exit $fail
