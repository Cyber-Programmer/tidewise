# pacman fixtures from the container (WBS 5.7)

Real pacman output for the **dummy packages of the Tidewise test container** (`tests/container/`), saved so the
helper's dry-run and parser work (M3, M4) has real transaction text, including a **third-party repository**
(`tidewise-test`) and a **dependency pull-in**, neither of which the host fixtures in `../` have.

- Captured: 2026-11-14, inside the test image `localhost/tidewise-test` built from base image
  `archlinux:base@sha256:fcee73ea1b96e236248f53a213f71276cc9072d9025b58bce9915e32f19963ae` (version 20261108.0.623074)
- pacman: **v7.1.0**, libalpm v16.0.1
- Environment: `env -i LC_ALL=C /usr/bin/pacman …` as the container's root, `--network=none`, output redirected to a file
  (so no line wrapping, like the app sees it)
- The dummy packages: `tidewise-dummy-hello` (no dependencies), `tidewise-dummy-lib`, `tidewise-dummy-app` (depends on
  the lib). Repository `tidewise-test` is `file:///srv/tidewise-test`.
- **`Build Date` and `Install Date` change every time the image is rebuilt or the capture is redone.** Tests must not
  depend on them.

| File | Command (exit code) | Why it's in the sample |
|---|---|---|
| `print-app.txt` | `pacman -S --print tidewise-dummy-app` (0) | The dry run. One `file://` URL per package to be installed, **dependency first** (lib, then app), in install order. Real repositories print `https://…` URLs instead |
| `print-format-app.txt` | `pacman -S --print --print-format '%r/%n %v' tidewise-dummy-app` (0) | Same dry run as `repo/name version` lines: the shape the helper can compare with the confirmed list (rule 5, T-09) |
| `print-not-found.stderr.txt` | stderr of `pacman -S --print tidewise-dummy-nosuch` (1) | `error: target not found: tidewise-dummy-nosuch`. stdout is empty |
| `si-dummy-app.txt` | `pacman -Si tidewise-dummy-app` (0) | A `-Si` record from a **third-party repository** (`Repository : tidewise-test`); `Installed Size : 0.02 KiB` |
| `ss-dummy.txt` | `pacman -Ss dummy` (0) | `-Ss` mixing `extra/…` (one with a group `(xorg-drivers)`) and `tidewise-test/…` results |
| `q-installed.txt` | `pacman -Q tidewise-dummy-app tidewise-dummy-lib` after `pacman -S --noconfirm tidewise-dummy-app` (0) | What "installed" looks like after the install, dependency included |
| `qi-dummy-app.txt` | `pacman -Qi tidewise-dummy-app` after that install | `Install Reason : Explicitly installed`, `Depends On : tidewise-dummy-lib` |
| `qi-dummy-lib.txt` | `pacman -Qi tidewise-dummy-lib` after that install | `Install Reason : Installed as a dependency for another package`, `Required By : tidewise-dummy-app` |

To capture again: build the image with `tests/container/run.sh`, then run each command with
`podman run --rm --pull=never --network=none <image> env -i LC_ALL=C /usr/bin/pacman …` and redirect to a file.
