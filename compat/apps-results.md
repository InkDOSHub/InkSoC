# Common programs on InkSoC

Engine 0aaa698. Each program is downloaded from its official site and its installer (or the program itself) runs on the engine in Node on GitHub Actions (.github/workflows/compat-apps.yml). "running" = still going at the time limit.

| Program | Group | Result | Where it stopped |
|---|---|---|---|
| chrome | browser | stopped | all threads are waiting for each other (deadlock) |
| firefox | browser | running, 1 window(s) |  |
| obs | media | stopped | access violation at 0x2 (rip 0x1001b26f) [wine32-advapi32.dll+0x1b26f] |
| vlc | media | finished, 592 files written |  |
| audacity | media | running, 1 window(s) |  |
| libreoffice | office | not run |  |
| adobe-reader | office | stopped | image cannot be moved (no relocations) |
| 7zip | tools | finished, 107 files written |  |
| notepadpp | tools | finished, 239 files written |  |
| winrar | tools | finished, 28 files written |  |
| python | tools | exit code 2147746291 |  |
| rufus | tools | running, 1 window(s) |  |
| gimp | graphics | exit code 3221225781 |  |
| krita | graphics | finished, 3459 files written |  |
| blender | graphics | not run |  |
| steam | games | finished, 36 files written |  |
| supertuxkart | games | finished, 5554 files written |  |
| luanti | games | stopped | access violation at 0x86 (rip 0x7ff00143b66c) [in wine-kernelbase.dll+0x3b66c] |

## Most common stops

- 1× all threads are waiting for each other (deadlock)
- 1× access violation at 0x2
- 1× image cannot be moved (no relocations)
- 1× access violation at 0x86
