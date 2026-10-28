#!/bin/bash
# Tests for the pure functions in tools/measure.sh, using the fake /proc tree in tests/fixtures/proc-tree.
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/../.." && pwd)
# shellcheck source=../../tools/measure.sh
source "$root/tools/measure.sh"
fake="$root/tests/fixtures/proc-tree"
fail=0
check() { # name expected actual
  if [[ "$2" == "$3" ]]; then echo "ok   $1"; else echo "FAIL $1: expected '$2', got '$3'"; fail=1; fi
}

check "tree of 100 is the root and all descendants" "100 101 102 103" "$(tree_pids "$fake" 100 | sort -n | tr '\n' ' ' | sed 's/ $//')"
check "tree of a leaf is itself" "102" "$(tree_pids "$fake" 102)"
check "tree of a missing pid is empty" "" "$(tree_pids "$fake" 4242)"
check "pss of the whole tree" "3450" "$(sum_pss_kb "$fake" 100)"
check "pss of a subtree" "2400" "$(sum_pss_kb "$fake" 101)"
check "pss of a missing pid is 0" "0" "$(sum_pss_kb "$fake" 4242)"
check "pss ignores Pss_Anon and Pss_File lines" "50" "$(sum_pss_kb "$fake" 103)"
check "cpu ticks of the tree" "192" "$(sum_cpu_ticks "$fake" 100)"
check "comm with spaces and parens does not break ppid" "100" "$(ppid_of "$fake" 101)"
check "comm with parens: ppid of 100" "1" "$(ppid_of "$fake" 100)"
exit $fail
