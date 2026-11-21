# Compatibility status

This page tracks how well the engine runs real Windows programs. It is updated after each weekly CI run.

- Program list: [tests/compat/corpus.json](../tests/compat/corpus.json). The `gate` subset runs on every pull request.
- Wine test DLLs: [tests/winetest/dlls.txt](../tests/winetest/dlls.txt).
- Raw results (logs, screenshots, per-unit output) are in the Summary and artifacts of each [weekly workflow](../../../actions/workflows/weekly.yml) run.

## Program corpus

Last run: 2026-11-21, after PR #27 (SEH). **22 of 30 pass.**

| Program | Status | Cause |
|---|---|---|
| cmd, winemine, 7zip, python, putty64 | pass (gate) | |
| steam-setup, libreoffice, python-bench, node, sqlite, ripgrep, rclone-go, 7zr-bench, plink, jq, putty32, sumatra, everything, npp32-portable, rufus, nasm32 | pass | |
| npp-nsis | pass | Fixed by PR #27 (access violations go to the program's SEH handlers). |
| audacity-wx | fail | Unhandled access violation (null + 0x10) in lib-crashpad-configurer. |
| winmerge-inno-mfc | fail | 32-bit (WoW64) SEH, added in #34. |
| innosetup-delphi | fail | 32-bit (WoW64) SEH, added in #34. |
| 7zip32 | fail | Installs outside `Program Files (x86)`: NSIS reads `ProgramFilesDir` from the registry. #33 adds the Wow6432Node view for 32-bit programs. |
| keepassxc-qt-msi | fail | msiexec deadlock (pipe/sync). |
| winrar | fail | Timeout. |
| coreinfo | fail | Needs instructions the engine lacks (SSE2 path). |
| busybox64 | fail | Download URL returns 403. |

## Wine conformance tests

Last run: 2026-11-21. Clean units (no failures) per DLL. 98.4% of all checks pass.

| DLL | Clean units | Week 1 (2026-11-20) |
|---|---|---|
| kernel32 | 4/33 | 2/33 |
| ntdll | 6/26 | 6/26 |
| user32 | 2/24 | 2/24 |
| gdi32 | 2/14 | 2/14 |
| ucrtbase | 3/8 | 3/8 |

Known causes still open:

- kernel32 `locale` unit runs past `NlsValidateLocale` since #31. Engine `GetLocaleInfo` panicked on flag bits (fixed in #32); some formats are still missing.
- kernel32 `heap` unit: `RtlWalkHeap`, `GetProcessHeaps` and `HeapValidate` are not real yet.
- ntdll `exception` unit now finishes (hung before #30). Still failing: debug registers, single step (TF), WoW64 contexts.
- `PEB->Ldr` was NULL, so `LdrEnumerateLoadedModules` and any program walking the module lists crashed. Fixed in #32.
- ntdll `pipe` and `sync` units deadlock; `threadpool` times out.
- ntdll `file` unit access violations inside the test.
- Missing: `NtAllocateReserveObject`, `NtSetIoCompletionEx`.

## Fixed

| PR | Fix |
|---|---|
| #27 | Hardware access violations reach the program's SEH handlers, also from JIT code. |
| #28 | User APCs (`NtQueueApcThread`, alertable waits, `KiUserApcDispatcher`). |
| #30 | `pop` to memory keeps rsp on a fault (ntdll `exception` unit hung). int3, hlt/cli/sti and unknown opcodes raise SEH exceptions. `HeapReAlloc` keeps blocks in place. |
| #31 | kernelbase loads its locale tables (`locale.nls`, `sortdefault.nls`) for 64-bit programs. |

## Next

1. 32-bit (WoW64) SEH: WinMerge, Inno Setup.
2. `GetCalendarInfoW` and the loader lock.
3. pipe/sync deadlocks and msiexec: KeePassXC.
4. 7-Zip 32-bit install path from the registry.
5. New busybox64 download URL.
