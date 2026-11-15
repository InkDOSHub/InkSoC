// 64-bit page table over a fixed pool of real RAM frames, with swap to OPFS.
// - The pool is allocated once and never grows.
// - Pages are committed lazily: real RAM only on first touch (zero-filled).
// - When the pool is full, a clock algorithm evicts a page; dirty pages are
//   written to the swap file, clean ones that already have a copy are dropped.
// - A small software TLB keeps the common case to a few instructions.

use std::collections::HashMap;

#[cfg(not(test))]
#[link(wasm_import_module = "env")]
extern "C" {
    fn host_swap_write(slot: u32, ptr: *const u8);
    fn host_swap_read(slot: u32, ptr: *mut u8);
}

// Host tests have no swap file: the swap slots live in memory for the test's own thread
// (cargo runs each test on its own thread, so slots never leak between tests).
#[cfg(test)]
thread_local! {
    static SWAP: std::cell::RefCell<HashMap<u32, Vec<u8>>> = std::cell::RefCell::new(HashMap::new());
}
#[cfg(test)]
unsafe fn host_swap_write(slot: u32, ptr: *const u8) {
    let page = std::slice::from_raw_parts(ptr, PAGE as usize).to_vec();
    SWAP.with(|s| s.borrow_mut().insert(slot, page));
}
#[cfg(test)]
unsafe fn host_swap_read(slot: u32, ptr: *mut u8) {
    SWAP.with(|s| {
        let s = s.borrow();
        let page = s.get(&slot).expect("swap slot read before it was written");
        std::ptr::copy_nonoverlapping(page.as_ptr(), ptr, PAGE as usize);
    });
}

pub const PAGE: u64 = 4096;

/// The pages covering `start..start+size`, as [s, e). None when the end does not fit
/// in a u64. A zero size gives an empty span.
pub fn page_span(start: u64, size: u64) -> Option<(u64, u64)> {
    let s = start & !(PAGE - 1);
    if size == 0 { return Some((s, s)); }
    let end = start.checked_add(size)?.checked_add(PAGE - 1)? & !(PAGE - 1);
    Some((s, end))
}

/// End of `a..a+size` and the bump pointer after it (the end plus `gap`), both checked.
/// None when either does not fit in a u64.
pub fn span_after(a: u64, size: u64, gap: u64) -> Option<(u64, u64)> {
    let end = a.checked_add(size)?;
    Some((end, end.checked_add(gap)?))
}

/// Effective range of a fixed-address reservation at `a` of `size` bytes: the base rounded
/// down to the 64 KB allocation granularity, and the end of the page that holds the last
/// byte of `a..a+size` (a zero size counts as one byte). None when the end does not fit in
/// a u64. The base is documented to round down; the exact end is the page-granular
/// interpretation used here, not a value checked against Windows.
pub fn fixed_span(a: u64, size: u64) -> Option<(u64, u64)> {
    let start = a & !0xFFFF;
    let last = a.checked_add(size.max(1) - 1)?;
    let end = (last & !(PAGE - 1)).checked_add(PAGE)?;
    Some((start, end))
}

/// `x` rounded up to a 64 KB boundary; None when that does not fit in a u64.
pub fn round_up_64k(x: u64) -> Option<u64> {
    Some(x.checked_add(0xFFFF)? & !0xFFFF)
}

/// `x` rounded up to a page boundary; None when that does not fit in a u64.
pub fn round_up_page(x: u64) -> Option<u64> {
    Some(x.checked_add(PAGE - 1)? & !(PAGE - 1))
}

/// Bytes of `list` (disjoint ranges) that lie inside [s, e).
fn covered(list: &[(u64, u64)], s: u64, e: u64) -> u64 {
    list.iter().map(|&(cs, ce)| ce.min(e).saturating_sub(cs.max(s))).sum()
}

/// The parts of [s, e) that the sorted, disjoint list `list` does not cover, in order.
fn uncovered(list: &[(u64, u64)], s: u64, e: u64) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut at = s;
    for &(cs, ce) in list {
        if ce <= at { continue; }
        if cs >= e { break; }
        if cs > at { out.push((at, cs)); }
        at = at.max(ce);
    }
    if at < e { out.push((at, e)); }
    out
}

/// Sorts a range list and merges entries that overlap or touch.
fn normalize(list: &mut Vec<(u64, u64)>) {
    list.sort_unstable();
    let mut out: Vec<(u64, u64)> = Vec::with_capacity(list.len());
    for &(s, e) in list.iter() {
        if s >= e { continue; }
        match out.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    *list = out;
}

// ---- TLB shootdown (multi-core) ----
// Every core has its own TLB over the shared frames. Before a frame is given
// to another page (eviction, VirtualFree, munmap), the core doing it (holding
// the engine lock) clears that page from every core's TLB, then waits until
// each other core is at a safe point: between two blocks (the run loop), off
// guest code (waiting for the lock, idle, back in JavaScript). A core inside a
// JIT block may be between its TLB check and the access, so it is waited for.
use core::sync::atomic::{AtomicU32, Ordering::*};
const MAXC: usize = 16;
const AWAY: u32 = u32::MAX;
static EPOCH: AtomicU32 = AtomicU32::new(1);
// one cache line per core: these are written between every two blocks
#[repr(align(64))]
struct Seen(AtomicU32);
static SEEN: [Seen; MAXC] = [const { Seen(AtomicU32::new(AWAY)) }; MAXC];
static TLBS: [AtomicU32; MAXC] = [const { AtomicU32::new(0) }; MAXC];
/// The core is between blocks (call at the top of the run loop).
#[inline(always)]
pub fn safe_point(core: u32) {
    if crate::lock::MT {
        let (s, e) = (&SEEN[core as usize].0, EPOCH.load(Acquire));
        if s.load(Relaxed) != e { s.store(e, Release); } // no write (no cache traffic) when nothing changed
    }
}
fn unsafe_tlb_unused(c: usize) -> bool { TLBS[c].load(Acquire) == 0 }
/// The core leaves program code (waits for the lock, sleeps, returns to JavaScript).
#[inline(always)]
#[allow(dead_code)] // used by the multi-core lock (lock.rs) only
pub fn is_away(core: u32) -> bool { crate::lock::MT && SEEN[core as usize].0.load(Relaxed) == AWAY }
#[inline(always)]
pub fn away(core: u32) { if crate::lock::MT { SEEN[core as usize].0.store(AWAY, Release); } }

const NONE: u32 = u32::MAX;
const TLB_SIZE: usize = 8192;

#[derive(Clone, Copy)]
struct Pte { frame: u32, slot: u32 }

#[derive(Clone, Copy)]
struct Frame { vpn: u64, used: bool, referenced: bool, dirty: bool, asid: u32 }

// layout read by JIT code: tag @0, add @8, writable @12 (16 bytes).
// add = host address of the page minus its guest address (mod 2^32): the host
// address of any byte of the page is then just (guest address as u32) + add.
#[derive(Clone, Copy)]
#[repr(C)]
struct TlbEntry { tag: u64, add: u32, writable: bool }

#[derive(Debug, PartialEq, Eq)]
pub struct Fault;

/// What every core shares: the frames of real RAM and the page table.
pub struct MmuShared {
    asid: u32,
    pool: Vec<u8>,
    frames: Vec<Frame>,
    free: Vec<u32>,
    /// freed frames other cores may still reach through a stale TLB entry: (epoch, frame)
    limbo: Vec<(u32, u32)>,
    hand: usize,
    table: HashMap<u64, Pte>,
    regions: Vec<(u64, u64)>,
    pub committed: u64,
    pub target_bytes: u64,
    pub faults: u64,
    pub swap_ins: u64,
    pub swap_outs: u64,
    pub swap_slots: u32,
    /// several address spaces (Linux processes): frames of the others must not be evicted
    pub no_evict: bool,
    /// committed ranges: sorted, disjoint, page-aligned; their total is `committed`.
    /// The write check reads these, not `regions`.
    commits: Vec<(u64, u64)>,
}

/// A saved address space (a Linux process that is not running).
#[derive(Default)]
pub struct Space { table: HashMap<u64, Pte>, regions: Vec<(u64, u64)>, committed: u64, commits: Vec<(u64, u64)> }

/// The page state VirtualQuery reports for one address (see `Mmu::page_run`).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct PageRun {
    /// Start of the region that holds the address (AllocationBase).
    pub region: u64,
    /// True when the page is committed; false when it is reserved only.
    pub committed: bool,
    /// End of the run of pages with the same state, never past the region.
    pub end: u64,
}

/// One core's view: its own TLB over the shared page table.
pub struct Mmu {
    tlb: Vec<TlbEntry>,
    core: u32,
    pub sh: &'static mut MmuShared,
    /// locked instruction in progress (several cores): 0 none, 1 armed, 2 its read
    /// was recorded, 3 its write lost the race (the instruction is run again), 4 written
    pub lk: u8,
    lk_at: (u64, u64, usize),
    /// the CPU state before the locked instruction (to run it again)
    pub lk_snap: Option<Box<crate::cpu::Cpu>>,
}

// ---- x86 memory order (TSO) with several cores ----
// Plain wasm loads/stores on ARM64 may be reordered (a store seen before an
// earlier one), which x86 programs never see. With more than one core, the JIT
// and the interpreter use atomic (sequentially consistent) accesses for
// aligned data: ldar/stlr on ARM64, which keeps x86's order.
static STRONG: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
pub fn set_strong(on: bool) { STRONG.store(on, Relaxed); }
#[inline(always)]
pub fn strong() -> bool { STRONG.load(Relaxed) }

impl Mmu {
    pub fn new(pool_mb: usize, target_mb: u64) -> Mmu {
        let n = pool_mb * 256;
        let sh = Box::leak(Box::new(MmuShared {
            asid: 0,
            pool: vec![0u8; n * PAGE as usize],
            frames: vec![Frame { vpn: 0, used: false, referenced: false, dirty: false, asid: 0 }; n],
            free: (0..n as u32).rev().collect(), limbo: Vec::new(),
            hand: 0,
            table: HashMap::new(),
            regions: Vec::new(), commits: Vec::new(),
            committed: 0,
            target_bytes: target_mb << 20,
            faults: 0, swap_ins: 0, swap_outs: 0, swap_slots: 0, no_evict: false,
        }));
        let m = Mmu { tlb: vec![TlbEntry { tag: u64::MAX, add: 0, writable: false }; TLB_SIZE], core: 0, sh, lk: 0, lk_at: (0, 0, 0), lk_snap: None };
        TLBS[0].store(m.tlb.as_ptr() as u32, Release);
        m
    }

