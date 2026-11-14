// NT system calls for Wine's ntdll (the part Wine runs on its Unix side):
// files, memory, process/thread/system information, registry (registry.rs) and
// tokens. Waits, events, timers and the like are in sched.rs.

use crate::files::Handle;
use crate::status::*;
use crate::Engine;

mod file;
mod loader;
mod memory;
mod process;
mod section;
mod security;

/// The byte offset an NtReadFile/NtWriteFile call names, or None for "the current position"
/// (no offset, or FILE_USE_FILE_POINTER_POSITION / FILE_WRITE_TO_END_OF_FILE style negatives).
fn file_offset(e: &mut Engine, offp: u64) -> Option<u64> {
    if offp == 0 {
        return None;
    }
    let o = e.mmu.read64(offp).unwrap_or(u64::MAX);
    if o as i64 >= 0 {
        Some(o)
    } else {
        None
    }
}

impl Engine {
    pub(crate) fn iosb(&mut self, p: u64, status: u64, info: u64) {
        if p != 0 {
            self.w64(p, status);
            self.w64(p + 8, info);
        }
    }
    pub(crate) fn zero_n(&mut self, a: u64, n: u64) {
        for i in 0..n {
            self.w8(a + i, 0);
        }
    }
    /// Name of an OBJECT_ATTRIBUTES (its UNICODE_STRING ObjectName).
    pub(crate) fn oa_name(&mut self, oa: u64) -> String {
        let us = self.r64(oa + 16);
        if us == 0 {
            return String::new();
        }
        let len = self.r16(us) as usize / 2;
        let buf = self.r64(us + 8);
        let v: Vec<u16> = (0..len).map(|i| self.r16(buf + 2 * i as u64)).collect();
        String::from_utf16_lossy(&v)
    }
    /// Text of a UNICODE_STRING.
    pub(crate) fn us_read(&mut self, us: u64) -> String {
        if us == 0 {
            return String::new();
        }
        let len = self.r16(us) as usize / 2;
        let buf = self.r64(us + 8);
        let v: Vec<u16> = (0..len).map(|i| self.r16(buf + 2 * i as u64)).collect();
        String::from_utf16_lossy(&v)
    }
    /// An LDR_DATA_TABLE_ENTRY for a module (made once, on demand).
    fn ldr_entry(&mut self, base: u64) -> u64 {
        if let Some(&a) = self.s.ldr_entries.get(&base) {
            return a;
        }
        let Some(m) = self.s.modules.iter().find(|m| m.base == base) else { return 0 };
        let (name, size, entry) = (m.name.clone(), m.size, m.entry);
        let a = self.heap_alloc(0x140, true);
        self.w64(a + 0x30, base);
        self.w64(a + 0x38, entry);
        self.w32(a + 0x40, size as u32);
        for (off, text) in [(0x48u64, format!("C:\\Windows\\System32\\{}", name)), (0x58, name)] {
            let w: Vec<u16> = text.encode_utf16().collect();
            let b = self.heap_alloc(2 * w.len() as u64 + 2, true);
            for (k, c) in w.iter().enumerate() {
                self.w16(b + 2 * k as u64, *c);
            }
            self.w16(a + off, 2 * w.len() as u16);
            self.w16(a + off + 2, 2 * w.len() as u16 + 2);
            self.w64(a + off + 8, b);
        }
        self.w16(a + 0x6C, 1); // ObsoleteLoadCount
        self.s.ldr_entries.insert(base, a);
        a
    }
    /// Loads the CPU from a CONTEXT (registers, flags, rip, XMM).
    /// Starts ntdll's KiUserExceptionDispatcher for an exception raised with `rec` and `ctx`:
    /// below the context's stack go the context, its (empty) CONTEXT_EX, the record and a
    /// machine frame (Wine's exc_stack_layout, 0x5c0 bytes).
    pub(crate) fn user_exception(&mut self, rec: u64, ctx: u64) -> Option<u64> {
        let nt = crate::pe::find_module(self, "wine-ntdll.dll").map(|i| self.s.modules[i].base)?;
        let disp = crate::pe::export_addr(self, nt, "KiUserExceptionDispatcher")?;
        let rsp = self.r64(ctx + 0x98) & !15;
        let st = (rsp - 0x5c0) & !63;
        let mut c = vec![0u8; 0x4d0];
        let _ = self.mmu.read_bytes(ctx, &mut c);
        let mut r = vec![0u8; 0x98];
        let _ = self.mmu.read_bytes(rec, &mut r);
        self.zero_n(st, 0x5c0);
        let _ = self.mmu.write_bytes(st, &c);
        let flags = self.r32(st + 0x30) & !0x40; // no XSTATE part
        self.w32(st + 0x30, flags);
        let _ = self.mmu.write_bytes(st + 0x4f0, &r);
        let (rip, sp) = (self.r64(ctx + 0xF8), self.r64(ctx + 0x98));
        self.w64(st + 0x590, rip);
        self.w64(st + 0x598, 0x33);
        let fl = self.r32(ctx + 0x44) as u64;
        self.w64(st + 0x5a0, fl);
        self.w64(st + 0x5a8, sp);
        self.w64(st + 0x5b0, 0x2b);
        self.cpu.regs[4] = st;
        self.cpu.rip = disp;
        Some(crate::kernel32::NO_RETURN)
    }

