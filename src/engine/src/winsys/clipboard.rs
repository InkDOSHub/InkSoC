// The clipboard (win32u's clipboard.c and wineserver's clipboard.c): one for every process.
// user32 hands win32u each format as bytes (marshal_data), and gets them back the same way;
// win32u keeps, per process, the handle each format was unmarshaled to (the cache).

use super::*;

const NOT_OPEN: u64 = 0xC001_0000 | 1418; // ERROR_CLIPBOARD_NOT_OPEN as a status
const CF_TEXT: u32 = 1;
const CF_OEMTEXT: u32 = 7;
const CF_UNICODETEXT: u32 = 13;
const CF_LOCALE: u32 = 16;

pub struct ClipFormat {
    pub id: u32,
    pub data: Vec<u8>,
    pub seqno: u32,
    /// (process, handle): what the data was unmarshaled to in each process
    pub handles: Vec<(u32, u64)>,
}

#[derive(Default)]
pub struct Clip {
    /// (process, thread) that has it open, and the window it was opened for
    pub open: Option<(u32, u32)>,
    pub open_win: u64,
    open_seqno: u32,
    pub owner: u64,
    pub seqno: u32,
    pub formats: Vec<ClipFormat>,
    pub viewer: u64,
    pub listeners: Vec<u64>,
}

impl Engine {
    fn clip_mine(&self) -> bool {
        self.s.win.clip.open == Some((self.s.wpid, self.cur_tid()))
    }

