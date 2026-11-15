// Our own kernel32: the functions a normal program needs to start and run,
// implemented natively (wasm) against the engine. Single thread for now.

use crate::{console, pe, Engine, PEB};
use std::collections::HashMap;

/// Returned by functions that already set rip themselves (no `ret`).
pub const NO_RETURN: u64 = u64::MAX - 0x5A5A;

const PROCESS_HEAP: u64 = 0x0000_0000_00AA_0000;
const ERROR_ENVVAR_NOT_FOUND: u32 = 203;

impl Engine {
    pub(crate) fn zero(&mut self, a: u64, n: u64) {
        for i in 0..n {
            self.w8(a + i, 0);
        }
    }

    fn module_by_handle(&self, h: u64) -> Option<usize> {
        if h == 0 {
            return self.s.modules.iter().position(|m| m.base == self.s.exe_base);
        }
        self.s.modules.iter().position(|m| m.base == h)
    }

    fn load_library(&mut self, name: &str) -> u64 {
        let before = self.s.init_queue.len();
        // a DLL nobody provides: fail like Windows (programs probe for optional DLLs)
        let n = pe::norm(name);
        let known = pe::find_module(self, &n).is_some()
            || pe::NATIVE_DLLS.contains(&n.as_str())
            || n.starts_with("api-ms-win-")
            || n.starts_with("ext-ms-")
            || self.s.files.contains_key(&n)
            || self.s.files.contains_key(&format!("wine-{}", n))
            || crate::files::key_of(name).map_or(false, |k| self.s.files.contains_key(&k))
            || self.s.dll_dirs.iter().any(|d| self.s.files.contains_key(&format!("{}\\{}", d, n)));
        if !known {
            self.set_last_error(126);
            return 0;
        }
        match pe::load_by_name(self, name) {
            Ok(i) => {
                let h = self.s.modules[i].base;
                if self.s.init_queue.len() > before {
                    // run the new DllMain(s), then return h to the caller
                    let pending: Vec<(u64, u64)> = self.s.init_queue.drain(before..).collect();
                    self.run_inits(pending, 0, Some(h));
                    return NO_RETURN;
                }
                h
            }
            Err(m) => {
                console(&format!("[inksoc] LoadLibrary {}: {}\n", name, m));
                self.set_last_error(126);
                0
            }
        }
    }

    pub(crate) fn write_out(&mut self, h: u64, p: u64, n: u64) -> bool {
        if h != 1 && h != 2 {
            return false;
        }
        let mut v = vec![0u8; n.min(1 << 20) as usize];
        if self.mmu.read_bytes(p, &mut v).is_err() {
            return false;
        }
        console(&String::from_utf8_lossy(&v));
        true
    }

    fn get_system_info(&mut self, a: u64) {
        self.zero(a, 48);
        self.w16(a, 9); // AMD64
        self.w32(a + 4, 4096);
        self.w64(a + 8, 0x10000);
        self.w64(a + 16, 0x7FFF_FFFE_FFFF);
        self.w64(a + 24, 1);
        self.w32(a + 32, self.s.cores); // processors: one per engine core
        self.w32(a + 36, 8664);
        self.w32(a + 40, 0x10000);
        self.w16(a + 44, 6);
    }
}

