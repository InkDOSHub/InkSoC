# Common programs on InkSoC

Engine bac13a9. Each program is downloaded from its official site and its installer (or the program itself) runs on the engine in Node on GitHub Actions (.github/workflows/compat-apps.yml). "running" = still going at the time limit.

| Program | Group | Result | Where it stopped |
|---|---|---|---|
| chrome | browser | stopped | all threads are waiting for each other (deadlock) |
| firefox | browser | running, 1 window(s) |  |
| obs | media | stopped | not implemented yet: ntdll.dll!NtQuerySecurityObject |
| vlc | media | exit |  |
| audacity | media | running, 1 window(s) |  |
| libreoffice | office | exit |  |
| adobe-reader | office | stopped | image cannot be moved (no relocations) |
| 7zip | tools | exit |  |
| notepadpp | tools | exit |  |
| winrar | tools | exit |  |
| python | tools | exit |  |
| rufus | tools | running, 1 window(s) |  |
| gimp | graphics | exit |  |
| krita | graphics | exit |  |
| blender | graphics | not run |  |
| steam | games | exit |  |
| supertuxkart | games | exit |  |
| luanti | games | stopped | not implemented yet: libc++.dll!__cxa_guard_acquire |

## Most common stops

- 1× all threads are waiting for each other (deadlock)
- 1× not implemented yet: ntdll.dll!NtQuerySecurityObject
- 1× image cannot be moved (no relocations)
- 1× not implemented yet: libc++.dll!__cxa_guard_acquire
- 1× missing: libintl-8.dll
- 1× missing: zlib1.dll
- 1× missing: libzstd.dll
- 1× missing: openal32.dll
- 1× missing: libvorbisfile.dll
- 1× missing: libsqlite3-0.dll
- 1× missing: libfreetype.dll
- 1× missing: libcurl.dll
- 1× missing: libleveldb.dll
- 1× missing: libc++.dll
- 1× missing: libunwind.dll
