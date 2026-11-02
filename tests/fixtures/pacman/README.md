Real fixtures, captured on Arch Linux with `LC_ALL=C` (read-only commands):
`Qi-bash.txt`, `Qi-vlc.txt`, `Qi-asar.txt`, `Qq-first40.txt` (first 40 lines), `Qdtq-first8.txt` (first 8 lines).

`Qi-asar-de.txt` is HAND-WRITTEN to imitate German output: no `de_DE` locale is installed on the
capture machine (see docs/questions.md). Replace it with a real capture when one exists.

`Rns-print-*-handwritten.txt` are HAND-WRITTEN from pacman's documented behaviour: `pacman -Rns --print`
prints one `name-version-release` per target in the default format; a refused removal prints
`:: removing X breaks dependency 'X' required by Y` lines on a C-locale run; an unknown name prints
`error: target not found: NAME`. The run was not allowed to run `pacman -R*` on the capture machine
(run 06 rules), so none of these is a real capture. Replace them with real ones (in a container) before
trusting the parser. `…-blocked-de-handwritten.txt` imitates a German run (the parser must refuse it,
not guess); `…-garbage-…` is a broken case.
