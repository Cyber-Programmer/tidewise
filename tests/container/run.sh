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
#  - every file runs with no-new-privileges except app_private* (see the comment at the `podman run` below)
#  - (5.5) the package test runs in a second image that holds the package of HEAD, built by makepkg in a container;
#    that build, and only that, needs network, and happens before any test runs
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

# The package image (5.5): the package of HEAD, built by makepkg inside a container (Containerfile.package), put into
# a copy of the test image. It is rebuilt whenever HEAD's tarball, the recipe or the Containerfile changes, and a
# release build of the whole workspace takes minutes, so it is built only for a full run or when a filter says
# "package" (`run.sh package`); `run.sh dependency` does not touch it.
want_package=1
if [ $# -gt 0 ]; then
    want_package=0
    for filter in "$@"; do
        case "$filter" in *package*) want_package=1 ;; esac
    done
fi
pkg_image=""
if [ "$want_package" -eq 1 ]; then
    # The unit tests read the working tree and this image is built from HEAD, so a green unit run and a green container
    # run could describe two different recipes (review R-06 of 5.5). Refuse when what the package is made of is dirty.
    if [ -n "$(git status --porcelain -- packaging Cargo.toml Cargo.lock LICENSE tests/container/Containerfile.package)" ]; then
        echo "run.sh: the package is built from HEAD (git archive), but packaging/, Cargo.toml, Cargo.lock, LICENSE or" \
             "Containerfile.package have uncommitted changes. Commit them first (or stash them)." >&2
        exit 1
    fi
    if [ -n "$(git status --porcelain)" ]; then
        echo "run.sh: WARNING: the working tree has uncommitted changes. The package is built from HEAD" \
             "(git archive), not from the working tree." >&2
    fi
    pkgver=$(git show HEAD:packaging/aur/PKGBUILD | sed -n 's/^pkgver=//p')
    ctx="${CARGO_TARGET_DIR:-target}/container-package"
    mkdir -p "$ctx"
    rm -f "$ctx/PKGBUILD" "$ctx/.SRCINFO" "$ctx"/tidewise-*.tar.gz
    # Deterministic: git archive uses the commit's time for every file, and gzip -n leaves its own clock out.
    git archive --format=tar --prefix="tidewise-$pkgver/" HEAD | gzip -n > "$ctx/tidewise-$pkgver.tar.gz"
    git show HEAD:packaging/aur/PKGBUILD > "$ctx/PKGBUILD"
    git show HEAD:packaging/aur/.SRCINFO > "$ctx/.SRCINFO"
    pkg_inputs=$( (echo "$image"; cat "$here/Containerfile.package" "$ctx/PKGBUILD" "$ctx/.SRCINFO" \
        "$ctx/tidewise-$pkgver.tar.gz") | sha256sum | cut -c1-12)
    pkg_image="localhost/tidewise-package-test:$pkg_inputs"
    echo "==> package image $pkg_image (builds the package with makepkg: needs network, takes minutes the first time)"
    podman build --tag "$pkg_image" --build-arg "BASE=$image" --file "$here/Containerfile.package" "$ctx"
fi

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
    # `package*` files run in the package image (the built package is in it); every other file in the plain test image.
    run_image="$image"
    case "$name" in package*) is_package=1 ;; *) is_package=0 ;; esac
    if [ "$is_package" -eq 1 ]; then
        if [ "$want_package" -eq 0 ]; then
            echo "==> skipping $name (the filter does not name it; the package image was not built)"
            continue
        fi
        run_image="$pkg_image"
    fi
    # `app_private*` (WBS 6.11) run WITHOUT no-new-privileges. They test the app's own refresh, started by an ordinary
    # user, whose session does not have that bit; with it set, pacman can apply its filesystem sandbox anyway and a
    # refresh that fails on a real machine would pass here. Only these files; still rootless, still --network=none.
    privileges=(--security-opt=no-new-privileges)
    case "$name" in app_private*) privileges=() ;; esac
    echo "==> running $name inside a fresh container (no network, binaries mounted read-only)"
    podman run --rm \
        --pull=never \
        --network=none \
        "${privileges[@]}" \
        --volume "$binary:/opt/tidewise/$name:ro" \
        --volume "$helper:/opt/tidewise/tidewise-helper:ro" \
        "$run_image" \
        "/opt/tidewise/$name" --ignored --test-threads=1 "$@" || status=$?
done
exit "$status"
