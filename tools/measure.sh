#!/bin/bash
# Measure a GUI app for the UI-toolkit comparison.
#
#   tools/measure.sh [--header] <command> [args...]
#
# Starts the command, waits 60 s, then prints one CSV line:
#   label,binary_bytes,pss_kb,cpu_percent,processes
#   - binary_bytes  size of the executable file
#   - pss_kb        sum of `Pss` from /proc/<pid>/smaps_rollup for the process AND all descendants
#                   (a WebView app keeps most of its memory in helper processes)
#   - cpu_percent   utime+stime of the same process tree over the next 60 s, 100 = one full core
#   - processes     how many processes were in the tree when Pss was taken
# Then it stops the whole tree.
#
# Environment: MEASURE_WAIT (60), MEASURE_WINDOW (60) in seconds, MEASURE_LABEL (the command line),
# MEASURE_PROC (/proc; tests point it at tests/fixtures/proc-tree).
#
# Limits: CPU of processes that start or exit during the window is only partly counted.

# --- pure functions (sourced by tests/measure/test_measure.sh) ---------------------------------

# Print the fields of /proc/<pid>/stat after the "(comm)" part, which may contain spaces and
# parentheses. First field printed is the state, second the parent pid, 12th/13th are utime/stime.
_stat_tail() { # proc_root pid
  local line
  read -r line 2>/dev/null <"$1/$2/stat" || return 1
  printf '%s\n' "${line##*) }"
}

ppid_of() { # proc_root pid
  local tail
  tail=$(_stat_tail "$1" "$2") || return 1
  set -- $tail
  printf '%s\n' "$2"
}

# The pid itself and every descendant, one per line. Nothing when the pid does not exist.
tree_pids() { # proc_root root_pid
  local proc=$1 root=$2 d pid line
  [[ -r $proc/$root/stat ]] || return 0
  local -A kids=()
  local -a rest
  for d in "$proc"/[0-9]*; do
    pid=${d##*/}
    read -r line 2>/dev/null <"$d/stat" || continue
    line=${line##*) }
    read -r -a rest <<<"$line"
    kids[${rest[1]}]+="$pid "
  done
  local -a queue=("$root")
  local cur kid
  while ((${#queue[@]})); do
    cur=${queue[0]}
    queue=("${queue[@]:1}")
    printf '%s\n' "$cur"
    for kid in ${kids[$cur]:-}; do queue+=("$kid"); done
  done
}

# Sum of `Pss:` over the tree, in kB.
sum_pss_kb() { # proc_root root_pid
  local pid key value sum=0
  for pid in $(tree_pids "$1" "$2"); do
    [[ -r $1/$pid/smaps_rollup ]] || continue
    while read -r key value _; do
      [[ $key == "Pss:" ]] && sum=$((sum + value))
    done 2>/dev/null <"$1/$pid/smaps_rollup"
  done
  printf '%s\n' "$sum"
}

# Sum of utime+stime (clock ticks) over the tree.
sum_cpu_ticks() { # proc_root root_pid
  local pid tail sum=0
  local -a f
  for pid in $(tree_pids "$1" "$2"); do
    tail=$(_stat_tail "$1" "$pid") || continue
    read -r -a f <<<"$tail"
    sum=$((sum + f[11] + f[12]))
  done
  printf '%s\n' "$sum"
}

# --- the measurement ----------------------------------------------------------------------------

main() {
  local header=0
  if [[ ${1:-} == "--header" ]]; then header=1; shift; fi
  if (($# == 0)); then
    ((header)) && { echo "label,binary_bytes,pss_kb,cpu_percent,processes"; return 0; }
    echo "usage: $0 [--header] <command> [args...]" >&2
    return 2
  fi
  ((header)) && echo "label,binary_bytes,pss_kb,cpu_percent,processes"

  local wait=${MEASURE_WAIT:-60} window=${MEASURE_WINDOW:-60} proc=${MEASURE_PROC:-/proc}
  local label=${MEASURE_LABEL:-$*}
  local bin size
  bin=$(command -v -- "$1") || { echo "not found: $1" >&2; return 2; }
  size=$(stat -c %s -- "$(readlink -f -- "$bin")") || return 2

  "$@" >/dev/null 2>&1 &
  local pid=$!
  sleep "$wait"
  if ! kill -0 "$pid" 2>/dev/null; then
    echo "the app exited before the measurement" >&2
    return 1
  fi

  local pss procs t0 t1 s0 s1 tick
  pss=$(sum_pss_kb "$proc" "$pid")
  procs=$(tree_pids "$proc" "$pid" | wc -l)
  tick=$(getconf CLK_TCK)
  t0=$(sum_cpu_ticks "$proc" "$pid")
  s0=$(date +%s.%N)
  sleep "$window"
  t1=$(sum_cpu_ticks "$proc" "$pid")
  s1=$(date +%s.%N)

  local cpu
  cpu=$(awk -v a="$t0" -v b="$t1" -v hz="$tick" -v s0="$s0" -v s1="$s1" \
    'BEGIN { d = (b - a) / hz / (s1 - s0) * 100; printf "%.1f", (d < 0 ? 0 : d) }')

  # Stop the whole tree: ask politely, then insist.
  local victims
  victims=$(tree_pids "$proc" "$pid")
  # shellcheck disable=SC2086
  kill $victims 2>/dev/null
  sleep 1
  # shellcheck disable=SC2086
  kill -9 $victims 2>/dev/null
  wait "$pid" 2>/dev/null

  printf '"%s",%s,%s,%s,%s\n' "${label//\"/\"\"}" "$size" "$pss" "$cpu" "$procs"
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  main "$@"
fi