    /// NtQueryDirectoryFile(Ex): (folder, event, apc, apc context, iosb, buffer, length, class,
    /// single entry / Ex: flags, mask, restart). The first call (or a restart) takes the mask
    /// and lists the folder; each call then fills the buffer with as many entries as fit.
    pub(crate) fn query_directory(&mut self, h: u64, ex: bool) -> u64 {
        let (iosb, buf, len, class) = (self.arg(4), self.arg(5), self.arg(6) as u32 as u64, self.arg(7) as u32);
        let (single, mask, restart) = if ex {
            let f = self.arg(8) as u32;
            (f & 2 != 0, self.arg(9), f & 1 != 0)
        } else {
            (self.arg(8) as u8 != 0, self.arg(9), self.arg(10) as u8 != 0)
        };
        // fixed part of each entry and where the name goes
        let (name_at, len_at) = match class {
            1 => (64, 60),  // FileDirectoryInformation
            2 => (68, 60),  // FileFullDirectoryInformation
            3 => (94, 60),  // FileBothDirectoryInformation
            12 => (12, 8),  // FileNamesInformation
            37 => (104, 60), // FileIdBothDirectoryInformation
            38 => (80, 60), // FileIdFullDirectoryInformation
            60 => (88, 60), // FileIdExtdDirectoryInformation
            63 => (114, 60), // FileIdExtdBothDirectoryInformation (Wine's FindFirstFile)
            _ => {
                if unsafe { crate::WINE_DEBUG } > 0 {
                    crate::console(&format!("[ntdll] NtQueryDirectoryFile class {} not handled\n", class));
                }
                return INVALID_INFO_CLASS;
            }
        };
        let Some((dir, _)) = self.s.fs.paths.get(&h).cloned() else { return NO_MORE_FILES };
        let dir = dir.trim_end_matches('\\').to_string();
        let first = restart || !self.s.fs.scans.contains_key(&h);
        if first {
            let pat = self.us_read(mask);
            let all = pat.is_empty() || pat == "*" || pat == "*.*";
            let mut v: Vec<String> = self
                .list_dir(&dir)
                .into_iter()
                .filter(|n| all || crate::files::wild(pat.as_bytes(), self.shown(n).as_bytes()))
                .collect();
            if !dir.ends_with(':') {
                for (i, d) in [".", ".."].iter().enumerate() {
                    if all || crate::files::wild(pat.as_bytes(), d.as_bytes()) {
                        v.insert(i.min(v.len()), format!("{}\\{}", dir, d));
                    }
                }
            }
            if v.is_empty() {
                self.s.fs.scans.insert(h, (v, 0));
                return 0xC000_000F; // STATUS_NO_SUCH_FILE
            }
            self.s.fs.scans.insert(h, (v, 0));
        }
        let (names, mut next) = self.s.fs.scans.get(&h).cloned().unwrap();
        let ft = crate::filetime_now();
        let (mut at, mut prev): (u64, Option<u64>) = (0, None);
        while next < names.len() {
            let k = &names[next];
            let leaf = k.rsplit('\\').next().unwrap_or(k).to_string();
            let shown = if leaf == "." || leaf == ".." { leaf.clone() } else { self.shown(k) };
            let w: Vec<u16> = shown.encode_utf16().collect();
            let size = name_at + 2 * w.len() as u64;
            if at + size > len {
                if prev.is_none() {
                    return BUFFER_OVERFLOW;
                }
                break;
            }
            let p = buf + at;
            self.zero_n(p, name_at);
            if class == 12 {
                self.w32(p + 4, next as u32);
            } else {
                let (dirp, fsize) = if leaf == "." || leaf == ".." { (true, 0) } else { match self.s.files.size(k) { Some(n) => (false, n), None => (true, 0) } };
                self.w32(p + 4, next as u32);
                for o in [8, 16, 24, 32] {
                    self.w64(p + o, ft);
                }
                self.w64(p + 40, fsize);
                self.w64(p + 48, (fsize + 4095) & !4095);
                self.w32(p + 56, if dirp { 0x10 } else { 0x20 });
                if class == 37 {
                    self.w64(p + 96, next as u64 + 1);
                } else if class == 38 || class == 60 || class == 63 {
                    self.w64(p + 72, next as u64 + 1);
                }
            }
            self.w32(p + len_at, 2 * w.len() as u32);
            for (i, c) in w.iter().enumerate() {
                self.w16(p + name_at + 2 * i as u64, *c);
            }
            if let Some(q) = prev {
                self.w32(buf + q, (at - q) as u32);
            }
            prev = Some(at);
            at = (at + size + 7) & !7;
            next += 1;
            if single {
                break;
            }
        }
        let Some(last) = prev else { return NO_MORE_FILES };
        let used = (last + name_at).max(at.min(len));
        if let Some(s) = self.s.fs.scans.get_mut(&h) {
            s.1 = next;
        }
        self.iosb(iosb, OK, used.min(len));
        OK
    }

