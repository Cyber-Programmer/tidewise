#!/usr/bin/env bash
# Checks README.md against the repository. Exit 0 = all good.
set -u
cd "$(dirname "$0")/.."
R=README.md
fail=0
ok()  { echo "ok   $1"; }
bad() { echo "FAIL $1"; fail=1; }

for h in "What it is" "Status" "Features" "Install" "Run" "What it never does" "Security" "Contributing" "Licence"; do
  grep -qE "^## $h\b" $R && ok "section: $h" || bad "section: $h"
done

grep -q 'Cyber-Programmer/tidewise/actions/workflows/ci.yml/badge.svg?branch=main' $R && [ -f .github/workflows/ci.yml ] \
  && ok "CI badge for ci.yml on main" || bad "CI badge for ci.yml on main"

for p in "Home" "Clean up" "Apps" "Storage" "Performance" "Processes" "Startup apps" "Services" "History" "Health" "Search" "Settings"; do
  grep -qE "^\| $p \| (Built|Partial|Not built) \|" $R && ok "status row: $p" || bad "status row: $p"
done

# Every relative link and image target exists.
missing=$(grep -oE '\]\(([^)#]+)(#[^)]*)?\)' $R | sed -E 's/^\]\(([^)#]+).*/\1/' | grep -vE '^(https?:|mailto:)' | sort -u | while read -r f; do [ -e "$f" ] || echo "$f"; done)
[ -z "$missing" ] && ok "every relative link target exists" || bad "missing link targets: $missing"

for f in docs/screenshots/*.png; do grep -q "$f" $R || { bad "screenshot not shown: $f"; }; done; ok "screenshot check done"

tags=$(git for-each-ref refs/tags | wc -l)
if [ "$tags" -eq 0 ]; then grep -qi 'no release yet' $R && ok "says there is no release yet (no tag exists)" || bad "says there is no release yet (no tag exists)"
else ! grep -qi 'no release yet' $R && ok "no stale 'no release yet' (a tag exists)" || bad "stale 'no release yet'"; fi

# Every number with MB or % must appear in docs/budgets.md (no unmeasured claims).
unm=""
for n in $(grep -oE '[0-9]+(\.[0-9]+)? ?(MB|%)' $R | sed -E 's/ ?(MB|%)$//' | sort -u); do grep -qF "$n" docs/budgets.md || unm="$unm $n"; done
[ -z "$unm" ] && ok "every MB / % number appears in docs/budgets.md" || bad "numbers not in docs/budgets.md:$unm"

# Every --flag named in the README exists in the CLI source; the renderer variable exists.
badflag=""
for f in $(grep -E 'release/tidewise|\.AppImage' $R | grep -oE -- '--[a-z][a-z-]+' | sort -u); do grep -qF -- "\"$f\"" crates/app/src/cli.rs || badflag="$badflag $f"; done
[ -z "$badflag" ] && ok "every --flag in the README exists in cli.rs" || bad "unknown flags:$badflag"
grep -q 'TIDEWISE_RENDERER=gpu' $R && grep -q 'TIDEWISE_RENDERER' crates/app/src/main.rs && ok "TIDEWISE_RENDERER documented and real" || bad "TIDEWISE_RENDERER documented and real"

for t in 'docs/budgets.md' 'SECURITY.md' 'docs/security/' 'CHANGELOG.md' 'LICENSE' 'tools/ci-local.sh'; do
  grep -q "$t" $R && ok "mentions $t" || bad "mentions $t"
done
grep -qi 'admin helper' $R && grep -qi 'needs admin access' $R && ok "says what needs admin access (not built)" || bad "says what needs admin access (not built)"
grep -qi 'AppImage' $R && grep -qi 'libfontconfig1-dev' $R && ok "AppImage and Ubuntu build libraries named" || bad "AppImage and Ubuntu build libraries named"
exit $fail
