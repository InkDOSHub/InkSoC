// The window system: Wine's win32u system calls (NtUser*), the part Wine
// runs on its Unix side. Wine's own user32.dll, gdi32.dll and comctl32.dll
// run as program code on top of these. Drawing (NtGdi*) is in wingdi.rs.
//
// Kept deliberately simple: one desktop, windows drawn into per-window
// surfaces and composed into a framebuffer the page shows on a canvas.

use crate::kernel32::NO_RETURN;
use crate::Engine;
use std::collections::{HashMap, HashSet, VecDeque};

pub const SCREEN_W: i32 = 1024;
pub const SCREEN_H: i32 = 768;
const DESKTOP: u64 = 0x10010;
const MSG_WINDOW: u64 = 0x10012;

// messages
pub const WM_CREATE: u32 = 0x01;
const WM_DESTROY: u32 = 0x02;
const WM_MOVE: u32 = 0x03;
const WM_SIZE: u32 = 0x05;
const WM_ACTIVATE: u32 = 0x06;
const WM_SETFOCUS: u32 = 0x07;
const WM_KILLFOCUS: u32 = 0x08;
const WM_SETTEXT: u32 = 0x0C;
const WM_GETTEXT: u32 = 0x0D;
const WM_GETTEXTLENGTH: u32 = 0x0E;
const WM_PAINT: u32 = 0x0F;
const WM_CLOSE: u32 = 0x10;
const WM_QUIT: u32 = 0x12;
const WM_ERASEBKGND: u32 = 0x14;
const WM_SHOWWINDOW: u32 = 0x18;
const WM_SETCURSOR: u32 = 0x20;
const WM_GETMINMAXINFO: u32 = 0x24;
const WM_NCCREATE: u32 = 0x81;
const WM_NCDESTROY: u32 = 0x82;
const WM_NCHITTEST: u32 = 0x84;
const WM_NCACTIVATE: u32 = 0x86;
const WM_KEYDOWN: u32 = 0x100;
const WM_KEYUP: u32 = 0x101;
const WM_CHAR: u32 = 0x102;
const WM_SYSKEYDOWN: u32 = 0x104;
const WM_COMMAND: u32 = 0x111;
const WM_SYSCOMMAND: u32 = 0x112;
const WM_TIMER: u32 = 0x113;
const WM_INITMENU: u32 = 0x116;
const WM_INITMENUPOPUP: u32 = 0x117;
const WM_MOUSEMOVE: u32 = 0x200;
const WM_LBUTTONDOWN: u32 = 0x201;
const WM_LBUTTONDBLCLK: u32 = 0x203;
const WM_PARENTNOTIFY: u32 = 0x210;

// styles
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_DISABLED: u32 = 0x0800_0000;
const WS_CAPTION: u32 = 0x00C0_0000;
const WS_BORDER: u32 = 0x0080_0000;
const WS_DLGFRAME: u32 = 0x0040_0000;
const WS_VSCROLL: u32 = 0x0020_0000;
const WS_HSCROLL: u32 = 0x0010_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_THICKFRAME: u32 = 0x0004_0000;
const WS_EX_DLGMODALFRAME: u32 = 0x1;
const WS_EX_NOPARENTNOTIFY: u32 = 0x4;
const WS_EX_CLIENTEDGE: u32 = 0x200;
const WS_EX_STATICEDGE: u32 = 0x20000;
const CW_USEDEFAULT: i32 = i32::MIN;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Rect {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
}
impl Rect {
    pub fn new(l: i32, t: i32, r: i32, b: i32) -> Rect {
        Rect { l, t, r, b }
    }
    pub fn w(&self) -> i32 {
        self.r - self.l
    }
    pub fn h(&self) -> i32 {
        self.b - self.t
    }
    pub fn empty(&self) -> bool {
        self.r <= self.l || self.b <= self.t
    }
    /// The overlap; when there is none, an empty rectangle with r >= l and b >= t (callers
    /// fill rows l..r for t..b: a negative width made the slice panic in ExtTextOut)
    pub fn inter(&self, o: &Rect) -> Rect {
        let (l, t) = (self.l.max(o.l), self.t.max(o.t));
        Rect::new(l, t, self.r.min(o.r).max(l), self.b.min(o.b).max(t))
    }
    pub fn union(&self, o: &Rect) -> Rect {
        if self.empty() {
            return *o;
        }
        if o.empty() {
            return *self;
        }
        Rect::new(self.l.min(o.l), self.t.min(o.t), self.r.max(o.r), self.b.max(o.b))
    }
    pub fn has(&self, x: i32, y: i32) -> bool {
        x >= self.l && x < self.r && y >= self.t && y < self.b
    }
    pub fn off(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(self.l + dx, self.t + dy, self.r + dx, self.b + dy)
    }
}

pub struct Class {
    pub atom: u16,
    pub name: String, // lower case
    pub style: u32,
    pub proc_: u64,
    pub proc_a: u64,
    pub proc_w: u64,
    pub ansi: bool,
    pub cls_extra: Vec<u8>,
    pub wnd_extra: usize,
    pub inst: u64,
    pub icon: u64,
    pub icon_sm: u64,
    pub cursor: u64,
    pub brush: u64,
    pub menu_name: u64,
    pub menu_name_a: u64,
    pub builtin: bool,
    pub pid: u32, // the process that registered it (classes are per process)
}