    pub fn load_context(&mut self, c: u64) {
        let flags = self.r32(c + 0x30);
        for i in 0..16 {
            self.cpu.regs[i] = self.r64(c + 0x78 + 8 * i as u64);
        }
        let f = self.mmu.read32(c + 0x44).unwrap_or(0x202) as u64;
        self.set_flags(f);
        if flags & 8 != 0 || flags == 0 {
            for i in 0..16u64 {
                let lo = self.r64(c + 0x1A0 + 16 * i) as u128;
                let hi = self.r64(c + 0x1A8 + 16 * i) as u128;
                self.cpu.xmm[i as usize] = lo | hi << 64;
            }
        }
        self.cpu.rip = self.r64(c + 0xF8);
    }
    fn file_info(&mut self, h: u64) -> Option<(bool, u64, u64)> {
        // (is directory, size, position)
        match self.s.fs.handles.get(&h) {
            Some(Handle::File { name, pos }) => {
                let p = *pos;
                let n = name.clone();
                Some((false, self.s.files.size(&n).unwrap_or(0), p))
            }
            Some(Handle::Dir) => Some((true, 0, 0)),
            _ => None,
        }
    }
}

/// Each family of calls lives in its own file; the first one that knows the name answers.
pub fn call(e: &mut Engine, name: &str) -> Option<u64> {
    const FAMILIES: [fn(&mut Engine, &str) -> Option<u64>; 6] =
        [loader::call, file::call, memory::call, process::call, security::call, section::call];
    for family in FAMILIES {
        e.not_mine = false;
        let r = family(e, name);
        if !e.not_mine {
            return r;
        }
    }
    None // e.not_mine stays set: no family has this name
}