    /// CF_TEXT, CF_OEMTEXT and CF_UNICODETEXT from one another, and CF_LOCALE, as wineserver
    /// adds them when the clipboard is closed after a change.
    fn clip_synthesize(&mut self) {
        let c = &mut self.s.win.clip;
        let has = |c: &Clip, f| c.formats.iter().any(|x| x.id == f && !x.data.is_empty());
        let text: Option<String> = if let Some(u) = c.formats.iter().find(|x| x.id == CF_UNICODETEXT && !x.data.is_empty()) {
            let w: Vec<u16> = u.data.chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]])).take_while(|&ch| ch != 0).collect();
            Some(String::from_utf16_lossy(&w))
        } else {
            c.formats
                .iter()
                .find(|x| (x.id == CF_TEXT || x.id == CF_OEMTEXT) && !x.data.is_empty())
                .map(|a| a.data.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect())
        };
        let Some(t) = text else { return };
        let seqno = c.seqno;
        let ansi = || -> Vec<u8> { t.chars().map(|ch| if (ch as u32) < 256 { ch as u8 } else { b'?' }).chain([0]).collect() };
        for f in [CF_UNICODETEXT, CF_TEXT, CF_OEMTEXT, CF_LOCALE] {
            if has(c, f) {
                continue;
            }
            let data = match f {
                CF_UNICODETEXT => t.encode_utf16().chain([0]).flat_map(|u| u.to_le_bytes()).collect(),
                CF_LOCALE => 0x409u32.to_le_bytes().to_vec(),
                _ => ansi(),
            };
            c.formats.retain(|x| x.id != f);
            c.formats.push(ClipFormat { id: f, data, seqno, handles: Vec::new() });
        }
    }

    fn clip_ids(&self) -> Vec<u32> {
        self.s.win.clip.formats.iter().map(|f| f.id).collect()
    }

    /// The clipboard's system calls; None for any other name.
    pub(crate) fn clip_call(&mut self, name: &str, ar: &[u64]) -> Option<u64> {
        let pid = self.s.wpid;
        let tid = self.cur_tid();
        Some(match name {
            "NtUserOpenClipboard" => {
                let c = &self.s.win.clip;
                if let Some((p, t)) = c.open {
                    let gone = self.s.pinfo.get(&p).is_some_and(|i| i.exit.is_some());
                    if (p, t) != (pid, tid) && !gone {
                        self.set_last_error(5); // ERROR_ACCESS_DENIED
                        return Some(0);
                    }
                }
                let c = &mut self.s.win.clip;
                if c.open != Some((pid, tid)) {
                    c.open_seqno = c.seqno;
                }
                c.open = Some((pid, tid));
                c.open_win = full(ar[0]);
                1
            }
            "NtUserCloseClipboard" => {
                if !self.clip_mine() {
                    self.set_last_error(1418);
                    return Some(0);
                }
                let changed = self.s.win.clip.seqno != self.s.win.clip.open_seqno;
                if changed {
                    self.clip_synthesize();
                }
                let c = &mut self.s.win.clip;
                c.open = None;
                c.open_win = 0;
                if changed {
                    let (viewer, owner, ls) = (c.viewer, c.owner, c.listeners.clone());
                    for l in ls {
                        self.post(l, 0x031D, 0, 0); // WM_CLIPBOARDUPDATE
                    }
                    if viewer != 0 {
                        self.post(viewer, 0x0308, owner, 0); // WM_DRAWCLIPBOARD
                    }
                }
                1
            }
            "NtUserEmptyClipboard" => {
                if !self.clip_mine() {
                    self.set_last_error(1418);
                    return Some(0);
                }
                let c = &mut self.s.win.clip;
                let old = c.owner;
                c.formats.clear();
                c.owner = c.open_win;
                c.seqno += 1;
                if old != 0 && old != c.owner {
                    self.post(old, 0x0307, 0, 0); // WM_DESTROYCLIPBOARD
                }
                1
            }
            "NtUserSetClipboardData" => {
                // (format, handle, set_clipboard_params*: data, size, cache_only, seqno)
                let (f, h, p) = (ar[0] as u32, ar[1], ar[2]);
                if p != 0 && self.r32(p + 16) != 0 {
                    let seq = self.r32(p + 20);
                    let Some(x) = self.s.win.clip.formats.iter_mut().find(|x| x.id == f && x.seqno == seq) else { return Some(0xC000_0001) };
                    x.handles.retain(|&(q, _)| q != pid);
                    x.handles.push((pid, h));
                    return Some(0);
                }
                if !self.clip_mine() {
                    return Some(NOT_OPEN);
                }
                let (data, size) = if p != 0 { (self.r64(p), self.r64(p + 8)) } else { (0, 0) };
                let mut buf = vec![0u8; if data != 0 { size.min(256 << 20) as usize } else { 0 }];
                if !buf.is_empty() && self.mmu.read_bytes(data, &mut buf).is_err() {
                    return Some(0xC000_0005);
                }
                let c = &mut self.s.win.clip;
                c.seqno += 1;
                let handles = if data != 0 && h != 0 { vec![(pid, h)] } else { Vec::new() };
                c.formats.retain(|x| x.id != f);
                c.formats.push(ClipFormat { id: f, data: buf, seqno: c.seqno, handles });
                0
            }
            "NtUserGetClipboardData" => {
                // (format, get_clipboard_params*: data, size, data_size, seqno, data_only)
                let (f, p) = (ar[0] as u32, ar[1]);
                if !self.clip_mine() {
                    self.set_last_error(1418);
                    return Some(0);
                }
                let (dst, room, data_only) = (self.r64(p), self.r64(p + 8), self.r32(p + 28) != 0);
                let Some(x) = self.s.win.clip.formats.iter().find(|x| x.id == f) else {
                    self.w64(p + 8, 0);
                    self.set_last_error(1168); // ERROR_NOT_FOUND
                    return Some(0);
                };
                let (seqno, data) = (x.seqno, x.data.clone());
                let cached = x.handles.iter().find(|&&(q, _)| q == pid).map(|&(_, h)| h);
                self.w64(p + 8, data.len() as u64);
                if data.is_empty() {
                    return Some(0); // rendered on request: not supported
                }
                if let (Some(h), false) = (cached, data_only) {
                    return Some(h);
                }
                if (room as usize) < data.len() {
                    self.w64(p + 16, data.len() as u64);
                    return Some(0);
                }
                let _ = self.mmu.write_bytes(dst, &data);
                if data_only {
                    return Some(dst);
                }
                self.w32(p + 24, seqno);
                self.w64(p + 16, u64::MAX);
                0
            }
            "NtUserCountClipboardFormats" => self.s.win.clip.formats.len() as u64,
            "NtUserIsClipboardFormatAvailable" => (ar[0] as u32 != 0 && self.clip_ids().contains(&(ar[0] as u32))) as u64,
            "NtUserGetUpdatedClipboardFormats" => {
                // (UINT *formats, UINT size, UINT *out_size)
                let (list, size, out) = (ar[0], ar[1] as u32 as usize, ar[2]);
                if out == 0 {
                    self.set_last_error(998); // ERROR_NOACCESS
                    return Some(0);
                }
                let ids = self.clip_ids();
                self.w32(out, ids.len() as u32);
                if list == 0 || size < ids.len() {
                    self.set_last_error(if list == 0 && !ids.is_empty() { 998 } else { 122 });
                    return Some(ids.is_empty() as u64);
                }
                for (i, id) in ids.iter().enumerate() {
                    self.w32(list + 4 * i as u64, *id);
                }
                1
            }
            "NtUserGetPriorityClipboardFormat" => {
                let ids = self.clip_ids();
                if ids.is_empty() {
                    return Some(0);
                }
                let n = (ar[1] as u32 as i32).max(0) as u64;
                for i in 0..n {
                    let f = self.r32(ar[0] + 4 * i);
                    if ids.contains(&f) {
                        return Some(f as u64);
                    }
                }
                u32::MAX as u64 // -1
            }
            "NtUserEnumClipboardFormats" => {
                if !self.clip_mine() {
                    self.set_last_error(1418);
                    return Some(0);
                }
                let ids = self.clip_ids();
                let prev = ar[0] as u32;
                let next = if prev == 0 { ids.first().copied() } else { ids.iter().position(|&i| i == prev).and_then(|k| ids.get(k + 1).copied()) };
                self.set_last_error(0);
                next.unwrap_or(0) as u64
            }
            "NtUserGetClipboardFormatName" => {
                // (format, WCHAR *buffer, maxlen): registered formats are global atoms
                let (f, buf, max) = (ar[0] as u32, ar[1], ar[2] as u32 as i32);
                let name = if f >= 0xC000 { self.s.atoms.get((f - 0xC000) as usize).cloned() } else { None };
                let Some(n) = name else {
                    self.set_last_error(87); // ERROR_INVALID_PARAMETER
                    return Some(0);
                };
                if max <= 0 {
                    return Some(0);
                }
                self.put_w(buf, &n, max as usize) as u64
            }
            "NtUserGetClipboardOwner" => self.s.win.clip.owner,
            "NtUserGetClipboardViewer" => self.s.win.clip.viewer,
            "NtUserGetOpenClipboardWindow" => self.s.win.clip.open_win,
            "NtUserGetClipboardSequenceNumber" => self.s.win.clip.seqno as u64,
            "NtUserSetClipboardViewer" => {
                let h = full(ar[0]);
                let c = &mut self.s.win.clip;
                let (prev, owner) = (c.viewer, c.owner);
                c.viewer = h;
                if h != 0 {
                    self.post(h, 0x0308, owner, 0); // WM_DRAWCLIPBOARD
                }
                prev
            }
            "NtUserChangeClipboardChain" => {
                let (h, next) = (full(ar[0]), full(ar[1]));
                if h == 0 {
                    return Some(0);
                }
                let v = self.s.win.clip.viewer;
                if v == h {
                    self.s.win.clip.viewer = next;
                } else if v != 0 {
                    self.post(v, 0x030D, h, next); // WM_CHANGECBCHAIN
                }
                1
            }
            "NtUserAddClipboardFormatListener" => {
                let h = full(ar[0]);
                if self.wnd(h).is_none() {
                    self.set_last_error(1400); // ERROR_INVALID_WINDOW_HANDLE
                    return Some(0);
                }
                let c = &mut self.s.win.clip;
                if c.listeners.contains(&h) {
                    self.set_last_error(87);
                    return Some(0);
                }
                c.listeners.push(h);
                1
            }
            "NtUserRemoveClipboardFormatListener" => {
                let h = full(ar[0]);
                let c = &mut self.s.win.clip;
                if !c.listeners.contains(&h) {
                    self.set_last_error(87);
                    return Some(0);
                }
                c.listeners.retain(|&l| l != h);
                1
            }
            _ => return None,
        })
    }

    /// A window going away stops owning or watching the clipboard.
    pub(super) fn clip_forget(&mut self, h: u64) {
        let c = &mut self.s.win.clip;
        if c.owner == h {
            c.owner = 0;
        }
        if c.viewer == h {
            c.viewer = 0;
        }
        if c.open_win == h {
            c.open_win = 0;
        }
        c.listeners.retain(|&l| l != h);
    }
}
