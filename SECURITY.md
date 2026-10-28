# Security policy

## Reporting a vulnerability

Please report privately, not in a public issue.

Use GitHub's private vulnerability reporting: open the **Security** tab of the repository and choose
**Report a vulnerability** (a private security advisory). Include what you did, what you expected, what
happened, and the Tidewise version (`Settings → About`) and distro.

You will get an acknowledgement within a week. This is a volunteer project, so fixes follow the severity:
anything that lets a non-root process gain root, or deletes user files, comes first.

## Supported versions

Tidewise is pre-1.0. Only the latest release (and `main`) receives security fixes.

| Version | Supported |
| --- | --- |
| latest release / `main` | yes |
| older | no |

## What is in scope

- The app, and the privileged helper (`tidewise-helper`) with its polkit policy.
- Anything that makes the app run as root, run a shell, or act outside the helper's allow-list.
- Deleting or changing user files without the confirm screen; Trash that silently deletes.
- Parsers of outside data (package-manager output, `/proc`, `.desktop` files) that crash, hang or
  misbehave on crafted input.
- The release artefacts and the build/release workflow (supply chain).
- Any network traffic other than the daily update check to GitHub Releases.

## Out of scope

- Attacks that already need root on the machine.
- Bugs in the distro's package managers, polkit or the desktop environment themselves.
- Denial of service by filling the disk or running thousands of processes.
- Issues in unreleased branches other than `main`.

The design is described in [`docs/security/threat-model.md`](docs/security/threat-model.md).
