#!/usr/bin/env bash
# Tests tools/check-secrets.sh against temp folders only. Fake secrets are assembled at run time
# so this file itself never contains one. Exit 0 = all good.
set -u
cd "$(dirname "$0")/.."
SCRIPT="$PWD/tools/check-secrets.sh"
fail=0
ok()  { echo "ok   $1"; }
bad() { echo "FAIL $1"; fail=1; }

T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
aws="AKI""AABCDEFGHIJKLMNOP"
gh="ghp""_$(printf 'a%.0s' $(seq 1 36))"
pem="-----BEGIN RSA ""PRIVATE KEY-----"

[ -x "$SCRIPT" ] && ok "script exists and is executable" || { bad "script exists and is executable"; exit 1; }

mkdir -p "$T/clean/src" && echo 'fn main() {}' > "$T/clean/src/a.rs"
"$SCRIPT" "$T/clean" >/dev/null 2>&1 && ok "clean tree exits 0" || bad "clean tree exits 0"

for name in aws gh pem; do
  d="$T/$name"; mkdir -p "$d/src"
  case $name in aws) v=$aws;; gh) v=$gh;; pem) v=$pem;; esac
  echo "key = \"$v\"" > "$d/src/config.txt"
  out=$("$SCRIPT" "$d" 2>&1); rc=$?
  [ $rc -ne 0 ] && ok "$name key is caught (exit $rc)" || bad "$name key is caught"
  echo "$out" | grep -q "src/config.txt:1" && ok "$name: output names file and line" || bad "$name: output names file and line"
  echo "$out" | grep -qF "$v" && bad "$name: secret value is not printed" || ok "$name: secret value is not printed"
done

mkdir -p "$T/fx/tests/fixtures" && echo "$aws" > "$T/fx/tests/fixtures/sample.txt"
"$SCRIPT" "$T/fx" >/dev/null 2>&1 && ok "tests/fixtures is ignored" || bad "tests/fixtures is ignored"

mkdir -p "$T/gitdir/.git" "$T/gitdir/target" && echo "$aws" > "$T/gitdir/.git/x" && echo "$aws" > "$T/gitdir/target/y"
"$SCRIPT" "$T/gitdir" >/dev/null 2>&1 && ok ".git and target are ignored" || bad ".git and target are ignored"

"$SCRIPT" . >/dev/null 2>&1 && ok "this repository is clean" || bad "this repository is clean"
exit $fail
