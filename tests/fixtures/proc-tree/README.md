HAND-WRITTEN fake /proc tree for `spikes/measure.sh` tests (format of real `stat` and `smaps_rollup`).
100 "my app (v2)" is the root: children 101 and 103, grandchild 102 -> Pss 1000+2000+400+50 = 3450 kB,
ticks (utime+stime) 150+30+10+2 = 192.
200 and 104 are not in the tree and must not be counted.