    /// Another core's view of the same memory (empty TLB).
    pub fn new_core(&self, core: u32) -> Mmu {
        let sh = unsafe { &mut *(self.sh as *const MmuShared as *mut MmuShared) };
        let m = Mmu { tlb: vec![TlbEntry { tag: u64::MAX, add: 0, writable: false }; TLB_SIZE], core, sh, lk: 0, lk_at: (0, 0, 0), lk_snap: None };
        TLBS[core as usize].store(m.tlb.as_ptr() as u32, Release);
        m
    }

    /// Removes page `vpn` from every core's TLB (engine lock held).
    fn shoot(&mut self, vpn: u64) {
        let t = (vpn as usize) & (TLB_SIZE - 1);
        if self.tlb[t].tag == vpn { self.tlb[t].tag = u64::MAX; }
        if !crate::lock::MT { return; }
        for c in 0..MAXC {
            let p = TLBS[c].load(Acquire);
            if p == 0 || c as u32 == self.core { continue; }
            unsafe {
                let e = (p as *mut TlbEntry).add(t);
                if core::ptr::read_volatile(&(*e).tag) == vpn { core::ptr::write_volatile(&mut (*e).tag, u64::MAX); }
            }
        }
    }
    /// Waits until no other core can still be using a TLB entry cleared by shoot().
    fn shoot_wait(&self) {
        if !crate::lock::MT { return; }
        let ep = EPOCH.fetch_add(1, AcqRel) + 1;
        for c in 0..MAXC {
            if c as u32 == self.core { continue; }
            while { let s = SEEN[c].0.load(Acquire); s != AWAY && s < ep } { core::hint::spin_loop(); }
        }
    }

    /// Addresses the JIT bakes into its code (both never move).
    pub fn tlb_addr(&self) -> u32 { self.tlb.as_ptr() as u32 }
    pub fn pool_addr(&self) -> u32 { self.sh.pool.as_ptr() as u32 }
    pub const TLB_MASK: u32 = (TLB_SIZE - 1) as u32;

    pub fn frames_total(&self) -> usize { self.sh.frames.len() }
    pub fn resident(&self) -> usize { self.sh.frames.len() - self.sh.free.len() }

    /// A new committed region (image, stack, alloc). Fails with nothing changed when the
    /// range does not fit in a u64. The memory target is not checked here: callers that
    /// allocate on demand (alloc, VirtualAlloc) check it before they call this.
    pub fn add_region(&mut self, start: u64, size: u64) -> Option<()> {
        let end = start.checked_add(size)?;
        let (s, e) = page_span(start, size)?;
        let added = (e - s) - covered(&self.sh.commits, s, e);
        let total = self.sh.committed.checked_add(added)?;
        self.sh.regions.push((start, end));
        self.sh.commits.push((s, e));
        normalize(&mut self.sh.commits);
        self.sh.committed = total;
        Some(())
    }
    /// Address space only (MEM_RESERVE): not committed, not counted against the memory
    /// target. Not in `commits`, so the range check refuses it. Fails with nothing changed
    /// when the range does not fit in a u64.
    pub fn add_reserved(&mut self, start: u64, size: u64) -> Option<()> {
        let end = start.checked_add(size)?;
        self.sh.regions.push((start, end));
        Some(())
    }

    /// True when [s, e) lies wholly inside one region (reserved or committed).
    fn inside_region(&self, s: u64, e: u64) -> bool {
        self.sh.regions.iter().any(|&(rs, re)| rs <= s && e <= re)
    }

    /// True when page `vpn` lies inside a committed range. `commits` is sorted and disjoint,
    /// so one binary search decides it. Reserved-only address space is not committed.
    fn page_committed(&self, vpn: u64) -> bool {
        let a = vpn << 12;
        let i = self.sh.commits.partition_point(|&(s, _)| s <= a);
        i > 0 && a < self.sh.commits[i - 1].1
    }

    /// True when every page touched by `a..a+n` is committed (MEM_COMMIT or add_region).
    /// Reserved-only address space is refused.
    /// Read-only: it does not materialize pages, count a fault, or touch frames.
    /// A range whose end overflows u64 is refused.
    pub fn can_write_range(&self, a: u64, n: u64) -> bool {
        if a == 0 || n == 0 {
            return false;
        }
        let Some(last) = a.checked_add(n - 1) else { return false };
        let last_page = last & !(PAGE - 1);
        let mut page = a & !(PAGE - 1);
        loop {
            if !self.sh.commits.iter().any(|&(s, e)| page >= s && page < e) {
                return false;
            }
            if page == last_page {
                return true;
            }
            match page.checked_add(PAGE) {
                Some(next) => page = next,
                None => return false,
            }
        }
    }

    /// Bytes committed in this address space (the commit counter).
    pub fn committed_bytes(&self) -> u64 { self.sh.committed }

    /// The region (start, end) holding `addr`, if any.
    pub fn region_of(&self, addr: u64) -> Option<(u64, u64)> {
        self.sh.regions.iter().copied().find(|&(s, e)| addr >= s && addr < e)
    }

    /// What VirtualQuery reports for `addr`, from the commit list: its region, whether the
    /// page is committed (false: reserved only), and the end of the run of pages with the same
    /// state, from the page of `addr` on, never past the region. None when no region holds addr.
    pub fn page_run(&self, addr: u64) -> Option<PageRun> {
        let (region, region_end) = self.region_of(addr)?;
        let i = self.sh.commits.partition_point(|&(s, _)| s <= addr);
        let committed = i > 0 && addr < self.sh.commits[i - 1].1;
        let end = if committed {
            self.sh.commits[i - 1].1.min(region_end)
        } else {
            self.sh.commits.get(i).map_or(region_end, |&(s, _)| s.min(region_end))
        };
        Some(PageRun { region, committed, end })
    }
    /// The lowest 64 KB aligned address in [from, limit) with `size` free bytes and `gap`
    /// free bytes after them.
    pub fn first_gap(&self, from: u64, limit: u64, size: u64, gap: u64) -> Option<u64> {
        let mut r: Vec<(u64, u64)> = self.sh.regions.iter().copied().filter(|&(_, e)| e > from).collect();
        r.sort_unstable();
        let mut a = from;
        for (s, e) in r {
            if a.checked_add(size)?.checked_add(gap)? <= s { break; }
            a = a.max(round_up_64k(e.checked_add(gap)?)?);
        }
        (a.checked_add(size)? <= limit).then_some(a)
    }
    /// The end of the furthest region that [start, end) runs into, if any.
    pub fn overlap_end(&self, start: u64, end: u64) -> Option<u64> {
        self.sh.regions.iter().filter(|&&(s, e)| start < e && end > s).map(|&(_, e)| e).max()
    }
    pub fn overlaps(&self, start: u64, end: u64) -> bool {
        self.sh.regions.iter().any(|&(s, e)| start < e && end > s)
    }

    /// Drops the pages of [start, end): their frames go back to the pool and
    /// their swap copies are forgotten, so they read as zero next time.
    pub fn drop_pages(&mut self, start: u64, end: u64) {
        let (first, last) = (start >> 12, (end + PAGE - 1) >> 12);
        let vpns: Vec<u64> = if last - first > self.sh.table.len() as u64 {
            self.sh.table.keys().copied().filter(|v| *v >= first && *v < last).collect()
        } else {
            (first..last).filter(|v| self.sh.table.contains_key(v)).collect()
        };
        let mut freed = Vec::new();
        for vpn in vpns {
            let pte = self.sh.table.remove(&vpn).unwrap();
            self.shoot(vpn);
            if pte.frame != NONE { freed.push(pte.frame); }
        }
        if freed.is_empty() { return; }
        if !crate::lock::MT {
            for f in freed { self.sh.frames[f as usize].used = false; self.sh.free.push(f); }
            return;
        }
        // no waiting here (a core may be deep in a JIT slice): the frames are
        // reused once every other core has passed a safe point (reclaim)
        let ep = EPOCH.fetch_add(1, AcqRel) + 1;
        for f in freed { self.sh.limbo.push((ep, f)); }
    }

    /// The bytes that committing [s, e) adds, and the counter after it. Fails when the
    /// counter would pass the memory target (8) or overflow (8); nothing is changed.
    fn commit_cost(&self, s: u64, e: u64) -> Result<(u64, u64), u32> {
        // covered() counts only bytes inside [s, e), so the subtraction cannot underflow
        let added = (e - s) - covered(&self.sh.commits, s, e);
        let total = self.sh.committed.checked_add(added).ok_or(8u32)?;
        if total > self.sh.target_bytes { return Err(8); }
        Ok((added, total))
    }

    /// Marks [s, e) committed. Returns the bytes that were not committed before, so an
    /// overlapping or repeated commit is not counted twice.
    fn commit_span(&mut self, s: u64, e: u64) -> Result<u64, u32> {
        if s >= e { return Ok(0); }
        let (added, total) = self.commit_cost(s, e)?;
        self.sh.commits.push((s, e));
        normalize(&mut self.sh.commits);
        self.sh.committed = total;
        Ok(added)
    }

    /// Marks [s, e) not committed, splitting ranges that are only partly covered.
    /// Returns the bytes that were committed before, so only those leave the counter.
    fn uncommit_span(&mut self, s: u64, e: u64) -> u64 {
        if s >= e { return 0; }
        let removed = covered(&self.sh.commits, s, e);
        if removed == 0 { return 0; }
        let mut kept = Vec::with_capacity(self.sh.commits.len() + 1);
        for &(cs, ce) in &self.sh.commits {
            if ce <= s || cs >= e { kept.push((cs, ce)); continue; }
            if cs < s { kept.push((cs, s)); }
            if ce > e { kept.push((e, ce)); }
        }
        self.sh.commits = kept;
        // the committed bytes are inside the counter by construction; a shortfall is a bug
        self.sh.committed = self.sh.committed.checked_sub(removed).expect("commit counter below its ranges");
        removed
    }

    /// VirtualAlloc MEM_COMMIT over an address range. The whole page span must lie
    /// inside one region (reserved or committed), and the commit must fit the memory
    /// target. When either check fails nothing changes. Ok: bytes newly committed;
    /// Err: the Windows error code (487 invalid address, 8 not enough memory).
    pub fn commit_in_region(&mut self, start: u64, size: u64) -> Result<u64, u32> {
        let (s, e) = page_span(start, size).ok_or(487u32)?;
        if !self.inside_region(s, e) { return Err(487); }
        self.commit_span(s, e)
    }

