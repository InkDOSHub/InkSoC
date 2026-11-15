// Wine's GDI objects and device contexts on the engine side: handles, DC attributes, owned DCs.

use super::*;

impl Engine {
    pub(super) fn s64(&mut self, a: u64, v: u64) {
        if a != 0 {
            self.w64(a, v);
        }
    }
    pub(super) fn bytes(&mut self, a: u64, n: usize) -> Vec<u8> {
        let mut v = vec![0u8; n];
        let _ = self.mmu.read_bytes(a, &mut v);
        v
    }

    /// Commits the pages that `a..a+len` touches, before the engine writes there. Only those
    /// pages are committed, never the whole region. False when they cannot be committed.
    /// On failure the commit list, the counter and the pages this call committed for the first
    /// time are restored; residence of pages committed before the call, and frames taken from
    /// other pages, may have changed (see `Mmu::commit_for_write`).
    pub(crate) fn prepare_write(&mut self, a: u64, len: u64) -> bool {
        self.mmu.commit_for_write(a, len).is_ok()
    }

    /// The GDI handle table (at PEB+0xF8) with the stock objects in Wine's slots.
    ///
    /// Guarantee: `gdi.table` and the PEB pointer are set only when every stock object was made
    /// and every flag byte was written. A failure gives the table back and leaves no object in
    /// `gdi`, so the next call starts again from nothing. Until then nothing outside this
    /// function can see the table: the stock objects are added to it directly (`gdi_add`).
    pub(crate) fn gdi_init(&mut self) {
        if self.s.gdi.table != 0 {
            return;
        }
        let Some(t) = self.reserve(0x10000 * 24) else { return };
        // the stock objects use the first FIRST+24 entries: commit them before any write
        if !self.prepare_write(t, 24 * (FIRST as u64 + 24)) {
            self.unreserve(t, 0x10000 * 24);
            return;
        }
        self.s.gdi.next = FIRST;
        let done = self.build_stock(t) && self.flag_stock(t) && self.mmu.write64(crate::PEB + 0xF8, t).is_ok();
        if !done {
            self.gdi_discard(t);
            return;
        }
        self.s.gdi.table = t;
    }

    /// Makes the stock objects, in the order Wine gives them. False at the first one that fails.
    fn build_stock(&mut self, t: u64) -> bool {
        for c in [0xFFFFFF, 0xC0C0C0, 0x808080, 0x404040, 0] {
            if self.gdi_add(t, E_BRUSH, Obj::Brush(Brush::Solid(c))) == 0 {
                return false;
            }
        }
        if self.gdi_add(t, E_BRUSH, Obj::Brush(Brush::Null)) == 0 {
            return false;
        }
        for (s, c) in [(0, 0xFFFFFF), (0, 0), (5, 0)] {
            let pen = Pen { style: s, width: 0, color: c, dc: false, ext: false };
            if self.gdi_add(t, E_PEN, Obj::Pen(pen)) == 0 {
                return false;
            }
        }
        let fonts = [
            (12, 400, "Terminal"), // 9: scaled OEM
            (12, 400, "Terminal"),
            (12, 400, "Courier"),
            (12, 400, "MS Sans Serif"),
            (16, 700, "System"),
            (16, 700, "System"),
        ];
        for (h, wt, face) in fonts {
            if !self.stock_font(t, h, wt, face) {
                return false;
            }
        }
        if self.gdi_add(t, E_PAL, Obj::Pal(default_pal().to_vec())) == 0 {
            return false;
        }
        for (h, wt, face) in [(16, 400, "Fixedsys"), (-11, 400, "MS Shell Dlg")] {
            if !self.stock_font(t, h, wt, face) {
                return false;
            }
        }
        if self.gdi_add(t, E_BRUSH, Obj::Brush(Brush::Dc)) == 0 {
            return false;
        }
        let dc_pen = Pen { style: 0, width: 0, color: 0, dc: true, ext: false };
        if self.gdi_add(t, E_PEN, Obj::Pen(dc_pen)) == 0 {
            return false;
        }
        let bmp = Bmp { w: 1, h: 1, bpp: 1, px: vec![0], dib: 0, stride: 2, pal: vec![], topdown: true, masks: [0; 3], comp: 0 };
        if self.gdi_add(t, E_BMP, Obj::Bmp(bmp)) == 0 {
            return false;
        }
        for (h, wt, face) in [(16, 700, "System"), (16, 400, "Fixedsys"), (-11, 400, "MS Shell Dlg")] {
            if !self.stock_font(t, h, wt, face) {
                return false;
            }
        }
        true
    }

