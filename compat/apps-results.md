# Common programs on InkSoC

Engine c151602. Each program is downloaded from its official site and its installer (or the program itself) runs on the engine in Node on GitHub Actions (.github/workflows/compat-apps.yml). "running" = still going at the time limit.

| Program | Group | Result | Where it stopped |
|---|---|---|---|
| chrome | browser | stopped | all threads are waiting for each other (deadlock) |
| firefox | browser | not run |  |
| obs | media | finished, 2079 files written |  |
| vlc | media | finished, 592 files written |  |
| audacity | media | not run |  |
| libreoffice | office | not run |  |
| adobe-reader | office | stopped | image cannot be moved (no relocations) |
| 7zip | tools | finished, 107 files written |  |
| notepadpp | tools | finished, 239 files written |  |
| winrar | tools | finished, 28 files written |  |
| python | tools | exit code 2147746291 |  |
| rufus | tools | not run |  |
| gimp | graphics | exit code 3221225781 |  |
| krita | graphics | finished, 3459 files written |  |
| blender | graphics | not run |  |
| steam | games | finished, 36 files written |  |
| supertuxkart | games | finished, 5554 files written |  |
| luanti | games | exit code 3 |  |

## Most common stops

- 1× all threads are waiting for each other (deadlock)
- 1× image cannot be moved (no relocations)
