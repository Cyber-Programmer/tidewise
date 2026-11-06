#!/usr/bin/env bash
# Fuzz smoke test: runs every target in fuzz/fuzz_targets for N seconds each (default 60).
#   tools/fuzz-smoke.sh [seconds]     run all targets, keep going after a failure, exit 1 if any failed
#   tools/fuzz-smoke.sh --list        print the target names (one per line) and exit
# The list comes from the directory, never from a hand-typed list. Needs `cargo +nightly fuzz`.
set -u
cd "$(dirname "$0")/.."

targets() { for f in fuzz/fuzz_targets/*.rs; do basename "$f" .rs; done; }

if [ "${1:-}" = "--list" ]; then targets; exit 0; fi

secs="${1:-60}"
failed=()
for t in $(targets); do
  echo "==> fuzz $t for ${secs}s"
  cargo +nightly fuzz run "$t" -- "-max_total_time=$secs" || failed+=("$t")
  echo "<== $t: exit status recorded"
done

if [ "${#failed[@]}" -ne 0 ]; then
  echo "fuzz-smoke: FAILED targets: ${failed[*]}" >&2
  exit 1
fi
echo "fuzz-smoke: all targets ran ${secs}s with no crash"
