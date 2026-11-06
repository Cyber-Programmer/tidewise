# Resume: merge run 07, then start run 08

## 1. Merge and push run 07 (in the repo folder, in a normal terminal)
```
git status --short                      # should be clean; if not: git add -A && git commit
git switch main
git merge --no-ff run/07-2026-11-02 -m "Merge run 07: history, startup, services, settings, health, search, states"
git push origin main
```
Then GitHub → Actions: wait for the CI run on that push. Red? Save the failing lines to `docs/ci-failure.log`.

## 2. Start run 08 (tmux keeps it alive; systemd-inhibit stops sleep)
```
tmux new -s tidewise
cd ~/Documents/"Creative Chaos"/OpenSource/tidewise
systemd-inhibit --what=sleep:idle --why="tidewise loop" claude
```
Then paste the `/loop` prompt below. Stop time is the first 08:00 local after the start; add
"Stop at HH:MM" to the prompt to change it.

## /loop prompt
```
/loop Do one round of docs/loop/LOOP.md. Read it, docs/loop/queue.md and the end of docs/loop/report.md, then do the next open task end to end: red test commit, then code commit, fmt/clippy/tests, report entry with evidence, Not verified and Errors, every decision into docs/questions.md, and the WBS sheet update. On the first round do the Start of the run steps first. Never call anything done, passing or updated without the command and its exit code; anything skipped, failed or unverified is reported as exactly that. Then schedule the next round in about 1 minute, or, if a stop condition or the stop time in LOOP.md is met, do End of the run and stop the loop.
```
Same prompt works after a crash or restart: the loop reads its state from the queue and the report.