pub struct Wnd {
    pub class: usize,
    pub parent: u64, // 0 = top level (desktop child)
    pub owner: u64,
    pub style: u32,
    pub ex_style: u32,
    pub wrect: Rect, // screen coordinates
    pub crect: Rect, // screen coordinates
    pub text: String,
    pub proc_: u64,
    pub unicode: bool,
    pub id: u64,
    pub inst: u64,
    pub userdata: u64,
    pub extra: Vec<u8>,
    pub tid: u32,
    pub update: Rect, // client coordinates
    pub erase: bool,
    pub menu: u64,
    pub children: Vec<u64>,
    pub dlg_info: u64,
    pub props: Vec<(u64, u64)>,
    pub nc_dirty: bool,
    pub icon: u64,
}

#[derive(Clone, Copy)]
pub struct Msg {
    pub hwnd: u64,
    pub msg: u32,
    pub wp: u64,
    pub lp: u64,
    pub time: u32,
    pub x: i32,
    pub y: i32,
}

#[derive(Default)]
pub struct Queue {
    pub posted: VecDeque<Msg>,
    pub input: VecDeque<Msg>,
    /// messages other threads sent to this thread's windows, with the id their sender
    /// waits on for the result (0: SendNotifyMessage, nobody waits)
    pub sent: VecDeque<(Msg, u64)>,
    pub quit: Option<i32>,
    /// a message was sent to one of this thread's windows (handled by the sender itself):
    /// the thread's message waits return a few times, so it looks again at what changed
    /// (the sender may still be inside the procedure at the first one)
    pub nudge: u8,
    pub nudge_at: f64,
}

pub struct Timer {
    pub hwnd: u64,
    pub id: u64,
    pub period: f64,
    pub next: f64,
    pub proc_: u64,
    pub tid: u32,
}

pub struct MenuItem {
    pub id: u64,
    pub text: String,
    pub flags: u32,
    pub state: u32,
    pub sub: u64,
    pub data: u64,
    pub bmp: u64,
}
#[derive(Default)]
pub struct Menu {
    pub items: Vec<MenuItem>,
    pub style: u32,
    pub data: u64,
    pub max_h: u32,
    pub help: u32,
    pub brush: u64,
}

/// A popup menu shown over the screen (drawn by the compositor).
pub struct Popup {
    pub menu: u64,
    pub hwnd: u64,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub hot: i32,
}

pub enum Step {
    Ret(u64),
    Send { hwnd: u64, msg: u32, wp: u64, lp: u64, then: Cont },
}
pub type Cont = Box<dyn FnOnce(&mut Engine, u64) -> Step>;

struct Frame {
    tid: u32,
    rsp: u64,
    regs: [u64; 8],
    xmm: [u128; 10],
    cont: Cont,
    back: (u64, usize),
}

pub struct Keys(pub [u8; 256]);
impl Default for Keys {
    fn default() -> Self {
        Keys([0; 256])
    }
}
impl std::ops::Index<usize> for Keys {
    type Output = u8;
    fn index(&self, i: usize) -> &u8 {
        &self.0[i]
    }
}
impl std::ops::IndexMut<usize> for Keys {
    fn index_mut(&mut self, i: usize) -> &mut u8 {
        &mut self.0[i]
    }
}

#[derive(Default)]
pub struct Win {
    pub unknown: HashSet<String>,
    pub classes: Vec<Class>,
    pub wins: HashMap<u64, Wnd>,
    pub tops: Vec<u64>, // top-level windows, back to front
    next_hwnd: u64,
    next_atom: u16,
    pub queues: HashMap<u32, Queue>,
    /// results of cross-thread SendMessage calls, by id, until the sender picks them up
    pub replies: HashMap<u64, u64>,
    pub reply_seq: u64,
    pub timers: Vec<Timer>,
    frames: Vec<Frame>,
    /// WoW64: (lparam, bytes) of the packed structure to copy back after the callback
    pack_back: Option<(u64, usize)>,
    pub procs_a: Vec<u64>,
    pub procs_w: Vec<u64>,
    pub builtins_done: bool,
    pub focus: u64,
    pub active: u64,
    pub capture: u64,
    pub menus: HashMap<u64, Menu>,
    next_menu: u64,
    pub cursors: HashMap<u64, u64>, // cursor/icon handle -> desc copy (guest)
    next_cursor: u64,
    pub mouse: (i32, i32),
    pub buttons: u32,
    pub keys: Keys,
    pub chars: HashMap<u32, u32>, // keydown time stamp -> character
    seq: u32,
    pub drag: Option<(u64, i32, i32)>, // window being moved, grab offset
    pub popup: Option<Popup>,
    pub last_click: (f64, i32, i32, u64),
    pub dirty: bool,
    pub props_atoms: HashMap<String, u16>,
    /// input contexts (HIMC): client pointer (imm32's data) and owner thread
    pub imcs: HashMap<u64, (u64, u32)>,
    next_imc: u64,
    /// windows given another input context than their thread's default (0: none)
    pub imc_of: HashMap<u64, u64>,
    /// each thread's default input context, for WoW64 threads (others keep it in the TEB)
    pub imc_default: HashMap<u32, u64>,
}

