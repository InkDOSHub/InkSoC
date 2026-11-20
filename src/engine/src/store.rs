// The program's files. Files of the saved drive C: start as a map entry only
// (their id on the browser's storage and their size); their bytes are read the
// first time the program needs them, in 1 MB pieces, and stay in memory after.
// A size or a folder listing never reads a file.

use std::cell::UnsafeCell;
use std::collections::HashMap;

#[cfg(not(test))]
#[link(wasm_import_module = "env")]
extern "C" {
    /// Reads len bytes at off of drive file id into ptr; returns the bytes read.
    fn host_drive_read(id: u32, off: f64, ptr: *mut u8, len: u32) -> u32;
    /// Writes len bytes from ptr at off of file id (only the spill file); returns the bytes written.
    fn host_drive_write(id: u32, off: f64, ptr: *const u8, len: u32) -> u32;
}

// Host tests have no drive or spill files. A host test that reads one is a bug in the test,
// so the stubs stop the test with a message rather than returning made-up bytes.
#[cfg(test)]
unsafe fn host_drive_read(_id: u32, _off: f64, _ptr: *mut u8, _len: u32) -> u32 {
    panic!("host test: a drive or spill file was read, but host tests have none")
}
#[cfg(test)]
unsafe fn host_drive_write(_id: u32, _off: f64, _ptr: *const u8, _len: u32) -> u32 {
    panic!("host test: a drive or spill file was written, but host tests have none")
}

/// The host's spill file: closed program files too big to keep in memory.
pub const SPILL: u32 = 0x7FFF_FFF0;
/// closed files from this size go to the spill file
const SPILL_MIN: usize = 64 << 10;
/// small closed files kept in memory, in all, before they go to the spill file too
const SMALL_KEPT: u64 = 32 << 20;

const PIECE: usize = 1 << 20;
/// files above this size are read piece by piece where they are, never whole
const BIG: u64 = 16 << 20;

/// A file's contents: bytes in memory, or (id, size) of one still on disk.
pub enum Entry {
    Data(Vec<u8>),
    Disk(u32, u64, u64),
}
impl From<Vec<u8>> for Entry {
    fn from(v: Vec<u8>) -> Self {
        Entry::Data(v)
    }
}

#[derive(Default)]
pub struct Store {
    // boxed so a file's bytes stay put while other files are read in
    data: UnsafeCell<HashMap<String, Box<Vec<u8>>>>,
    /// on disk: (file id, offset in it, size)
    lazy: UnsafeCell<HashMap<String, (u32, u64, u64)>>,
    /// files read in and not changed since: where their bytes still are on disk
    clean: UnsafeCell<HashMap<String, (u32, u64, u64)>>,
    pub loaded_bytes: u64,
    /// where the next spilled file goes in the spill file
    spill_at: u64,
    /// bytes of small closed files kept in memory
    small_kept: u64,
    /// files written straight into the spill file: room reserved there for them to grow
    room: UnsafeCell<HashMap<String, u64>>,
    /// debug census: spill writes that failed, and big files a write left in memory (and why)
    pub spill_fails: std::cell::Cell<u64>,
    pub big_refused: std::cell::Cell<u64>,
    pub big_clean: std::cell::Cell<u64>,
}

// all use is under the engine lock (one core at a time)
unsafe impl Sync for Store {}

impl Store {
    fn data(&self) -> &mut HashMap<String, Box<Vec<u8>>> {
        unsafe { &mut *self.data.get() }
    }
    fn lazy(&self) -> &mut HashMap<String, (u32, u64, u64)> {
        unsafe { &mut *self.lazy.get() }
    }
    fn clean(&self) -> &mut HashMap<String, (u32, u64, u64)> {
        unsafe { &mut *self.clean.get() }
    }
    fn room(&self) -> &mut HashMap<String, u64> {
        unsafe { &mut *self.room.get() }
    }

