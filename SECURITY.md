# Security policy

Tidewise will run a small helper with administrator (root) rights to install and remove packages, so security
reports are taken seriously.

## Reporting a vulnerability

**Please do not open a public issue.** Report privately through GitHub:

1. Go to the repository's **Security** tab.
2. Click **Report a vulnerability**
   (direct link: https://github.com/Cyber-Programmer/tidewise/security/advisories/new).

Please include what you found, how to reproduce it, and what an attacker could do with it.

## What to expect

Tidewise is maintained by one person in their spare time, so please be patient:

- Acknowledgement within **7 days**.
- A first assessment (confirmed or not, and how serious) within **14 days**.
- Fixes for confirmed issues are prioritised over all other work. You'll be credited in the advisory unless you
  prefer not to be.

## Supported versions

There is no release yet. Until v0.1, only the latest commit on `main` is supported.

| Version | Supported |
|---|---|
| `main` (latest commit) | ✔ |
| tag `archive/old-v1` (old, unmaintained code) | ✘ |

## In scope

Especially interesting once they exist:

- `tidewise-helper` (the root helper) and its request validation
- The polkit policy and how the app starts the helper
- Parsers of pacman and `/proc` output
- Anything that could delete or change files the user didn't confirm
- Any way to trigger a partial upgrade or bypass the confirm screen

Dependency advisories are tracked automatically with `cargo deny` (see `deny.toml`).