    /// One stock font in the table at `t`. False when it could not be made.
    fn stock_font(&mut self, t: u64, h: i32, wt: i32, face: &str) -> bool {
        let lf = mk_lf(h, wt, face);
        let fi = pick_font(h, wt);
        self.gdi_add(t, E_FONT, Obj::Font(Font { lf, fi })) != 0
    }

    /// Sets the 0x80 flag on the stock entries. False when a byte cannot be read or written.
    fn flag_stock(&mut self, t: u64) -> bool {
        for i in FIRST..FIRST + 24 {
            let a = t + 24 * i as u64 + 12;
            let Ok(v) = self.mmu.read8(a) else { return false };
            if self.mmu.write8(a, v | 0x80).is_err() {
                return false;
            }
        }
        true
    }

    /// Gives back a table whose setup failed: its entries are forgotten, its pages are
    /// released, and the address range can be used again.
    fn gdi_discard(&mut self, t: u64) {
        let g = &mut self.s.gdi;
        g.objs.clear();
        g.free.clear();
        g.next = 0;
        g.table = 0;
        self.unreserve(t, 0x10000 * 24);
    }

    pub(super) fn gdi_new(&mut self, ext: u32, obj: Obj) -> u64 {
        self.gdi_init();
        let t = self.s.gdi.table;
        if t == 0 {
            // the table could not be made: there is no handle to give
            return 0;
        }
        self.gdi_add(t, ext, obj)
    }

    /// Puts `obj` in the first free slot of the table at `t`. Returns its handle, or 0 with
    /// nothing changed: no index taken, no object registered, no entry half-written.
    fn gdi_add(&mut self, t: u64, ext: u32, obj: Obj) -> u64 {
        // the slot is chosen first; nothing is taken until its entry is written
        let (i, from_free) = match self.s.gdi.free.last() {
            Some(&i) => (i, true),
            None => (self.s.gdi.next, false),
        };
        if i >= 0x10000 {
            return 0;
        }
        let e = t + 24 * i as u64;
        // the entry is committed and resident before it is read or written
        if !self.prepare_write(e, 24) {
            return 0;
        }
        let Ok(old_gen) = self.mmu.read8(e + 13) else { return 0 };
        let gen = if old_gen + 1 >= 0x80 { 1 } else { old_gen + 1 };
        // the whole entry is written in one checked write, and the handle is published only after it
        if self.mmu.write_bytes(e, &gdi_entry(i, ext, gen)).is_err() {
            return 0;
        }
        let g = &mut self.s.gdi;
        if from_free { g.free.pop(); } else { g.next += 1; }
        g.objs.insert(i, obj);
        let tag = (ext as u8) as u64 | (gen as u64) << 8;
        i as u64 | tag << 16
    }

    /// Full handle (with the type and generation bits) of a table slot.
    pub(super) fn handle_of(&mut self, i: u32) -> u64 {
        let t = self.s.gdi.table;
        if t == 0 {
            return 0;
        }
        i as u64 | ((self.r16(t + 24 * i as u64 + 12) as u32) as u64) << 16
    }
    pub(super) fn stock(&mut self, i: u32) -> u64 {
        self.gdi_init();
        self.handle_of(FIRST + i)
    }
    pub(super) fn is_stock(&self, h: u64) -> bool {
        let i = idx(h);
        i >= FIRST && i < FIRST + 24
    }

