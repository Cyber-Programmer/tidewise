#!/bin/bash
# Build dist/Tidewise-<version>-x86_64.AppImage.
#   packaging/appimage/build.sh                       release build, then the AppImage
#   packaging/appimage/build.sh --binary <file> --out <dir> --appdir-only
#                                                     assemble the AppDir around a given binary (used by tests)
# Needs `appimagetool` on PATH (the human installs it) unless --appdir-only is given.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

binary="" out="$root/dist" appdir_only=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --binary) binary="${2:?--binary needs a file}"; shift 2 ;;
    --out) out="${2:?--out needs a folder}"; shift 2 ;;
    --appdir-only) appdir_only=1; shift ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done

version="$(sed -n '/^\[workspace.package\]/,/^\[/{s/^version *= *"\(.*\)"/\1/p}' Cargo.toml | head -n1)"
[[ -n $version ]] || { echo "cannot read the version from Cargo.toml" >&2; exit 1; }

if [[ -z $binary ]]; then
  cargo build --release --locked -p tidewise-app
  binary="$root/target/release/tidewise"
fi
[[ -x $binary ]] || { echo "no runnable binary at $binary" >&2; exit 1; }

appdir="$out/AppDir"
rm -rf "$appdir"
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/applications" "$appdir/usr/share/icons/hicolor/scalable/apps"
install -m 0755 "$binary" "$appdir/usr/bin/tidewise"
install -m 0644 packaging/appimage/io.github.tidewise.desktop "$appdir/io.github.tidewise.desktop"
install -m 0644 packaging/appimage/io.github.tidewise.desktop "$appdir/usr/share/applications/io.github.tidewise.desktop"
install -m 0644 docs/design/icons/logo.svg "$appdir/io.github.tidewise.svg"
install -m 0644 docs/design/icons/logo.svg "$appdir/usr/share/icons/hicolor/scalable/apps/io.github.tidewise.svg"
cat > "$appdir/AppRun" <<'RUN'
#!/bin/sh
here="$(dirname "$(readlink -f "$0")")"
exec "$here/usr/bin/tidewise" "$@"
RUN
chmod 0755 "$appdir/AppRun"

if [[ $appdir_only == 1 ]]; then
  echo "$appdir"
  exit 0
fi

command -v appimagetool >/dev/null || { echo "appimagetool is not on PATH; install it and run again (AppDir is in $appdir)" >&2; exit 3; }
mkdir -p "$out"
file="$out/Tidewise-$version-x86_64.AppImage"
ARCH=x86_64 appimagetool "$appdir" "$file"
echo "$file"
