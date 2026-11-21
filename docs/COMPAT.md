# Compatibility status

This page tracks how well the engine runs real Windows programs. It is updated after each weekly CI run.

- Program list: [tests/compat/corpus.json](../tests/compat/corpus.json). The `gate` subset runs on every pull request.
- Wine test DLLs: [tests/winetest/dlls.txt](../tests/winetest/dlls.txt).
- Raw results (logs, screenshots, per-unit output) are in the Summary and artifacts of each [weekly workflow](../../../actions/workflows/weekly.yml) run.

## Program corpus

Last run: 2026-11-21, after PR #34 (32-bit SEH). **22 of 29 finished pass**; libreoffice (passed in the previous run) was still running.

| Program | Status | Cause |
|---|---|---|
| cmd, winemine, 7zip, python, putty64 | pass (gate) | |
| steam-setup, python-bench, node, sqlite, ripgrep, rclone-go, 7zr-bench, plink, jq, putty32, sumatra, everything, npp32-portable, rufus, nasm32 | pass | |
| npp-nsis | pass | Fixed by PR #27 (access violations go to the program's SEH handlers). |
| 7zip32 | pass | Fixed by #33/#34 (Wow6432Node registry view, KEY_WOW64_64KEY honored). |
| libreoffice | pending | Passed in the previous run; still running at write time. |
| audacity-wx | fail | Unhandled access violation (null + 0x10) in lib-crashpad-configurer. |
| winmerge-inno-mfc | fail | Installer now exits 0 (#34), but WinMergeU.exe is not at the expected path. |
| innosetup-delphi | fail | Installer now exits 0 (#34), but Compil32.exe is not at the expected path. |
| keepassxc-qt-msi | fail | msiexec deadlock (pipe/sync). |
| winrar | fail | Timeout. |
| coreinfo | fail | Needs instructions the engine lacks (SSE2 path). |
| busybox64 | fail | Download URL returns 403. |

## Wine conformance tests

Last run: 2026-11-21, after #34. Clean units (no failures) per DLL. 98.5% of all checks pass (98.4% before).

| DLL | Clean units | Checks passed | Week 1 (2026-11-20) |
|---|---|---|---|
| kernel32 | 5/33 | 92.0% | 2/33 |
| ntdll | 6/26 | 100.0% (1307 failures) | 6/26 |
| user32 | 3/24 | 99.6% | 2/24 |
| gdi32 | 2/14 | 94.6% | 2/14 |
| ucrtbase | 3/8 | 100.0% (55 failures) | 3/8 |

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
| #32 | `PEB->Ldr` module lists; `GetLocaleInfo` no longer panics on flag bits. |
| #33 | Wow6432Node registry view for 32-bit programs (`ProgramFilesDir` = `Program Files (x86)`). |
| #34 | 32-bit (WoW64) exceptions reach the program's SEH handlers; `KEY_WOW64_64KEY`/`KEY_WOW64_32KEY` honored. |

## Next

1. pipe/sync deadlocks and msiexec: KeePassXC.
2. Inno Setup install location (WinMerge, Inno Setup).
3. Heap walk: `RtlWalkHeap`, `GetProcessHeaps`, `HeapValidate`.
4. `GetCalendarInfoW` and the loader lock.
5. New busybox64 download URL.