pub fn call(e: &mut Engine, name: &str) -> Option<u64> {
    let a0 = e.arg(0);
    let r = match name {
        // ---- console & process ----
        "GetStdHandle" => match a0 as u32 as i32 {
            -10 => e.s.std[0],
            -11 => e.s.std[1],
            -12 => e.s.std[2],
            _ => u64::MAX,
        },
        "WriteConsoleA" => {
            let (p, n, w) = (e.arg(1), e.arg(2) as u32 as u64, e.arg(3));
            if e.write_out(a0, p, n) {
                e.w32(w, n as u32);
                1
            } else {
                e.set_last_error(6);
                0
            }
        }
        "WriteConsoleW" => {
            let (p, n, w) = (e.arg(1), e.arg(2) as u32 as u64, e.arg(3));
            let mut v = vec![0u16; n as usize];
            for (i, c) in v.iter_mut().enumerate() {
                *c = e.mmu.read16(p + 2 * i as u64).unwrap_or(63);
            }
            console(&String::from_utf16_lossy(&v));
            e.w32(w, n as u32);
            1
        }
        "GetConsoleMode" => {
            if a0 == 0 || a0 > 3 {
                return Some(0);
            }
            let a = e.arg(1);
            e.w32(a, if a0 == 3 { 0x1F7 } else { 7 });
            1
        }
        "SetConsoleMode" | "SetConsoleCtrlHandler" | "SetConsoleOutputCP" | "SetConsoleCP" => 1,
        "GetConsoleCP" | "GetConsoleOutputCP" => 65001,
        "OutputDebugStringA" => {
            let s = e.read_cstr(a0, 4096);
            console(&format!("[debug] {}", s));
            0
        }
        "ExitProcess" => {
            e.exit_process(a0 as u32);
            return Some(NO_RETURN);
        }
        "TerminateProcess" => {
            let code = e.arg(1) as u32;
            match e.s.fs.handles.get(&a0) {
                Some(crate::files::Handle::Process { pid }) => {
                    let p = *pid;
                    if let Some(i) = e.s.pinfo.get_mut(&p) {
                        if i.exit.is_none() {
                            i.exit = Some(code);
                        }
                    }
                    for t in e.s.sched.th.iter_mut() {
                        if t.wpid == p {
                            t.st = crate::sched::St::Done;
                        }
                    }
                    1
                }
                _ => {
                    e.exit_process(code);
                    return Some(NO_RETURN);
                }
            }
        }
        "GetCurrentProcessId" => 4,
        "GetCurrentProcess" => u64::MAX,
        "GetCurrentThread" => u64::MAX - 1,
        "GetCommandLineA" => e.s.cmdline.0,
        "GetCommandLineW" => e.s.cmdline.1,
        "GetEnvironmentVariableA" | "GetEnvironmentVariableW" => {
            let wide = name.ends_with('W');
            let n = if wide { e.read_wstr(a0, 32767) } else { e.read_cstr(a0, 32767) };
            let (buf, size) = (e.arg(1), e.arg(2) as u32 as u64);
            match e.env_get(&n) {
                None => {
                    e.set_last_error(ERROR_ENVVAR_NOT_FOUND);
                    0
                }
                Some(v) => {
                    let len = if wide { v.encode_utf16().count() } else { v.len() } as u64;
                    if buf == 0 || size <= len {
                        len + 1
                    } else if wide {
                        for (k, c) in v.encode_utf16().chain([0]).enumerate() {
                            e.w16(buf + 2 * k as u64, c);
                        }
                        len
                    } else {
                        let _ = e.mmu.write_bytes(buf, v.as_bytes());
                        e.w8(buf + len, 0);
                        len
                    }
                }
            }
        }
        "GetStartupInfoA" | "GetStartupInfoW" => {
            // STARTUPINFO is 104 bytes on x64. The destination is checked with a
            // read-only query before anything is stored; the structure is then
            // written in one piece.
            if !e.mmu.can_write_range(a0, 104) {
                e.fault(a0);
                0
            } else {
                let wide = name.ends_with('W');
                let mut s = [0u8; 104];
                s[0..4].copy_from_slice(&104u32.to_le_bytes()); // cb
                let p = e.r64(PEB + 0x20);
                if p != 0 {
                    let title = e.r64(p + 0xB8);   // WindowTitle.Buffer (UTF-16)
                    let desktop = e.r64(p + 0xC8); // Desktop.Buffer (UTF-16)
                    let (lp_desktop, lp_title) = if wide {
                        (desktop, title)
                    } else {
                        (startup_ansi(e, desktop), startup_ansi(e, title))
                    };
                    s[16..24].copy_from_slice(&lp_desktop.to_le_bytes());
                    s[24..32].copy_from_slice(&lp_title.to_le_bytes());
                    for (dst, src) in [
                        (32, 0x88), // dwX
                        (36, 0x8C), // dwY
                        (40, 0x90), // dwXSize
                        (44, 0x94), // dwYSize
                        (48, 0x98), // dwXCountChars
                        (52, 0x9C), // dwYCountChars
                        (56, 0xA0), // dwFillAttribute
                        (60, 0xA4), // dwFlags
                    ] {
                        let value = e.r32(p + src);
                        s[dst..dst + 4].copy_from_slice(&value.to_le_bytes());
                    }
                    let show_window = e.r16(p + 0xA8);
                    s[64..66].copy_from_slice(&show_window.to_le_bytes()); // wShowWindow
                    let stdin = e.r64(p + 0x20);
                    let stdout = e.r64(p + 0x28);
                    let stderr = e.r64(p + 0x30);
                    s[80..88].copy_from_slice(&stdin.to_le_bytes());  // hStdInput
                    s[88..96].copy_from_slice(&stdout.to_le_bytes()); // hStdOutput
                    s[96..104].copy_from_slice(&stderr.to_le_bytes()); // hStdError
                }
                let _ = e.mmu.write_bytes(a0, &s);
                0
            }
        }
        "IsDebuggerPresent" => 0,
        "SetDllDirectoryW" | "SetDllDirectoryA" => {
            let p = if a0 == 0 {
                String::new()
            } else if name.ends_with('W') {
                e.read_wstr(a0, 32767)
            } else {
                e.read_cstr(a0, 32767)
            };
            if let Some(k) = crate::files::key_of(&p) {
                if !e.s.dll_dirs.contains(&k) {
                    e.s.dll_dirs.push(k);
                }
            }
            1
        }
        "SetDefaultDllDirectories" => 1,
        "SetFileApisToOEM" | "SetFileApisToANSI" => 0,
        "AreFileApisANSI" => 1,
        "GetConsoleCursorInfo" => {
            let p = e.arg(1);
            if a0 == 0 || a0 > 3 || p == 0 {
                e.set_last_error(6);
                return Some(0);
            }
            e.w32(p, 25);
            e.w32(p + 4, 1);
            1
        }
        "SetConsoleCursorInfo"
        | "SetConsoleScreenBufferSize"
        | "SetConsoleWindowInfo"
        | "FillConsoleOutputCharacterW"
        | "FillConsoleOutputCharacterA"
        | "FillConsoleOutputAttribute" => {
            // a count of written cells (fill functions): report all done
            if name.starts_with("Fill") {
                let (n, out) = (e.arg(2), e.arg(4));
                e.w32(out, n as u32);
            }
            1
        }
        "GetConsoleScreenBufferInfo" => {
            // the page's terminal: 120 x 30 (only for the console handles)
            let p = e.arg(1);
            if a0 == 0 || a0 > 3 || p == 0 {
                e.set_last_error(6);
                return Some(0);
            }
            for (o, v) in [(0u64, 120u16), (2, 9000), (4, 0), (6, 0), (8, 7), (10, 0), (12, 0), (14, 119), (16, 29), (18, 120), (20, 30)] {
                e.w16(p + o, v);
            }
            1
        }
        "GetProcessAffinityMask" => {
            let (p, s) = (e.arg(1), e.arg(2));
            let m = (1u64 << e.s.cores) - 1;
            e.w64(p, m);
            e.w64(s, m);
            1
        }
        "SetProcessAffinityMask" | "SetThreadAffinityMask" => 1,
        "GetLargePageMinimum" => 0,
        // user32 text helpers (no windows needed)
        "CharPrevExA" => {
            let (start, cur) = (e.arg(1), e.arg(2));
            if cur > start {
                cur - 1
            } else {
                start
            }
        }
        "CharPrevA" | "CharPrevW" => {
            let cur = e.arg(1);
            let step = if name.ends_with('W') { 2 } else { 1 };
            if cur > a0 {
                cur - step
            } else {
                a0
            }
        }
        "CharNextA" => {
            if e.r8(a0) != 0 {
                a0 + 1
            } else {
                a0
            }
        }
        "CharNextW" => {
            if e.r16(a0) != 0 {
                a0 + 2
            } else {
                a0
            }
        }
        "CharUpperW" | "CharLowerW" => {
            let up = name == "CharUpperW";
            let conv = |c: u16| -> u16 {
                let ch = char::from_u32(c as u32).unwrap_or('?');
                let s: String = if up { ch.to_uppercase().collect() } else { ch.to_lowercase().collect() };
                s.encode_utf16().next().unwrap_or(c)
            };
            if a0 >> 16 == 0 {
                conv(a0 as u16) as u64
            } else {
                let mut i = 0;
                loop {
                    let c = e.r16(a0 + 2 * i);
                    if c == 0 {
                        break;
                    }
                    e.w16(a0 + 2 * i, conv(c));
                    i += 1;
                }
                a0
            }
        }
        "CharUpperBuffW" | "CharLowerBuffW" | "CharUpperBuffA" | "CharLowerBuffA" => {
            // (text, length): changed in place, returns the length (Wine's needs NLS tables the 64-bit side lacks)
            let (up, wide, n) = (name.starts_with("CharUpper"), name.ends_with('W'), e.arg(1) as u32 as u64);
            for i in 0..n {
                if wide {
                    let c = e.r16(a0 + 2 * i);
                    let ch = char::from_u32(c as u32).unwrap_or('?');
                    let s: String = if up { ch.to_uppercase().collect() } else { ch.to_lowercase().collect() };
                    e.w16(a0 + 2 * i, s.encode_utf16().next().filter(|_| s.chars().count() == 1).unwrap_or(c));
                } else {
                    let c = e.r8(a0 + i);
                    e.w8(a0 + i, if up { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() });
                }
            }
            n
        }
        "IsCharAlphaW"
        | "IsCharAlphaNumericW"
        | "IsCharUpperW"
        | "IsCharLowerW"
        | "IsCharSpaceW"
        | "IsCharAlphaA"
        | "IsCharAlphaNumericA"
        | "IsCharUpperA"
        | "IsCharLowerA" => {
            let ch = char::from_u32(if name.ends_with('A') { a0 as u8 as u32 } else { a0 as u16 as u32 }).unwrap_or('\0');
            (match &name[6..name.len() - 1] {
                "Alpha" => ch.is_alphabetic(),
                "AlphaNumeric" => ch.is_alphanumeric(),
                "Upper" => ch.is_uppercase(),
                "Lower" => ch.is_lowercase(),
                _ => ch.is_whitespace(),
            }) as u64
        }
        "GetActiveProcessorGroupCount" | "GetMaximumProcessorGroupCount" => 1,
        "GetActiveProcessorCount" | "GetMaximumProcessorCount" => e.s.cores as u64,
        // advapi32: the process token is ntsys.rs's one user (Windows Installer reads its SID);
        // no privileges to turn on
        "LookupPrivilegeValueW" | "LookupPrivilegeValueA" | "AdjustTokenPrivileges" => {
            e.set_last_error(5);
            0
        }
        "GetProcessTimes" | "GetThreadTimes" => {
            let h = e.arg(0);
            let Some((c, u)) = e.cpu_times(h, name == "GetProcessTimes") else {
                e.set_last_error(6);
                return Some(0);
            };
            for (i, v) in [(1, c), (2, 0), (3, 0), (4, u)] {
                let p = e.arg(i);
                e.w64(p, v);
            }
            1
        }
        "SetUnhandledExceptionFilter" | "UnhandledExceptionFilter" => 0,
        "GetVersion" => (19045 << 16) | 10,

        // ---- time ----
        "GetTickCount" => crate::now_ms() as u64 & 0xFFFF_FFFF,
        "GetTickCount64" => crate::now_ms() as u64,
        "QueryPerformanceFrequency" => {
            e.w64(a0, 10_000_000);
            1
        }
        "QueryPerformanceCounter" => {
            let v = (crate::now_ms() * 10_000.0) as u64;
            e.w64(a0, v);
            1
        }
        "GetSystemTimeAsFileTime" | "GetSystemTimePreciseAsFileTime" => {
            let ft = crate::filetime_now();
            e.w64(a0, ft);
            0
        }

        // ---- system ----
        "GetSystemInfo" | "GetNativeSystemInfo" => {
            e.get_system_info(a0);
            0
        }
        "GlobalMemoryStatusEx" => {
            // the program sees the memory target, not the real RAM pool
            let total = e.mmu.sh.target_bytes;
            let free = total.saturating_sub(e.mmu.sh.committed);
            e.w32(a0 + 4, (e.mmu.sh.committed * 100 / total.max(1)) as u32);
            e.w64(a0 + 8, total);
            e.w64(a0 + 16, free);
            e.w64(a0 + 24, total);
            e.w64(a0 + 32, free);
            e.w64(a0 + 40, 0x7FFF_FFFE_0000);
            e.w64(a0 + 48, 0x7FFF_FFFE_0000 - e.mmu.sh.committed);
            e.w64(a0 + 56, 0);
            1
        }
        "IsProcessorFeaturePresent" => matches!(a0, 6 | 10 | 3 | 8 | 12 | 20) as u64, // SSE, SSE2, MMX, RDTSC, NX, ...
        "GetLastError" => e.r32(e.cpu.gs_base + 0x68) as u64,
        "SetLastError" => {
            e.set_last_error(a0 as u32);
            0
        }

        // ---- TLS / FLS (64 slots in the TEB) ----
        "TlsAlloc" | "FlsAlloc" => match (0..64).find(|i| e.s.tls_used & (1 << i) == 0) {
            Some(i) => {
                e.s.tls_used |= 1 << i;
                e.w64(e.cpu.gs_base + 0x1480 + 8 * i, 0);
                i
            }
            None => 0xFFFF_FFFF,
        },
        "TlsFree" | "FlsFree" => {
            if a0 < 64 {
                e.s.tls_used &= !(1 << a0);
            }
            1
        }
        "TlsGetValue" | "FlsGetValue" => {
            e.set_last_error(0);
            if a0 < 64 {
                e.r64(e.cpu.gs_base + 0x1480 + 8 * a0)
            } else {
                0
            }
        }
        "TlsSetValue" | "FlsSetValue" => {
            let v = e.arg(1);
            if a0 < 64 {
                e.w64(e.cpu.gs_base + 0x1480 + 8 * a0, v);
                1
            } else {
                0
            }
        }

        // ---- locks: one thread for now, so they only need to exist ----

        // ---- heap ----
        "GetProcessHeap" | "HeapCreate" => PROCESS_HEAP,
        "HeapDestroy" => 1,
        "HeapAlloc" => {
            let (f, n) = (e.arg(1), e.arg(2));
            e.heap_alloc(n, f & 8 != 0)
        }
        "HeapFree" => {
            let p = e.arg(2);
            (p == 0 || e.heap_free(p)) as u64
        }
        "HeapReAlloc" => {
            let (f, p, n) = (e.arg(1), e.arg(2), e.arg(3));
            e.heap_realloc(p, n, f & 8 != 0)
        }
        "HeapSize" => {
            let p = e.arg(2);
            e.heap_size(p)
        }
        "LocalAlloc" | "GlobalAlloc" => {
            let n = e.arg(1);
            e.heap_alloc(n, a0 & 0x40 != 0)
        }
        "LocalFree" | "GlobalFree" => {
            if a0 != 0 {
                e.heap_free(a0);
            }
            0
        }

        // ---- virtual memory ----
        "VirtualAlloc" => {
            let size = e.arg(1);
            let kind = e.arg(2);
            let reserve = kind & 0x2000 != 0;
            let commit = kind & 0x1000 != 0;
            if a0 == 0 {
                // a NULL base: the size goes up to whole pages, the base on the 64 KB granularity
                let made = if commit { e.alloc_pages(size) } else { e.reserve_pages(size) };
                match made {
                    Some(a) => a,
                    None => {
                        // out of address space or over the memory target
                        e.set_last_error(8);
                        0
                    }
                }
            } else if e.mmu.region_of(a0).is_some() {
                // inside an existing reservation: only MEM_COMMIT, only within that region
                if !commit || reserve {
                    e.set_last_error(487);
                    0
                } else {
                    match e.mmu.commit_in_region(a0, size) {
                        Ok(_) => a0 & !0xFFF,
                        Err(code) => {
                            e.set_last_error(code);
                            0
                        }
                    }
                }
            } else if commit && !reserve {
                // committing an address that was never reserved (ERROR_INVALID_ADDRESS)
                e.set_last_error(487);
                0
            } else {
                match reserve_at(e, a0, size, commit) {
                    Ok(base) => base,
                    Err(code) => {
                        e.set_last_error(code);
                        0
                    }
                }
            }
        }
        "VirtualFree" => {
            let (size, kind) = (e.arg(1), e.arg(2));
            if kind & 0x8000 != 0 {
                // MEM_RELEASE: the size must be 0, and the address the base of a reservation
                if size != 0 {
                    e.set_last_error(87);
                    0
                } else if e.mmu.release_region(a0) {
                    1
                } else {
                    e.set_last_error(487);
                    0
                }
            } else {
                match e.mmu.decommit_range(a0, size) {
                    Ok(_) => 1,
                    Err(code) => {
                        e.set_last_error(code);
                        0
                    }
                }
            }
        }
        "VirtualProtect" => {
            let old = e.arg(3);
            e.w32(old, 4);
            1
        }
        "VirtualQuery" => {
            let mbi = e.arg(1);
            e.zero(mbi, 48);
            if !write_region_info(e, mbi, a0) {
                // no region holds the address: free
                e.w64(mbi, a0 & !0xFFF);
                e.w64(mbi + 24, 0x1000);
                e.w32(mbi + 32, 0x10000);
                e.w32(mbi + 36, 1);
            }
            48
        }

        // ---- modules ----
        "GetModuleHandleA" | "GetModuleHandleW" => {
            if a0 == 0 {
                e.s.exe_base
            } else {
                let n = if name.ends_with('W') { e.read_wstr(a0, 260) } else { e.read_cstr(a0, 260) };
                match pe::find_module(e, &n) {
                    Some(i) => e.s.modules[i].base,
                    None => {
                        e.set_last_error(126);
                        0
                    }
                }
            }
        }
        "GetModuleFileNameA" | "GetModuleFileNameW" => {
            let (buf, n) = (e.arg(1), e.arg(2));
            let path = match e.module_by_handle(a0) {
                Some(i) => format!("{}\\{}", crate::files::home_win(), e.s.modules[i].name),
                None => {
                    e.set_last_error(126);
                    return Some(0);
                }
            };
            let k = (path.len() as u64).min(n.saturating_sub(1));
            if name.ends_with('W') {
                for (i, c) in path.encode_utf16().take(k as usize).enumerate() {
                    e.w16(buf + 2 * i as u64, c);
                }
                e.w16(buf + 2 * k, 0);
            } else {
                let _ = e.mmu.write_bytes(buf, &path.as_bytes()[..k as usize]);
                e.w8(buf + k, 0);
            }
            k
        }
        // ntdll's lookup by name (winecrt0 finds __wine_unix_call_dispatcher with it)
        "RtlFindExportedRoutineByName" => {
            let name_at = e.arg(1);
            let f = e.read_cstr(name_at, 256);
            // (any 64-bit ntdll: wow64cpu asks too; the 32-bit ntdll of WoW64 has its real variable)
            let dll = e.module_by_handle(a0).map(|i| e.s.modules[i].name.trim_start_matches("wine-").to_string()).unwrap_or_default();
            if a0 != e.s.wow64_ntdll {
                if let Some(v) = crate::unixlib::data_export(e, &dll, &f) {
                    return Some(v);
                }
            }
            // the engine's own DLLs: Wine's real export (data such as LdrSystemDllInitBlock too);
            // a DLL mapped by a loader: its export table; any other DLL: the usual lookup
            match e.module_by_handle(a0) {
                Some(i) if e.s.modules[i].native => e.wine_fallback(&dll, &f, 0).unwrap_or(0),
                Some(i) => pe::proc_address(e, i, &f).unwrap_or(0),
                None => pe::export_addr(e, a0, &f).unwrap_or(0),
            }
        }
        "GetProcAddress" => {
            let f = if e.arg(1) >> 16 == 0 {
                format!("#{}", e.arg(1))
            } else {
                let p = e.arg(1);
                e.read_cstr(p, 256)
            };
            // (only the engine's own ntdll: the 32-bit ntdll of WoW64 has its real variable)
            let dll = e.module_by_handle(a0).filter(|&i| e.s.modules[i].native).map(|i| e.s.modules[i].name.clone()).unwrap_or_default();
            if let Some(v) = crate::unixlib::data_export(e, &dll, &f) {
                return Some(v);
            }
            match e.module_by_handle(a0) {
                Some(i)
                    if e.s.modules[i].native
                        && !{
                            let d = e.s.modules[i].name.clone();
                            e.wine_has(&d, &f)
                        } =>
                {
                    e.set_last_error(127);
                    0
                }
                Some(i) => pe::proc_address(e, i, &f).unwrap_or_else(|| {
                    e.set_last_error(127);
                    0
                }),
                None => {
                    e.set_last_error(126);
                    0
                }
            }
        }
        "LoadLibraryA" | "LoadLibraryExA" => {
            let n = e.read_cstr(a0, 260);
            return Some(e.load_library(&n));
        }
        "LoadLibraryW" | "LoadLibraryExW" => {
            let n = e.read_wstr(a0, 260);
            return Some(e.load_library(&n));
        }
        "FreeLibrary" => 1,
        _ => return e.not_mine(),
    };
    let _ = PEB;
    Some(r)
}

