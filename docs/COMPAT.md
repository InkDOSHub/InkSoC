# Compatibility status

This page tracks how well the engine runs real Windows programs. It is updated after each weekly CI run.

- Program list: [tests/compat/corpus.json](../tests/compat/corpus.json). The `gate` subset runs on every pull request.
- Wine test DLLs: [tests/winetest/dlls.txt](../tests/winetest/dlls.txt).
- Raw results (logs, screenshots, per-unit output) are in the Summary and artifacts of each [weekly workflow](../../../actions/workflows/weekly.yml) run.

## Program corpus

Last run: 2026-11-22, after PR #42 (input contexts). **24 of 30 pass.** Inno Setup passes since #40.

| Program | Status | Cause |
|---|---|---|
| cmd, winemine, 7zip, python, putty64 | pass (gate) | |
| steam-setup, python-bench, node, sqlite, ripgrep, rclone-go, 7zr-bench, plink, jq, putty32, sumatra, everything, npp32-portable, rufus | pass | |
| nasm32 | pass | |
| npp-nsis | pass | Fixed by PR #27 (access violations go to the program's SEH handlers). |
| 7zip32 | pass | Fixed by #33/#34 (Wow6432Node registry view, KEY_WOW64_64KEY honored). |
| libreoffice | pass | 400 MB MSI. |
| innosetup-delphi | pass | Fixed by #37 (x87 BCD), #38 (child program folder), #40 (bcrypt RSA keys). |
| winmerge-inno-mfc | fail | The installer finishes; WinMergeU.exe, which it then starts, crashes at its first window (rip inside imm32's data section: a bad call target). |
| keepassxc-qt-msi | fail | Error 1627: msi's 32-bit custom action DLL thread never runs. |
| audacity-wx | fail | Unhandled access violation (null + 0x10) in lib-crashpad-configurer. |
| winrar | fail | Timeout. |
| coreinfo | fail | "Error getting processor information" (processor topology queries). |
| busybox64 | fail | Download URL returns 403. |

## Wine conformance tests

Last run: 2026-11-22, after #42 (comctl32, gdi32, shell32 and user32 re-run; the other DLLs as after #41). Clean units (no failures) per DLL. 98.5% of 7.39 million checks pass; **32/192 units clean** (31 before #41). comctl32 misc no longer stops with an engine panic. With the COM classes registered, more units get further: advapi32 now runs 4244 checks (321 before), ole32 and shell32 run about 1000 more.

| DLL | Clean units | Checks passed | Week 1 (2026-11-20) |
|---|---|---|---|
| kernel32 | 5/33 | 92.0% | 2/33 |
| ntdll | 6/26 | 100.0% (1307 failures) | 6/26 |
| user32 | 3/24 | 99.6% | 2/24 |
| gdi32 | 2/14 | 94.6% | 2/14 |
| ucrtbase | 3/8 | 100.0% (55 failures) | 3/8 |
| msvcrt | 4/13 | 99.5% | |
| advapi32 | 2/12 | 83.0% of 4244 (99.4% of 321 before) | |
| ole32 | 3/14 | 92.7% | |
| shell32 | 3/19 | 87.9% | |
| comctl32 | 1/29 | 96.9% of 19593 (590 more checks run) | |

Known causes still open:

- Crashes from APIs that return NULL where Windows returns data (clipboard, dde, om, path, menu units then crash in `strcmp`/`strlen`).
- kernel32 `heap` unit: `RtlWalkHeap`, `GetProcessHeaps` and `HeapValidate` are not real yet.
- ntdll `sync` (keyed events) and `pipe` deadlocks; `threadpool` times out.
- advapi32: `SystemFunction001/007/032` (cryptsp, cryptbase) not implemented.

## Fixed

| PR | Fix |
|---|---|
| #27 | Hardware access violations reach the program's SEH handlers, also from JIT code. |
| #28 | User APCs (`NtQueueApcThread`, alertable waits, `KiUserApcDispatcher`). |
| #30 | `pop` to memory keeps rsp on a fault (ntdll `exception` unit hung). int3, hlt/cli/sti and unknown opcodes raise SEH exceptions. `HeapReAlloc` keeps blocks in place. |
| #31 | kernelbase loads its locale tables (`locale.nls`, `sortdefault.nls`) for 64-bit programs. |
| #32 | `PEB->Ldr` module lists; `GetLocaleInfo` no longer panics on flag bits. |
| #33 | Wow6432Node registry view for 32-bit programs (`ProgramFilesDir` = `Program Files (x86)`). |
| #34 | 32-bit (WoW64) exceptions reach the program's SEH handlers; `KEY_WOW64_64KEY`/`KEY_WOW64_32KEY` honored. |
| #36 | Alertable pipe waits; `RtlWaitOnAddress` size check; 32-bit msiexec for .msi custom actions; `ConnectNamedPipe` after a waiting client. |
| #37 | 41 more Wine DLLs per architecture loaded on demand (wtsapi32, cfgmgr32, DirectInput, XInput, MSVC 2010/2013 runtimes...); 32-bit Wine debug output; x87 `FBLD`/`FBSTP` (Delphi). |
| #38 | A child process gets its own program folder (image path, DLL search, `GetModuleFileName`). |
| #40 | bcrypt RSA and ECC keys without GnuTLS: import, export, RSA PKCS#1/PSS and ECDSA P-256/384/521 verification (Inno Setup). |
| #41 | Registrations that wineboot writes, taken from the Wine DLLs' `.rgs` resources: COM classes (`CoCreateInstance` no longer fails with `REGDB_E_CLASSNOTREG`), interfaces, type libraries, MIME types, time zones, NLS tables. |
| #42 | win32u input contexts (imm32's default context was NULL); empty rectangle intersections no longer panic in `ExtTextOut`. |
| #44 | The whole GDI handle table is committed: gdi32's lookup of an unused or bogus handle no longer faults (gdiobj, cursoricon, shlfileop). |

## Next

1. WinMergeU crash at first window (bad call target in imm32's data).
2. msi 32-bit custom actions (1627): KeePassXC.
3. NULL-returning APIs that crash test units (clipboard, dde, path, menu).
4. Heap walk: `RtlWalkHeap`, `GetProcessHeaps`, `HeapValidate`.
5. coreinfo processor topology; new busybox64 download URL.
