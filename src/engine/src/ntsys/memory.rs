// Virtual memory.

use super::*;
use crate::mmu::Mmu;

const MEM_COMMIT: u32 = 0x1000;
const MEM_RESERVE: u32 = 0x2000;

/// What NtAllocateVirtualMemory does for a base address and allocation flags. Pure: it
/// touches no address space, so the host tests can check every combination.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(super) enum Plan {
    /// Refused, nothing changes. Covers flags with neither bit, and MEM_COMMIT alone with
    /// no reservation under it (a NULL base or an address outside any reservation).
    Refuse,
    /// NULL base, MEM_RESERVE only: a new reservation, nothing committed.
    Reserve,
    /// NULL base, MEM_RESERVE and MEM_COMMIT: a new reservation, committed.
    Alloc,
    /// Base inside a reservation, MEM_RESERVE only: nothing changes.
    InRegionKeep,
    /// Base inside a reservation, MEM_COMMIT: the pages asked for are committed, all or nothing.
    InRegionCommit,
    /// Fixed base outside any reservation, MEM_RESERVE with or without MEM_COMMIT.
    Fixed { commit: bool },
}

pub(super) fn nt_plan(base: u64, in_region: bool, kind: u32) -> Plan {
    let commit = kind & MEM_COMMIT != 0;
    let reserve = kind & MEM_RESERVE != 0;
    if !commit && !reserve {
        return Plan::Refuse;
    }
    if base == 0 {
        return match (reserve, commit) {
            (false, _) => Plan::Refuse,
            (true, false) => Plan::Reserve,
            (true, true) => Plan::Alloc,
        };
    }
    if in_region {
        return if commit { Plan::InRegionCommit } else { Plan::InRegionKeep };
    }
    if !reserve {
        return Plan::Refuse;
    }
    Plan::Fixed { commit }
}

/// Applies a fixed-base plan to the address space. Returns the base, or None with nothing
/// changed. The NULL-base plans need the engine's allocator, so they are not handled here.
pub(super) fn apply_fixed(mmu: &mut Mmu, plan: Plan, base: u64, size: u64) -> Option<u64> {
    match plan {
        Plan::InRegionKeep => Some(base & !0xFFF),
        Plan::InRegionCommit => mmu.commit_in_region(base, size).ok().map(|_| base & !0xFFF),
        Plan::Fixed { commit } => mmu.reserve_fixed(base, size, commit).ok(),
        Plan::Refuse | Plan::Reserve | Plan::Alloc => None,
    }
}

