// Processes, threads and system information.

use super::*;

pub(super) fn call(e: &mut Engine, name: &str) -> Option<u64> {
    let a0 = e.arg(0);
    Some(match name {
        // ---- processes, threads, system ----
        "NtQueryInformationProcess" | "NtWow64QueryInformationProcess64" => {
            let (class, buf, len, ret) = (e.arg(1) as u32, e.arg(2), e.arg(3) as u32 as u64, e.arg(4));
            let need: u64 = match class {
                0 => 48,
                7 | 26 | 30 | 31 => 8,
                12 | 34 | 36 | 31_000 => 4,
                20 => 4,
                2 | 3 | 4 => 112.min(len),
                _ => 0,
            };
            if need == 0 {
                return Some(INVALID_INFO_CLASS);
            }
            if len < need {
                return Some(INFO_LENGTH_MISMATCH);
            }
            e.zero_n(buf, need);
            match class {
                0 => {
                    e.w64(buf + 8, crate::PEB);
                    e.w64(buf + 16, (1u64 << e.s.cores) - 1);
                    e.w32(buf + 24, 8);
                    e.w64(buf + 32, 4);
                }
                31 => {
                    e.w32(buf, 1);
                } // ProcessDebugFlags: no debugger
                26 => {
                    if e.s.wow64 {
                        e.w64(buf, crate::PEB32);
                    }
                } // ProcessWow64Information: the 32-bit PEB
                4 => {
                    if let Some((c, u)) = e.cpu_times(a0, true) {
                        e.w64(buf, c);
                        e.w64(buf + 24, u);
                    }
                } // ProcessTimes
                36 => {
                    e.w32(buf, 0x1234_5678);
                } // ProcessCookie
                _ => {}
            }
            if ret != 0 {
                e.w32(ret, need as u32);
            }
            if class == 30 {
                return Some(PORT_NOT_SET);
            } // ProcessDebugObjectHandle: STATUS_PORT_NOT_SET
            OK
        }
        // ThreadWow64Context: the 32-bit registers live in the thread's CPU area (TEB TlsSlots[1] + 4)
        "NtSetInformationThread" if e.arg(1) == 0x1D => {
            let Some(area) = e.wow_ctx_of(a0) else { return Some(INVALID_HANDLE) };
            let mut b = vec![0u8; 0x2CC];
            let src = e.arg(2);
            if e.mmu.read_bytes(src, &mut b).is_err() {
                return Some(ACCESS_VIOLATION);
            }
            // WOW64_CPURESERVED_FLAG_RESET_STATE: wow64cpu reloads every register on its way back
            let fl = e.r16(area - 4);
            e.w16(area - 4, fl | 1);
            let _ = e.mmu.write_bytes(area, &b);
            OK
        }
        "NtQueryInformationThread" if e.arg(1) == 0x1D => {
            let Some(area) = e.wow_ctx_of(a0) else { return Some(INVALID_HANDLE) };
            let mut b = vec![0u8; 0x2CC];
            let _ = e.mmu.read_bytes(area, &mut b);
            let (dst, rl) = (e.arg(2), e.arg(4));
            let flags = e.mmu.read32(dst).unwrap_or(0x1003F);
            let _ = e.mmu.write_bytes(dst, &b);
            e.w32(dst, flags);
            if rl != 0 {
                e.w32(rl, 0x2CC);
            }
            OK
        }
        "NtSetInformationProcess" | "NtSetInformationThread" | "NtSetInformationObject" => OK,
        "NtQueryInformationThread" => {
            let (class, buf, len, ret) = (e.arg(1) as u32, e.arg(2), e.arg(3) as u32 as u64, e.arg(4));
            match class {
                0 => {
                    if len < 48 {
                        return Some(INFO_LENGTH_MISMATCH);
                    }
                    e.zero_n(buf, 48);
                    let teb = e.cpu.gs_base;
                    e.w32(buf, 0x103);
                    e.w64(buf + 8, teb);
                    e.w64(buf + 16, 4);
                    let tid = e.cur_tid() as u64;
                    e.w64(buf + 24, tid);
                    e.w64(buf + 32, 1);
                    if ret != 0 {
                        e.w32(ret, 48);
                    }
                    OK
                }
                1 => {
                    // ThreadTimes
                    if len < 32 {
                        return Some(INFO_LENGTH_MISMATCH);
                    }
                    e.zero_n(buf, 32);
                    if let Some((c, u)) = e.cpu_times(a0, false) {
                        e.w64(buf, c);
                        e.w64(buf + 24, u);
                    }
                    if ret != 0 {
                        e.w32(ret, 32);
                    }
                    OK
                }
                _ => {
                    e.zero_n(buf, len.min(64));
                    if ret != 0 {
                        e.w32(ret, len.min(64) as u32);
                    }
                    OK
                }
            }
        }
        "NtQuerySystemInformation" | "NtQuerySystemInformationEx" => {
            let (class, buf, len, ret) = if name == "NtQuerySystemInformation" {
                (a0 as u32, e.arg(1), e.arg(2) as u32 as u64, e.arg(3))
            } else {
                (a0 as u32, e.arg(3), e.arg(4) as u32 as u64, e.arg(5))
            };
            // SystemInterruptInformation: Wine's RtlGenRandom, ProcessPrng and BCryptGenRandom
            // read random bytes from it
            if class == 0x17 {
                let b = crate::random_bytes(len.min(1 << 20) as usize);
                let _ = e.mmu.write_bytes(buf, &b);
                if ret != 0 {
                    e.w32(ret, b.len() as u32);
                }
                return Some(OK);
            }
            let need: u64 = match class {
                0 | 0x3E => 64,
                1 | 0x3F => 12,
                3 => 48,
                8 => 48 * e.s.cores as u64,
                2 => 0x150,
                0x67 => 8,
                0xB5 => {
                    if e.s.wow64 {
                        12
                    } else {
                        8
                    }
                }
                _ => 0,
            };
            if need == 0 {
                return Some(INVALID_INFO_CLASS);
            }
            if ret != 0 {
                e.w32(ret, need as u32);
            }
            if len < need {
                return Some(INFO_LENGTH_MISMATCH);
            }
            e.zero_n(buf, need);
            match class {
                // SystemSupportedProcessorArchitectures: x86-64 native, i386 for this (WoW64) process
                // (a 64-bit process: x86-64 is native and the process's own machine)
                0xB5 if !e.s.wow64 => {
                    e.w32(buf, 0x8664 | 0xF << 16);
                }
                0xB5 => {
                    e.w32(buf, 0x8664 | 7 << 16);
                    e.w32(buf + 4, 0x14C | 2 << 16 | 1 << 19 | 1 << 20);
                }
                0 | 0x3E => {
                    let w32 = |e: &mut Engine, o: u64, v: u32| {
                        e.w32(buf + o, v);
                    };
                    w32(e, 4, 156_250);
                    w32(e, 8, 4096);
                    w32(e, 12, 2 << 20);
                    w32(e, 16, 1);
                    w32(e, 20, 2 << 20);
                    w32(e, 24, 0x10000);
                    e.w64(buf + 0x20, 0x10000);
                    e.w64(buf + 0x28, if class == 0x3E { 0x7FFE_FFFF } else { 0x7FFF_FFFE_FFFF });
                    e.w64(buf + 0x30, (1u64 << e.s.cores) - 1);
                    e.w8(buf + 0x38, e.s.cores as u8);
                }
                1 | 0x3F => {
                    e.w16(buf, if class == 0x3F { 0 } else { 9 });
                    e.w16(buf + 2, 6);
                    e.w16(buf + 4, 0x9E00);
                    e.w16(buf + 6, e.s.cores as u16);
                    e.w32(buf + 8, 0x0000_3FFF);
                }
                3 => {
                    e.w64(buf, crate::filetime_now());
                    e.w64(buf + 8, crate::filetime_now());
                }
                _ => {}
            }
            OK
        }
        "NtGetTickCount" => crate::now_ms() as u64 & 0xFFFF_FFFF,
        "NtQueryPerformanceCounter" => {
            let v = (crate::now_ms() * 10_000.0) as u64;
            e.w64(a0, v);
            let f = e.arg(1);
            if f != 0 {
                e.w64(f, 10_000_000);
            }
            OK
        }
        "NtQueryTimerResolution" => {
            let (b, c) = (e.arg(1), e.arg(2));
            e.w32(a0, 156_250);
            e.w32(b, 5000);
            e.w32(c, 156_250);
            OK
        }
        "NtSetTimerResolution" => {
            let p = e.arg(2);
            e.w32(p, 10_000);
            OK
        }
        "NtGetCurrentProcessorNumber" => 0,
        "NtTerminateProcess" => {
            let code = e.arg(1) as u32;
            if a0 == 0 || a0 == u64::MAX {
                e.exit_process(code);
                return Some(crate::kernel32::NO_RETURN);
            }
            if let Some(Handle::Process { pid }) = e.s.fs.handles.get(&a0) {
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
            }
            OK
        }
        // a LUID unique for this run
        "NtAllocateLocallyUniqueId" => {
            e.s.next_luid += 1;
            let v = 0x1000 + e.s.next_luid;
            e.w64(a0, v);
            OK
        }
        "NtQueryDefaultLocale" => {
            let p = e.arg(1);
            e.w32(p, 0x409);
            OK
        }
        "NtQueryDefaultUILanguage" | "NtQueryInstallUILanguage" => {
            e.w16(a0, 0x409);
            OK
        }
        "NtQueryLicenseValue" => OBJECT_NAME_NOT_FOUND,
        "NtTraceControl" | "NtTraceEvent" => OK,
        "NtDuplicateObject" => {
            let (h, out) = (e.arg(1), e.arg(3));
            let h = e.dup_handle(h);
            if out != 0 {
                e.w64(out, h);
            }
            OK
        }
        "NtQueryObject" => query_object(e),
        "NtDisplayString" => OK,
        // CreateProcess from Wine's kernelbase (a 32-bit program's, through WoW64)
        "NtCreateUserProcess" => create_user_process(e),
        // kernel transactions (CreateTransaction): like Wine, a placeholder handle, and
        // commit or rollback do nothing (the file changes were made directly)
        "NtCreateTransaction" => {
            let h = e.new_handle(crate::files::Handle::Event { set: true, manual: true });
            e.w64(a0, h);
            OK
        }
        "NtCommitTransaction" | "NtRollbackTransaction" => OK,
        _ => return e.not_mine(),
    })
}