    /// Writes v at pos of a big file directly in the spill file, so it never has to fit
    /// in memory (an installer extracting a 200 MB package). False = the caller writes
    /// it in memory as usual (a small file, or no spill file here).
    pub fn write_at(&mut self, k: &str, pos: u64, v: &[u8]) -> bool {
        let end = pos + v.len() as u64;
        let on_spill = matches!(self.lazy().get(k), Some(&(SPILL, _, _))) && self.room().contains_key(k);
        if !on_spill {
            // only a file in memory that is becoming big moves to the spill file
            let Some(d) = self.data().get(k) else {
                // a big file still on disk (a drive file, or one spilled when closed) is
                // copied to a place with room on the spill file, never read in whole
                if let Some(&(id, base, size)) = self.lazy().get(k) {
                    if end.max(size) > BIG && self.move_to_spill(k, id, base, size, end) {
                        return self.write_at(k, pos, v);
                    }
                }
                if end > BIG {
                    self.big_refused.set(self.big_refused.get() + 1);
                }
                return false;
            };
            if end.max(d.len() as u64) <= BIG {
                return false;
            }
            if self.clean().contains_key(k) {
                self.big_clean.set(self.big_clean.get() + 1);
                return false;
            }
            let n = d.len() as u64;
            let cap = (2 * end).max(64 << 20);
            let base = self.spill_at;
            if n > 0 && !self.spill_write(base, d) {
                return false;
            }
            self.spill_at += cap;
            self.data().remove(k);
            self.lazy().insert(k.to_string(), (SPILL, base, n));
            self.room().insert(k.to_string(), cap);
        }
        let (_, base, size) = self.lazy()[k];
        let cap = self.room()[k];
        let base = if end <= cap {
            base
        } else {
            // out of room: move the file to a bigger place at the end of the spill file
            let cap = (2 * end).max(cap * 2);
            let to = self.spill_at;
            let mut buf = vec![0u8; PIECE];
            let mut at = 0u64;
            while at < size {
                let n = (size - at).min(PIECE as u64) as usize;
                let got = unsafe { host_drive_read(SPILL, (base + at) as f64, buf.as_mut_ptr(), n as u32) } as usize;
                if got == 0 || !self.spill_write(to + at, &buf[..got]) {
                    return false;
                }
                at += got as u64;
            }
            self.spill_at += cap;
            self.room().insert(k.to_string(), cap);
            to
        };
        if !self.spill_write(base + pos, v) {
            return false;
        }
        self.lazy().insert(k.to_string(), (SPILL, base, size.max(end)));
        true
    }
    /// Copies a file on disk, in pieces, to a new place on the spill file with room to grow.
    fn move_to_spill(&mut self, k: &str, id: u32, base: u64, size: u64, end: u64) -> bool {
        let cap = (2 * end.max(size)).max(64 << 20);
        let to = self.spill_at;
        let mut buf = vec![0u8; PIECE];
        let mut at = 0u64;
        while at < size {
            let n = (size - at).min(PIECE as u64) as usize;
            let got = unsafe { host_drive_read(id, (base + at) as f64, buf.as_mut_ptr(), n as u32) } as usize;
            if got == 0 || !self.spill_write(to + at, &buf[..got]) {
                return false;
            }
            at += got as u64;
        }
        self.spill_at += cap;
        self.clean().remove(k);
        self.lazy().insert(k.to_string(), (SPILL, to, size));
        self.room().insert(k.to_string(), cap);
        true
    }
    /// (bytes of files in memory, the biggest one) for the debug census.
    pub fn spilled(&self) -> (u64, u64) {
        let v: Vec<u64> = self.lazy().values().filter(|x| x.0 == SPILL).map(|x| x.2).collect();
        (v.len() as u64, v.iter().sum())
    }
    pub fn mem_stats(&self) -> (u64, String, u64) {
        let mut total = 0u64;
        let mut big = (String::new(), 0u64);
        for (k, v) in self.data().iter() {
            total += v.len() as u64;
            if v.len() as u64 > big.1 {
                big = (k.clone(), v.len() as u64);
            }
        }
        (total, big.0, big.1)
    }
    /// Sets the file's length (SetEndOfFile, a section bigger than its file). A big length
    /// is set where the file lives on the spill file, never as one buffer in memory (an
    /// installer that sizes a 300 MB file before writing it).
    pub fn set_len(&mut self, k: &str, n: u64) {
        if let Some(&(SPILL, base, size)) = self.lazy().get(k) {
            if let Some(&cap) = self.room().get(k) {
                if n <= cap {
                    // shrinking keeps the bytes past the end; growing must read as zeros
                    if n > size && !self.spill_write(base + size, &vec![0u8; (n - size).min(PIECE as u64) as usize]) {
                        return;
                    }
                    if n > size + PIECE as u64 {
                        let mut at = size + PIECE as u64;
                        let z = vec![0u8; PIECE];
                        while at < n {
                            let m = (n - at).min(PIECE as u64) as usize;
                            if !self.spill_write(base + at, &z[..m]) {
                                return;
                            }
                            at += m as u64;
                        }
                    }
                    self.lazy().insert(k.to_string(), (SPILL, base, n));
                    return;
                }
                let _ = self.write_at(k, n - 1, &[0]);
                return;
            }
        }
        if n > BIG {
            let cur = self.size(k).unwrap_or(0);
            if n > cur {
                let _ = self.entry_or_default(k.to_string());
                if self.write_at(k, n - 1, &[0]) {
                    return;
                }
            }
        }
        let d = self.entry_or_default(k.to_string());
        d.resize(n as usize, 0);
    }
    fn read_spill(&self, base: u64, size: u64) -> Vec<u8> {
        let mut v = vec![0u8; size as usize];
        let mut at = 0usize;
        while at < v.len() {
            let n = PIECE.min(v.len() - at);
            let got = unsafe { host_drive_read(SPILL, (base + at as u64) as f64, v.as_mut_ptr().add(at), n as u32) } as usize;
            if got == 0 {
                break;
            }
            at += got;
        }
        v
    }
    fn spill_write(&self, at: u64, b: &[u8]) -> bool {
        let mut done = 0usize;
        while done < b.len() {
            let n = (b.len() - done).min(PIECE);
            let got = unsafe { host_drive_write(SPILL, (at + done as u64) as f64, b.as_ptr().add(done), n as u32) } as usize;
            if got != n {
                self.spill_fails.set(self.spill_fails.get() + 1);
                return false;
            }
            done += n;
        }
        true
    }