/// VirtualAlloc at a fixed address that is not reserved yet. Windows rounds the base down to
/// the 64 KB allocation granularity and the size up to whole pages; `fixed_span` gives that
/// effective range, which is checked against every other reservation before anything is
/// changed. Returns the base actually reserved. With MEM_COMMIT the requested pages are
/// committed, and a failed commit leaves no reservation behind. Err: the Windows error code.
fn reserve_at(e: &mut Engine, a0: u64, size: u64, commit: bool) -> Result<u64, u32> {
    e.mmu.reserve_fixed(a0, size, commit)
}

/// Writes the MEMORY_BASIC_INFORMATION (48 bytes, zeroed by the caller) of the page holding
/// `addr`, when a region holds it. The state comes from the commit list: MEM_COMMIT for a
/// committed page, MEM_RESERVE for a reserved-only one. RegionSize is the run of pages with
/// that state, not the whole region. Returns false and writes nothing when no region holds `addr`.
pub(crate) fn write_region_info(e: &mut Engine, buf: u64, addr: u64) -> bool {
    let Some(run) = e.mmu.page_run(addr) else { return false };
    let page = addr & !0xFFF;
    e.w64(buf, page);
    e.w64(buf + 8, run.region);
    e.w32(buf + 16, 4);
    e.w64(buf + 24, run.end - page);
    if run.committed {
        e.w32(buf + 32, 0x1000); // MEM_COMMIT
        e.w32(buf + 36, 4); // PAGE_READWRITE, the protection the engine gives committed pages
    } else {
        e.w32(buf + 32, 0x2000); // MEM_RESERVE
        e.w32(buf + 36, 0); // a reserved page has no protection
    }
    e.w32(buf + 40, 0x20000); // MEM_PRIVATE
    true
}