    /// VirtualAlloc / NtAllocateVirtualMemory at a fixed address: reserves the 64 KB span that
    /// `fixed_span` gives for `a..a+size`, and commits the pages of that range when `commit`
    /// is set. The span must not overlap a region. A commit that fails releases the new
    /// reservation, so nothing is left behind. Ok: the reserved start. Err: 487 (overlap or a
    /// range that does not fit in a u64) or 8 (memory target), with nothing changed.
    pub fn reserve_fixed(&mut self, a: u64, size: u64, commit: bool) -> Result<u64, u32> {
        let (start, end) = fixed_span(a, size).ok_or(487u32)?;
        if self.overlaps(start, end) { return Err(487); }
        self.add_reserved(start, end - start).ok_or(487u32)?;
        if commit {
            if let Err(code) = self.commit_in_region(a, size) {
                self.release_region(start);
                return Err(code);
            }
        }
        Ok(start)
    }

    /// Commits the pages that `start..start+len` touches, before the engine writes to that
    /// range. Only those pages are committed, never the whole region. An empty range changes
    /// nothing. Ok: bytes newly committed. Err: 487 (outside a region, or the range does not
    /// fit in a u64) or 8 (memory target), with nothing changed.
    ///
    /// The pages are then made resident, so a later write to them needs no frame. When a page
    /// cannot get a frame the call fails with 8. On that failure:
    /// - restored: the commit list, the commit counter, and the PTEs and frames of the pages
    ///   this call committed for the first time (they are dropped and uncommitted again);
    /// - not restored: pages committed before the call that the loop made resident (their PTE
    ///   and frame stay), frames taken from other pages by eviction, and the fault and swap
    ///   counters. None of these changes the contents a program reads from a committed page.
    /// Pages not committed are never made resident: `fill` refuses them.
    pub fn commit_for_write(&mut self, start: u64, len: u64) -> Result<u64, u32> {
        if len == 0 { return Ok(0); }
        let (s, e) = page_span(start, len).ok_or(487u32)?;
        if !self.inside_region(s, e) { return Err(487); }
        // only these parts were not committed before, so only these may be taken back
        let fresh = uncovered(&self.sh.commits, s, e);
        let added = self.commit_span(s, e)?;
        let mut p = s;
        while p < e {
            if self.tr(p, true).is_err() {
                for &(fs, fe) in &fresh {
                    self.drop_pages(fs, fe);
                    self.uncommit_span(fs, fe);
                }
                return Err(8);
            }
            p += PAGE;
        }
        Ok(added)
    }

    /// VirtualFree MEM_DECOMMIT. With a size, decommits [start, start+size): the whole
    /// range must lie inside one region. With size 0, `start` must be the base address of
    /// a region, and the whole region is decommitted; the reservation stays. Every check
    /// is made before anything changes, so a refused call leaves no partial state.
    /// Ok: bytes that were committed and are now decommitted. Err: 487 invalid address.
    pub fn decommit_range(&mut self, start: u64, size: u64) -> Result<u64, u32> {
        let (s, e) = if size == 0 {
            let (rs, re) = *self.sh.regions.iter().find(|r| r.0 == start).ok_or(487u32)?;
            (rs, re)
        } else {
            let (s, e) = page_span(start, size).ok_or(487u32)?;
            if !self.inside_region(s, e) { return Err(487); }
            (s, e)
        };
        let removed = self.uncommit_span(s, e);
        self.drop_pages(s, e);
        Ok(removed)
    }

    /// Releases a whole region (VirtualFree MEM_RELEASE): its pages and its committed
    /// ranges. `start` must be the region's base address. Only the committed bytes leave
    /// the counter, never the reserved size.
    pub fn release_region(&mut self, start: u64) -> bool {
        let Some(i) = self.sh.regions.iter().position(|&(s, _)| s == start) else { return false };
        let (s, e) = self.sh.regions.swap_remove(i);
        let removed = covered(&self.sh.commits, s, e);
        self.sh.commits.retain(|&(cs, ce)| ce <= s || cs >= e);
        self.drop_pages(s, e);
        self.sh.committed = self.sh.committed.checked_sub(removed).expect("commit counter below its ranges");
        true
    }

    /// Which process's pages new frames belong to (eviction only takes the running one's).
    pub fn set_asid(&mut self, a: u32) { self.sh.asid = a; }
    pub fn flush_tlb(&mut self) { for t in self.tlb.iter_mut() { t.tag = u64::MAX; } }

    /// Moves the current address space out (another process is going to run).
    pub fn take_space(&mut self) -> Space {
        self.flush_tlb();
        Space { table: std::mem::take(&mut self.sh.table), regions: std::mem::take(&mut self.sh.regions), committed: std::mem::replace(&mut self.sh.committed, 0), commits: std::mem::take(&mut self.sh.commits) }
    }
    pub fn put_space(&mut self, sp: Space) {
        self.flush_tlb();
        self.sh.table = sp.table; self.sh.regions = sp.regions; self.sh.committed = sp.committed; self.sh.commits = sp.commits;
    }
    /// A full copy of the current address space (fork); None = out of real RAM.
    pub fn clone_space(&mut self) -> Option<Space> {
        let mut table = HashMap::with_capacity(self.sh.table.len());
        let entries: Vec<(u64, Pte)> = self.sh.table.iter().map(|(k, v)| (*k, *v)).collect();
        for (vpn, pte) in entries {
            if pte.frame == NONE { continue; }
            let Some(f) = self.sh.free.pop() else {
                for (_, p) in table.drain() { let p: Pte = p; self.sh.frames[p.frame as usize].used = false; self.sh.free.push(p.frame); }
                return None;
            };
            let (src, dst) = (pte.frame as usize * PAGE as usize, f as usize * PAGE as usize);
            self.sh.pool.copy_within(src..src + PAGE as usize, dst);
            self.sh.frames[f as usize] = Frame { vpn, used: true, referenced: false, dirty: true, asid: self.sh.asid };
            table.insert(vpn, Pte { frame: f, slot: NONE });
        }
        Some(Space { table, regions: self.sh.regions.clone(), committed: self.sh.committed, commits: self.sh.commits.clone() })
    }
    /// Frees every page of the current address space (exec, exit).
    pub fn free_space(&mut self) {
        self.flush_tlb();
        for (_, p) in self.sh.table.drain() {
            if p.frame != NONE { self.sh.frames[p.frame as usize].used = false; self.sh.free.push(p.frame); }
        }
        self.sh.regions.clear();
        self.sh.commits.clear();
        self.sh.committed = 0;
    }
    /// Frees a saved address space (a process that ended while not running).
    pub fn free_saved(&mut self, sp: Space) {
        for (_, p) in sp.table { if p.frame != NONE { self.sh.frames[p.frame as usize].used = false; self.sh.free.push(p.frame); } }
    }

    fn evict_one(&mut self) -> u32 {
        loop {
            let i = self.sh.hand;
            self.sh.hand = (self.sh.hand + 1) % self.sh.frames.len();
            let f = self.sh.frames[i];
            if !f.used { return i as u32; }
            // a page of a parked process (Windows child processes): its table is not here
            if f.asid != self.sh.asid { continue; }
            let t = (f.vpn as usize) & (TLB_SIZE - 1);
            if f.referenced {
                // second chance; drop the TLB entry so the next access marks it again
                self.sh.frames[i].referenced = false;
                if self.tlb[t].tag == f.vpn { self.tlb[t].tag = u64::MAX; }
                continue;
            }
            self.shoot(f.vpn);
            self.shoot_wait();
            let pte = self.sh.table.get_mut(&f.vpn).unwrap();
            if f.dirty || pte.slot == NONE {
                if pte.slot == NONE { pte.slot = self.sh.swap_slots; self.sh.swap_slots += 1; }
                unsafe { host_swap_write(pte.slot, self.sh.pool.as_ptr().add(i * PAGE as usize)); }
                self.sh.swap_outs += 1;
            }
            pte.frame = NONE;
            self.sh.frames[i].used = false;
            return i as u32;
        }
    }

    /// Moves freed frames that no core can reach any more to the free list.
    fn reclaim(&mut self) {
        if self.sh.limbo.is_empty() { return; }
        let mut low = u32::MAX; // oldest epoch some other core may still be before
        for c in 0..MAXC {
            if c as u32 == self.core || unsafe_tlb_unused(c) { continue; }
            let s = SEEN[c].0.load(Acquire);
            if s != AWAY { low = low.min(s); }
        }
        let sh = &mut *self.sh;
        let mut i = 0;
        while i < sh.limbo.len() {
            let (ep, f) = sh.limbo[i];
            if ep <= low { sh.limbo.swap_remove(i); sh.frames[f as usize].used = false; sh.free.push(f); } else { i += 1; }
        }
    }

    fn get_frame(&mut self) -> Option<u32> {
        if self.sh.free.is_empty() { self.reclaim(); }
        if self.sh.free.is_empty() && !self.sh.limbo.is_empty() { self.shoot_wait(); self.reclaim(); }
        match self.sh.free.pop() {
            Some(f) => Some(f),
            // any core may evict: its swap I/O goes through the swap worker (swapio.js)
            None if self.sh.no_evict => None,
            None => Some(self.evict_one()),
        }
    }

    #[cold]
    fn fill(&mut self, vpn: u64, write: bool) -> Result<usize, Fault> {
        let _g = crate::lock::lock();
        // The commit rule, on the real access path: a page is materialized only when it is
        // committed. This runs first, so a refused access changes no counter, PTE, frame or
        // swap slot, and an existing PTE for an uncommitted page cannot be used either.
        if !self.page_committed(vpn) { return Err(Fault); }
        self.sh.faults += 1;
        let pte = match self.sh.table.get(&vpn) {
            Some(p) => *p,
            None => {
                let p = Pte { frame: NONE, slot: NONE };
                self.sh.table.insert(vpn, p);
                p
            }
        };
        let frame = if pte.frame != NONE {
            pte.frame
        } else {
            let Some(f) = self.get_frame() else { return Err(Fault) };
            let off = f as usize * PAGE as usize;
            if pte.slot != NONE {
                unsafe { host_swap_read(pte.slot, self.sh.pool.as_mut_ptr().add(off)); }
                self.sh.swap_ins += 1;
            } else {
                self.sh.pool[off..off + PAGE as usize].fill(0);
            }
            self.sh.frames[f as usize] = Frame { vpn, used: true, referenced: false, dirty: false, asid: self.sh.asid };
            self.sh.table.get_mut(&vpn).unwrap().frame = f;
            f
        };
        let fr = &mut self.sh.frames[frame as usize];
        fr.referenced = true;
        if write { fr.dirty = true; }
        let off = frame as usize * PAGE as usize;
        let add = (self.sh.pool.as_ptr() as u32).wrapping_add(off as u32).wrapping_sub((vpn << 12) as u32);
        self.tlb[(vpn as usize) & (TLB_SIZE - 1)] = TlbEntry { tag: vpn, add, writable: write || fr.dirty };
        Ok(off)
    }

