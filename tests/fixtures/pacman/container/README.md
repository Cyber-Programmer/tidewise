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

## Providers (WBS 6.1, captured 19 Nov 2026)

Captured by `tests/container/tests/pacman_providers.rs` (`measure_provider_prompts`) in a **throwaway layer** on the
cached test image (the image rebuild could not reach the Arch Archive that day; the layer added the six packages offline,
see `docs/progress.md`). Same pacman 7.1.0 and flags as above, but with a pipe on stdin: the prompts are printed even
off a terminal. Packages: `tidewise-dummy-vprov-1/2/3` provide `tidewise-vvirtual=1/2/3`, `needs-vvirtual` depends on
`tidewise-vvirtual>=2`, `needs-virtual` on `tidewise-virtual` (provided, unversioned, by `prov-a` and `prov-b`),
`needs-one` on `tidewise-single` (provided only by `one-prov`). Nothing installed unless the file says so.

| File | Command (exit code), stdin | What it shows |
|---|---|---|
| `provider-prompt-versioned.stdout.txt` / `.stderr.txt` | `pacman -S needs-vvirtual` (1), `1\nn\n` | **The prompt filters by version:** "There are 2 providers available for tidewise-vvirtual>=2: 1) vprov-2  2) vprov-3". `vprov-1` is not offered. The question text is on stdout, the "Enter a number" and "Proceed" prompts on stderr |
| `provider-prompt-unversioned.stdout.txt` | `pacman -S needs-virtual` (1), `1\nn\n` | Without a version rule every provider is listed (`prov-a`, `prov-b`) |
| `provider-prompt-single.*` | `pacman -S needs-one` (1), `n\n` | **One provider: no question**, the provider is just in the list |
| `provider-prompt-eof.*` | `pacman -S needs-vvirtual` (1), stdin closed at once | The provider prompt takes its default (1), then "Proceed? [Y/n]" reads EOF and pacman ends with exit 1. No hang |
| `print-vvirtual-unnamed.stdout.txt`, `print-virtual-unnamed.stdout.txt`, `print-one-unnamed.stdout.txt` | `pacman -S --print --print-format '%r/%n %v' needs-…` (0) | The dry run never asks: it takes the first provider (`vprov-2`, `prov-a`, `one-prov`), dependency first |
| `noconfirm-vvirtual-unnamed.stdout.txt` | `pacman -S --noconfirm --ask 66 needs-vvirtual` (0), no stdin | The real run prints the same question, answers it with the default and installs the **same provider as the dry run** (`vprov-2`). The base case for review 5.8 R-08 |
| `print-b-vprov-{1,2,3}-with-hello.stdout.txt` | `pacman -S --print --print-format '%r/%n %v' needs-vvirtual hello vprov-N` (0), **nothing installed** | Option (b) of the plan. Naming `vprov-1` (provides `=1`) makes pacman **add** `vprov-2`; naming `vprov-2` or `vprov-3` adds nothing |

### Nothing satisfies a dependency, and a provider chain (6.1, 19 Nov 2026)

`tidewise-dummy-stale-prov` provides `tidewise-stale=1`; `needs-stale` depends on `tidewise-stale>=2`. `chain-a` provides
`tidewise-chain` **and depends on `chain-b`, which also provides it**; `chain-c` provides it plainly; `needs-chain` depends
on `tidewise-chain`.

| File | Command (exit code) | What it shows |
|---|---|---|
| `print-no-fit.stdout.txt` / `.stderr.txt` | `pacman -S --print … needs-stale` (1) | No provider fits (the only one is too old): stdout `:: unable to satisfy dependency 'tidewise-stale>=2' required by tidewise-dummy-needs-stale`, stderr `error: failed to prepare transaction (could not satisfy dependencies)`. No list, no provider question. The state "nothing provides X>=N" |
| `print-no-fit-old-provider-named.*` | the same with `stale-prov` named too (1) | Naming the too-old provider does not help; same two lines |
| `prompt-no-fit.stdout.txt` / `.stderr.txt` | `pacman -S needs-stale` (1), `n\n` | The real run says the same, with an extra "skip the above package?" question |
| `provider-prompt-chain.*` | `pacman -S needs-chain` (1), `1\nn\n` | Three providers offered (a, b, c); default 1 = `chain-a`, which pulls in `chain-b` |
| `print-chain-unnamed.stdout.txt` | `--print … needs-chain` (0) | `chain-b`, `chain-a`, `needs-chain`: **two providers of one name in the list**, because one is the other's dependency |
| `print-chain-{a,b,c}-with-hello.stdout.txt` | `--print … needs-chain hello chain-N` (0) | All three are acceptable answers; `chain-a` brings `chain-b` along |
| `print-chain-{a,b,c}-alone.stdout.txt` | `--print … chain-N` (0) | What the candidate needs by itself: `chain-a` → `chain-b`, `chain-a` |
