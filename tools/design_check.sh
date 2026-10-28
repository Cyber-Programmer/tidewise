#!/bin/bash
# Render every reference image of a screen in demo mode and print its LAYOUT MISMATCH.
#   tools/design_check.sh performance|processes|home
set -eu
cd "$(dirname "${BASH_SOURCE[0]}")/.."
case "${1:-}" in
  performance) images="Performance: Performance--details:details Performance--memory:memory Performance--memory-details:memory-details Performance--network:network Performance--disk:disk" ; screen=performance ;;
  processes) images="Processes: Processes--details:details Processes--apps:apps Processes--protected-selected:protected-selected" ; screen=processes ;;
  home) images="Main:" ; screen=home ;;
  *) echo "usage: $0 performance|processes|home" >&2; exit 2 ;;
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
