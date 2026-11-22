# Weekly roadmap

Work budget per week: 14 points (easy 1, medium 2, hard 3.5). Phases are bug classes, ranked by
functional gain per credit cost. Built Thursday 19:07 São Paulo, revised Friday 05:00; see
[COMPAT.md](COMPAT.md) for scores.

## Week of 2026-11-22 (partial, before the first full cycle)

Spent: 1 of 1 point (only ~6% of the weekly credit is left until Friday).

Daily share: 1 point on 2026-11-22, then 0 until Thursday 2026-11-26.

| # | Phase (class) | Points | Gain | Status |
|---|---|---|---|---|
| 1 | comctl32 v6 activation context: 14 comctl32 units fail "failed to find comctl32.dll in active context" | 1 if easy | 14 units | done 2026-11-22 (#57): no "active context" failures; combo, edit, ipaddress, listbox now finish |
| 2 | New threads do not inherit the creator's activation context (kernel32 fiber, 7 failures) | 1 | 1 unit | |
| 3 | msvcrt.dll access violation after an API returned NULL (lsa, path, clipboard, dde, menu...) | 2 | 6 units | |
| 4 | Timeouts (gdi32 bitmap/dib/metafile, kernel32 console/file...) | 3.5 | 15 units | |
