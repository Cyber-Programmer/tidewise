# Human checklist (things only a person can verify)

Run 07 built a lot that nobody has clicked. Go through this once, with `./target/release/tidewise`
(live data) and note what is wrong in `docs/questions.md` or as GitHub issues.

## Before run 08 starts
1. Commit, merge run 07 into `main` and push (commands in `docs/loop/RESUME.md`).
2. Open GitHub → Actions. Is the CI run for the push green? If red: open the failed job, copy the failing
   lines into `docs/ci-failure.log`, and run 08's first task will start from it.
3. Answer or skim `docs/questions.md`, especially the "Run 07" sections.

## Click through
- **Clean up:** scan, untick a group, confirm, read the end dialog. The slow-cache group (Hugging Face,
  Playwright, uv, pip, Docker…) must be off by default. Try with a folder you cannot remove: the result
  must say so, not "Cleaned".
- **Apps:** search, open the uninstall panel, the "also delete my settings" box on a **user** Flatpak only.
- **Storage:** Trash a small test file, then Undo.
- **Processes:** End task and Force quit on a throwaway process (e.g. `sleep 1000`).
- **Startup apps:** switch an entry off and on; look in `~/.config/autostart/` afterwards.
- **Services:** the tabs; failed units; nothing should be startable (admin access needed).
- **History:** Undo, "Copy for a help forum" (paste it somewhere and read it: no private paths?).
- **Settings:** theme buttons, Follow system. **Health:** each rule and its fix button; Home "Why N?".
- **Search (Ctrl K):** open, type, ↑ ↓, Enter, Esc; from a text field; opening an app and a process.
- **Loading / empty / error states:** unplug nothing; just look for skeleton rows and "Already tidy".

## Run by hand
- `docs/security/pentest-checklist.md` section A and B (A16 and A21 first). Use a throwaway VM for B.
- `./packaging/appimage/build.sh`, then run the AppImage on a second distro (Ubuntu 22.04 is the minimum).
- Compare the apt/dnf/rpm adapters with real output on Debian/Ubuntu/Fedora (fixtures are hand-written).