    #[inline(always)]
    fn tr(&mut self, addr: u64, write: bool) -> Result<usize, Fault> {
        let vpn = addr >> 12;
        let e = self.tlb[(vpn as usize) & (TLB_SIZE - 1)];
        if e.tag == vpn && (!write || e.writable) {
            return Ok(e.add.wrapping_add(addr as u32).wrapping_sub(self.sh.pool.as_ptr() as u32) as usize);
        }
        Ok(self.fill(vpn, write)? + (addr & 0xFFF) as usize)
    }

    /// Aligned atomic load of N bytes at pool offset o.
    #[inline(always)]
    fn ld_atomic<const N: usize>(&self, o: usize) -> u64 {
        use core::sync::atomic::*;
        let p = unsafe { self.sh.pool.as_ptr().add(o) };
        unsafe { match N {
            1 => (*(p as *const AtomicU8)).load(SeqCst) as u64,
            2 => (*(p as *const AtomicU16)).load(SeqCst) as u64,
            4 => (*(p as *const AtomicU32)).load(SeqCst) as u64,
            _ => (*(p as *const AtomicU64)).load(SeqCst),
        } }
    }
    #[inline(always)]
    fn st_atomic<const N: usize>(&self, o: usize, v: u64) {
        use core::sync::atomic::*;
        let p = unsafe { self.sh.pool.as_ptr().add(o) };
        unsafe { match N {
            1 => (*(p as *const AtomicU8)).store(v as u8, SeqCst),
            2 => (*(p as *const AtomicU16)).store(v as u16, SeqCst),
            4 => (*(p as *const AtomicU32)).store(v as u32, SeqCst),
            _ => (*(p as *const AtomicU64)).store(v, SeqCst),
        } }
    }
    /// Compare-and-swap of N bytes at pool offset o; true = it was `old` and is now `new`.
    fn cas<const N: usize>(&self, o: usize, old: u64, new: u64) -> bool {
        use core::sync::atomic::*;
        let p = unsafe { self.sh.pool.as_ptr().add(o) };
        unsafe { match N {
            1 => (*(p as *const AtomicU8)).compare_exchange(old as u8, new as u8, SeqCst, SeqCst).is_ok(),
            2 => (*(p as *const AtomicU16)).compare_exchange(old as u16, new as u16, SeqCst, SeqCst).is_ok(),
            4 => (*(p as *const AtomicU32)).compare_exchange(old as u32, new as u32, SeqCst, SeqCst).is_ok(),
            _ => (*(p as *const AtomicU64)).compare_exchange(old, new, SeqCst, SeqCst).is_ok(),
        } }
    }

    #[inline(always)]
    fn rd<const N: usize>(&mut self, addr: u64) -> Result<u64, Fault> {
        if (addr & 0xFFF) as usize <= 4096 - N {
            let o = self.tr(addr, false)?;
            if crate::lock::MT && (self.lk != 0 || strong()) && o % N == 0 {
                let v = self.ld_atomic::<N>(o);
                if self.lk == 1 { self.lk = 2; self.lk_at = (addr, v, N); }
                return Ok(v);
            }
            let mut b = [0u8; 8];
            b[..N].copy_from_slice(&self.sh.pool[o..o + N]);
            Ok(u64::from_le_bytes(b))
        } else {
            let mut v = 0u64;
            for i in 0..N { v |= (self.read8(addr + i as u64)? as u64) << (8 * i); }
            Ok(v)
        }
    }

    #[inline(always)]
    fn wr<const N: usize>(&mut self, addr: u64, v: u64) -> Result<(), Fault> {
        if (addr & 0xFFF) as usize <= 4096 - N {
            let o = self.tr(addr, true)?;
            if crate::lock::MT && (self.lk != 0 || strong()) && o % N == 0 {
                match self.lk {
                    3 => return Ok(()), // lost the race: nothing more is written
                    2 if self.lk_at.0 == addr && self.lk_at.2 == N => {
                        // the locked read-modify-write: only if no other core changed it meanwhile
                        self.lk = if self.cas::<N>(o, self.lk_at.1, v) { 4 } else { 3 };
                        return Ok(());
                    }
                    _ => { self.st_atomic::<N>(o, v); return Ok(()); }
                }
            }
            self.sh.pool[o..o + N].copy_from_slice(&v.to_le_bytes()[..N]);
            Ok(())
        } else {
            for i in 0..N { self.write8(addr + i as u64, (v >> (8 * i)) as u8)?; }
            Ok(())
        }
    }

    #[inline(always)]
    pub fn read8(&mut self, a: u64) -> Result<u8, Fault> {
        if crate::lock::MT && (self.lk != 0 || strong()) { return Ok(self.rd::<1>(a)? as u8); }
        let o = self.tr(a, false)?; Ok(self.sh.pool[o])
    }
    #[inline(always)]
    pub fn write8(&mut self, a: u64, v: u8) -> Result<(), Fault> {
        if crate::lock::MT && (self.lk != 0 || strong()) { return self.wr::<1>(a, v as u64); }
        let o = self.tr(a, true)?; self.sh.pool[o] = v; Ok(())
    }
    pub fn read16(&mut self, a: u64) -> Result<u16, Fault> { Ok(self.rd::<2>(a)? as u16) }
    pub fn read32(&mut self, a: u64) -> Result<u32, Fault> { Ok(self.rd::<4>(a)? as u32) }
    pub fn read64(&mut self, a: u64) -> Result<u64, Fault> { self.rd::<8>(a) }
    pub fn write16(&mut self, a: u64, v: u16) -> Result<(), Fault> { self.wr::<2>(a, v as u64) }
    pub fn write32(&mut self, a: u64, v: u32) -> Result<(), Fault> { self.wr::<4>(a, v as u64) }
    pub fn write64(&mut self, a: u64, v: u64) -> Result<(), Fault> { self.wr::<8>(a, v) }

    pub fn read_sized(&mut self, a: u64, size: u32) -> Result<u64, Fault> {
        match size { 8 => Ok(self.read8(a)? as u64), 16 => self.rd::<2>(a), 32 => self.rd::<4>(a), _ => self.rd::<8>(a) }
    }
    pub fn write_sized(&mut self, a: u64, size: u32, v: u64) -> Result<(), Fault> {
        match size { 8 => self.write8(a, v as u8), 16 => self.wr::<2>(a, v), 32 => self.wr::<4>(a, v), _ => self.wr::<8>(a, v) }
    }

    /// Address in wasm memory of a program byte (for Wine code compiled to wasm).
    #[inline(always)]
    pub fn host_addr(&mut self, a: u64, write: bool) -> Result<u64, Fault> {
        let o = self.tr(a, write)?;
        Ok(self.sh.pool.as_ptr() as u64 + o as u64)
    }

    /// Bulk copy into program memory, one page at a time.
    pub fn write_bytes(&mut self, mut a: u64, mut data: &[u8]) -> Result<(), Fault> {
        while !data.is_empty() {
            let room = (4096 - (a & 0xFFF) as usize).min(data.len());
            let o = self.tr(a, true)?;
            self.sh.pool[o..o + room].copy_from_slice(&data[..room]);
            a += room as u64;
            data = &data[room..];
        }
        Ok(())
    }
    pub fn read_bytes(&mut self, mut a: u64, out: &mut [u8]) -> Result<(), Fault> {
        let mut k = 0;
        while k < out.len() {
            let room = (4096 - (a & 0xFFF) as usize).min(out.len() - k);
            let o = self.tr(a, false)?;
            out[k..k + room].copy_from_slice(&self.sh.pool[o..o + room]);
            a += room as u64;
            k += room;
        }
        Ok(())
    }

