#!/usr/bin/env bash
# Runs the container tests: builds the test image (cached), builds the test binary on the host, then runs it as
# root INSIDE a rootless Podman container. Nothing on this machine's pacman, files or network is touched.
#
#   tests/container/run.sh                 run every container test
#   tests/container/run.sh dependency      only tests whose name contains "dependency"
#
# Rules this script keeps (docs/security/threat-model.md, tests/container/README.md):
#  - rootless Podman only; never --privileged; refuses to run as root
#  - the only host path inside the container is the test binary, read-only
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
# builds a new image and an unchanged one is reused from podman's cache.
inputs=$(cat "$here/Containerfile" "$here"/pkgs/*/PKGBUILD | sha256sum | cut -c1-12)
image="localhost/tidewise-test:$inputs"

echo "==> image $image (the first build needs network, later runs do not)"
podman build --tag "$image" --file "$here/Containerfile" "$here"

echo "==> building the test binary on the host"
build_log=$(mktemp)
trap 'rm -f "$build_log"' EXIT
if ! cargo test -p tidewise-container-tests --test smoke --no-run >"$build_log" 2>&1; then
    cat "$build_log" >&2
    exit 1
fi
binary=$(sed -n 's/^ *Executable tests\/smoke\.rs (\(.*\))$/\1/p' "$build_log")
if [ -z "$binary" ]; then
    echo "run.sh: could not find the test binary in cargo's output:" >&2
    cat "$build_log" >&2
    exit 1
fi
binary=$(realpath "$binary")

echo "==> running inside the container (no network, binary mounted read-only)"
exec podman run --rm \
    --pull=never \
    --network=none \
    --security-opt=no-new-privileges \
    --volume "$binary:/opt/tidewise/smoke:ro" \
    "$image" \
    /opt/tidewise/smoke --ignored --test-threads=1 "$@"
