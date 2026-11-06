#!/usr/bin/env bash
# Runs the same cargo commands as .github/workflows/ci.yml, in the same order, and stops at the
# first failure. Prints each command's exit code. (tools/test-ci.sh checks the two stay in step.)
set -u
cd "$(dirname "$0")/.."

run() {
  echo "==> $1"
  sh_status=0
  # shellcheck disable=SC2086  # words are split on purpose: these are fixed, quote-free commands
  $1 || sh_status=$?
  echo "<== exit $sh_status: $1"
  [ "$sh_status" -eq 0 ] || exit "$sh_status"
}

run "cargo fmt --all --check"
run "cargo clippy --all-targets --locked -- -D warnings"
run "cargo test --all --locked"
run "cargo build --release -p tidewise-app --locked"
run "cargo audit"
run "cargo deny check"
echo "ci-local: all commands exited 0"