    /// A drive file known by id and size; read when first used.
    pub fn add_lazy(&mut self, k: String, id: u32, size: u64) {
        self.room().remove(&k);
        self.data().remove(&k);
        self.clean().remove(&k);
        self.lazy().insert(k, (id, 0, size));
    }

    /// A file no handle holds any more: its bytes leave memory when they are still on disk
    /// (unchanged), or go to the spill file when big.
    pub fn release(&mut self, k: &str) {
        if self.forget(k) {
            return;
        }
        let Some(d) = self.data().get(k) else { return };
        // small files stay in memory, up to SMALL_KEPT in all (an installer writes thousands)
        if d.len() < SPILL_MIN && self.small_kept + d.len() as u64 <= SMALL_KEPT {
            self.small_kept += d.len() as u64;
            return;
        }
        if d.is_empty() {
            return;
        }
        let n = d.len() as u64;
        let got = unsafe { host_drive_write(SPILL, self.spill_at as f64, d.as_ptr(), n as u32) } as u64;
        if got != n || n >= 1 << 32 {
            self.spill_fails.set(self.spill_fails.get() + 1);
            return;
        } // no spill file here: it stays in memory
        self.data().remove(k);
        self.lazy().insert(k.to_string(), (SPILL, self.spill_at, n));
        self.spill_at += n;
    }

    fn fill(&self, k: &str) {
        let Some((id, base, size)) = self.lazy().remove(k) else { return };
        // its place in the spill file is left behind (the bytes are in memory now)
        if self.room().remove(k).is_some() {
            let v = self.read_spill(base, size);
            self.data().insert(k.to_string(), Box::new(v));
            return;
        }
        let mut v = vec![0u8; size as usize];
        let mut at = 0usize;
        while at < v.len() {
            let n = PIECE.min(v.len() - at);
            let got = unsafe { host_drive_read(id, (base + at as u64) as f64, v.as_mut_ptr().add(at), n as u32) } as usize;
            if got == 0 {
                break;
            }
            at += got;
        }
        unsafe {
            (*(self as *const Store as *mut Store)).loaded_bytes += size;
        }
        self.clean().insert(k.to_string(), (id, base, size));
        self.data().insert(k.to_string(), Box::new(v));
    }

    /// Up to n bytes at pos. A big drive file still on disk is read there, without
    /// bringing the whole file in (a 400 MB installer package).
    pub fn read_at(&self, k: &str, pos: u64, n: u64) -> Option<Vec<u8>> {
        if let Some(&(id, base, size)) = self.lazy().get(k) {
            if size > BIG {
                let n = n.min(size.saturating_sub(pos)) as usize;
                let mut v = vec![0u8; n];
                let mut at = 0usize;
                while at < n {
                    let got =
                        unsafe { host_drive_read(id, (base + pos + at as u64) as f64, v.as_mut_ptr().add(at), (n - at).min(PIECE) as u32) } as usize;
                    if got == 0 {
                        break;
                    }
                    at += got;
                }
                v.truncate(at);
                unsafe {
                    (*(self as *const Store as *mut Store)).loaded_bytes += at as u64;
                }
                return Some(v);
            }
        }
        let d = self.get(k)?;
        let s = (pos as usize).min(d.len());
        Some(d[s..(s + n as usize).min(d.len())].to_vec())
    }
    /// The first n bytes (a PE header), read where the file is: a file still on disk stays there.
    pub fn head(&self, k: &str, n: u64) -> Option<Vec<u8>> {
        if let Some(&(id, base, size)) = self.lazy().get(k) {
            if self.room().contains_key(k) {
                return self.read_at(k, 0, n);
            }
            let n = n.min(size) as usize;
            let mut v = vec![0u8; n];
            let got = unsafe { host_drive_read(id, base as f64, v.as_mut_ptr(), n as u32) } as usize;
            v.truncate(got);
            return Some(v);
        }
        let d = self.data().get(k)?;
        Some(d[..(n as usize).min(d.len())].to_vec())
    }
    /// dst becomes a copy of src without reading it, when src is still on disk.
    pub fn alias(&mut self, dst: String, src: &str) -> bool {
        // both now share the bytes: neither may be written in place any more
        self.room().remove(src);
        self.room().remove(&dst);
        let Some(v) = self.lazy().get(src).or_else(|| self.clean().get(src)).copied() else { return false };
        self.data().remove(&dst);
        self.clean().remove(&dst);
        self.lazy().insert(dst, v);
        true
    }
    /// The disk id of a file not read yet.
    pub fn lazy_id(&self, k: &str) -> Option<u32> {
        self.lazy().get(k).map(|v| v.0)
    }
    /// (file id, offset, size) of a file still on disk.
    pub fn disk_of(&self, k: &str) -> Option<(u32, u64, u64)> {
        self.lazy().get(k).copied()
    }