use crate::now_ms;

mod callback;
mod calls;
mod class;
mod input;
mod menu;
mod message;
mod params;
mod window;

pub use calls::call;

/// Window handles are used with and without the upper 16 bits.
pub fn full(h: u64) -> u64 {
    h & 0xFFFF_FFFF
}

impl crate::Engine {
    /// win32u's input contexts (imm32 keeps its data behind them; NtUserCreateInputContext
    /// returning 0 left it with a NULL context it then used, on the first window).
    pub(crate) fn imc_create(&mut self, client: u64) -> u64 {
        let tid = self.cur_tid();
        let w = &mut self.s.win;
        w.next_imc += 1;
        let h = 0x00E0_0000 + w.next_imc * 4;
        w.imcs.insert(h, (client, tid));
        h
    }
    /// The thread's default input context (ntuser_thread_info.default_imc, in the TEB at
    /// Win32ClientInfo, where imm32 also reads and sets it), made on first use.
    pub(crate) fn imc_default(&mut self) -> u64 {
        let tid = self.cur_tid();
        let at = self.cpu.gs_base + 0x800 + 52;
        let have = if self.s.wow64 { self.s.win.imc_default.get(&tid).copied().unwrap_or(0) } else { self.ur32(at) as u64 };
        if have != 0 && self.s.win.imcs.contains_key(&have) {
            return have;
        }
        let h = self.imc_create(0);
        if self.s.wow64 {
            self.s.win.imc_default.insert(tid, h);
        } else {
            self.w32(at, h as u32);
        }
        h
    }
    pub(crate) fn imc_call(&mut self, name: &str, ar: &[u64]) -> Option<u64> {
        const AICR_FAILED: u64 = 2;
        Some(match name {
            "NtUserCreateInputContext" => self.imc_create(ar[0]),
            "NtUserDestroyInputContext" => self.s.win.imcs.remove(&(ar[0] & 0xFFFF_FFFF)).is_some() as u64,
            "NtUserUpdateInputContext" => match (self.s.win.imcs.get_mut(&(ar[0] & 0xFFFF_FFFF)), ar[1] as u32) {
                (Some(c), 0) => {
                    c.0 = ar[2];
                    1
                }
                _ => 0,
            },
            "NtUserQueryInputContext" => match (self.s.win.imcs.get(&(ar[0] & 0xFFFF_FFFF)), ar[1] as u32) {
                (Some(c), 0) => c.0,
                (Some(c), 1) => c.1 as u64,
                _ => 0,
            },
            "NtUserAssociateInputContext" => {
                let (h, ctx, flags) = (ar[0] & 0xFFFF_FFFF, ar[1] & 0xFFFF_FFFF, ar[2] as u32);
                let ctx = match flags {
                    0x10 => self.imc_default(), // IACE_DEFAULT
                    0 | 0x20 => ctx,            // IACE_IGNORENOCONTEXT
                    _ => return Some(AICR_FAILED),
                };
                let tid = self.cur_tid();
                if ctx != 0 && self.s.win.imcs.get(&ctx).map_or(true, |c| c.1 != tid) {
                    return Some(AICR_FAILED);
                }
                let Some(owner) = self.s.win.wins.get(&h).map(|w| w.tid) else { return Some(AICR_FAILED) };
                if ctx != 0 && owner != tid {
                    return Some(AICR_FAILED);
                }
                let old = self.win_imc(h);
                if flags != 0x20 || old != 0 {
                    self.s.win.imc_of.insert(h, ctx);
                }
                (old != ctx && self.s.win.focus == h) as u64 // AICR_FOCUS_CHANGED
            }
            "NtUserBuildHimcList" => {
                let (tid, count, buf, size) = (ar[0] as u32, ar[1] as usize, ar[2], ar[3]);
                let list: Vec<u64> =
                    self.s.win.imcs.iter().filter(|(_, c)| tid == 0 || c.1 == tid).map(|(h, _)| *h).take(count).collect();
                let step = if self.s.wow64 { 4 } else { 8 };
                for (i, h) in list.iter().enumerate() {
                    if step == 4 {
                        self.w32(buf + 4 * i as u64, *h as u32);
                    } else {
                        self.w64(buf + 8 * i as u64, *h);
                    }
                }
                if size != 0 {
                    self.w32(size, list.len() as u32);
                }
                0
            }
            "NtUserDisableThreadIme" => 1,
            _ => return None,
        })
    }
    /// The input context of a window: its own, else its thread's default one.
    pub(crate) fn win_imc(&mut self, h: u64) -> u64 {
        match self.s.win.imc_of.get(&h) {
            Some(c) => *c,
            None if self.s.win.wins.contains_key(&h) => self.imc_default(),
            None => 0,
        }
    }
}
