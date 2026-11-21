// Process heap (HeapAlloc & co.) living in program memory.
// Small blocks: power-of-two size classes carved from 16 MB arenas (pages
// get real RAM only when touched). Large blocks: their own region.

use crate::Engine;
use std::collections::HashMap;

const ARENA: u64 = 16 << 20;
const MAX_SMALL: u64 = 64 << 10;

#[derive(Default)]
pub struct Heap {
    free: Vec<Vec<u64>>,
    bump: u64,
    bump_end: u64,
    sizes: HashMap<u64, u64>, // block -> requested size (for HeapSize / ReAlloc)
    large: HashMap<u64, u64>, // block -> region size
    cap: HashMap<u64, u64>,   // block -> bytes it really has (its class, or the region)
}

fn class(size: u64) -> usize { (size.max(16).next_power_of_two().trailing_zeros() - 4) as usize }

impl Engine {
    pub fn heap_alloc(&mut self, size: u64, zero: bool) -> u64 {
        if size > MAX_SMALL {
            let Some(a) = self.alloc(size) else { return 0 };
            self.s.heap.large.insert(a, size);
            self.s.heap.sizes.insert(a, size);
            self.s.heap.cap.insert(a, crate::mmu::round_up_64k(size).unwrap_or(size));
            return a;
        }
        let c = class(size);
        let csize = 16u64 << c;
        if self.s.heap.free.len() <= c { self.s.heap.free.resize(c + 1, Vec::new()); }
        let a = if let Some(a) = self.s.heap.free[c].pop() {
            if zero { for i in (0..csize).step_by(8) { self.w64(a + i, 0); } }
            a
        } else {
            if self.s.heap.bump + csize > self.s.heap.bump_end {
                let Some(r) = self.alloc(ARENA) else { return 0 };
                self.s.heap.bump = r;
                self.s.heap.bump_end = r + ARENA;
            }
            let a = self.s.heap.bump;
            self.s.heap.bump += csize;
            a // fresh arena memory is already zero
        };
        self.s.heap.sizes.insert(a, size);
        self.s.heap.cap.insert(a, csize);
        a
    }

    pub fn heap_free(&mut self, a: u64) -> bool {
        let Some(size) = self.s.heap.sizes.remove(&a) else { return false };
        let cap = self.s.heap.cap.remove(&a).unwrap_or(size);
        if self.s.heap.large.remove(&a).is_some() {
            self.mmu.release_region(a);
        } else {
            self.s.heap.free[class(cap)].push(a);
        }
        true
    }

    pub fn heap_size(&self, a: u64) -> u64 { self.s.heap.sizes.get(&a).copied().unwrap_or(u64::MAX) }

    /// HeapReAlloc: the block stays where it is while the new size fits the bytes it has
    /// (shrinking, or growing back); `in_place` (HEAP_REALLOC_IN_PLACE_ONLY) fails otherwise.
    pub fn heap_realloc(&mut self, a: u64, size: u64, zero: bool, in_place: bool) -> u64 {
        let old = self.heap_size(a);
        if old == u64::MAX { return 0; }
        let cap = self.s.heap.cap.get(&a).copied().unwrap_or(old);
        if size <= cap && (in_place || size >= cap / 2 || self.s.heap.large.contains_key(&a)) {
            if zero && size > old { for i in old..size { self.w8(a + i, 0); } }
            self.s.heap.sizes.insert(a, size);
            if let Some(r) = self.s.heap.large.get_mut(&a) { *r = size; }
            return a;
        }
        if in_place {
            self.set_last_error(8); // ERROR_NOT_ENOUGH_MEMORY
            return 0;
        }
        let n = self.heap_alloc(size, zero);
        if n == 0 { return 0; }
        let k = old.min(size);
        let mut i = 0;
        while i + 8 <= k { let v = self.r64(a + i); self.w64(n + i, v); i += 8; }
        while i < k { let v = self.r8(a + i); self.w8(n + i, v); i += 1; }
        self.heap_free(a);
        n
    }
}