    pub fn get(&self, k: &str) -> Option<&Vec<u8>> {
        if self.lazy().contains_key(k) {
            self.fill(k);
        }
        self.data().get(k).map(|b| &**b)
    }
    /// A copy of the file's bytes (a DLL or program being loaded). A drive file that was
    /// only on disk goes back there afterwards instead of staying in memory twice.
    pub fn get_copy(&mut self, k: &str) -> Option<Vec<u8>> {
        let was_on_disk = self.lazy().contains_key(k);
        if was_on_disk {
            // read in only for this: hand the bytes over (no second copy) and keep the file on disk
            self.fill(k);
            if let Some(v) = self.clean().remove(k) {
                let d = self.data().remove(k).map(|b| *b);
                self.lazy().insert(k.to_string(), v);
                return d;
            }
        }
        self.get(k).cloned()
    }
    /// Drops the bytes of a file read in and unchanged since (they are still on disk).
    fn forget(&mut self, k: &str) -> bool {
        let Some(v) = self.clean().remove(k) else { return false };
        self.data().remove(k);
        self.lazy().insert(k.to_string(), v);
        true
    }
    pub fn get_mut(&mut self, k: &str) -> Option<&mut Vec<u8>> {
        if self.lazy().contains_key(k) {
            self.fill(k);
        }
        self.clean().remove(k);
        self.data().get_mut(k).map(|b| &mut **b)
    }
    /// The file's size without reading it.
    pub fn size(&self, k: &str) -> Option<u64> {
        if let Some(&(_, _, s)) = self.lazy().get(k) {
            return Some(s);
        }
        self.data().get(k).map(|d| d.len() as u64)
    }
    pub fn contains_key(&self, k: &str) -> bool {
        self.data().contains_key(k) || self.lazy().contains_key(k)
    }
    /// Room for `n` more entries in the map, or false when the heap cannot give it. The map
    /// is not changed when it says no.
    pub fn reserve_count(&mut self, n: usize) -> bool {
        self.data().try_reserve(n).is_ok()
    }
    /// Room for one more entry in the map, or false when the heap cannot give it.
    pub fn reserve_one(&mut self) -> bool {
        self.reserve_count(1)
    }
    pub fn insert(&mut self, k: String, v: impl Into<Entry>) {
        self.room().remove(&k);
        self.lazy().remove(&k);
        self.data().remove(&k);
        self.clean().remove(&k);
        match v.into() {
            Entry::Data(v) => {
                self.data().insert(k, Box::new(v));
            }
            Entry::Disk(id, at, size) => {
                self.lazy().insert(k, (id, at, size));
            }
        }
    }
    /// Takes a file out (a rename puts it back under its new name; one on disk stays there).
    pub fn remove(&mut self, k: &str) -> Option<Entry> {
        self.room().remove(k);
        if let Some((id, at, size)) = self.lazy().remove(k) {
            return Some(Entry::Disk(id, at, size));
        }
        if let Some(v) = self.clean().remove(k) {
            self.data().remove(k);
            return Some(Entry::Disk(v.0, v.1, v.2));
        }
        self.data().remove(k).map(|b| Entry::Data(*b))
    }
    /// The file's bytes, made empty if it is new.
    pub fn entry_or_default(&mut self, k: String) -> &mut Vec<u8> {
        if self.lazy().contains_key(&k) {
            self.fill(&k);
        }
        self.clean().remove(&k);
        &mut **self.data().entry(k).or_default()
    }
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.data().keys().chain(self.lazy().keys())
    }
    /// Every file with its bytes (reads all drive files).
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Vec<u8>)> {
        let ks: Vec<String> = self.lazy().keys().cloned().collect();
        for k in ks {
            self.fill(&k);
        }
        self.data().iter().map(|(k, v)| (k, &**v))
    }
}

impl std::ops::Index<&String> for Store {
    type Output = Vec<u8>;
    fn index(&self, k: &String) -> &Vec<u8> {
        self.get(k).expect("file")
    }
}
