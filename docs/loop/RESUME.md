# Resume: merge run 08, then start run 09

GitHub's last CI run (#8, 2 Nov) is from the run 06 merge, so `origin/main` is still behind: this push
carries run 07 and run 08 too.

## 1. Merge and push (normal terminal, repo folder)
```
git status --short                       # should be empty; if not: git add -A && git commit
git switch main
git merge --no-ff run/08-2026-11-06 -m "Merge run 08: CI fix, secret scan, community files, pen-test fixes"
git push origin main
```
Then GitHub -> Actions -> the new CI run. Green or red, tell the next Claude session which. Red: copy the
failing lines into `docs/ci-failure.log` (run 09's Q01 reads it).

## 2. Things only you can do (details: `docs/loop/human-checklist.md`)
- Enable private vulnerability reporting (Settings -> Security) so `SECURITY.md` is true.
- Read `docs/design-proposals.md` and approve/reject each proposed accepted difference.
- Skim `docs/questions.md` (the "Run 07" and "Run 08" sections).

## 3. Start run 09 (tmux keeps it alive; systemd-inhibit stops sleep)
```
tmux new -s tidewise
cd ~/Documents/"Creative Chaos"/OpenSource/tidewise
systemd-inhibit --what=sleep:idle --why="tidewise loop" claude
```
Stop time: first 08:00 local after the start. To change it add "Stop at HH:MM" to the prompt. Daytime run of
about 3 hours: add "Stop at 18:00".

## /loop prompt
```
/loop Do one round of docs/loop/LOOP.md. Read it, docs/loop/queue.md and the end of docs/loop/report.md, then do the next open task end to end: red test commit, then code commit, fmt/clippy/tests, report entry with evidence, Not verified and Errors, every decision into docs/questions.md, and the WBS sheet update. On the first round do the Start of the run steps first. Never call anything done, passing or updated without the command and its exit code; anything skipped, failed or unverified is reported as exactly that. Then schedule the next round in about 1 minute, or, if a stop condition or the stop time in LOOP.md is met, do End of the run and stop the loop.
```
Same prompt after a crash or restart: state lives in the queue and the report.