    pub(super) fn gdi_free(&mut self, i: u32) {
        if self.s.gdi.objs.remove(&i).is_none() {
            return;
        }
        let e = self.s.gdi.table + 24 * i as u64;
        self.w64(e, 0);
        self.w8(e + 14, 0);
        self.w64(e + 16, 0);
        self.s.gdi.free.push(i);
    }

    pub(super) fn dc(&self, h: u64) -> Option<&Dc> {
        match self.s.gdi.objs.get(&idx(h)) {
            Some(Obj::Dc(d)) => Some(d),
            _ => None,
        }
    }
    pub(super) fn dc_mut(&mut self, h: u64) -> Option<&mut Dc> {
        match self.s.gdi.objs.get_mut(&idx(h)) {
            Some(Obj::Dc(d)) => Some(d),
            _ => None,
        }
    }

    pub(super) fn attr_new(&mut self, hdc: u64) -> u64 {
        // a freed block first; otherwise the next slot, or a new 64 KB block when the current
        // one is full. The slot's pages are committed before any state changes.
        let (a, from_free, fresh) = match self.s.gdi.attr_free.last() {
            Some(&a) => (a, true, None),
            None => {
                if self.s.gdi.attr_next + ATTR_SIZE > self.s.gdi.attr_end {
                    let Some(p) = self.reserve(0x10000) else { return 0 };
                    (p, false, Some(p))
                } else {
                    (self.s.gdi.attr_next, false, None)
                }
            }
        };
        // the whole block is written in one checked write; the allocator changes only after it
        let block = attr_block(hdc);
        if !self.prepare_write(a, ATTR_SIZE) || self.mmu.write_bytes(a, &block).is_err() {
            if let Some(p) = fresh {
                self.unreserve(p, 0x10000);
            }
            return 0;
        }
        let g = &mut self.s.gdi;
        if from_free {
            g.attr_free.pop();
        } else if let Some(p) = fresh {
            g.attr_end = p + 0x10000;
            g.attr_next = p + ATTR_SIZE;
        } else {
            g.attr_next += ATTR_SIZE;
        }
        a
    }

    pub(super) fn dc_new(&mut self, hwnd: u64, whole: bool, mem: bool) -> u64 {
        let (bmp, brush, pen, font, pal) = (if mem { self.stock(20) } else { 0 }, self.stock(0), self.stock(7), self.stock(13), self.stock(15));
        let h = self.gdi_new(
            if mem { E_MEMDC } else { E_DC },
            Obj::Dc(Dc { attr: 0, hwnd, whole, mem, bmp, brush, pen, font, pal, clip: None, vis: None, saved: vec![] }),
        );
        // no handle (the table or the entry could not be written): no attribute block either
        if h == 0 {
            return 0;
        }
        let a = self.attr_new(h);
        if a == 0 {
            // a device context without its attribute block is not handed out
            self.gdi_free(idx(h));
            return 0;
        }
        if let Some(d) = self.dc_mut(h) {
            d.attr = a;
        }
        let e = self.s.gdi.table + 24 * idx(h) as u64;
        self.s64(e + 16, a);
        // visible rectangle (screen coordinates)
        let r = if hwnd == 0 || mem {
            Rect::new(0, 0, SCREEN_W, SCREEN_H)
        } else {
            self.wnd(hwnd).map_or(Rect::default(), |w| if whole { w.wrect } else { w.crect })
        };
        self.wr_rect(a + 72, r);
        h
    }

    pub(super) fn dc_delete(&mut self, h: u64) {
        let i = idx(h);
        let a = match self.s.gdi.objs.get(&i) {
            Some(Obj::Dc(d)) => d.attr,
            _ => return,
        };
        self.gdi_free(i);
        if a != 0 {
            self.s.gdi.attr_free.push(a);
        }
        self.s.gdi.owndc.retain(|_, &mut v| idx(v) != i);
    }

