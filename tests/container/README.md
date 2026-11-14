# Container tests (real pacman, as root, in a throwaway Arch container)

Some tests must run the real `pacman` as root (the helper, from M3 on). That is never done on your machine: these
tests run inside a **rootless Podman container** built from a pinned Arch image, with a tiny local repository of dummy
packages, and with **no network**.

## What runs where

```
your machine (normal user)            rootless Podman container (no network)
------------------------------        ----------------------------------------------
tests/container/run.sh        --->    pinned archlinux:base image + Arch Archive snapshot
  cargo builds the test binary        local repo /srv/tidewise-test (dummy packages)
  podman run ... --volume bin:ro      the test binary, read-only, runs as the container's "root"
                                      (= your normal user id on the host, see "Why rootless is safe")
```

Rules the harness keeps:

- Rootless Podman only. Never `--privileged`. `run.sh` refuses to run as root.
- The only host path inside the container is the built test binary, mounted read-only.
- `--network=none` at test time. The network is used only while the image is built (base image and the Arch Archive
  snapshot), once.
- Every container test starts with `require_container()` (`src/lib.rs`). It says yes only if **both**
  `/run/.containerenv` (Podman/Docker make it) **and** `/srv/tidewise-test/tidewise-test.db.tar.gz` (only our image has
  it) exist. So the tests refuse on your host, and also in some other container such as a distrobox that shares your
  home folder. The smoke tests are `#[ignore]`, so `cargo test --all` never runs them.

## One-time setup (you run these; they need `sudo`)

```
sudo pacman -S podman                                  # if `pacman -Qu` is empty; otherwise: sudo pacman -Syu podman
podman info --format '{{.Host.Security.Rootless}}'     # must print: true
podman unshare cat /proc/self/uid_map                  # expect "0 <your uid> 1" and "1 100000 65536"
```

Rootless containers need a range of extra user ids for your user in `/etc/subuid` and `/etc/subgid`
(`<user>:100000:65536`). Arch creates them for new users; check with `grep "$USER" /etc/subuid /etc/subgid`.

Sources: ArchWiki, *Podman* (installation, rootless setup) <https://wiki.archlinux.org/title/Podman>; `subuid(5)` and
`subgid(5)`; `podman-run(1)` (`--network`, `--volume ...:ro`, `--security-opt`, and `/run/.containerenv`).

**Why rootless is safe here:** inside the container the test is "root" (uid 0), but rootless Podman maps container uid
0 to *your* uid on the host and the other container ids to the subuid range, so "root in the container" is just your
user outside it (user namespaces, `user_namespaces(7)`).

## Run

```
tests/container/run.sh                  # all container tests
tests/container/run.sh dependency       # only tests whose name contains "dependency"
```

The first run builds the image (needs network, a few minutes). Later runs reuse it from Podman's cache; the image tag is a
hash of the Containerfile and the PKGBUILDs, so editing those builds a new image.

CI: not built yet. GitHub's Ubuntu runners should be able to run rootless Podman, but I have not checked, so a
CI job comes in its own task.

## What is in the image

- `docker.io/library/archlinux:base`, **pinned by digest** in the `Containerfile` (image version `20261108.0.623074`).
- Packages from the **Arch Linux Archive snapshot** of the same day, not a live mirror
  (<https://wiki.archlinux.org/title/Arch_Linux_Archive>), so a rebuild gives the same versions.
- `/srv/tidewise-test`: a local repository `[tidewise-test]` (`SigLevel = Never`, for this repository in this image
  only) with three dummy packages built from `pkgs/*/PKGBUILD` by `makepkg` and indexed with `repo-add`:
  `tidewise-dummy-hello`, `tidewise-dummy-lib`, and `tidewise-dummy-app` (depends on the lib).
- The package lists were refreshed once at build time, so tests use a plain `pacman -S` (never `-Sy`: golden rule 4).

## Refreshing the image

Do it about once a month, or when `run.sh` fails with `GLIBC_x.y not found` (your machine's glibc is newer than the
image's, and the test binary is built on your machine):

1. `podman pull docker.io/library/archlinux:base`
2. `podman image inspect docker.io/library/archlinux:base --format '{{.Digest}} {{.Labels}}'` for the new digest and the
   `org.opencontainers.image.version` label (its date, `YYYYMMDD`).
3. In the `Containerfile`, set the new digest in `FROM` and the same day in `ARCHIVE_DAY` (`YYYY/MM/DD`) and the version
   in the comment.
4. `tests/container/run.sh`, then one commit: `chore(container): refresh base image to YYYY-MM-DD`.

## Troubleshooting

- **`pasta failed ... Failed to open() /dev/net/tun: No such device` while building.** Seen on 14 Nov 2026: the running
  kernel (7.2.9) was older than the installed `linux` package (7.3.4), so its modules directory was gone and `tun`
  could not load. Reboot into the installed kernel. (Only the build needs this network; tests use `--network=none`.)
- **`run.sh: podman is not running rootless`**: see the one-time setup.
- **A test says "refusing to run"**: you ran the binary outside `run.sh`. That is the guard working.