/// ANSI copy of a UTF-16 string held by the process parameters. The copy is made
/// once per (process, buffer) and kept for the life of the engine, so the pointer
/// stays valid. The engine's ANSI code page is UTF-8, so the copy is UTF-8.
fn startup_ansi(e: &mut Engine, wide: u64) -> u64 {
    if wide == 0 {
        return 0;
    }
    let pid = e.s.wpid;
    let text = e.read_wstr(wide, 4096);
    let mut cache = std::mem::take(&mut e.s.startup_ansi);
    let a = cached_copy(&mut cache, pid, wide, || e.put_str(&text, false));
    e.s.startup_ansi = cache;
    a
}

/// Returns the copy kept for (pid, wide), making it with `make` on the first call.
fn cached_copy(cache: &mut HashMap<(u32, u64), u64>, pid: u32, wide: u64, make: impl FnOnce() -> u64) -> u64 {
    if let Some(&a) = cache.get(&(pid, wide)) {
        return a;
    }
    let a = make();
    cache.insert((pid, wide), a);
    a
}

#[cfg(test)]
mod startup_tests {
    use super::{cached_copy, HashMap};

    #[test]
    fn ansi_copies_are_per_process() {
        let mut cache = HashMap::new();
        let mut made = 0;
        let a = cached_copy(&mut cache, 1, 0x1000, || { made += 1; 0xA000 });
        let b = cached_copy(&mut cache, 2, 0x1000, || { made += 1; 0xB000 });
        assert_ne!(a, b, "two processes with the same buffer address share a copy");
        assert_eq!(cached_copy(&mut cache, 1, 0x1000, || { made += 1; 0xC000 }), 0xA000);
        assert_eq!(made, 2);
    }
}