pub(super) fn call(e: &mut Engine, name: &str) -> Option<u64> {
    Some(match name {
        // ---- memory ----
        "NtAllocateVirtualMemory" | "NtAllocateVirtualMemoryEx" => {
            let (bp, sp) = (e.arg(1), if name == "NtAllocateVirtualMemory" { e.arg(3) } else { e.arg(2) });
            let (want, size) = (e.r64(bp), e.r64(sp));
            let kind = e.arg(if name == "NtAllocateVirtualMemory" { 4 } else { 3 }) as u32;
            let in_region = want != 0 && e.mmu.region_of(want).is_some();
            let a = match nt_plan(want, in_region, kind) {
                Plan::Reserve => e.reserve(size),
                Plan::Alloc => e.alloc(size),
                other => apply_fixed(&mut e.mmu, other, want, size),
            };
            // the size written back is rounded up to a page; checked, so a huge size fails
            let out = a.and_then(|a| size.checked_add(0xFFF).map(|r| (a, r & !0xFFF)));
            match out {
                Some((a, r)) => {
                    e.w64(bp, a);
                    e.w64(sp, r);
                    OK
                }
                None => NO_MEMORY,
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
            let (base_ptr, size_ptr, new_prot, old_ptr) = (e.arg(1), e.arg(2), e.arg(3) as u32, e.arg(4));
            let (base, size) = (e.r64(base_ptr), e.r64(size_ptr));
            if base == 0 || size == 0 { return Some(INVALID_PARAMETER); }
            let page = base & !0xFFF;
            let (old_prot, ok) = e.mmu.protect_pages(page, size, new_prot);
            if ok {
                e.w32(old_ptr, old_prot);
                OK
            } else {
                INVALID_PARAMETER
            }
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
            if !crate::kernel32::write_region_info(e, buf, addr) {
                {
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

#[cfg(test)]
mod nt_tests {
    //! NtAllocateVirtualMemory's decision and the fixed-base application, on the MMU alone.
    use super::*;

    const A: u64 = 0x5D00_0000;
    const COMMIT: u32 = MEM_COMMIT;
    const RESERVE: u32 = MEM_RESERVE;

    #[test]
    fn every_flag_combination_has_one_plan() {
        // NULL base
        assert_eq!(nt_plan(0, false, 0), Plan::Refuse, "no flags");
        assert_eq!(nt_plan(0, false, COMMIT), Plan::Refuse, "MEM_COMMIT alone at a NULL base");
        assert_eq!(nt_plan(0, false, RESERVE), Plan::Reserve);
        assert_eq!(nt_plan(0, false, COMMIT | RESERVE), Plan::Alloc);
        // inside a reservation
        assert_eq!(nt_plan(A, true, RESERVE), Plan::InRegionKeep, "MEM_RESERVE does not commit");
        assert_eq!(nt_plan(A, true, COMMIT), Plan::InRegionCommit);
        assert_eq!(nt_plan(A, true, COMMIT | RESERVE), Plan::InRegionCommit);
        assert_eq!(nt_plan(A, true, 0), Plan::Refuse);
        // outside any reservation
        assert_eq!(nt_plan(A, false, COMMIT), Plan::Refuse, "MEM_COMMIT alone creates no region");
        assert_eq!(nt_plan(A, false, RESERVE), Plan::Fixed { commit: false });
        assert_eq!(nt_plan(A, false, COMMIT | RESERVE), Plan::Fixed { commit: true });
        assert_eq!(nt_plan(A, false, 0), Plan::Refuse);
    }

    #[test]
    fn commit_alone_outside_a_reservation_creates_no_region() {
        let mut m = Mmu::new(1, 8);
        let plan = nt_plan(A, false, COMMIT);
        assert_eq!(apply_fixed(&mut m, plan, A, 0x1000), None);
        assert!(m.region_of(A).is_none(), "the refused commit created a region");
        assert_eq!(m.committed_bytes(), 0);
        assert!(m.read8(A).is_err());
    }

    #[test]
    fn reserve_alone_commits_no_page() {
        let mut m = Mmu::new(1, 8);
        assert_eq!(apply_fixed(&mut m, nt_plan(A, false, RESERVE), A, 0x3000), Some(A));
        assert!(m.region_of(A).is_some());
        assert_eq!(m.committed_bytes(), 0, "MEM_RESERVE committed pages");
        assert!(m.read8(A).is_err());
        // then MEM_COMMIT inside it: the reserved range is committed, not the rest of the region
        assert_eq!(apply_fixed(&mut m, nt_plan(A, true, COMMIT), A + 0x1000, 0x1000), Some(A + 0x1000));
        assert_eq!(m.committed_bytes(), 0x1000);
        assert_eq!(m.read8(A + 0x1000), Ok(0));
        assert!(m.read8(A).is_err());
    }

    #[test]
    fn reserve_and_commit_outside_commits_the_pages() {
        let mut m = Mmu::new(1, 8);
        assert_eq!(apply_fixed(&mut m, nt_plan(A, false, COMMIT | RESERVE), A + 0x800, 0x1000), Some(A));
        assert_eq!(m.committed_bytes(), 0x2000, "the pages of the unaligned range were not all committed");
        assert_eq!(m.read8(A + 0x1000), Ok(0));
        assert!(m.read8(A + 0x2000).is_err());
    }

    #[test]
    fn commit_past_the_reservation_changes_nothing() {
        let mut m = Mmu::new(1, 8);
        apply_fixed(&mut m, nt_plan(A, false, RESERVE), A, 0x1000).unwrap(); // [A, A+0x1000)
        let before = m.committed_bytes();
        assert_eq!(apply_fixed(&mut m, nt_plan(A, true, COMMIT), A, 0x2000), None);
        assert_eq!(m.committed_bytes(), before);
        assert!(m.read8(A).is_err());
    }

    #[test]
    fn repeated_commit_keeps_the_contents() {
        let mut m = Mmu::new(1, 8);
        apply_fixed(&mut m, nt_plan(A, false, COMMIT | RESERVE), A, 0x1000).unwrap();
        m.write8(A, 0x5A).unwrap();
        assert_eq!(apply_fixed(&mut m, nt_plan(A, true, COMMIT), A, 0x1000), Some(A));
        assert_eq!(m.read8(A), Ok(0x5A));
        assert_eq!(m.committed_bytes(), 0x1000);
    }

    #[test]
    fn memory_limit_refuses_without_partial_state() {
        let mut m = Mmu::new(1, 1); // 1 MB target
        assert_eq!(apply_fixed(&mut m, nt_plan(A, false, COMMIT | RESERVE), A, 0x20_0000), None);
        assert!(m.region_of(A).is_none());
        assert_eq!(m.committed_bytes(), 0);
    }

    #[test]
    fn range_past_u64_is_refused() {
        let mut m = Mmu::new(1, 8);
        let base = u64::MAX - 0x10;
        assert_eq!(apply_fixed(&mut m, nt_plan(base, false, COMMIT | RESERVE), base, 0x100), None);
        assert_eq!(m.committed_bytes(), 0);
    }
}
