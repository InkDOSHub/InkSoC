# Compatibility status

This page tracks how well the engine runs real Windows programs. It is updated after each weekly CI run.

- Program list: [tests/compat/corpus.json](../tests/compat/corpus.json). The `gate` subset runs on every pull request.
- Wine test DLLs: [tests/winetest/dlls.txt](../tests/winetest/dlls.txt).
- Raw results (logs, screenshots, per-unit output) are in the Summary and artifacts of each [weekly workflow](../../../actions/workflows/weekly.yml) run.

## Program corpus

Last run: 2026-11-21, after PR #37 (Wine DLLs on demand, x87 BCD). **23 of 30 pass** (22/29 before; libreoffice passes again).

| Program | Status | Cause |
|---|---|---|
| cmd, winemine, 7zip, python, putty64 | pass (gate) | |
| steam-setup, python-bench, node, sqlite, ripgrep, rclone-go, 7zr-bench, plink, jq, putty32, sumatra, everything, npp32-portable, rufus, nasm32 | pass | |
| npp-nsis | pass | Fixed by PR #27 (access violations go to the program's SEH handlers). |
| 7zip32 | pass | Fixed by #33/#34 (Wow6432Node registry view, KEY_WOW64_64KEY honored). |
| libreoffice | pass | 400 MB MSI. |
| innosetup-delphi | fail | Installs since #37/#38. Compil32 now stops at `BCryptImportKeyPair` (RSA keys are not implemented, 0xC0000002). |
| winmerge-inno-mfc | fail | Inno installer; same class as innosetup-delphi. |
| keepassxc-qt-msi | fail | Error 1627 inside msi's 32-bit custom action server. |
| audacity-wx | fail | Unhandled access violation (null + 0x10) in lib-crashpad-configurer. |
| winrar | fail | Timeout. |
| coreinfo | fail | Needs instructions the engine lacks (SSE2 path). |
| busybox64 | fail | Download URL returns 403. |

## Wine conformance tests

Last run: 2026-11-21, after #37. Clean units (no failures) per DLL. 98.5% of all 7.38 million checks pass; 31/192 units clean.

| DLL | Clean units | Checks passed | Week 1 (2026-11-20) |
|---|---|---|---|
| kernel32 | 5/33 | 92.0% | 2/33 |
| ntdll | 6/26 | 100.0% (1307 failures) | 6/26 |
| user32 | 3/24 | 99.6% | 2/24 |
| gdi32 | 2/14 | 94.6% | 2/14 |
| ucrtbase | 3/8 | 100.0% (55 failures) | 3/8 |
| msvcrt | 4/13 | 99.5% | |
| advapi32 | 2/12 | 99.4% | |
| ole32 | 3/14 | 92.5% | |
| shell32 | 2/19 | 89.2% | |
| comctl32 | 1/29 | 97.1% | |

Known causes still open:

- Crashes from APIs that return NULL where Windows returns data (clipboard, dde, om, path, menu units then crash in `strcmp`/`strlen`).
- GDI handle validation (gdiobj, cursoricon units: access violations on invalid handles).
- kernel32 `heap` unit: `RtlWalkHeap`, `GetProcessHeaps` and `HeapValidate` are not real yet.
- ntdll `sync` (keyed events) and `pipe` deadlocks; `threadpool` times out.
- ntdll `exception`: debug registers, single step (TF), WoW64 contexts.

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

## Next

1. bcrypt RSA keys (`BCryptImportKeyPair`): Inno Setup, WinMerge, any signed-update check.
2. msi 32-bit custom actions (1627): KeePassXC.
3. NULL-returning APIs that crash test units (clipboard, dde, path, menu).
4. GDI handle validation.
5. Heap walk: `RtlWalkHeap`, `GetProcessHeaps`, `HeapValidate`.
6. New busybox64 download URL.