/// NtQueryObject(handle, class, buffer, length, &needed): basic information (0) and, for
/// files and folders, the name (1) as "\\??\\C:\\..." (GetFinalPathNameByHandleW reads it).
fn query_object(e: &mut Engine) -> u64 {
    let (h, class, buf, len, ret) = (e.arg(0), e.arg(1) as u32, e.arg(2), e.arg(3) as u32 as u64, e.arg(4));
    match class {
        0 => {
            // ObjectBasicInformation: attributes, access, handle count, pointer count, ...
            if len < 56 {
                e.w32(ret, 56);
                return INFO_LENGTH_MISMATCH;
            }
            e.zero_n(buf, 56);
            e.w32(buf + 4, 0x1F01FF);
            e.w32(buf + 8, 1);
            e.w32(buf + 12, 1);
            e.w32(ret, 56);
            OK
        }
        1 => {
            let key = match e.s.fs.handles.get(&h) {
                Some(Handle::File { name, .. }) => Some(name.clone()),
                Some(Handle::Dir) => e.s.fs.paths.get(&h).map(|p| p.0.clone()),
                _ => None,
            };
            let name = match key {
                Some(k) => {
                    let cased = e.s.names.get(&k).cloned().unwrap_or(k);
                    let full = if cased == "c:" { "C:\\".to_string() } else { crate::files::win_path(&cased) };
                    format!("\\??\\{}", full.trim_end_matches('\\'))
                }
                None => String::new(),
            };
            let w: Vec<u16> = name.encode_utf16().collect();
            let need = 16 + 2 * w.len() as u64 + if w.is_empty() { 0 } else { 2 };
            e.w32(ret, need as u32);
            if len < 16 {
                return INFO_LENGTH_MISMATCH;
            }
            if len < need {
                return BUFFER_OVERFLOW;
            }
            e.zero_n(buf, 16);
            if !w.is_empty() {
                e.w16(buf, 2 * w.len() as u16);
                e.w16(buf + 2, 2 * w.len() as u16 + 2);
                e.w64(buf + 8, buf + 16);
                for (i, c) in w.iter().enumerate() {
                    e.w16(buf + 16 + 2 * i as u64, *c);
                }
                e.w16(buf + 16 + 2 * w.len() as u64, 0);
            }
            OK
        }
        _ => INVALID_INFO_CLASS,
    }
}

