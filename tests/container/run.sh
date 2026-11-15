#!/usr/bin/env bash
# Runs the container tests: builds the test image (cached), builds the test binary on the host, then runs it as
# root INSIDE a rootless Podman container. Nothing on this machine's pacman, files or network is touched.
#
#   tests/container/run.sh                 run every container test
#   tests/container/run.sh dependency      only tests whose name contains "dependency"
#
# Rules this script keeps (docs/security/threat-model.md, tests/container/README.md):
#  - rootless Podman only; never --privileged; refuses to run as root
#  - the only host paths inside the container are the built test binaries and the built tidewise-helper
#    (5.3a), each mounted read-only; nothing else from the host
#  - each test binary runs in its own fresh container, so one test file cannot leave state for the next
#  - the container has no network (--network=none)
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
here=tests/container

if [ "$(id -u)" -eq 0 ]; then
    echo "run.sh: do not run this as root. Rootless Podman as your normal user is the point." >&2
    exit 1
fi
if ! command -v podman >/dev/null; then
    echo "run.sh: podman is not installed. See $here/README.md, section 'One-time setup'." >&2
    exit 1
fi
if [ "$(podman info --format '{{.Host.Security.Rootless}}')" != "true" ]; then
    echo "run.sh: podman is not running rootless for this user. See $here/README.md." >&2
    exit 1
fi

# The image tag is a hash of everything the image is built from, so changing the Containerfile or a PKGBUILD
# builds a new image and an unchanged one is reused from podman's cache (every file of pkgs/ and pkgs-later/ counts).
inputs=$( (cat "$here/Containerfile"; find "$here/pkgs" "$here/pkgs-later" -type f | sort | xargs cat) | sha256sum | cut -c1-12)
image="localhost/tidewise-test:$inputs"

echo "==> image $image (the first build needs network, later runs do not)"
podman build --tag "$image" --file "$here/Containerfile" "$here"

echo "==> building the helper and the test binaries on the host"
build_log=$(mktemp)
trap 'rm -f "$build_log"' EXIT
if ! cargo build -p tidewise-helper >"$build_log" 2>&1; then
    cat "$build_log" >&2
    exit 1
fi
helper=$(realpath "${CARGO_TARGET_DIR:-target}/debug/tidewise-helper")
if ! cargo test -p tidewise-container-tests --no-run >"$build_log" 2>&1; then
    cat "$build_log" >&2
    exit 1
fi
# One line per integration test file: "Executable tests/<name>.rs (<path>)".
mapfile -t tests < <(sed -n 's/^ *Executable tests\/\([a-z_]*\)\.rs (\(.*\))$/\1 \2/p' "$build_log")
if [ "${#tests[@]}" -eq 0 ]; then
    echo "run.sh: could not find the test binaries in cargo's output:" >&2
    cat "$build_log" >&2
    exit 1
fi

status=0
for entry in "${tests[@]}"; do
    name=${entry%% *}
    binary=$(realpath "${entry#* }")
    echo "==> running $name inside a fresh container (no network, binaries mounted read-only)"
    podman run --rm \
        --pull=never \
        --network=none \
        --security-opt=no-new-privileges \
        --volume "$binary:/opt/tidewise/$name:ro" \
        --volume "$helper:/opt/tidewise/tidewise-helper:ro" \
        "$image" \
        "/opt/tidewise/$name" --ignored --test-threads=1 "$@" || status=$?
done
exit "$status"
