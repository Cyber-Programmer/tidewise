# Human checklist (things only a person can verify)

Runs 07 and 08 built and measured a lot that nobody has clicked. Go through this once, with
`./target/release/tidewise` (live data) and note what is wrong in `docs/questions.md` or as GitHub issues.
Everything the loop could only read, parse or measure here is listed in `docs/loop/report.md` under "Not verified".

## First, on GitHub (run 08 changed the pipeline)
1. Merge `run/08-2026-11-06` into `main` and push (commands in `docs/loop/RESUME.md`), then open **Actions**.
   Is **CI** green? The old failure was `cargo audit` (pulldown-cmark); a fix is in. If red: paste the failing
   lines into `docs/ci-failure.log`. Also look at the new `release-build` job and the `supply-chain` secret check.
2. **Settings → Code security:** switch on *Private vulnerability reporting* (SECURITY.md and the issue chooser
   link to it). Open **Issues → New issue**: do the bug and feature forms render, and the security link work?
3. Open the **README** on GitHub: badge, status table, screenshots, links. Run the Ubuntu 22.04 and Arch install
   lines on a clean machine (the loop could not).
4. Run the weekly **Fuzz smoke** workflow once by hand (Actions → Fuzz smoke → Run workflow) and see it finish.
5. Skim `docs/questions.md` ("Run 08 — R8-Q01 … R8-Q12") and `docs/design-proposals.md`; approve or reject.

## Click through
- **Clean up:** scan, untick a group, confirm, read the end dialog; slow caches off by default; a folder you cannot
  remove must say so. Look at it in the **light** theme: the Safe and Review chips must be readable.
- **Apps:** search, uninstall panel, "also delete my settings" on a **user** Flatpak only.
- **Storage:** Trash a small test file, then Undo. **Startup apps:** switch off and on; look in `~/.config/autostart/`.
- **Processes:** End task and Force quit on a throwaway (`sleep 1000`). Try a protected row (desktop, sound): no button.
- **Services:** tabs; failed first; nothing startable. **History:** Undo; "Copy for a help forum" (read it: no private paths?).
- **Settings:** theme, Follow system; About shows the version. **Health:** each rule and its fix; Home "Why N?".
- **Search (Ctrl K):** open, type, arrows, Enter, Esc, from a text field. **Keyboard:** in the Clean up, Apps and
  End task dialogs press Tab, Enter and Esc. Run 08 found by reading the markup that they do nothing (a gap, not fixed).
- **States:** skeleton rows, "Already tidy", `--demo --state …` pages.

## Run by hand
- `docs/security/pentest-checklist.md` sections A and B on a throwaway VM. A16 (End task) and A21 (`--out`, `--state`) now
  have automated tests and results written in the file; still do them yourself, plus the other 20 items.
- `tools/ci-local.sh` (the same commands as CI) and `./packaging/appimage/build.sh`; run the AppImage on Ubuntu 22.04.
- Open Ctrl K, then `tools/measure.sh` against that process (budget ≤ 40 MB PSS); run 08 could not press the key.
- Compare apt/dnf/rpm adapters with real output on Debian/Ubuntu/Fedora (fixtures are hand-written).

## Decide (each blocks work)
- Home file search (R8-Q08: a resident index does not fit the memory budget; options listed).
- Keyboard use of dialogs (what Esc and Enter do; Enter must never confirm a destructive action by default).
- Which missing Settings, Startup, Services, History and Health pieces are v1.0 (R8-Q10 to Q12), and the light-theme `warn`.