/// NtCreateUserProcess(&process, &thread, access, access, attr, attr, process_flags,
/// thread_flags, params, create_info, attribute_list): the program and its command line
/// come from the process parameters; the attribute list asks for the ids and image info.
fn create_user_process(e: &mut Engine) -> u64 {
    let (ph_out, th_out) = (e.arg(0), e.arg(1));
    let (tflags, params, attrs) = (e.arg(7) as u32, e.arg(8), e.arg(10));
    // the parameters may hold offsets instead of pointers (not normalized)
    let base = if e.r32(params + 8) & 1 == 0 { params } else { 0 };
    let ustr = |e: &mut Engine, at: u64| -> String {
        let len = e.r16(at) as u64 / 2;
        let buf = e.r64(at + 8);
        if buf == 0 { return String::new(); }
        let buf = buf + base;
        let v: Vec<u16> = (0..len).map(|i| e.r16(buf + 2 * i)).collect();
        String::from_utf16_lossy(&v)
    };
    let image = ustr(e, params + 0x60);
    let cmd = ustr(e, params + 0x70);
    let image = image.strip_prefix("\\??\\").unwrap_or(&image).to_string();
    let envp = e.r64(params + 0x80);
    let env: Vec<u16> = if envp == 0 { e.env_vec() } else {
        let envp = envp + base;
        let n = e.r64(params + 0x3F0) / 2;
        let mut v: Vec<u16> = (0..n).map(|i| e.r16(envp + 2 * i)).collect();
        while v.len() >= 2 && v[v.len() - 1] == 0 && v[v.len() - 2] == 0 && v.len() > 2 { v.pop(); }
        if v.len() < 2 || v[v.len() - 1] != 0 || v[v.len() - 2] != 0 { v.push(0); v.push(0); }
        v
    };
    let std = [e.r64(params + 0x20), e.r64(params + 0x28), e.r64(params + 0x30)];
    let image = crate::pe::norm(&image);
    let Some(bytes) = e.s.files.get(&image).cloned() else {
        if unsafe { crate::WINE_DEBUG } > 0 { crate::console(&format!("[inksoc] CreateProcess: {} not found\n", image)); }
        return 0xC000_0034; // OBJECT_NAME_NOT_FOUND
    };
    let (pid, tid, th) = match e.create_process(&image, &cmd, env, std, tflags & 1 != 0) {
        Ok(r) => r,
        Err(m) => {
            crate::console(&format!("[inksoc] CreateProcess {}: {}\n", image, m));
            return 0xC000_0034;
        }
    };
    if unsafe { crate::WINE_DEBUG } > 1 { crate::console(&format!("[inksoc] CreateProcess {} -> pid {} tid {}\n", cmd, pid, tid)); }
    let ph = e.new_handle(Handle::Process { pid });
    e.w64(ph_out, ph);
    e.w64(th_out, th);
    // the attribute list: ULONG_PTR total length, then {attribute, size, value, return length}
    if attrs != 0 {
        let total = e.r64(attrs);
        let mut a = attrs + 8;
        while a + 32 <= attrs + total {
            let (kind, size, val) = (e.r64(a), e.r64(a + 8), e.r64(a + 16));
            match kind & 0xFFFF {
                3 if size >= 16 => { e.w64(val, pid as u64); e.w64(val + 8, tid as u64); } // client id
                6 if size >= 0x40 => { // SECTION_IMAGE_INFORMATION
                    e.zero_n(val, 0x40);
                    e.w64(val + 0x10, 0x10_0000);
                    e.w64(val + 0x18, 0x1000);
                    e.w32(val + 0x20, crate::pe::subsystem(&bytes).unwrap_or(2) as u32);
                    e.w16(val + 0x26, 6);
                    e.w16(val + 0x30, if crate::wow64::is_pe32(&bytes) { 0x14C } else { 0x8664 });
                    e.w8(val + 0x32, 1);
                    e.w32(val + 0x38, bytes.len() as u32);
                }
                _ => {}
            }
            a += 32;
        }
    }
    OK
}