    /// A DC for a window (GetDC / BeginPaint), or the screen for hwnd 0.
    pub(crate) fn get_dc(&mut self, h: u64, whole: bool) -> u64 {
        let h = full(h);
        let h = if h == 0x10010 { 0 } else { h };
        if h != 0 && self.wnd(h).is_none() {
            return 0;
        }
        let own = h != 0 && !whole && self.wnd(h).map_or(false, |w| self.s.win.classes[w.class].style & 0x60 != 0);
        if own {
            if let Some(&d) = self.s.gdi.owndc.get(&h) {
                if self.dc(d).is_some() {
                    if let Some(dc) = self.dc_mut(d) {
                        dc.vis = None;
                    }
                    return d;
                }
            }
            let d = self.dc_new(h, false, false);
            self.s.gdi.owndc.insert(h, d);
            return d;
        }
        self.dc_new(h, whole, false)
    }

    pub(crate) fn release_dc(&mut self, hdc: u64) {
        if self.s.gdi.owndc.values().any(|&v| idx(v) == idx(hdc)) {
            if let Some(d) = self.dc_mut(hdc) {
                d.vis = None;
            }
            return;
        }
        if self.dc(hdc).map_or(false, |d| !d.mem) {
            self.dc_delete(hdc);
        }
    }

    pub(crate) fn dc_clip_to(&mut self, hdc: u64, r: Rect) {
        if let Some(d) = self.dc_mut(hdc) {
            d.vis = Some(r);
        }
    }

    /// Fills with a brush handle or a system color index + 1 (class brushes).
    pub(crate) fn fill_rect_brush(&mut self, hdc: u64, r: Rect, brush: u64) {
        let b = if brush != 0 && brush <= 31 { self.sys_brush(brush as u32 - 1) } else { brush };
        let Some(old) = self.dc(hdc).map(|d| d.brush) else { return };
        if let Some(d) = self.dc_mut(hdc) {
            d.brush = b;
        }
        // the rectangle is in device units: draw without the mapping
        if let Some(v) = self.view(hdc) {
            let be = self.brush_eval(b, v.attr, v.mono, v.ox, v.oy);
            self.fill_with(v, r.off(v.ox, v.oy), &be, 0xF0);
        }
        if let Some(d) = self.dc_mut(hdc) {
            d.brush = old;
        }
    }

    pub(crate) fn sys_brush(&mut self, i: u32) -> u64 {
        self.gdi_init();
        if self.s.gdi.sysbrush.is_empty() {
            for k in 0..31 {
                let c = colorref(sys_color_px(k));
                let h = self.gdi_new(E_BRUSH, Obj::Brush(Brush::Solid(c)));
                let a = self.s.gdi.table + 24 * idx(h) as u64 + 12;
                let v = self.r8(a);
                self.w8(a, v | 0x80);
                let h = self.handle_of(idx(h));
                self.s.gdi.sysbrush.push(h);
            }
        }
        self.s.gdi.sysbrush.get(i as usize).copied().unwrap_or(0)
    }

    pub(crate) fn sys_pen(&mut self, i: u32) -> u64 {
        self.gdi_init();
        if self.s.gdi.syspen.is_empty() {
            for k in 0..31 {
                let c = colorref(sys_color_px(k));
                let h = self.gdi_new(E_PEN, Obj::Pen(Pen { style: 0, width: 0, color: c, dc: false, ext: false }));
                let a = self.s.gdi.table + 24 * idx(h) as u64 + 12;
                let v = self.r8(a);
                self.w8(a, v | 0x80);
                let h = self.handle_of(idx(h));
                self.s.gdi.syspen.push(h);
            }
        }
        self.s.gdi.syspen.get(i as usize).copied().unwrap_or(0)
    }

