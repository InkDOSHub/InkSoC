# Wine startup parity audit

**Audit baseline:** `main` at `425ab35c2905d51cbb107aa483233553253c556a`  
**Bundled Wine version:** 9.0  
**Scope:** process startup and the initialization of `wine-ntdll.dll`, `wine-kernelbase.dll`, and `wine-kernel32.dll`.

This document records verified differences between InkSoC's startup path and the corresponding Wine 9.0 initialization code. It does not change runtime behavior.

## Executive summary

InkSoC deliberately suppresses the `DllMain` initialization queue for the three core Wine modules because they share state with the engine. It then supplies selected state itself: TEB/PEB fields, process parameters, shared user data, NLS tables, and native NT/Win32 call implementations.

That strategy is viable only if every skipped initialization responsibility has an explicit engine equivalent. The current code demonstrates equivalents for some responsibilities, but not all.

## Verified differences

### 1. Core Wine DLL entry points are skipped

In `src/engine/src/winefb.rs`, `wine_module()` removes queued initialization callbacks for `wine-ntdll.dll`, `wine-kernelbase.dll`, and `wine-kernel32.dll` (around lines 233–252). The comment says their own `DllMain` must not run because the modules share engine state.

The engine then applies custom NLS initialization to ntdll and kernelbase. This is a deliberate partial replacement, not full Wine process initialization.

### 2. KernelBase startup has more responsibilities than NLS

Wine 9.0's `dlls/kernelbase/main.c` calls these functions during process attach:

- `IsWow64Process`
- `init_global_data`
- `init_locale`
- `init_startup_info`
- `init_console`

InkSoC has a manual locale-table path in `wine_nls()` and `wine_kernelbase_nls()`, and creates `RTL_USER_PROCESS_PARAMETERS` in `process_parameters()`. However, the audited boot path does not visibly execute the KernelBase startup functions as a group.

A concrete parity risk is command-line state. In Wine 9.0, `init_startup_info()` sets the private `command_lineA` and `command_lineW` pointers used by KernelBase's `GetCommandLineA/W`. InkSoC's `process_parameters()` populates the PEB's process-parameters structure and stores its own command-line pointers, but no corresponding initialization of KernelBase's private static pointers is visible. Direct calls through the Wine KernelBase implementation therefore need a focused test.

Another responsibility is `init_global_data()`, which allocates KernelBase's global-memory handle table. The native engine has its own `GlobalAlloc/GlobalFree` handling, but that does not by itself prove parity for calls that execute the Wine KernelBase implementation.

### 3. Kernel32 process attach is also skipped

Wine 9.0's `dlls/kernel32/kernel_main.c` calls `process_attach()` from `DllMain`. That routine installs an unhandled-exception filter, queries system information, obtains KernelBase global data, and copies startup information.

InkSoC has native equivalents for some related APIs and its own exception and process structures. The exact coverage of the skipped Kernel32 initialization still needs to be mapped function-by-function; it should not be assumed to be covered merely because individual APIs exist.

### 4. NT security calls cannot simply fall back to the Wine PE stub

The compatibility report records OBS stopping at `ntdll.dll!NtSetSecurityObject`.

In Wine 9.0, the PE export is declared as a syscall in `dlls/ntdll/ntdll.spec`. Its Unix-side implementation in `dlls/ntdll/unix/security.c` marshals the security descriptor and sends a `set_security_object` request to wineserver.

InkSoC does not provide Wine's wineserver. Consequently, routing this missing native call into the PE stub is not a safe general fallback. The appropriate long-term fix is a native implementation backed by InkSoC's own object/security model, or an explicit and accurate unsupported status until that model exists.

## Current InkSoC implementation points

- `src/engine/src/lib.rs`: process boot, TEB/PEB setup, process parameters, shared data, and initial DLL queue.
- `src/engine/src/winefb.rs`: Wine module loading, skipped core `DllMain` callbacks, ntdll redirection, and manual NLS initialization.
- `src/engine/src/ntsys/security.rs`: current token/security handling; `NtSetSecurityObject` is not handled.
- `src/engine/src/kernel32.rs`: native `GetCommandLineA/W`, `GetStartupInfoA/W`, and global/local allocation paths.

## Recommended implementation sequence

1. Add a startup-parity test executable that imports APIs directly from `kernelbase.dll` and `kernel32.dll`, not only through forwarded `kernel32` names.
2. Verify `GetCommandLineA/W` return valid, stable pointers and correct contents for both direct KernelBase and native Kernel32 imports.
3. Verify `GetStartupInfoA/W`, standard handles, and console startup fields against the values constructed in `RTL_USER_PROCESS_PARAMETERS`.
4. Verify `GlobalAlloc/GlobalFree` through both native and Wine KernelBase entry points.
5. Map every skipped core `DllMain` responsibility to an engine implementation or a deliberately unsupported capability.
6. Implement missing NT security behavior only after defining the engine's object and security-descriptor model; do not treat a wineserver-dependent PE syscall stub as a substitute.

## Acceptance criteria for the next runtime PR

- No core `DllMain` is re-enabled wholesale as a shortcut.
- Each newly emulated startup field has a documented source, owner, and test.
- Tests cover direct KernelBase imports as well as Kernel32 forwarding.
- Existing compatibility tests for VLC, LibreOffice, 7-Zip, Notepad++, and WinRAR remain passing.
- OBS is only marked fixed if it progresses beyond `NtSetSecurityObject` and completes a defined functional test, not merely if the installer exits.

## Wine 9.0 source references

- `kernelbase` process attach: https://github.com/wine-mirror/wine/blob/wine-9.0/dlls/kernelbase/main.c
- `kernelbase` command-line initialization and getters: https://github.com/wine-mirror/wine/blob/wine-9.0/dlls/kernelbase/process.c
- `kernelbase` global memory initialization: https://github.com/wine-mirror/wine/blob/wine-9.0/dlls/kernelbase/memory.c
- `kernel32` process attach: https://github.com/wine-mirror/wine/blob/wine-9.0/dlls/kernel32/kernel_main.c
- NT security syscall declaration: https://github.com/wine-mirror/wine/blob/wine-9.0/dlls/ntdll/ntdll.spec
- NT security Unix-side implementation: https://github.com/wine-mirror/wine/blob/wine-9.0/dlls/ntdll/unix/security.c
