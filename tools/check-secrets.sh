#!/usr/bin/env bash
# Looks for things that look like secrets in text files under a folder (default: the repo).
# Prints "file:line: <kind>" only, never the secret itself. Exit 1 if anything is found.
# Skips .git, target, dist, fuzz corpora and tests/fixtures (saved real command output).
# This is a safety net, not a full scanner: see docs/questions.md (R8-Q02).
set -u
root="${1:-$(dirname "$0")/..}"
cd "$root" || exit 2

# kind|extended regex
patterns=(
  "AWS access key id|AKIA[0-9A-Z]{16}"
  "GitHub token|gh[pousr]_[A-Za-z0-9]{36,}"
  "GitHub fine-grained token|github_pat_[A-Za-z0-9_]{22,}"
  "Slack token|xox[baprs]-[A-Za-z0-9-]{10,}"
  "Private key block|-----BEGIN [A-Z ]*PRIVATE KEY-----"
  "Google API key|AIza[0-9A-Za-z_-]{35}"
)

found=0
for entry in "${patterns[@]}"; do
  kind="${entry%%|*}"
  re="${entry#*|}"
  # -I skips binary files; -o is not used so the value is never echoed; cut keeps file:line only.
  while IFS= read -r hit; do
    echo "${hit}: ${kind}"
    found=1
  done < <(grep -rInE -e "$re" . \
      --exclude-dir=.git --exclude-dir=target --exclude-dir=dist --exclude-dir=corpus \
      --exclude-dir=fixtures --exclude-dir=node_modules 2>/dev/null \
    | cut -d: -f1,2 | sed 's|^\./||')
done

if [ "$found" -ne 0 ]; then
  echo "check-secrets: possible secrets found (values not shown)" >&2
  exit 1
fi
echo "check-secrets: nothing found"