    /// Creates or resizes the surface of a top-level window (keeps old pixels).
    pub(crate) fn surface_for(&mut self, h: u64) {
        let Some(w) = self.wnd(h) else { return };
        if w.parent != 0 && w.style & 0x4000_0000 != 0 {
            return;
        }
        let (nw, nh) = (w.wrect.w().clamp(0, 4096), w.wrect.h().clamp(0, 4096));
        let s = self.s.gdi.surfs.entry(full(h)).or_insert(Surf { w: 0, h: 0, px: vec![] });
        if s.w == nw && s.h == nh {
            return;
        }
        let mut px = vec![FACE; (nw * nh) as usize];
        for y in 0..nh.min(s.h) {
            for x in 0..nw.min(s.w) {
                px[(y * nw + x) as usize] = s.px[(y * s.w + x) as usize];
            }
        }
        *s = Surf { w: nw, h: nh, px };
    }

    pub(crate) fn drop_surface(&mut self, h: u64) {
        self.s.gdi.surfs.remove(&full(h));
        if let Some(d) = self.s.gdi.owndc.remove(&full(h)) {
            self.dc_delete(d);
        }
    }
}

/// The 24 bytes of a GDI table entry: the handle (index plus one), zero, the type and the
/// generation, and the rest zero. The same bytes the entry had when it was written field by field.
pub(super) fn gdi_entry(i: u32, ext: u32, gen: u8) -> [u8; 24] {
    let mut b = [0u8; 24];
    b[0..8].copy_from_slice(&(1 + i as u64).to_le_bytes());
    b[12] = ext as u8;
    b[13] = gen;
    b[14] = (ext & 0x1F) as u8;
    b
}

fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn put16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

/// The bytes of a new device-context attribute block: the defaults a DC starts with, and zero
/// everywhere else. Written in one piece, so no field is left at an old value.
pub(super) fn attr_block(hdc: u64) -> [u8; ATTR_SIZE as usize] {
    let mut b = [0u8; ATTR_SIZE as usize];
    put32(&mut b, 0, hdc as u32);
    put32(&mut b, 12, 0xFFFFFF); // background
    put32(&mut b, 16, 0xFFFFFF); // DC brush
    put32(&mut b, 36, 1); // GM_COMPATIBLE
    put32(&mut b, 40, 1); // AD_COUNTERCLOCKWISE
    put16(&mut b, 50, 2); // OPAQUE
    put16(&mut b, 52, 1); // ALTERNATE
    put16(&mut b, 54, 13); // R2_COPYPEN
    put16(&mut b, 56, 1);
    put16(&mut b, 58, 1); // BLACKONWHITE
    put32(&mut b, 60, 1); // MM_TEXT
    put32(&mut b, 88, 10.0f32.to_bits());
    for o in [108usize, 112, 124, 128] {
        put32(&mut b, o, 1);
    }
    put32(&mut b, 132, SCREEN_W as u32);
    put32(&mut b, 136, SCREEN_H as u32);
    put32(&mut b, 140, 320);
    put32(&mut b, 144, 240);
    b
}

#[cfg(test)]
mod gdi_state_tests {
    use super::*;

    #[test]
    fn gdi_entry_holds_the_whole_entry() {
        let b = gdi_entry(40, 0x12, 3);
        assert_eq!(u64::from_le_bytes(b[0..8].try_into().unwrap()), 41);
        assert_eq!(&b[8..12], &[0u8; 4]);
        assert_eq!(b[12], 0x12);
        assert_eq!(b[13], 3);
        assert_eq!(b[14], 0x12 & 0x1F);
        assert_eq!(&b[15..24], &[0u8; 9]);
    }

    #[test]
    fn attr_block_has_every_default_and_zero_elsewhere() {
        let b = attr_block(0x1234);
        assert_eq!(u32::from_le_bytes(b[0..4].try_into().unwrap()), 0x1234);
        assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(b[12..16].try_into().unwrap()), 0xFFFFFF);
        assert_eq!(u16::from_le_bytes(b[54..56].try_into().unwrap()), 13);
        assert_eq!(u32::from_le_bytes(b[144..148].try_into().unwrap()), 240);
        // the block has the size of an attribute, and the bytes after the last default are zero
        assert_eq!(b.len(), ATTR_SIZE as usize);
        assert_eq!(&b[148..160], &[0u8; 12]);
    }
}