    /// Copies up to 16 instruction bytes starting at `a` (stops at a bad page).
    pub fn fetch16(&mut self, a: u64, out: &mut [u8; 16]) -> Result<(), Fault> {
        let room = 4096 - (a & 0xFFF) as usize;
        let o = self.tr(a, false)?;
        if room >= 16 {
            out.copy_from_slice(&self.sh.pool[o..o + 16]);
        } else {
            out[..room].copy_from_slice(&self.sh.pool[o..o + room]);
            if let Ok(o2) = self.tr(a + room as u64, false) {
                out[room..].copy_from_slice(&self.sh.pool[o2..o2 + 16 - room]);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod access_tests {
    //! The real access path (read8/write8 through tr and fill), not the range helpers.
    use super::*;

    const A: u64 = 0x5E00_0000;

    fn check(m: &Mmu) {
        let sum: u64 = m.sh.commits.iter().map(|&(s, e)| e - s).sum();
        assert_eq!(m.sh.committed, sum, "committed counter and commit list disagree");
    }

    fn frames_used(m: &Mmu) -> usize {
        m.sh.frames.iter().filter(|f| f.used).count()
    }

    #[test]
    fn reserved_read_faults_and_materializes_nothing() {
        let mut m = Mmu::new(1, 8);
        m.add_reserved(A, 0x1_0000);
        let (pages, faults, frames, committed) = (m.sh.table.len(), m.sh.faults, frames_used(&m), m.sh.committed);
        assert!(m.read8(A).is_err());
        assert!(m.read64(A + 0x2000).is_err());
        assert_eq!(m.sh.table.len(), pages, "a refused read created a PTE");
        assert_eq!(m.sh.faults, faults, "a refused read counted a fault");
        assert_eq!(frames_used(&m), frames, "a refused read took a frame");
        assert_eq!(m.sh.committed, committed);
        assert!(m.sh.commits.is_empty());
        check(&m);
    }

    #[test]
    fn reserved_write_faults_without_effects() {
        let mut m = Mmu::new(1, 8);
        m.add_reserved(A, 0x1_0000);
        let (pages, faults, frames) = (m.sh.table.len(), m.sh.faults, frames_used(&m));
        assert!(m.write8(A, 0x5A).is_err());
        assert!(m.write_bytes(A + 0x1000, &[1, 2, 3]).is_err());
        assert_eq!(m.sh.table.len(), pages, "a refused write created a PTE");
        assert_eq!(m.sh.faults, faults, "a refused write counted a fault");
        assert_eq!(frames_used(&m), frames, "a refused write took a frame");
        assert!(m.sh.commits.is_empty());
        check(&m);
    }

    #[test]
    fn commit_allows_read_and_write_and_starts_zeroed() {
        let mut m = Mmu::new(1, 8);
        m.add_reserved(A, 0x1_0000);
        assert!(m.read8(A).is_err());
        assert!(m.write8(A, 1).is_err());
        m.commit_in_region(A, 0x1000).unwrap();
        assert_eq!(m.read8(A), Ok(0), "first read of a committed page is zero");
        m.write8(A, 0x77).unwrap();
        assert_eq!(m.read8(A), Ok(0x77));
        // the neighbouring page is still reserved only
        assert!(m.read8(A + 0x1000).is_err());
        assert!(m.write8(A + 0x1000, 1).is_err());
        check(&m);
    }

    #[test]
    fn decommit_blocks_access_again_and_recommit_is_zeroed() {
        let mut m = Mmu::new(1, 8);
        m.add_reserved(A, 0x1_0000);
        m.commit_in_region(A, 0x1_0000).unwrap();
        m.write8(A, 0x5A).unwrap();
        assert_eq!(m.read8(A), Ok(0x5A));
        m.decommit_range(A, 0x1000).unwrap();
        assert!(m.read8(A).is_err(), "a decommitted page still reads");
        assert!(m.write8(A, 1).is_err(), "a decommitted page still writes");
        assert!(m.sh.table.get(&(A >> 12)).is_none(), "decommit left a PTE behind");
        m.commit_in_region(A, 0x1000).unwrap();
        assert_eq!(m.read8(A), Ok(0), "recommitted page kept the old contents");
        check(&m);
    }

    #[test]
    fn address_spaces_do_not_share_commit_state() {
        let mut m = Mmu::new(1, 8);
        // space 1: commit and write
        m.add_reserved(A, 0x1000);
        m.commit_in_region(A, 0x1000).unwrap();
        m.write8(A, 0x11).unwrap();
        let sp1 = m.take_space();
        // space 2: the same virtual range is only reserved
        m.add_reserved(A, 0x1000);
        assert!(m.read8(A).is_err(), "space 2 read a page committed only in space 1");
        assert!(m.write8(A, 1).is_err(), "space 2 wrote a page committed only in space 1");
        m.commit_in_region(A, 0x1000).unwrap();
        m.write8(A, 0x22).unwrap();
        let sp2 = m.take_space();
        // back to space 1: its own value, and its commit still applies
        m.put_space(sp1);
        assert_eq!(m.read8(A), Ok(0x11));
        // back to space 2
        let _ = m.take_space();
        m.put_space(sp2);
        assert_eq!(m.read8(A), Ok(0x22));
        check(&m);
    }

    #[test]
    fn commit_for_write_commits_only_the_pages_touched() {
        let mut m = Mmu::new(1, 8);
        m.add_reserved(A, 0x1_0000);
        // 200 bytes that straddle the page boundary at A+0x1000: two pages, not the region
        assert_eq!(m.commit_for_write(A + 0xF90, 200), Ok(0x2000));
        assert_eq!(m.sh.committed, 0x2000);
        m.write8(A + 0xF90, 1).unwrap();
        m.write8(A + 0x1080, 2).unwrap();
        // the page after them is still reserved only
        assert!(m.write8(A + 0x2000, 3).is_err());
        // a repeat adds nothing
        assert_eq!(m.commit_for_write(A + 0xF90, 200), Ok(0));
        check(&m);
    }

    #[test]
    fn commit_for_write_refuses_bad_ranges_without_changes() {
        let mut m = Mmu::new(1, 8);
        m.add_reserved(A, 0x1000);
        let committed = m.sh.committed;
        assert_eq!(m.commit_for_write(A, 0), Ok(0)); // empty range
        assert_eq!(m.commit_for_write(A + 0x1000, 8), Err(487)); // after the region
        assert_eq!(m.commit_for_write(A - 8, 16), Err(487)); // starts before the region
        assert_eq!(m.commit_for_write(u64::MAX - 4, 100), Err(487)); // end overflows u64
        assert_eq!(m.sh.committed, committed);
        assert!(m.sh.commits.is_empty());
        check(&m);
    }

    #[test]
    fn commit_for_write_respects_the_memory_target() {
        let mut m = Mmu::new(1, 1); // target 1 MB
        m.add_reserved(A, 0x20_0000);
        assert_eq!(m.commit_for_write(A, 0x20_0000), Err(8));
        assert_eq!(m.sh.committed, 0);
        assert!(m.sh.commits.is_empty());
        check(&m);
    }

    /// Commits and touches `pages` pages from `base`, so each one holds a frame.
    fn fill_frames(m: &mut Mmu, base: u64, pages: u64) {
        m.add_reserved(base, pages * PAGE);
        m.commit_in_region(base, pages * PAGE).unwrap();
        for i in 0..pages {
            m.read8(base + i * PAGE).unwrap();
        }
    }

    #[test]
    fn commit_for_write_without_a_frame_changes_nothing() {
        let mut m = Mmu::new(1, 8); // 256 frames
        fill_frames(&mut m, A, 256);
        m.sh.no_evict = true; // a Linux child: frames are never taken from other pages
        m.add_reserved(A + 0x10_0000, PAGE);
        let (committed, commits, frames) = (m.sh.committed, m.sh.commits.clone(), frames_used(&m));
        assert_eq!(m.commit_for_write(A + 0x10_0000, PAGE), Err(8));
        assert_eq!(m.sh.committed, committed);
        assert_eq!(m.sh.commits, commits);
        assert_eq!(frames_used(&m), frames);
        assert!(m.sh.table.get(&((A + 0x10_0000) >> 12)).is_none(), "a failed prepare left a PTE");
        check(&m);
    }

    #[test]
    fn partial_prepare_drops_only_its_own_fresh_pages() {
        let mut m = Mmu::new(1, 8);
        fill_frames(&mut m, A, 255); // one frame left
        m.sh.no_evict = true;
        // an old page, committed but not resident, before the call: it must stay committed
        let old = A + 0x20_0000;
        m.add_reserved(old, PAGE);
        m.commit_in_region(old, PAGE).unwrap();
        // two fresh pages: the first takes the last frame, the second gets none
        let new = A + 0x30_0000;
        m.add_reserved(new, 2 * PAGE);
        let (committed, commits, frames) = (m.sh.committed, m.sh.commits.clone(), frames_used(&m));
        assert_eq!(m.commit_for_write(new, 2 * PAGE), Err(8));
        assert_eq!(m.sh.committed, committed, "a failed prepare changed the commit count");
        assert_eq!(m.sh.commits, commits, "a failed prepare changed the commit list");
        assert_eq!(frames_used(&m), frames, "the first page's frame was not returned");
        assert!(m.sh.table.get(&(new >> 12)).is_none(), "a dropped page left a PTE");
        // the old page is still committed: its first touch gets the returned frame and reads zero
        assert_eq!(m.read8(old), Ok(0));
        check(&m);
    }

    #[test]
    fn region_is_reused_after_a_failed_prepare() {
        let mut m = Mmu::new(1, 8);
        fill_frames(&mut m, A, 256);
        m.sh.no_evict = true;
        let b = A + 0x40_0000;
        m.add_reserved(b, PAGE);
        assert_eq!(m.commit_for_write(b, PAGE), Err(8));
        // give the frames back, then the same range is reserved and prepared again
        m.drop_pages(A, A + 256 * PAGE);
        assert!(m.release_region(b));
        m.add_reserved(b, PAGE);
        assert_eq!(m.commit_for_write(b, PAGE), Ok(PAGE));
        assert_eq!(m.read8(b), Ok(0), "reused range is not zeroed");
        check(&m);
    }

    #[test]
    fn stale_pte_does_not_bypass_the_commit_rule() {
        let mut m = Mmu::new(1, 8);
        m.add_reserved(A, 0x1000);
        // a PTE for an uncommitted page, as an old mapping might leave behind
        let vpn = A >> 12;
        m.sh.table.insert(vpn, Pte { frame: NONE, slot: NONE });
        let frames = frames_used(&m);
        assert!(m.read8(A).is_err());
        assert!(m.write8(A, 1).is_err());
        assert!(m.sh.table.get(&vpn).map_or(false, |p| p.frame == NONE), "the stale PTE was changed");
        assert_eq!(frames_used(&m), frames);
        check(&m);
    }

    /// The child's copy of the GDI handle table: `copied` bytes, written at `gt` the way
    /// create_process does it (commit_for_write, then one checked write).
    fn copy_table(m: &mut Mmu, gt: u64, copied: usize) -> Result<(), u32> {
        let tab: Vec<u8> = (0..copied).map(|i| (i as u8) ^ 0x6B).collect();
        m.add_reserved(gt, 0x10000 * 24); // 384 pages: the whole table's address space
        m.commit_for_write(gt, copied as u64)?;
        m.write_bytes(gt, &tab).map_err(|_| 8u32)
    }

    #[test]
    fn child_table_copy_commits_only_the_pages_it_copies() {
        let mut m = Mmu::new(1, 8);
        let gt = A + 0x80_0000;
        // 56 entries as at init (1344 bytes) and 3 pages of entries: neither needs the 384 pages
        copy_table(&mut m, gt, 56 * 24).unwrap();
        assert_eq!(m.sh.committed, 0x1000, "a 1344-byte table committed more than one page");
        assert_eq!(m.read8(gt), Ok(0x6B));
        assert!(m.read8(gt + 0x1000).is_err(), "the page after the copy is committed");
        check(&m);
        let mut big = Mmu::new(1, 8);
        copy_table(&mut big, gt, 3 * PAGE as usize - 5).unwrap();
        assert_eq!(big.sh.committed, 3 * PAGE);
        assert_eq!(big.read8(gt + 2 * PAGE + 100), Ok(((2 * PAGE + 100) as u8) ^ 0x6B));
        assert!(big.read8(gt + 3 * PAGE).is_err());
        check(&big);
    }

    #[test]
    fn child_table_copy_into_a_small_pool_fails_cleanly() {
        let mut m = Mmu::new(1, 8); // 256 frames
        fill_frames(&mut m, A, 254); // two frames left for the copy
        m.sh.no_evict = true;
        let gt = A + 0x80_0000;
        // a copy of two pages fits the two frames
        copy_table(&mut m, gt, 2 * PAGE as usize).unwrap();
        assert_eq!(frames_used(&m), 256);
        m.release_region(gt);
        assert_eq!(frames_used(&m), 254);
        // a copy of three pages does not: nothing of the table is left behind
        let committed = m.sh.committed;
        m.add_reserved(gt, 0x10000 * 24);
        assert_eq!(m.commit_for_write(gt, 3 * PAGE), Err(8));
        assert_eq!(m.sh.committed, committed, "the failed copy left commits");
        assert_eq!(frames_used(&m), 254, "the failed copy left frames");
        assert!(m.sh.table.get(&(gt >> 12)).is_none(), "the failed copy left a PTE");
        assert!(m.read8(gt).is_err(), "a table page the copy never made is readable");
        // the caller releases the child's range; the range can be used again
        assert!(m.release_region(gt));
        assert!(m.region_of(gt).is_none());
        check(&m);
    }

    #[test]
    fn swapped_page_is_not_restored_after_decommit() {
        // target 8 MB, pool 1 MB: committing 300 pages forces eviction to the swap
        let mut m = Mmu::new(1, 8);
        let pages = 300u64;
        m.add_reserved(A, pages * PAGE);
        m.commit_in_region(A, pages * PAGE).unwrap();
        for i in 0..pages {
            m.write8(A + i * PAGE, (i as u8) ^ 0x3C).unwrap();
        }
        assert!(m.sh.swap_outs > 0, "the test did not reach the swap");
        // page 0 was evicted and comes back from the swap with its value
        assert_eq!(m.read8(A), Ok(0x3C));
        // decommit: its PTE and its swap slot go, so no old value can come back
        m.decommit_range(A, PAGE).unwrap();
        assert!(m.read8(A).is_err(), "a decommitted swapped page still reads");
        assert!(m.sh.table.get(&(A >> 12)).is_none());
        m.commit_in_region(A, PAGE).unwrap();
        assert_eq!(m.read8(A), Ok(0), "recommit restored the old swapped contents");
        check(&m);
    }

    #[test]
    fn failed_prepare_keeps_residence_of_pages_committed_before() {
        let mut m = Mmu::new(1, 8); // 256 frames
        fill_frames(&mut m, A + 0x50_0000, 255); // one frame left
        m.sh.no_evict = true;
        // one page committed before the call (not yet resident), then two fresh pages
        m.add_reserved(A, 3 * PAGE);
        m.commit_in_region(A, PAGE).unwrap();
        let (committed, commits, frames) = (m.sh.committed, m.sh.commits.clone(), frames_used(&m));
        // the old page takes the last frame; the first fresh page gets none
        assert_eq!(m.commit_for_write(A, 3 * PAGE), Err(8));
        // the fresh pages are uncommitted again: counter and list are the ones from before
        assert_eq!(m.sh.committed, committed, "the commit counter changed");
        assert_eq!(m.sh.commits, commits, "the commit list changed");
        assert!(m.sh.table.get(&((A + PAGE) >> 12)).is_none(), "a fresh page kept a PTE");
        assert!(m.sh.table.get(&((A + 2 * PAGE) >> 12)).is_none(), "a fresh page kept a PTE");
        // documented: the old page stays resident, so the one frame it took stays taken
        assert_eq!(frames_used(&m), frames + 1, "the old page's frame accounting changed");
        let pte = m.sh.table.get(&(A >> 12)).copied();
        assert!(pte.map_or(false, |p| p.frame != NONE), "the old page lost its residence");
        // the old page is still committed and reads zero; the fresh pages still fault
        assert_eq!(m.read8(A), Ok(0));
        assert!(m.read8(A + PAGE).is_err());
        assert!(m.write8(A + 2 * PAGE, 1).is_err());
        check(&m);
    }
}

#[cfg(test)]
mod fixed_tests {
    //! VirtualAlloc / NtAllocateVirtualMemory at a fixed address, through reserve_fixed.
    use super::*;

    const A: u64 = 0x5F00_0000;

    fn check(m: &Mmu) {
        let sum: u64 = m.sh.commits.iter().map(|&(s, e)| e - s).sum();
        assert_eq!(m.sh.committed, sum, "committed counter and commit list disagree");
    }

    #[test]
    fn reserve_only_creates_no_commit() {
        let mut m = Mmu::new(1, 8);
        assert_eq!(m.reserve_fixed(A, 0x1000, false), Ok(A));
        // base floored to 64 KB (already aligned here), end at the page of the last byte
        assert_eq!(m.sh.regions, vec![(A, A + 0x1000)]);
        assert!(m.sh.commits.is_empty());
        assert_eq!(m.sh.committed, 0);
        assert!(m.read8(A).is_err(), "reserved-only page reads");
        check(&m);
    }

    #[test]
    fn unaligned_base_rounds_down_to_64k_and_end_up_to_a_page() {
        let mut m = Mmu::new(1, 8);
        assert_eq!(m.reserve_fixed(A + 0x800, 0x1000, true), Ok(A));
        // the bytes lie in pages A and A+0x1000: the region runs from the 64 KB base to A+0x2000
        assert_eq!(m.sh.regions, vec![(A, A + 0x2000)]);
        // the page of the requested bytes is committed, not the 64 KB base
        assert_eq!(m.sh.committed, 0x2000);
        assert_eq!(m.sh.commits, vec![(A, A + 0x2000)]);
        check(&m);
    }

    #[test]
    fn unaligned_size_commits_whole_pages() {
        let mut m = Mmu::new(1, 8);
        assert_eq!(m.reserve_fixed(A, 0x1001, true), Ok(A));
        assert_eq!(m.sh.committed, 0x2000);
        assert_eq!(m.read8(A + 0x1000), Ok(0), "the second page holding the last byte is not committed");
        assert!(m.read8(A + 0x2000).is_err(), "a page past the last byte is committed");
        check(&m);
    }

    #[test]
    fn commit_outside_any_reservation_creates_no_region() {
        let mut m = Mmu::new(1, 8);
        assert_eq!(m.commit_in_region(A, 0x1000), Err(487));
        assert!(m.sh.regions.is_empty(), "a commit created a reservation");
        assert!(m.sh.commits.is_empty());
        assert_eq!(m.sh.committed, 0);
        check(&m);
    }

    #[test]
    fn commit_past_the_upper_bound_changes_nothing() {
        let mut m = Mmu::new(1, 8);
        m.reserve_fixed(A, 0x1000, false).unwrap(); // region [A, A+0x1000)
        let (regions, committed) = (m.sh.regions.clone(), m.sh.committed);
        assert_eq!(m.commit_in_region(A, 0x2000), Err(487), "commit ran past the region");
        assert_eq!(m.commit_in_region(A + 0x1000, 0x1000), Err(487), "commit started past the region");
        assert_eq!(m.sh.regions, regions);
        assert_eq!(m.sh.committed, committed);
        assert!(m.sh.commits.is_empty());
        check(&m);
    }

    #[test]
    fn commit_crossing_the_lower_bound_changes_nothing() {
        let mut m = Mmu::new(1, 8);
        m.reserve_fixed(A + 0x1_0000, 0x1000, false).unwrap();
        assert_eq!(m.commit_in_region(A + 0x1_0000 - 0x800, 0x1000), Err(487));
        assert!(m.sh.commits.is_empty());
        assert_eq!(m.sh.committed, 0);
        check(&m);
    }

    #[test]
    fn repeated_commit_keeps_contents_and_counts_once() {
        let mut m = Mmu::new(1, 8);
        m.reserve_fixed(A, 0x1_0000, true).unwrap();
        m.write8(A, 0x5A).unwrap();
        assert_eq!(m.commit_in_region(A, 0x1000), Ok(0), "a repeated commit counted bytes again");
        assert_eq!(m.read8(A), Ok(0x5A), "a repeated commit zeroed the page");
        check(&m);
    }

    #[test]
    fn memory_limit_failure_leaves_no_region_and_no_commit() {
        let mut m = Mmu::new(1, 1); // target 1 MB
        assert_eq!(m.reserve_fixed(A, 0x20_0000, true), Err(8));
        assert!(m.sh.regions.is_empty(), "the failed reservation was not released");
        assert!(m.sh.commits.is_empty());
        assert_eq!(m.sh.committed, 0);
        // the same address is usable afterwards
        assert_eq!(m.reserve_fixed(A, 0x1000, true), Ok(A));
        assert_eq!(m.sh.committed, 0x1000);
        check(&m);
    }

    #[test]
    fn overlapping_reservation_is_refused_without_change() {
        let mut m = Mmu::new(1, 8);
        m.reserve_fixed(A, 0x1000, false).unwrap();
        let regions = m.sh.regions.clone();
        assert_eq!(m.reserve_fixed(A + 0x8000, 0x1000, true), Err(487), "overlap with the first region was accepted");
        assert_eq!(m.reserve_fixed(A - 0x800, 0x1000, true), Err(487), "overlap from below was accepted");
        assert_eq!(m.sh.regions, regions);
        assert_eq!(m.sh.committed, 0);
        check(&m);
    }

    #[test]
    fn range_that_overflows_u64_is_refused() {
        let mut m = Mmu::new(1, 8);
        assert_eq!(m.reserve_fixed(u64::MAX - 0x10, 0x100, true), Err(487));
        assert!(m.sh.regions.is_empty());
        assert!(m.sh.commits.is_empty());
        check(&m);
    }

    /// The VirtualAlloc probe (tests/startup/virtualalloc-api.c, CI run 43752719049) reserved
    /// `r5` at 0x5B400000 while `top` (0x5B3FFFF0, 0x20 bytes) was still reserved. `top` covers
    /// the page at 0x5B400000, so the 487 is the overlap rule, and the probe's expectation was wrong.
    #[test]
    fn probe_r5_at_0x5b400000_overlaps_the_top_reservation() {
        let mut m = Mmu::new(1, 8);
        assert_eq!(m.reserve_fixed(0x5B3F_FFF0, 0x20, false), Ok(0x5B3F_0000));
        assert_eq!(m.region_of(0x5B40_0000), Some((0x5B3F_0000, 0x5B40_1000)), "top's region does not end at the page of its last byte");
        assert_eq!(m.reserve_fixed(0x5B40_0000, 0x1_0000, false), Err(487), "r5 at 0x5B400000 was accepted over top");
        assert_eq!(m.sh.regions, vec![(0x5B3F_0000, 0x5B40_1000)], "the refused reservation changed the regions");
        // the probe's new address is clear of top
        assert_eq!(m.reserve_fixed(0x5B50_0000, 0x1_0000, false), Ok(0x5B50_0000));
        check(&m);
    }
}

#[cfg(test)]
mod range_tests {
    use super::*;

    #[test]
    fn range_check_has_no_side_effects() {
        let mut m = Mmu::new(1, 1);
        m.add_region(0x5A00_0000, 0x1_0000);
        let pages = m.sh.table.len();
        let faults = m.sh.faults;
        let frames = m.sh.frames.iter().filter(|f| f.used).count();

        assert!(m.can_write_range(0x5A00_0000, 104));
        assert!(m.can_write_range(0x5A00_FF00, 0x100));
        // straddles the end of the committed region
        assert!(!m.can_write_range(0x5A00_FFF0, 104));
        // entirely outside the region
        assert!(!m.can_write_range(0x5A01_0100, 104));
        // ranges whose end overflows u64, including the top of the address space
        assert!(!m.can_write_range(u64::MAX - 10, 104));
        assert!(!m.can_write_range(u64::MAX, 2));
        assert!(!m.can_write_range(u64::MAX - 100, 200));
        // degenerate arguments
        assert!(!m.can_write_range(0, 104));
        assert!(!m.can_write_range(0x5A00_0000, 0));

        assert_eq!(m.sh.table.len(), pages, "range check materialized a page");
        assert_eq!(m.sh.faults, faults, "range check counted a fault");
        assert_eq!(m.sh.frames.iter().filter(|f| f.used).count(), frames, "range check took a frame");
    }

    #[test]
    fn reserved_only_range_is_not_writable() {
        let mut m = Mmu::new(1, 1);
        m.add_region(0x5A00_0000, 0x1_0000);
        m.add_reserved(0x5B00_0000, 0x1_0000);
        let pages = m.sh.table.len();
        let faults = m.sh.faults;
        let frames = m.sh.frames.iter().filter(|f| f.used).count();
        let committed = m.sh.committed;

        // reserved address space is not committed memory
        assert!(!m.can_write_range(0x5B00_0000, 104));
        assert!(!m.can_write_range(0x5B00_FF00, 0x100));
        // a committed range that runs into a reserved one is still refused
        assert!(!m.can_write_range(0x5A00_FFF0, 104));
        // committed memory is unchanged by the reservation
        assert!(m.can_write_range(0x5A00_0000, 104));

        assert_eq!(m.sh.table.len(), pages, "reserved check materialized a page");
        assert_eq!(m.sh.faults, faults, "reserved check counted a fault");
        assert_eq!(m.sh.frames.iter().filter(|f| f.used).count(), frames, "reserved check took a frame");
        assert_eq!(m.sh.committed, committed, "reservation changed the commit count");
    }

    #[test]
    fn reserve_then_commit_then_decommit() {
        let mut m = Mmu::new(1, 1);
        m.add_reserved(0x5C00_0000, 0x1_0000);
        assert!(!m.can_write_range(0x5C00_0000, 104)); // reserved only
        m.commit_in_region(0x5C00_0000, 0x1_0000).unwrap();
        assert!(m.can_write_range(0x5C00_0000, 104)); // committed after reserve
        // partial decommit of the middle: the ends stay committed
        m.decommit_range(0x5C00_4000, 0x4000).unwrap();
        assert!(m.can_write_range(0x5C00_0000, 0x4000));
        assert!(!m.can_write_range(0x5C00_4000, 104));
        assert!(m.can_write_range(0x5C00_8000, 104));
        // total decommit
        m.decommit_range(0x5C00_0000, 0x1_0000).unwrap();
        assert!(!m.can_write_range(0x5C00_0000, 104));
        assert!(!m.can_write_range(0x5C00_8000, 104));
        // release clears the committed ranges of the released region
        m.commit_in_region(0x5C00_0000, 0x1_0000).unwrap();
        assert!(m.release_region(0x5C00_0000));
        assert!(!m.can_write_range(0x5C00_0000, 104));
        assert!(m.sh.commits.is_empty());
    }
}

#[cfg(test)]
mod space_tests {
    use super::*;

    #[test]
    fn committed_ranges_follow_their_address_space() {
        let mut m = Mmu::new(1, 1);
        m.add_region(0x5A00_0000, 0x1_0000); // process A
        let a = m.take_space(); // A goes out
        assert!(!m.can_write_range(0x5A00_0000, 104)); // B does not see A's commit
        m.add_region(0x5B00_0000, 0x1_0000); // process B
        assert!(m.can_write_range(0x5B00_0000, 104));
        let b = m.take_space();
        m.put_space(a); // back to A
        assert!(m.can_write_range(0x5A00_0000, 104));
        assert!(!m.can_write_range(0x5B00_0000, 104));
        m.put_space(b);
        assert!(!m.can_write_range(0x5A00_0000, 104));
        assert!(m.can_write_range(0x5B00_0000, 104));
    }

    #[test]
    fn clone_and_free_carry_their_commits() {
        let mut m = Mmu::new(1, 1);
        m.add_region(0x5A00_0000, 0x1_0000);
        let copy = m.clone_space().expect("clone");
        m.free_space();
        assert!(!m.can_write_range(0x5A00_0000, 104)); // freed space has no commits
        m.put_space(copy);
        assert!(m.can_write_range(0x5A00_0000, 104)); // the clone kept its commits
        m.free_space();
        assert!(!m.can_write_range(0x5A00_0000, 104));
    }
}

#[cfg(test)]
mod accounting_tests {
    use super::*;

    const BASE: u64 = 0x5D00_0000;

    /// The committed counter must equal the bytes in the commit list.
    fn check(m: &Mmu) {
        let sum: u64 = m.sh.commits.iter().map(|&(s, e)| e - s).sum();
        assert_eq!(m.sh.committed, sum, "committed counter and commit list disagree");
    }

    fn snapshot(m: &Mmu) -> (Vec<(u64, u64)>, Vec<(u64, u64)>, u64) {
        (m.sh.commits.clone(), m.sh.regions.clone(), m.sh.committed)
    }

    #[test]
    fn reserve_without_commit_counts_nothing() {
        let mut m = Mmu::new(1, 1);
        m.add_reserved(BASE, 0x1_0000);
        assert_eq!(m.sh.committed, 0);
        assert!(!m.can_write_range(BASE, 104));
        check(&m);
    }

    #[test]
    fn full_and_partial_commit() {
        let mut m = Mmu::new(1, 1);
        m.add_reserved(BASE, 0x1_0000);
        assert_eq!(m.commit_in_region(BASE, 0x1_0000), Ok(0x1_0000));
        assert_eq!(m.sh.committed, 0x1_0000);
        m.add_reserved(BASE + 0x10_0000, 0x1_0000);
        assert_eq!(m.commit_in_region(BASE + 0x10_0000, 0x2000), Ok(0x2000));
        assert_eq!(m.sh.committed, 0x1_0000 + 0x2000);
        check(&m);
    }

    #[test]
    fn page_sized_regions_count_whole_pages() {
        // a 1-byte size takes one page; a page plus one byte takes two
        assert_eq!(round_up_page(1), Some(0x1000));
        assert_eq!(round_up_page(0x1000), Some(0x1000));
        assert_eq!(round_up_page(0x1001), Some(0x2000));
        assert_eq!(round_up_page(u64::MAX), None);
        let mut m = Mmu::new(1, 1);
        m.add_region(BASE, 0x1000).unwrap();
        assert_eq!(m.sh.committed, 0x1000);
        m.add_region(BASE + 0x1_0000, 0x2000).unwrap();
        assert_eq!(m.sh.committed, 0x3000);
        check(&m);
    }

    #[test]
    fn repeated_and_overlapping_commit_is_not_counted_twice() {
        let mut m = Mmu::new(1, 1);
        m.add_reserved(BASE, 0x1_0000);
        assert_eq!(m.commit_in_region(BASE, 0x4000), Ok(0x4000));
        assert_eq!(m.commit_in_region(BASE, 0x4000), Ok(0));
        // 0x2000..0x6000 overlaps the committed 0x0000..0x4000: only 0x2000 is new
        assert_eq!(m.commit_in_region(BASE + 0x2000, 0x4000), Ok(0x2000));
        assert_eq!(m.sh.committed, 0x6000);
        check(&m);
    }

    #[test]
    fn partial_then_full_decommit_subtracts_only_committed_bytes() {
        let mut m = Mmu::new(1, 1);
        m.add_reserved(BASE, 0x1_0000);
        m.commit_in_region(BASE, 0x1_0000).unwrap();
        assert_eq!(m.decommit_range(BASE + 0x4000, 0x4000), Ok(0x4000));
        assert_eq!(m.sh.committed, 0xC000);
        check(&m);
        // spans a hole and a committed part: only the 0x2000 committed bytes leave
        assert_eq!(m.decommit_range(BASE + 0x3000, 0x6000), Ok(0x2000));
        assert_eq!(m.sh.committed, 0xA000);
        check(&m);
        assert_eq!(m.decommit_range(BASE, 0x1_0000), Ok(0xA000));
        assert_eq!(m.sh.committed, 0);
        assert!(m.sh.commits.is_empty());
        check(&m);
    }

    #[test]
    fn repeated_decommit_subtracts_nothing_more() {
        let mut m = Mmu::new(1, 1);
        m.add_reserved(BASE, 0x1_0000);
        m.commit_in_region(BASE, 0x2000).unwrap();
        assert_eq!(m.decommit_range(BASE, 0x2000), Ok(0x2000));
        assert_eq!(m.decommit_range(BASE, 0x2000), Ok(0));
        assert_eq!(m.sh.committed, 0);
        check(&m);
    }

    #[test]
    fn release_of_reserved_only_region_subtracts_nothing() {
        let mut m = Mmu::new(1, 1);
        m.add_region(BASE + 0x20_0000, 0x1_0000);
        m.add_reserved(BASE, 0x1_0000);
        assert!(m.release_region(BASE));
        assert_eq!(m.sh.committed, 0x1_0000, "releasing a reservation changed the commit count");
        assert!(!m.release_region(BASE), "a released region was released again");
        check(&m);
    }

    #[test]
    fn release_of_partly_committed_region_subtracts_committed_bytes_only() {
        let mut m = Mmu::new(1, 1);
        m.add_reserved(BASE, 0x4_0000);
        m.commit_in_region(BASE + 0x1_0000, 0x1_0000).unwrap();
        m.commit_in_region(BASE + 0x3_0000, 0x3000).unwrap();
        assert_eq!(m.sh.committed, 0x1_3000);
        assert!(m.release_region(BASE));
        assert_eq!(m.sh.committed, 0, "the reserved size was subtracted, not the committed bytes");
        assert!(m.sh.commits.is_empty());
        check(&m);
    }

    #[test]
    fn commit_past_the_reservation_fails_without_changes() {
        let mut m = Mmu::new(1, 1);
        m.add_reserved(BASE, 0x1_0000);
        m.commit_in_region(BASE, 0x1000).unwrap();
        let before = snapshot(&m);
        assert_eq!(m.commit_in_region(BASE + 0xF000, 0x2000), Err(487)); // starts inside, ends after
        assert_eq!(m.commit_in_region(BASE + 0x1_0000, 0x1000), Err(487)); // wholly after
        assert_eq!(m.commit_in_region(BASE - 0x800, 0x1000), Err(487)); // starts before
        assert_eq!(snapshot(&m), before);
        check(&m);
    }

    #[test]
    fn commit_over_the_memory_target_fails_without_changes() {
        let mut m = Mmu::new(1, 1); // target: 1 MB
        m.add_reserved(BASE, 0x40_0000);
        let before = snapshot(&m);
        assert_eq!(m.commit_in_region(BASE, 0x20_0000), Err(8));
        assert_eq!(snapshot(&m), before);
        assert_eq!(m.commit_in_region(BASE, 0x8_0000), Ok(0x8_0000));
        check(&m);
    }

    #[test]
    fn extreme_ranges_are_refused_without_changes() {
        let mut m = Mmu::new(1, 1);
        m.add_reserved(BASE, 0x1_0000);
        m.commit_in_region(BASE, 0x1000).unwrap();
        let before = snapshot(&m);
        assert_eq!(page_span(u64::MAX - 0xFFF, 0x1000), None);
        assert_eq!(m.commit_in_region(u64::MAX - 0x10, 0x100), Err(487));
        assert_eq!(m.commit_in_region(u64::MAX, 1), Err(487));
        assert_eq!(m.decommit_range(u64::MAX - 0x10, 0x100), Err(487));
        assert_eq!(m.decommit_range(u64::MAX, u64::MAX), Err(487));
        assert!(!m.release_region(u64::MAX));
        assert_eq!(snapshot(&m), before);
        check(&m);
    }

    #[test]
    fn decommit_must_lie_inside_one_reservation() {
        let mut m = Mmu::new(1, 1);
        m.add_reserved(BASE, 0x1_0000);
        m.add_reserved(BASE + 0x1_0000, 0x1_0000); // adjacent reservation
        m.commit_in_region(BASE, 0x1_0000).unwrap();
        m.commit_in_region(BASE + 0x1_0000, 0x1_0000).unwrap();
        let before = snapshot(&m);
        assert_eq!(m.decommit_range(BASE + 0x9_0000, 0x1000), Err(487), "never reserved");
        assert_eq!(m.decommit_range(BASE - 0x800, 0x1000), Err(487), "starts before the reservation");
        assert_eq!(m.decommit_range(BASE + 0xF000, 0x2000), Err(487), "crosses two reservations");
        assert_eq!(snapshot(&m), before, "a refused decommit changed the state");
        // the pages on both sides are still committed
        assert_eq!(m.commit_in_region(BASE + 0xF000, 0x1000), Ok(0));
        assert_eq!(m.commit_in_region(BASE + 0x1_0000, 0x1000), Ok(0));
        check(&m);
    }

    #[test]
    fn zero_size_decommit_takes_the_whole_region_and_keeps_it() {
        let mut m = Mmu::new(1, 1);
        m.add_reserved(BASE, 0x1_0000);
        m.add_reserved(BASE + 0x2_0000, 0x1_0000); // another reservation
        m.commit_in_region(BASE, 0x4000).unwrap();
        m.commit_in_region(BASE + 0x2_0000, 0x1000).unwrap();
        let before = snapshot(&m);
        // not the base of a region, and not reserved: refused, nothing changes
        assert_eq!(m.decommit_range(BASE + 0x4000, 0), Err(487));
        assert_eq!(m.decommit_range(BASE + 0x9_0000, 0), Err(487));
        assert_eq!(snapshot(&m), before, "a refused zero-size decommit changed the state");
        // the base: the committed part of that region goes, the region stays
        assert_eq!(m.decommit_range(BASE, 0), Ok(0x4000));
        assert_eq!(m.sh.committed, 0x1000);
        assert!(m.sh.regions.contains(&(BASE, BASE + 0x1_0000)), "the reservation was removed");
        assert!(!m.can_write_range(BASE, 104));
        assert!(m.can_write_range(BASE + 0x2_0000, 104), "another reservation was touched");
        check(&m);
        // a later commit over the same region works again
        assert_eq!(m.commit_in_region(BASE, 0x1000), Ok(0x1000));
        assert_eq!(m.decommit_range(BASE, 0), Ok(0x1000));
        assert_eq!(m.sh.committed, 0x1000);
        check(&m);
    }

    #[test]
    fn zero_size_decommit_of_a_reservation_without_commits_removes_nothing() {
        let mut m = Mmu::new(1, 1);
        m.add_reserved(BASE, 0x1_0000);
        assert_eq!(m.decommit_range(BASE, 0), Ok(0));
        assert!(m.sh.regions.contains(&(BASE, BASE + 0x1_0000)));
        assert_eq!(m.sh.committed, 0);
        check(&m);
    }

    #[test]
    fn overflowing_requests_change_nothing() {
        let mut m = Mmu::new(1, 1);
        m.add_reserved(BASE, 0x1_0000);
        let before = snapshot(&m);
        assert_eq!(m.add_reserved(u64::MAX - 0x10, 0x100), None);
        assert_eq!(m.add_region(u64::MAX - 0x10, 0x100), None);
        assert_eq!(m.first_gap(u64::MAX - 0x100, u64::MAX, 0x1000, 0x10000), None);
        // no room below the limit: refused, not wrapped
        assert_eq!(m.first_gap(BASE, BASE + 0x1000, 0x2000, 0x10000), None);
        assert_eq!(snapshot(&m), before);
    }

    #[test]
    fn fixed_span_rounds_base_down_and_end_up_to_pages() {
        // aligned base, whole pages
        assert_eq!(fixed_span(0x5B10_0000, 0x2000), Some((0x5B10_0000, 0x5B10_2000)));
        // misaligned base: the base goes down to 64 KB, the end to the page of the last byte
        assert_eq!(fixed_span(0x5B18_0800, 0x1000), Some((0x5B18_0000, 0x5B18_2000)));
        // near the 64 KB boundary: the last byte is in the page before the boundary
        assert_eq!(fixed_span(0x5B1F_FFF0, 0x10), Some((0x5B1F_0000, 0x5B20_0000)));
        // zero size is one byte
        assert_eq!(fixed_span(0x5B18_0800, 0), Some((0x5B18_0000, 0x5B18_1000)));
        // the end does not fit
        assert_eq!(fixed_span(u64::MAX - 0x10, 0x100), None);
        assert_eq!(fixed_span(u64::MAX, 0), None);
    }

    #[test]
    fn span_after_is_checked() {
        assert_eq!(span_after(BASE, 0x1000, 0x10000), Some((BASE + 0x1000, BASE + 0x11000)));
        assert_eq!(span_after(u64::MAX - 0x10, 0x100, 0), None, "end overflows");
        assert_eq!(span_after(u64::MAX - 0x1000, 0x1000, 1), None, "bump pointer overflows");
        assert_eq!(span_after(u64::MAX - 0x1000, 0x1000, 0), Some((u64::MAX, u64::MAX)));
    }

    #[test]
    fn rounding_to_64k_is_checked() {
        assert_eq!(round_up_64k(1), Some(0x1_0000));
        assert_eq!(round_up_64k(0x1_0000), Some(0x1_0000));
        assert_eq!(round_up_64k(u64::MAX - 0xFFFF), Some(0xFFFF_FFFF_FFFF_0000));
        assert_eq!(round_up_64k(u64::MAX - 0xF000), None);
        assert_eq!(round_up_64k(u64::MAX), None);
    }
}

#[cfg(test)]
mod page_run_tests {
    //! The page state VirtualQuery reports (Mmu::page_run), from the region and commit lists.
    use super::*;

    const A: u64 = 0x5C00_0000;

    #[test]
    fn free_address_has_no_run() {
        let mut m = Mmu::new(1, 8);
        assert_eq!(m.page_run(A), None);
        m.add_reserved(A, 0x1000);
        m.release_region(A);
        assert_eq!(m.page_run(A), None, "a released range still reports a run");
    }

    #[test]
    fn reserved_only_region_reports_reserve_to_its_end() {
        let mut m = Mmu::new(1, 8);
        m.add_reserved(A, 0x4000);
        assert_eq!(m.page_run(A), Some(PageRun { region: A, committed: false, end: A + 0x4000 }));
        // an address in the middle of the region: the run still ends at the region end
        assert_eq!(m.page_run(A + 0x2345), Some(PageRun { region: A, committed: false, end: A + 0x4000 }));
    }

    #[test]
    fn committed_region_reports_commit_to_its_end() {
        let mut m = Mmu::new(1, 8);
        m.add_reserved(A, 0x2000);
        m.commit_in_region(A, 0x2000).unwrap();
        assert_eq!(m.page_run(A + 0x1800), Some(PageRun { region: A, committed: true, end: A + 0x2000 }));
    }

    #[test]
    fn partly_committed_region_splits_into_runs() {
        let mut m = Mmu::new(1, 8);
        m.add_reserved(A, 0x4000);
        m.commit_in_region(A + 0x1000, 0x1000).unwrap();
        assert_eq!(m.page_run(A), Some(PageRun { region: A, committed: false, end: A + 0x1000 }));
        assert_eq!(m.page_run(A + 0x1000), Some(PageRun { region: A, committed: true, end: A + 0x2000 }));
        assert_eq!(m.page_run(A + 0x2000), Some(PageRun { region: A, committed: false, end: A + 0x4000 }));
    }

    #[test]
    fn decommit_turns_the_pages_back_into_reserve() {
        let mut m = Mmu::new(1, 8);
        m.add_reserved(A, 0x2000);
        m.commit_in_region(A, 0x2000).unwrap();
        m.decommit_range(A, 0x1000).unwrap();
        assert_eq!(m.page_run(A), Some(PageRun { region: A, committed: false, end: A + 0x1000 }));
        assert_eq!(m.page_run(A + 0x1000), Some(PageRun { region: A, committed: true, end: A + 0x2000 }));
    }

    #[test]
    fn adjacent_regions_keep_their_own_bases() {
        let mut m = Mmu::new(1, 8);
        m.add_reserved(A, 0x1000);
        m.add_reserved(A + 0x1000, 0x1000);
        assert_eq!(m.page_run(A), Some(PageRun { region: A, committed: false, end: A + 0x1000 }));
        assert_eq!(m.page_run(A + 0x1000), Some(PageRun { region: A + 0x1000, committed: false, end: A + 0x2000 }));
    }
}
