// Virtual memory.

use super::*;

pub(super) fn call(e: &mut Engine, name: &str) -> Option<u64> {
    Some(match name {
        // ---- memory ----
        "NtAllocateVirtualMemory" | "NtAllocateVirtualMemoryEx" => {
            let (bp, sp) = (e.arg(1), if name == "NtAllocateVirtualMemory" { e.arg(3) } else { e.arg(2) });
            let (want, size) = (e.r64(bp), e.r64(sp));
            let kind = e.arg(if name == "NtAllocateVirtualMemory" { 4 } else { 3 }) as u32;
            let a = if want == 0 {
                if kind & 0x1000 == 0 {
                    e.reserve(size)
                } else {
                    e.alloc(size)
                }
            } else if e.mmu.region_of(want).is_some() {
                Some(want & !0xFFF)
            } else {
                let s = want & !0xFFFF;
                let end = (want + size + 0xFFFF) & !0xFFFF;
                if e.mmu.overlaps(s, end) {
                    None
                } else {
                    e.mmu.add_region(s, end - s);
                    Some(s)
                }
            };
            match a {
                Some(a) => {
                    e.w64(bp, a);
                    e.w64(sp, (size + 0xFFF) & !0xFFF);
                    OK
                }
                None => NO_MEMORY, // NO_MEMORY
            }
        }
        "NtFreeVirtualMemory" => {
            let (bp, sp, kind) = (e.arg(1), e.arg(2), e.arg(3) as u32);
            let (a, size) = (e.r64(bp), e.r64(sp));
            if unsafe { crate::WINE_DEBUG } > 1 {
                let mut chain = String::new();
                if let Some(c) = e.wow_ctx_of(u64::MAX - 1) {
                    let (mut bp, sp) = (e.r32(c + 0xB4) as u64, e.r32(c + 0xC4) as u64);
                    chain += &format!(" eip {:x} esp {:x}:", e.r32(c + 0xB8), sp);
                    for i in 0..24 { chain += &format!(" {:x}", e.mmu.read32(sp + i * 4).unwrap_or(0)); }
                    chain += " | bp:";
                    for _ in 0..12 { let r = e.mmu.read32(bp + 4).unwrap_or(0); chain += &format!(" {:x}", r); bp = e.mmu.read32(bp).unwrap_or(0) as u64; if bp == 0 { break; } }
                }
                crate::console(&format!("[mem] free {:x} {:x} kind {:x} region {:x?} tid {}{}\n", a, size, kind, e.mmu.region_of(a), e.cur_tid(), chain));
            }
            if kind & 0x8000 != 0 {
                e.mmu.release_region(a);
            } else {
                e.mmu.drop_pages(a & !0xFFF, a + size);
            }
            OK
        }
        "NtProtectVirtualMemory" => {
            let old = e.arg(4);
            e.w32(old, 4);
            OK
        }
        "NtFlushInstructionCache" | "NtLockVirtualMemory" | "NtUnlockVirtualMemory" => OK,
        "NtQueryVirtualMemory" => {
            let (addr, class, buf, len, ret) = (e.arg(1), e.arg(2) as u32, e.arg(3), e.arg(4), e.arg(5));
            if class == crate::unixlib::MEMORY_WINE_UNIX_FUNCS || class == crate::unixlib::MEMORY_WINE_UNIX_WOW64_FUNCS {
                let wow64 = class == crate::unixlib::MEMORY_WINE_UNIX_WOW64_FUNCS;
                let Some(h) = crate::unixlib::handle_of(e, addr, wow64) else {
                    if unsafe { crate::TRACE } {
                        let m = e.s.modules.iter().find(|m| m.base == addr).map(|m| m.name.clone()).or_else(|| e.s.image_files.get(&addr).cloned()).unwrap_or_default();
                        crate::console(&format!("[unixlib] no Unix side for {} ({:x})\n", m, addr));
                    }
                    return Some(INVALID_INFO_CLASS);
                };
                e.w64(buf, h);
                return Some(OK);
            }
            if class != 0 {
                return Some(INVALID_INFO_CLASS);
            }
            if len < 48 {
                return Some(INFO_LENGTH_MISMATCH);
            }
            e.zero_n(buf, 48);
            let page = addr & !0xFFF;
            e.w64(buf, page);
            match e.mmu.region_of(addr) {
                Some((s, end)) => {
                    e.w64(buf + 8, s);
                    e.w32(buf + 16, 4);
                    e.w64(buf + 24, end - page);
                    e.w32(buf + 32, 0x1000);
                    e.w32(buf + 36, 4);
                    e.w32(buf + 40, 0x20000);
                }
                None => {
                    // inside a loaded image?
                    match e.s.modules.iter().find(|m| !m.native && addr >= m.base && addr < m.base + m.size).map(|m| (m.base, m.size)) {
                        Some((b, sz)) => {
                            e.w64(buf + 8, b);
                            e.w32(buf + 16, 0x80);
                            e.w64(buf + 24, b + sz - page);
                            e.w32(buf + 32, 0x1000);
                            e.w32(buf + 36, 0x20);
                            e.w32(buf + 40, 0x100_0000);
                        }
                        None => {
                            e.w64(buf + 24, 0x1000);
                            e.w32(buf + 32, 0x10000);
                            e.w32(buf + 36, 1);
                        }
                    }
                }
            }
            if ret != 0 {
                e.w64(ret, 48);
            }
            OK
        }
        "NtReadVirtualMemory" => {
            let (src, dst, n, ret) = (e.arg(1), e.arg(2), e.arg(3), e.arg(4));
            let mut v = vec![0u8; n as usize];
            if e.mmu.read_bytes(src, &mut v).is_err() || e.mmu.write_bytes(dst, &v).is_err() {
                return Some(BUFFER_OVERFLOW + 0x4000_0000);
            }
            if ret != 0 {
                e.w64(ret, n);
            }
            OK
        }
        "NtWriteVirtualMemory" => {
            let (dst, src, n, ret) = (e.arg(1), e.arg(2), e.arg(3), e.arg(4));
            let mut v = vec![0u8; n as usize];
            if e.mmu.read_bytes(src, &mut v).is_err() || e.mmu.write_bytes(dst, &v).is_err() {
                return Some(ACCESS_DENIED);
            }
            if ret != 0 {
                e.w64(ret, n);
            }
            OK
        }
        _ => return e.not_mine(),
    })
}
