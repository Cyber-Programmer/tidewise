#!/bin/bash
# Render every reference image of a screen in demo mode and print its LAYOUT MISMATCH.
#   tools/design_check.sh performance|processes|home|storage|settings|health
# The picture is rendered at the reference image's own height (Settings is 2011 px tall: the default
# 960 px crop scored 86.3 % in run 07, the same page at 2011 px scores 70.3 %).
set -eu
cd "$(dirname "${BASH_SOURCE[0]}")/.."
case "${1:-}" in
  performance) images="Performance: Performance--details:details Performance--memory:memory Performance--memory-details:memory-details Performance--network:network Performance--disk:disk" ; screen=performance ;;
  processes) images="Processes: Processes--details:details Processes--apps:apps Processes--protected-selected:protected-selected" ; screen=processes ;;
  home) images="Main:" ; screen=home ;;
  storage) images="Storage: Storage--downloads:downloads" ; screen=storage ;;
  settings) images="Settings:" ; screen=settings ;;
  health) images="Health:" ; screen=health ;;
  *) echo "usage: $0 performance|processes|home|storage|settings|health" >&2; exit 2 ;;
esac
out=target/design-check
mkdir -p "$out"
cargo build -q -p tidewise-app
for item in $images; do
  name=${item%%:*}; state=${item#*:}
  height=$(python3 -c "from PIL import Image; print(Image.open('docs/design/png/$name.png').size[1])")
  args=(--screenshot "$screen" --out "$out/$name.png" --height "$height")
  [[ -n $state ]] && args+=(--state "$state")
  target/debug/tidewise "${args[@]}"
  printf '%-32s ' "$name"
  python3 tools/design_diff.py "docs/design/png/$name.png" "$out/$name.png" --out "$out/$name" | grep -m1 'LAYOUT MISMATCH' | awk '{print $3, $4}'
done
