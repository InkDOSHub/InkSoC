// InkSoC engine: PE32+ loader, x86-64 interpreter (integer + SSE/SSE2),
// 64-bit page table over a fixed pool of real RAM with swap to OPFS, our own
// kernel32, and a bridge to "Wine side" C code compiled to wasm.
// License: GPL-3.0-or-later.
#![cfg_attr(target_feature = "atomics", feature(thread_local, stdarch_wasm_atomic_wait))]

mod cpu;
mod eh;
mod files;
mod heap;
mod jit;
mod kernel32;
mod lock;
mod linux;
mod x87;
mod f80;
mod mmu;
mod msvcrt;
mod oleaut;
mod pe;
mod sched;
mod sse;
mod winapi;
mod winefb;
mod ntsys;
mod winsys;
mod wingdi;
mod proc;
mod registry;
mod npipe;
mod hash;
mod status;
mod store;
mod wow64;
mod audio;
mod opengl;
mod unixlib;
mod bcrypt;
mod net;

use cpu::Cpu;
use mmu::Mmu;
use std::collections::HashMap;
use std::rc::Rc;

#[link(wasm_import_module = "env")]
extern "C" {
    fn host_console(ptr: *const u8, len: u32);
    pub fn host_now_ms() -> f64;
    pub fn host_epoch_ms() -> f64;
    fn host_random(ptr: *mut u8, len: u32);
}

/// Random bytes from the browser's cryptographic generator (crypto.getRandomValues).
pub(crate) fn random_bytes(n: usize) -> Vec<u8> {
    let mut v = vec![0u8; n];
    unsafe { host_random(v.as_mut_ptr(), n as u32) };
    v
}

/// The host's clock: milliseconds since the page started (monotonic), and since 1970.
pub(crate) fn now_ms() -> f64 { unsafe { host_now_ms() } }
pub(crate) fn epoch_ms() -> f64 { unsafe { host_epoch_ms() } }

/// Seconds between 1601-01-01 (the FILETIME origin) and 1970-01-01.
pub(crate) const UNIX_TO_FILETIME_SECS: u64 = 11_644_473_600;

/// The current time as a Windows FILETIME (100 ns ticks since 1601).
pub(crate) fn filetime_now() -> u64 { (epoch_ms() as u64 + UNIX_TO_FILETIME_SECS * 1000) * 10_000 }

extern "C" {
    fn wine_RtlComputeCrc32(init: u32, data: u64, len: i32) -> u32;
    fn wine_lstrlenA(s: u64) -> i32;
}

pub const THUNK_BASE: u64 = 0x7FFE_0000_0000;
pub const RET_EXIT: u64 = THUNK_BASE + 0xFFF0;
pub const RET_INIT: u64 = THUNK_BASE + 0xFFE0;
pub const RET_LOADLIB: u64 = THUNK_BASE + 0xFFD0;
pub const RET_INITTERM: u64 = THUNK_BASE + 0xFFC0;
pub const RET_EH: u64 = THUNK_BASE + 0xFFB0;
pub const RET_THREAD: u64 = THUNK_BASE + 0xFFA0;
pub const RET_FBINIT: u64 = THUNK_BASE + 0xFF90;
// Below 4 GB (like Windows does for WoW64 processes): 32-bit code reaches its
// TEB32 (TEB + 0x2000) and PEB32 through 32-bit pointers.
pub const TEB: u64 = 0x7FF8_0000;
pub const PEB: u64 = 0x7FF9_0000;
pub const TEB32_OFF: u64 = 0x2000;
pub const PEB32: u64 = 0x7FF9_1000;
pub(crate) const STACK_SIZE: u64 = 1 << 20;

/// One core: the program thread it is running and its private caches.
pub struct Engine {
    pub mmu: Mmu,
    pub cpu: Cpu,
    pub eh: eh::Eh,
    pub jit: jit::Jit,
    pub core: u32,
    pub cur: usize,        // index of the program thread on this core
    pub want_switch: bool, // pick another thread at the next chance
    pub slice: u64,        // instructions left in this thread's turn
    pub idle_ms: f64,
    pub(crate) not_mine: bool,      // set by a native module that has no case for the called name
    /// Start of the instruction the interpreter is running (0 while native code runs): the
    /// address an access violation reports.
    pub ins_rip: u64,
    pub av: Av,
    pub s: &'static mut Shared,
}

/// Access violations that go to the program's exception handlers.
#[derive(Default)]
pub struct Av {
    pub addr: u64,      // the address a JIT block faulted on (cpu.jfault set)
    pub write: bool,
    last: (u64, u64),   // (rip, address) of the last one and how often it came in a row
    repeat: u32,
    /// The message for the last one, shown if nobody handles it.
    pub text: Option<String>,
}

/// Everything the cores share (used under the engine lock with several cores).
pub struct Shared {
    pub exited: bool,
    pub main_done: bool,   // the first process ended while child processes still run
    pub exit_code: u32,
    pub error: Option<String>,
    pub next_alloc: u64,
    pub next_dll: u64,
    pub bridge_calls: u64,
    pub files: store::Store,
    pub modules: Vec<pe::Module>,
    /// (dll, function) of each thunk, by index; shared strings so a call costs no copies.
    pub thunks: Vec<(Rc<str>, Rc<str>)>,
    thunk_ids: HashMap<String, u64>,
    pub heap: heap::Heap,
    pub tls_used: u64,
    pub no_init: bool,
    pub thunk_fb: HashMap<usize, u64>,
    /// For each thunk, which of the native modules know its name (see `native_chain`).
    routes: Vec<u8>,
    pub wine_fb: HashMap<String, u64>,
    pub env_block: u64,
    pub tls_tmpl: Vec<(u64, u64, u64)>, // static TLS of the images: (template, size, zero fill)
    pub init_queue: Vec<(u64, u64)>,
    pub resume: Vec<winefb::Resume>,
    pub user_shared: bool,
    pub win: winsys::Win,
    pub gdi: wingdi::Gdi,
    pub gl: opengl::Gl,
    pub audio: audio::Audio,
    pub net: net::Net,
    /// ntdll's __wine_unix_call_dispatcher variable (see unixlib.rs).
    pub unix_dispatcher: u64,
    pub next_luid: u64,
    pub win_ki: u64,
    pub recent: [(u32, u64); 16], // last native calls (thunk, return address), for error reports
    pub recent_n: usize,
    pub stubbed: std::collections::HashSet<String>, // system calls answered "not implemented" (logged once)
    pub wine_exports: HashMap<String, std::collections::HashSet<String>>,
    pub ldr_entries: HashMap<u64, u64>,
    pub loadlib: Vec<(u64, u64)>, // (module handle, caller rsp) while a DllMain runs
    pub stack_top: u64,
    pub exe_entry: u64,
    pub exe_base: u64,
    pub cmdline: (u64, u64),
    /// ANSI copies of startup strings, keyed by (process, address of the UTF-16 buffer)
    pub startup_ansi: HashMap<(u32, u64), u64>,
    pub unknown_calls: u64,
    pub crt: msvcrt::Crt,
    pub fs: files::Files,
    pub sched: sched::Sched,
    pub cores: u32,
    pub insns: u64, // instructions of every core, added after each run
    pub jit_on: bool,
    pub lx: Option<Box<linux::Lx>>,
    pub names: HashMap<String, String>, // file key -> name as given (Linux keeps case, no .dll)
    /// Windows processes: the running one, the parked ones, and what is known of each child
    pub wpid: u32,
    pub procs: HashMap<u32, proc::ProcSave>,
    pub pinfo: HashMap<u32, proc::ProcInfo>,
    /// GetStdHandle: input, output, error
    pub std: [u64; 3],
    /// console input typed in the page (winapi/console.rs); live = the page sends lines
    pub con_in: Vec<u8>,
    pub con_live: bool,
    /// the page's game controller (winapi/xinput.rs): connected, packet number, XINPUT_GAMEPAD bytes
    pub pad_on: bool,
    pub pad_packet: u32,
    pub pad: [u8; 12],
    /// folders searched for DLLs after C:\InkSoC (SetDllDirectory, AddDllDirectory, folders DLLs were loaded from)
    pub dll_dirs: Vec<String>,
    /// folder of the running process's program (empty: the home folder)
    pub image_dir: String,
    /// 32-bit program (WoW64): its ntdll, and the CONTEXT passed to Wow64LdrpInitialize
    pub wow64: bool,
    pub wow64_ntdll: u64,
    pub wow64_ctx: u64,
    /// images mapped by map_image (base -> size)
    pub images: HashMap<u64, u64>,
    /// file names of the images mapped as sections (NtMapViewOfSection): base -> name
    pub image_files: HashMap<u64, String>,
    /// where each moved image file was last mapped (a reload goes back there)
    pub image_slots: HashMap<String, u64>,
    /// crypt32's enum_root_certs: where the next certificate starts in roots.der
    pub root_off: usize,
    /// global atom table (NtAddAtom): atom 0xC000 + index
    pub atoms: Vec<String>,
}

#[cfg_attr(target_feature = "atomics", thread_local)]
static mut ENGINE: Option<Engine> = None;
static mut SHARED: Option<Shared> = None;
static mut BUF: Vec<u8> = Vec::new();
static mut MSG: Vec<u8> = Vec::new();

#[allow(static_mut_refs)]
#[allow(static_mut_refs)]
pub(crate) fn set_out(v: Vec<u8>) -> u32 { unsafe { OUT = v; OUT.len() as u32 } }

#[allow(static_mut_refs)]
pub(crate) unsafe fn take_buf() -> Vec<u8> { std::mem::take(&mut BUF) }

#[allow(static_mut_refs)]
pub(crate) fn eng() -> &'static mut Engine { unsafe { ENGINE.as_mut().unwrap() } }

pub fn console(s: &str) { unsafe { host_console(s.as_ptr(), s.len() as u32) } }
/// The engine's own heap (files, windows, tables; not the program's memory): counts the
/// bytes in use, and says what was asked when the browser gives no more memory.
struct CountingAlloc;
static HEAP_USED: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
unsafe impl std::alloc::GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, l: std::alloc::Layout) -> *mut u8 {
        let p = std::alloc::System.alloc(l);
        note_alloc(p, l.size());
        p
    }
    unsafe fn alloc_zeroed(&self, l: std::alloc::Layout) -> *mut u8 {
        let p = std::alloc::System.alloc_zeroed(l);
        note_alloc(p, l.size());
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: std::alloc::Layout) {
        std::alloc::System.dealloc(p, l);
        HEAP_USED.fetch_sub(l.size(), core::sync::atomic::Ordering::Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, l: std::alloc::Layout, n: usize) -> *mut u8 {
        let q = std::alloc::System.realloc(p, l, n);
        if !q.is_null() {
            HEAP_USED.fetch_sub(l.size(), core::sync::atomic::Ordering::Relaxed);
        }
        note_alloc(q, n);
        q
    }
}
fn note_alloc(p: *mut u8, n: usize) {
    if p.is_null() {
        let used = HEAP_USED.load(core::sync::atomic::Ordering::Relaxed);
        let mut b = [0u8; 120];
        let s = fmt_oom(&mut b, n, used);
        unsafe { host_console(s.as_ptr(), s.len() as u32) }
    } else {
        HEAP_USED.fetch_add(n, core::sync::atomic::Ordering::Relaxed);
    }
}
/// The out-of-memory line, written without allocating.
fn fmt_oom(b: &mut [u8; 120], asked: usize, used: usize) -> &[u8] {
    use std::io::Write;
    let mut c = std::io::Cursor::new(&mut b[..]);
    let _ = write!(c, "[engine] out of memory: asked {} KB, engine heap {} MB in use\n", asked >> 10, used >> 20);
    let n = c.position() as usize;
    &b[..n]
}
#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;
/// Bytes of the engine's own heap in use.
#[no_mangle]
pub extern "C" fn heap_used() -> f64 {
    HEAP_USED.load(core::sync::atomic::Ordering::Relaxed) as f64
}

/// An engine bug stops the page with only "unreachable": print where it happened first.
fn panic_note() {
    std::panic::set_hook(Box::new(|i| console(&format!("[engine] internal error: {}\n", i))));
}

// ---- exports to JS ----

/// Scratch buffer the page fills before `add_file` / `start`.
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn buffer(len: u32) -> *mut u8 {
    unsafe { BUF = vec![0u8; len as usize]; BUF.as_mut_ptr() }
}

/// Creates the machine: `pool_mb` of real RAM (fixed, never grows) and a
/// memory target of `target_mb` that the program sees (the rest is swap).
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn init(pool_mb: u32, target_mb: u32) {
    panic_note();
    unsafe {
        SHARED = Some(Shared {
            exited: false, main_done: false, exit_code: 0, error: None,
            next_alloc: 0x0000_0200_0000_0000, // 2 TB: far above 4 GB on purpose
            next_dll: 0x0000_7FF0_0000_0000,
            bridge_calls: 0,
            files: Default::default(), modules: Vec::new(),
            thunks: Vec::new(), thunk_ids: HashMap::new(),
            heap: heap::Heap::default(), tls_used: 0, no_init: false, thunk_fb: HashMap::new(), routes: Vec::new(), wine_fb: HashMap::new(), env_block: 0, tls_tmpl: Vec::new(),
            init_queue: Vec::new(), resume: Vec::new(), user_shared: false, win: Default::default(), gdi: Default::default(), gl: Default::default(), audio: Default::default(), net: Default::default(), unix_dispatcher: 0, next_luid: 0, win_ki: 0, recent: [(0, 0); 16], recent_n: 0, stubbed: Default::default(), wine_exports: HashMap::new(), ldr_entries: HashMap::new(), loadlib: Vec::new(),
            stack_top: 0, exe_entry: 0, exe_base: 0, cmdline: (0, 0), startup_ansi: HashMap::new(), unknown_calls: 0, crt: Default::default(), fs: Default::default(),
            sched: Default::default(), cores: 1, insns: 0, jit_on: true, lx: None, names: HashMap::new(),
            wpid: 0, procs: HashMap::new(), pinfo: HashMap::new(), std: [3, 1, 2], con_in: Vec::new(), con_live: false, pad_on: false, pad_packet: 0, pad: [0; 12], dll_dirs: Vec::new(), image_dir: String::new(), wow64: false, wow64_ntdll: 0, wow64_ctx: 0, images: HashMap::new(), image_files: HashMap::new(), root_off: 0, image_slots: HashMap::new(), atoms: Vec::new(),
        });
        ENGINE = Some(Engine {
            mmu: Mmu::new(pool_mb as usize, target_mb as u64),
            cpu: Cpu::new(), eh: Default::default(), jit: Default::default(),
            core: 0, cur: 0, want_switch: false, slice: 0, idle_ms: 0.0, not_mine: false, ins_rip: 0, av: Default::default(),
            s: SHARED.as_mut().unwrap(),
        });
    }
}

// ---- several cores (multi-core build only) ----

/// Memory for another core's stack and thread-local area (called on core 0).
#[no_mangle]
pub extern "C" fn core_alloc(size: u32) -> u32 {
    let v: Vec<u8> = vec![0u8; size as usize + 64];
    let p = Box::leak(v.into_boxed_slice()).as_mut_ptr() as u32;
    (p + 63) & !63
}

static mut CORE0_MMU: usize = 0;

/// How many cores will run (set on core 0 before start: the program sees this many processors).
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn set_cores(n: u32) {
    let e = eng();
    e.s.cores = n.max(1);
    mmu::set_strong(lock::MT && n > 1 && e.mmu.pool_addr() % 8 == 0);
    unsafe { CORE0_MMU = &e.mmu as *const Mmu as usize; }
}

/// Turns the strict x86 memory order between cores on or off (measuring its cost).
#[no_mangle]
pub extern "C" fn set_strong_order(on: u32) { mmu::set_strong(on != 0 && lock::MT && eng().s.cores > 1); }

/// Turns the newer JIT code improvements on or off (?opt=0 on the page: A/B on a device).
#[no_mangle]
pub extern "C" fn set_jit_opt(on: u32) { jit::set_opt(on != 0); }

/// Sets up this worker as core `id` (after its stack and thread-local area are set).
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn core_init(id: u32) {
    unsafe {
        let mmu0 = &*(CORE0_MMU as *const Mmu);
        let mut jit: jit::Jit = Default::default();
        jit.on = SHARED.as_ref().unwrap().jit_on;
        ENGINE = Some(Engine {
            mmu: mmu0.new_core(id),
            cpu: Cpu::new(), eh: Default::default(), jit,
            core: id, cur: usize::MAX, want_switch: true, slice: 0, idle_ms: 0.0, not_mine: false, ins_rip: 0, av: Default::default(),
            s: SHARED.as_mut().unwrap(),
        });
    }
}

/// The bytes after a file's name (and the NUL that ends it), moved to the front of `b`.
/// Nothing is allocated: the file is not copied a second time while it is added.
fn body_after_name(mut b: Vec<u8>, nl: usize) -> Vec<u8> {
    let cut = (nl + 1).min(b.len());
    b.drain(..cut);
    b
}

/// Adds a file (from BUF) that the program can load, e.g. a DLL.
/// Returns 0 when it is added, -1 when the engine's heap has no room for it: the page stops
/// the run with that error; the engine does not trap.
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn add_file() -> i32 {
    unsafe {
        let b = std::mem::take(&mut BUF);
        let nl = b.iter().position(|&c| c == 0).unwrap_or(0);
        let raw = String::from_utf8_lossy(&b[..nl]).into_owned();
        // a file of the saved drive C: comes with its Windows path; the others are plain names
        if raw.contains('\\') || raw.contains(':') {
            let e = eng();
            let Some(k) = files::key_of(&raw) else { return 0 };
            if raw.ends_with('\\') {
                e.s.fs.dirs.insert(k.clone());
            } else {
                if store_file_with(&mut e.s.files, k.clone(), body_after_name(b, nl), 1) < 0 { return -1; }
                e.s.fs.drive.insert(k.clone());
            }
            e.note_case(&k, &raw);
            return 0;
        }
        let name = pe::norm(&raw);
        if eng().s.names.try_reserve(1).is_err() { return -1; }
        if store_file_with(&mut eng().s.files, name.clone(), body_after_name(b, nl), 1) < 0 { return -1; }
        eng().s.names.insert(name.clone(), raw.rsplit(['\\', '/']).next().unwrap_or(&raw).to_string());
        0
    }
}

/// Stores a file's bytes under `k` if the map can take one more entry: 0, or -1 with the map
/// unchanged. `need` is the room asked for (1 in the engine); a test asks for more than the
/// heap can give, to see the refusal without running out of memory.
fn store_file_with(s: &mut store::Store, k: String, body: Vec<u8>, need: usize) -> i32 {
    if !s.reserve_count(need) { return -1; }
    s.insert(k, body);
    0
}

#[cfg(test)]
mod add_file_tests {
    use super::body_after_name;

    #[test]
    fn body_is_moved_not_copied() {
        let b = b"name.dll\0BODY".to_vec();
        let p = b.as_ptr();
        let out = body_after_name(b, 8);
        assert_eq!(out, b"BODY");
        assert_eq!(out.as_ptr(), p, "the body must stay in the buffer it arrived in");
    }

    #[test]
    fn name_without_bytes_gives_an_empty_body() {
        assert!(body_after_name(b"name\0".to_vec(), 4).is_empty());
        assert!(body_after_name(b"name".to_vec(), 4).is_empty());
    }

    #[test]
    fn no_nul_uses_position_zero_as_before() {
        // add_file takes a missing NUL as position 0 (unwrap_or(0)). The old code cut b[nl + 1..],
        // so the first byte goes and the rest stays; the move must keep exactly that.
        assert_eq!(body_after_name(b"xyz".to_vec(), 0), b"yz");
        assert_eq!(body_after_name(b"x".to_vec(), 0), b"");
    }

    fn keys_of(s: &crate::store::Store) -> Vec<String> {
        let mut k: Vec<String> = s.keys().cloned().collect();
        k.sort();
        k
    }

    #[test]
    fn a_file_is_stored_when_the_map_has_room() {
        let mut s = crate::store::Store::default();
        assert_eq!(super::store_file_with(&mut s, "a.dll".into(), b"AAAA".to_vec(), 1), 0);
        assert_eq!(s.get("a.dll").map(|v| v.as_slice()), Some(&b"AAAA"[..]));
    }

    #[test]
    fn a_refused_reserve_gives_minus_one_and_stores_nothing() {
        let mut s = crate::store::Store::default();
        assert_eq!(super::store_file_with(&mut s, "a.dll".into(), b"AAAA".to_vec(), 1), 0);
        let before = keys_of(&s);
        // usize::MAX entries cannot be reserved: the map refuses without allocating
        assert_eq!(super::store_file_with(&mut s, "b.dll".into(), b"BBBB".to_vec(), usize::MAX), -1);
        assert_eq!(keys_of(&s), before, "a refused file must leave the map as it was");
        assert!(!s.contains_key("b.dll"));
    }

    #[test]
    fn files_stored_before_a_refusal_keep_their_bytes() {
        let mut s = crate::store::Store::default();
        assert_eq!(super::store_file_with(&mut s, "a.dll".into(), b"AAAA".to_vec(), 1), 0);
        assert_eq!(super::store_file_with(&mut s, "c.dll".into(), b"CCCCCC".to_vec(), 1), 0);
        assert_eq!(super::store_file_with(&mut s, "b.dll".into(), b"BBBB".to_vec(), usize::MAX), -1);
        assert_eq!(s.get("a.dll").map(|v| v.as_slice()), Some(&b"AAAA"[..]));
        assert_eq!(s.get("c.dll").map(|v| v.as_slice()), Some(&b"CCCCCC"[..]));
    }

    #[test]
    fn reserve_count_refuses_an_impossible_count() {
        let mut s = crate::store::Store::default();
        assert!(s.reserve_count(1));
        assert!(!s.reserve_count(usize::MAX));
    }
}

/// A file of the saved drive C: known only by its Windows path (in BUF), its id on
/// the browser's storage and its size: read when the program first uses it.
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn add_drive_file(id: u32, size: f64) {
    let b = unsafe { std::mem::take(&mut BUF) };
    let raw = String::from_utf8_lossy(&b).into_owned();
    let e = eng();
    let Some(k) = files::key_of(&raw) else { return };
    e.s.files.add_lazy(k.clone(), id, size as u64);
    e.s.fs.drive.insert(k.clone());
    e.note_case(&k, &raw);
}

/// Starts the program named in BUF ("name\0command line"). 0 = ok.
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn start() -> i32 {
    let b = unsafe { std::mem::take(&mut BUF) };
    let nl = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    let name = pe::norm(&String::from_utf8_lossy(&b[..nl]));
    let mut cmd = String::from_utf8_lossy(b.get(nl + 1..).unwrap_or(&[])).into_owned();
    // a program path with spaces starts the command line in quotes, as Windows writes it
    // (else the program reads "C:\Program" and "Files\..." as two arguments)
    let raw = String::from_utf8_lossy(&b[..nl]).into_owned();
    if raw.contains(' ') && cmd.to_ascii_lowercase().starts_with(&raw.to_ascii_lowercase()) {
        cmd = format!("\"{}\"{}", &cmd[..raw.len()], &cmd[raw.len()..]);
    }
    let e = eng();
    // a program outside its home folder (at the root of C:): found by its path, run by its name
    if !e.s.files.contains_key(&name) {
        if let Some(k) = files::key_of(&raw).filter(|k| e.s.files.contains_key(k)) { if !e.s.files.alias(name.clone(), &k) { if let Some(b) = e.s.files.get(&k).cloned() { e.s.files.insert(name.clone(), b); } } }
    }
    // the program's own folder is searched for its DLLs, as on Windows (programs that ship
    // their DLLs next to the .exe in a subfolder, like Luanti's bin\)
    if let Some(k) = files::key_of(&raw) {
        if let Some(i) = k.rfind('\\') { let d = k[..i].to_string(); if !e.s.dll_dirs.contains(&d) { e.s.dll_dirs.push(d); } }
    }
    if let Some(d) = e.s.files.get("roots.der").cloned() {
        e.s.fs.reg.seed_roots(&d);
    }
    match e.boot(&name, &cmd) { Ok(()) => 0, Err(m) => { e.set_error(m); -1 } }
}

#[no_mangle]
pub extern "C" fn poke32(addr: f64, v: u32) -> i32 {
    match eng().mmu.write32(addr as u64, v) { Ok(_) => 0, Err(_) => -1 }
}

/// Runs up to `budget` instructions. 0 = still running, 1 = exited, 2 = error,
/// 3 = every thread is waiting (call again after stat(17) ms).
#[no_mangle]
pub extern "C" fn run(budget: u32) -> i32 {
    let e = eng();
    let mut left = budget as u64;
    let mut idle = false;
    let start = e.cpu.icount;
    let mut done_other = 0u64; // instructions of threads this core switched away from
    mmu::safe_point(e.core);
    if e.s.lx.is_none() && e.s.user_shared { e.shared_time(); }
    let mut tick = 0u32;
    while left > 0 && !e.s.exited && e.s.error.is_none() {
        // a safe point every 64 blocks: frees and evictions wait a little longer, the loop pays less
        tick = tick.wrapping_add(1);
        if tick & 63 == 0 { mmu::safe_point(e.core); }
        if e.want_switch {
            let before = e.cpu.icount;
            let ok = { let _g = lock::lock(); e.sched_switch() };
            done_other += before.wrapping_sub(start).min(u64::MAX / 2);
            let _ = done_other;
            if !ok {
                idle = e.s.error.is_none();
                // the page's core goes back to JavaScript when there are windows (screen updates, input) or sockets
                if lock::MT && idle && !(e.core == 0 && { let _g = lock::lock(); !e.s.win.tops.is_empty() || e.s.net.busy() }) { let g = lock::gen(); lock::idle(g, e.idle_ms); idle = false; left = left.saturating_sub(1000); continue; }
                break;
            }
        }
        if e.s.exited || e.s.error.is_some() { break; }
        let rip = e.cpu.rip;
        if rip >= THUNK_BASE && rip < THUNK_BASE + 0x10000 {
            e.materialize();
            { let _g = lock::lock(); e.call_native(); }
            if lock::MT { lock::kick(); }
            continue;
        }
        if e.jit.on {
            let lim = left.min(e.slice_left());
            if let Some(f) = jit::Jit::lookup(e, rip) {
                let before = e.cpu.icount;
                e.cpu.jit_limit = before + lim;
                jit::call(f);
                if e.cpu.jfault != 0 {
                    // a memory access of the block faulted: the block left at that instruction
                    e.cpu.jfault = 0;
                    let (a, w, rip) = (e.av.addr, e.av.write, e.cpu.rip);
                    if !e.raise_av(rip, a, w as u64) { e.fault_stop(a); }
                }
                let n = e.cpu.icount - before;
                #[cfg(feature = "prof")]
                { jit::prof::add(0xB0000000, 1); jit::prof::add(0xB0000001, n); jit::prof::add(f, n); }
                left = left.saturating_sub(n);
                e.sched_tick(n);
                continue;
            }
            let n = cpu::step_trace(e, lim.min(4096));
            #[cfg(feature = "prof")]
            { jit::prof::add(0xC0000000, 1); jit::prof::add(0xC0000001, n); }
            left = left.saturating_sub(n);
            e.sched_tick(n);
            continue;
        }
        let n = cpu::step_block(e, left.min(e.slice_left()).min(4096));
        left = left.saturating_sub(n);
        e.sched_tick(n);
    }
    { let _g = lock::lock(); e.s.insns += budget as u64 - left; }
    mmu::away(e.core);
    if e.s.error.is_some() { 2 } else if e.s.exited { 1 } else if idle { 3 } else { 0 }
}

/// Debug: stop-free watch point (NOJIT); up to 8.
#[no_mangle]
pub extern "C" fn add_watch(a: u32) { unsafe { if let Some(w) = cpu::WATCH.iter_mut().find(|w| **w == 0) { *w = a as u64; } } }

static mut TRACE: bool = false;
/// Prints every native call (debugging).
#[no_mangle]
pub extern "C" fn set_trace(on: u32) { unsafe { TRACE = on != 0; } }

/// Wine's own messages (FIXME and ERR lines of its DLLs) on the console: on or off.
pub(crate) static mut WINE_DEBUG: u32 = 0;
#[no_mangle]
pub extern "C" fn set_wine_debug(level: u32) { unsafe { WINE_DEBUG = level; } }
/// The channels whose TRACE lines are printed (comma-separated, from the buffer; empty = all).
pub(crate) static mut WINE_TRACE: Vec<String> = Vec::new();
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn set_wine_trace() {
    unsafe { WINE_TRACE = String::from_utf8_lossy(&BUF).split(',').filter(|c| !c.is_empty()).map(String::from).collect(); }
}

/// Turns the JIT on or off (call before start).
#[no_mangle]
pub extern "C" fn set_jit(on: u32) { let e = eng(); e.jit.on = on != 0; e.s.jit_on = on != 0; }

static mut OUT: Vec<u8> = Vec::new();

/// The i-th file the program wrote: fills out_ptr() with "name\0data" and
/// returns its length, or 0 when there are no more.
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn out_file(i: u32) -> u32 {
    let e = eng();
    let (name, data) = if e.s.lx.is_some() {
        let Some(x) = linux::written(e).into_iter().nth(i as usize) else { return 0 };
        x
    } else {
        let Some(name) = e.s.fs.written.iter().nth(i as usize).cloned() else { return 0 };
        // a copy of a file still on disk is copied there (out_disk), not read in
        let data = if e.s.files.lazy_id(&name).is_some() { Vec::new() } else { e.s.files.get(&name).cloned().unwrap_or_default() };
        (name, data)
    };
    unsafe { OUT = name.into_bytes(); OUT.push(0); OUT.extend_from_slice(&data); OUT.len() as u32 }
}
/// Offset and size of the i-th written file in the file out_disk names.
#[no_mangle]
pub extern "C" fn out_disk_at(i: u32) -> f64 { let e = eng(); e.s.fs.written.iter().nth(i as usize).and_then(|k| e.s.files.disk_of(k)).map_or(0.0, |v| v.1 as f64) }
#[no_mangle]
pub extern "C" fn out_disk_size(i: u32) -> f64 { let e = eng(); e.s.fs.written.iter().nth(i as usize).and_then(|k| e.s.files.disk_of(k)).map_or(0.0, |v| v.2 as f64) }
/// The disk id the i-th written file is a copy of (a renamed or copied drive file), or -1.
#[no_mangle]
pub extern "C" fn out_disk(i: u32) -> i32 {
    let e = eng();
    e.s.fs.written.iter().nth(i as usize).and_then(|k| e.s.files.lazy_id(k)).map_or(-1, |v| v as i32)
}
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn out_ptr() -> *const u8 { unsafe { OUT.as_ptr() } }

/// Sets the program's home folder from BUF (a Windows path, before any add_file): an
/// installed program runs from its own folder, picked files from C:\InkSoC.
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn set_home() {
    let b = unsafe { std::mem::take(&mut BUF) };
    files::set_home(&String::from_utf8_lossy(&b));
}

/// The saved drive C:, step by step: OUT = the Windows path of the i-th file the program
/// wrote (data: out_file(i)), or of a drive file it deleted (i past the written ones, "-"
/// prefix), or of a folder it made ("\\" suffix). 0 = no more.
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn drive_entry(i: u32) -> u32 {
    let e = eng();
    let shown = |e: &Engine, k: &str| files::win_path(&e.s.names.get(k).cloned().unwrap_or_else(|| k.to_string()));
    let mut i = i as usize;
    let s = if let Some(k) = e.s.fs.written.iter().nth(i) { shown(e, k) } else {
        i -= e.s.fs.written.len();
        let gone: Vec<&String> = e.s.fs.drive.iter().filter(|k| !e.s.files.contains_key(*k)).collect();
        if let Some(k) = gone.get(i) { format!("-{}", shown(e, k)) } else {
            i -= gone.len();
            match e.s.fs.dirs.iter().nth(i) { Some(k) => format!("{}\\", shown(e, k)), None => return 0 }
        }
    };
    unsafe { OUT = s.into_bytes(); OUT.len() as u32 }
}

/// The registry as bytes (OUT), to keep with the drive; reg_load reads them back from BUF.
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn reg_save() -> u32 { let v = eng().s.fs.reg.save(); unsafe { OUT = v; OUT.len() as u32 } }
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn reg_load() { let b = unsafe { std::mem::take(&mut BUF) }; eng().s.fs.reg.load(&b); }

#[no_mangle]
pub extern "C" fn stat(i: u32) -> f64 {
    let e = eng();
    match i {
        0 => e.s.insns as f64,
        1 => e.mmu.sh.faults as f64,
        2 => e.mmu.sh.swap_ins as f64,
        3 => e.mmu.sh.swap_outs as f64,
        4 => e.mmu.resident() as f64,
        5 => e.mmu.frames_total() as f64,
        6 => e.s.exit_code as f64,
        7 => e.s.bridge_calls as f64,
        8 => e.mmu.sh.swap_slots as f64,
        9 => e.s.modules.len() as f64,
        10 => e.s.thunks.len() as f64,
        11 => e.cpu.rip as f64,
        12 => e.cpu.regs[4] as f64,
        13 => e.jit.blocks as f64,
        14 => e.jit.ms,
        15 => e.jit.bytes as f64,
        16 => e.jit.failed as f64,
        17 => e.idle_ms,
        18 => e.s.sched.th.len() as f64,
        19 => e.s.sched.switches as f64,
        20 => lock::counter(0),
        21 => lock::counter(1),
        22 => lock::counter(2),
        23 => lock::counter(3),
        24 => e.s.files.loaded_bytes as f64,
        _ => 0.0,
    }
}

#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn msg_ptr() -> *const u8 {
    unsafe { MSG = eng().s.error.clone().unwrap_or_default().into_bytes(); MSG.as_ptr() }
}
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn msg_len() -> u32 { unsafe { MSG.len() as u32 } }

// ---- bridge: Wine-side wasm code reads program memory through the MMU ----

#[no_mangle]
pub extern "C" fn guest_read8(addr: u64) -> u8 {
    let e = eng();
    match e.mmu.read8(addr) { Ok(v) => v, Err(_) => { e.fault(addr); 0 } }
}
#[no_mangle]
pub extern "C" fn guest_read32(addr: u64) -> u32 {
    let e = eng();
    match e.mmu.read32(addr) { Ok(v) => v, Err(_) => { e.fault(addr); 0 } }
}

/// A free range found by find_free: its address, end, the bump pointer after it, and
/// whether it came from a gap below the bump pointer.
#[derive(Clone, Copy)]
struct Found { a: u64, end: u64, next: u64, gap: bool }

impl Engine {
    /// An access violation of the running program. In 64-bit Windows code it goes to the
    /// program's exception handlers (as on Windows); elsewhere, or when that fails, it stops
    /// the program.
    pub fn fault(&mut self, addr: u64) {
        if self.seh_ok() && self.ins_rip != 0 {
            let rip = self.ins_rip;
            if self.raise_av(rip, addr, if addr == rip { 8 } else { 0 }) {
                return;
            }
        }
        self.fault_stop(addr);
    }
    /// A JIT memory access faulted: the block leaves at the faulting instruction (it checks
    /// cpu.jfault) and `run` raises the exception there.
    pub fn fault_jit(&mut self, addr: u64, write: bool) {
        if self.seh_ok() {
            self.av.addr = addr;
            self.av.write = write;
            self.cpu.jfault = 1;
            return;
        }
        self.fault_stop(addr);
        self.cpu.jit_limit = 0;
    }
    /// Guest code of a 64-bit Windows program is running (not native code, Linux or WoW64).
    pub(crate) fn seh_ok(&self) -> bool {
        self.s.lx.is_none() && (!self.cpu.mode32 || self.s.wow64) && !(THUNK_BASE..THUNK_BASE + 0x10000).contains(&self.cpu.rip)
    }
    /// Starts ntdll's exception dispatcher for STATUS_ACCESS_VIOLATION at instruction `rip`
    /// (kind: 0 read, 1 write, 8 execute). False when it cannot (no ntdll, no stack room,
    /// or the same fault keeps coming back).
    pub(crate) fn raise_av(&mut self, rip: u64, addr: u64, kind: u64) -> bool {
        if self.av.last == (rip, addr) {
            self.av.repeat += 1;
            if self.av.repeat > 1000 {
                return false;
            }
        } else {
            self.av.last = (rip, addr);
            self.av.repeat = 0;
        }
        self.materialize();
        self.cpu.rip = rip;
        // the message shown when nobody handles it
        let saved = self.s.error.take();
        self.fault_stop(addr);
        let text = core::mem::replace(&mut self.s.error, saved);
        self.raise_exc(rip, 0xC000_0005, &[kind, addr], text)
    }
    /// Raises an instruction exception (breakpoint, illegal or privileged instruction) at
    /// `rip` when the program can take it; otherwise stops with `m`.
    pub(crate) fn raise_or_stop(&mut self, rip: u64, code: u32, params: &[u64], m: String) {
        let key = (rip, u64::MAX - code as u64);
        if self.av.last == key {
            self.av.repeat += 1;
        } else {
            self.av.last = key;
            self.av.repeat = 0;
        }
        if !(self.seh_ok() && self.av.repeat <= 1000 && self.raise_exc(rip, code, params, Some(m.clone()))) {
            self.set_error(m);
        }
    }
    /// Raises a hardware exception (`code`, `params`) at `rip` through the program's SEH
    /// handlers; `text` is shown if nobody handles it. False when it can't be raised.
    pub(crate) fn raise_exc(&mut self, rip: u64, code: u32, params: &[u64], text: Option<String>) -> bool {
        self.materialize();
        self.cpu.rip = rip;
        self.av.text = text;
        if unsafe { WINE_DEBUG } > 0 {
            console(&format!("[engine] first chance: {}\n", self.av.text.as_deref().unwrap_or("").lines().next().unwrap_or("")));
        }
        if self.cpu.mode32 {
            return self.raise_exc32(rip, code, params);
        }
        let c = self.context_bytes(rip);
        let mut r = vec![0u8; 0x98];
        r[0..4].copy_from_slice(&code.to_le_bytes());
        r[16..24].copy_from_slice(&rip.to_le_bytes());
        r[24..28].copy_from_slice(&(params.len() as u32).to_le_bytes());
        for (k, p) in params.iter().enumerate() {
            r[32 + 8 * k..40 + 8 * k].copy_from_slice(&p.to_le_bytes());
        }
        let rsp = self.cpu.regs[4];
        self.exception_frame(&c, &r, rsp).is_some()
    }
    /// A 32-bit program's exception (WoW64): what Wine's wow64 call_user_exception_dispatcher
    /// does. Below the 32-bit stack go the record and context pointers, an EXCEPTION_RECORD32
    /// and an I386_CONTEXT; the 32-bit ntdll's KiUserExceptionDispatcher then runs its
    /// handlers and goes back through NtContinue.
    fn raise_exc32(&mut self, eip: u64, code: u32, params: &[u64]) -> bool {
        let nt = self.s.wow64_ntdll;
        let Some(disp) = (if nt != 0 { pe::export_addr(self, nt, "KiUserExceptionDispatcher") } else { None }) else { return false };
        const CTX: u64 = 0x2CC;
        const REC: u64 = 0x50;
        let esp = self.cpu.regs[4] & 0xFFFF_FFFC;
        let st = (esp - (8 + REC + CTX)) & !15;
        if self.mmu.write8(st, 0).is_err() || self.mmu.write8(esp - 1, 0).is_err() {
            return false;
        }
        let (rec, ctx) = (st + 8, st + 8 + REC);
        self.zero_n(st, 8 + REC + CTX);
        self.w32(st, rec as u32);
        self.w32(st + 4, ctx as u32);
        // EXCEPTION_RECORD32
        self.w32(rec, code);
        self.w32(rec + 12, eip as u32);
        self.w32(rec + 16, params.len() as u32);
        for (k, p) in params.iter().enumerate() {
            self.w32(rec + 20 + 4 * k as u64, *p as u32);
        }
        // I386_CONTEXT (CONTEXT_I386_FULL | FLOATING_POINT | EXTENDED_REGISTERS)
        self.w32(ctx, 0x1002F);
        self.w32(ctx + 0x1C, 0x27F); // FloatSave.ControlWord
        self.w32(ctx + 0x24, 0xFFFF); // TagWord: empty
        for (o, v) in [(0x8Cu64, 0x2Bu32), (0x90, 0x53), (0x94, 0x2B), (0x98, 0x2B), (0xBC, 0x23), (0xC8, 0x2B)] {
            self.w32(ctx + o, v);
        }
        let r = self.cpu.regs;
        for (o, k) in [(0x9Cu64, 7usize), (0xA0, 6), (0xA4, 3), (0xA8, 2), (0xAC, 1), (0xB0, 0), (0xB4, 5), (0xC4, 4)] {
            self.w32(ctx + o, r[k] as u32);
        }
        self.w32(ctx + 0xB8, eip as u32);
        let fl = self.flags() as u32 | 0x202;
        self.w32(ctx + 0xC0, fl);
        let x = ctx + 0xCC; // ExtendedRegisters: FXSAVE layout
        let fcw = self.cpu.fcw;
        self.w16(x, fcw);
        self.w32(x + 24, 0x1F80);
        for k in 0..8 {
            let v = self.cpu.xmm[k].to_le_bytes();
            let _ = self.mmu.write_bytes(x + 160 + 16 * k as u64, &v);
        }
        self.cpu.regs[4] = st;
        self.cpu.rip = disp;
        true
    }
    /// A CONTEXT (0x4d0 bytes) of the current registers, at `rip`.
    pub(crate) fn context_bytes(&mut self, rip: u64) -> Vec<u8> {
        let mut c = vec![0u8; 0x4d0];
        c[0x30..0x34].copy_from_slice(&0x0010_000Fu32.to_le_bytes()); // CONTEXT_FULL | SEGMENTS
        c[0x34..0x38].copy_from_slice(&0x1F80u32.to_le_bytes());
        for (o, v) in [(0x38usize, 0x33u16), (0x3A, 0x2B), (0x3C, 0x2B), (0x3E, 0x53), (0x40, 0x2B), (0x42, 0x2B)] {
            c[o..o + 2].copy_from_slice(&v.to_le_bytes());
        }
        c[0x44..0x48].copy_from_slice(&(self.flags() as u32 | 0x202).to_le_bytes());
        for k in 0..16 {
            c[0x78 + 8 * k..0x80 + 8 * k].copy_from_slice(&self.cpu.regs[k].to_le_bytes());
        }
        c[0xF8..0x100].copy_from_slice(&rip.to_le_bytes());
        c[0x100..0x102].copy_from_slice(&self.cpu.fcw.to_le_bytes());
        c[0x118..0x11C].copy_from_slice(&0x1F80u32.to_le_bytes());
        for k in 0..16 {
            c[0x1A0 + 16 * k..0x1B0 + 16 * k].copy_from_slice(&self.cpu.xmm[k].to_le_bytes());
        }
        c
    }
    /// Stops the program for an access violation, with what is known about where.
    pub fn fault_stop(&mut self, addr: u64) {
        let mut m = format!("access violation at 0x{:x} (rip 0x{:x})", addr, self.cpu.rip);
        // inside a DLL that a loader mapped as a section (WoW64's 32-bit ones): name it
        let rip = self.cpu.rip;
        if let Some((&b, _)) = self.s.images.iter().find(|(b, n)| **b <= rip && rip < **b + **n) {
            if let Some(f) = self.s.image_files.get(&b) {
                m += &format!(" [{}+0x{:x}]", f.rsplit(['\\', '/']).next().unwrap_or(f), rip - b);
            }
        }
        // a jump to a null function pointer: say who called it
        if self.cpu.rip < 0x10000 {
            if let Ok(r) = self.mmu.read64(self.cpu.regs[4]) {
                match self.s.modules.iter().find(|x| !x.native && r >= x.base && r < x.base + x.size) {
                    Some(x) => m += &format!(" called from {}+0x{:x}", x.name, r - x.base),
                    None => m += &format!(" [rsp]=0x{:x}", r),
                }
            }
        }
        if unsafe { WINE_DEBUG } > 1 { { let iat = self.mmu.read64(0x7a406000).unwrap_or(1); console(&format!("[fault] tid {} pid {} {} region {:x?} wow64cpu iat {:x}\n", self.cur_tid(), self.s.wpid, m, self.mmu.region_of(addr), iat)); } }
        self.set_error(m);
    }
    /// Stops the program for an exception nobody handled, with the message made when it came.
    pub(crate) fn unhandled(&mut self, t: String) {
        let _g = lock::lock();
        if self.s.error.is_none() {
            self.s.error = Some(t);
        }
    }
    /// Stops the program with a message (the first error wins).
    pub fn set_error(&mut self, m: String) {
        // a Linux child process that crashes ends alone (like a signal), the shell goes on
        if self.s.lx.is_some() && linux::crash(self, &m) { return; }
        let _g = lock::lock();
        if self.s.error.is_none() {
            // where it happened: module + offset (Windows programs)
            let rip = self.cpu.rip;
            let at = if self.s.lx.is_none() { self.s.modules.iter().find(|x| !x.native && rip >= x.base && rip < x.base + x.size).map(|x| format!(" [in {}+0x{:x}]", x.name, rip - x.base)) } else { None };
            let mut calls = String::new();
            if self.s.lx.is_none() && self.s.recent_n > 0 {
                calls += "\nlast calls:";
                for j in (self.s.recent_n.saturating_sub(16)..self.s.recent_n).rev() {
                    let (t, ra) = self.s.recent[j % 16];
                    let f = self.s.thunks.get(t as usize).map_or(String::from("?"), |(d, n)| format!("{}!{}", d, n));
                    let at = self.s.modules.iter().find(|x| !x.native && ra >= x.base && ra < x.base + x.size).map_or(format!("0x{:x}", ra), |x| format!("{}+0x{:x}", x.name, ra - x.base));
                    calls += &format!("\n  {} from {}", f, at);
                }
            }
            self.s.error = Some(m + &at.unwrap_or_default() + &calls);
        }
    }

    /// Reserves address space only (pages are backed when first touched).
    /// Address space for `size` bytes (rounded up to 64 KB), reserved only. Internal use;
    /// the VirtualAlloc API uses `reserve_pages`.
    pub fn reserve(&mut self, size: u64) -> Option<u64> {
        self.map_range(size, crate::mmu::round_up_64k, false)
    }

    /// Committed memory for `size` bytes (rounded up to 64 KB). Internal use; the VirtualAlloc
    /// API uses `alloc_pages`. Nothing changes when it fails.
    pub fn alloc(&mut self, size: u64) -> Option<u64> {
        self.map_range(size, crate::mmu::round_up_64k, true)
    }

    /// VirtualAlloc with a NULL base, reserve only: `size` rounded up to whole pages, the base
    /// on the 64 KB allocation granularity.
    pub fn reserve_pages(&mut self, size: u64) -> Option<u64> {
        self.map_range(size, crate::mmu::round_up_page, false)
    }

    /// VirtualAlloc with a NULL base, committed: `size` rounded up to whole pages, the base on
    /// the 64 KB allocation granularity. Nothing changes when it fails.
    pub fn alloc_pages(&mut self, size: u64) -> Option<u64> {
        self.map_range(size, crate::mmu::round_up_page, true)
    }

    /// Finds a free range for `size` (at least one byte, rounded by `round`) and maps it:
    /// reserved only, or committed within the memory target. Nothing changes when it fails.
    fn map_range(&mut self, size: u64, round: fn(u64) -> Option<u64>, commit: bool) -> Option<u64> {
        let size = round(size.max(1))?;
        if commit && self.mmu.sh.committed.checked_add(size)? > self.mmu.sh.target_bytes { return None; }
        let f = self.find_free(size)?;
        if commit { self.mmu.add_region(f.a, size)?; } else { self.mmu.add_reserved(f.a, size)?; }
        self.take_free(f);
        Some(f.a)
    }

    /// Gives back a range that `reserve` just took and that cannot be used (its commit failed).
    /// The region is released, with its pages. When it was the last range taken from the bump
    /// pointer, the pointer goes back, so failed attempts do not use up address space. Nothing
    /// refers to the range yet, so it can be taken again.
    pub(crate) fn unreserve(&mut self, start: u64, size: u64) {
        self.mmu.release_region(start);
        if start.checked_add(size).and_then(|end| end.checked_add(0x10000)) == Some(self.s.next_alloc) {
            self.s.next_alloc = start;
        }
    }

    /// Where the next `size` bytes go, found without changing anything. The bump pointer
    /// steps over anything already there (an image at its preferred base, a fixed
    /// allocation), with a 64 KB gap after it. A 32-bit program's memory stays below 4 GB.
    /// When the top is used up, the search goes back for a gap below the bump pointer.
    fn find_free(&self, size: u64) -> Option<Found> {
        let limit = if self.s.next_alloc < 0x1_0000_0000 { 0xFFFF_0000 } else { 0x7F00_0000_0000 };
        // every base is on the 64 KB granularity, whatever the size (a page-sized one included)
        let mut a = crate::mmu::round_up_64k(self.s.next_alloc)?;
        while let Some(end) = self.mmu.overlap_end(a, a.checked_add(size)?) {
            a = crate::mmu::round_up_64k(end)?;
        }
        let (end, next) = crate::mmu::span_after(a, size, 0x10000)?;
        let next = crate::mmu::round_up_64k(next)?;
        if end > limit {
            // the address space is used up to the top: go back for room that was freed
            // (a 32-bit program that allocates and frees big buffers gets there quickly)
            let from = if limit < 0x1_0000_0000 { crate::wow64::LOW_ALLOC } else { 0x0000_0200_0000_0000 };
            let a = self.mmu.first_gap(from, limit, size, 0x10000)?;
            let (end, _) = crate::mmu::span_after(a, size, 0)?;
            return Some(Found { a, end, next: self.s.next_alloc, gap: true });
        }
        Some(Found { a, end, next, gap: false })
    }

    /// Takes a range found by find_free, after the mapping succeeded. Nothing here can fail:
    /// the values were checked when they were found. A gap leaves the bump pointer alone;
    /// the JIT forgets any code it had compiled for that address.
    fn take_free(&mut self, f: Found) {
        if f.gap {
            self.jit.forget(f.a, f.end);
        } else {
            self.s.next_alloc = f.next;
        }
    }

    /// Static TLS blocks of every image for the thread whose TEB is `teb`.
    pub fn tls_blocks(&mut self, teb: u64) {
        for i in 0..self.s.tls_tmpl.len() { self.tls_block(teb, i); }
    }
    pub fn tls_block(&mut self, teb: u64, i: usize) {
        let (t, n, z) = self.s.tls_tmpl[i];
        let b = self.heap_alloc((n + z).max(16), true);
        let mut v = vec![0u8; n as usize];
        if self.mmu.read_bytes(t, &mut v).is_ok() { let _ = self.mmu.write_bytes(b, &v); }
        self.w64(teb + 0x1800 + 8 * i as u64, b);
    }

    pub fn thunk_lookup(&self, dll: &str, func: &str) -> Option<u64> { self.s.thunk_ids.get(&format!("{}!{}", dll, func)).copied() }

    pub fn thunk(&mut self, dll: &str, func: &str) -> u64 {
        let key = format!("{}!{}", dll, func);
        if let Some(&a) = self.s.thunk_ids.get(&key) { return a; }
        let a = THUNK_BASE + 16 * self.s.thunks.len() as u64;
        self.s.thunks.push((Rc::from(dll), Rc::from(func)));
        self.s.routes.push(0);
        self.s.thunk_ids.insert(key, a);
        a
    }

    pub fn read_cstr(&mut self, a: u64, max: usize) -> String {
        let mut s = Vec::new();
        for i in 0..max as u64 {
            match self.mmu.read8(a + i) { Ok(0) | Err(_) => break, Ok(c) => s.push(c) }
        }
        String::from_utf8_lossy(&s).into_owned()
    }
    pub fn read_wstr(&mut self, a: u64, max: usize) -> String {
        let mut s = Vec::new();
        for i in 0..max as u64 {
            match self.mmu.read16(a + 2 * i) { Ok(0) | Err(_) => break, Ok(c) => s.push(c) }
        }
        String::from_utf16_lossy(&s)
    }
    pub fn put_str(&mut self, s: &str, wide: bool) -> u64 {
        if wide {
            let w: Vec<u16> = s.encode_utf16().chain([0]).collect();
            let a = self.heap_alloc(w.len() as u64 * 2, false);
            for (i, c) in w.iter().enumerate() { self.w16(a + 2 * i as u64, *c); }
            a
        } else {
            let a = self.heap_alloc(s.len() as u64 + 1, false);
            let _ = self.mmu.write_bytes(a, s.as_bytes());
            self.w8(a + s.len() as u64, 0);
            a
        }
    }

    fn boot(&mut self, name: &str, cmd: &str) -> Result<(), String> {
        // the usual folders of drive C: exist from the start
        for d in ["C:\\Program Files", "C:\\Program Files\\Common Files", "C:\\Program Files (x86)", "C:\\ProgramData", "C:\\Users\\Public",
            "C:\\InkSoC\\AppData\\Roaming", "C:\\InkSoC\\AppData\\Local", "C:\\InkSoC\\Documents", "C:\\InkSoC\\Desktop"] {
            let mut p = String::new();
            for part in d.split('\\') { if !p.is_empty() { p.push('\\'); } p.push_str(part); if let Some(k) = files::key_of(&p) { if !self.s.files.contains_key(&k) { self.note_case(&k, &p); self.s.fs.dirs.insert(k); } } }
        }
        // Thread and process blocks (reached by the program through gs:)
        self.mmu.add_region(TEB, 0x2000);
        self.mmu.add_region(PEB, 0x1000);
        // a 32-bit program needs its memory below 2 GB: allocations start low
        let bytes = self.exe_image(name).ok_or_else(|| format!("{} not found", name))?;
        if wow64::is_pe32(&bytes) { self.s.wow64 = true; self.s.next_alloc = wow64::LOW_ALLOC; }
        let stack = self.alloc(STACK_SIZE).ok_or("out of memory")?;
        self.s.stack_top = stack + STACK_SIZE - 0x100;
        let w = |e: &mut Engine, a: u64, v: u64| { e.w64(a, v); };
        w(self, TEB + 0x08, stack + STACK_SIZE);
        w(self, TEB + 0x10, stack);
        w(self, TEB + 0x30, TEB);
        w(self, TEB + 0x40, 4);
        w(self, TEB + 0x48, 8);
        w(self, TEB + 0x58, TEB + 0x1800); // ThreadLocalStoragePointer (static TLS array)
        w(self, TEB + 0x60, PEB);
        // StaticUnicodeString (ANSI functions convert names into it): 522 bytes at +0x1268
        self.w16(TEB + 0x125A, 522); w(self, TEB + 0x1260, TEB + 0x1268);
        self.cpu.gs_base = TEB;
        self.sched_init();
        if bytes.starts_with(b"\x7fELF") { return self.linux_boot(name, cmd); }
        let wow = wow64::is_pe32(&bytes);
        // before the imports are bound: msvcrt's _acmdln points at it
        self.s.cmdline = (self.put_str(cmd, false), self.put_str(cmd, true));
        self.process_parameters(name);
        self.teb_extras(TEB);
        self.shared_data();
        if wow {
            self.wow64_boot(name, cmd, &bytes)?;
        } else {
            let idx = pe::load_image(self, name, &bytes, true)?;
            self.s.exe_base = self.s.modules[idx].base;
            self.s.exe_entry = self.s.modules[idx].entry;
        }
        w(self, PEB + 0x10, self.s.exe_base);
        w(self, PEB + 0x30, 0x0000_0000_00AA_0000); // ProcessHeap handle
        self.cpu.regs[4] = self.s.stack_top;
        self.next_init();
        Ok(())
    }

    /// The program file for loading: a big one still on disk (an installer with hundreds of MB
    /// of data after its image) is read only up to the end of its sections; the program reads
    /// the rest itself through a file handle.
    fn exe_image(&self, name: &str) -> Option<Vec<u8>> {
        let size = self.s.files.size(name)?;
        if size < 64 << 20 || self.s.files.disk_of(name).is_none() { return self.s.files.get(name).cloned(); }
        let h = self.s.files.read_at(name, 0, 0x1000)?;
        let r32 = |b: &[u8], at: usize| b.get(at..at + 4).map_or(0, |x| u32::from_le_bytes([x[0], x[1], x[2], x[3]])) as u64;
        let r16 = |b: &[u8], at: usize| b.get(at..at + 2).map_or(0, |x| u16::from_le_bytes([x[0], x[1]])) as usize;
        let pe = r32(&h, 0x3C) as usize;
        if !h.starts_with(b"MZ") || h.get(pe..pe + 4) != Some(b"PE\0\0") { return self.s.files.get(name).cloned(); }
        let (nsec, opt) = (r16(&h, pe + 6), r16(&h, pe + 20));
        let sec = pe + 24 + opt;
        let mut end = r32(&h, pe + 24 + 60); // SizeOfHeaders (same place in PE32 and PE32+)
        for i in 0..nsec {
            let s = sec + 40 * i;
            end = end.max(r32(&h, s + 20) + r32(&h, s + 16));
        }
        self.s.files.read_at(name, 0, end.min(size))
    }

    /// Calls a program function from native code, returning to `ret`.
    pub fn call_guest(&mut self, f: u64, args: [u64; 3], ret: u64) {
        let sp = (self.cpu.regs[4] - 0x60) & !0xF;
        let sp = sp - 8;
        self.w64(sp, ret);
        self.cpu.regs[4] = sp;
        self.cpu.regs[1] = args[0];
        self.cpu.regs[2] = args[1];
        self.cpu.regs[8] = args[2];
        self.cpu.rip = f;
    }

    /// Runs the next pending DllMain, or the program entry when none is left.
    pub(crate) fn next_init(&mut self) {
        self.ldr_link();
        self.cpu.regs[4] = self.s.stack_top;
        if !self.s.init_queue.is_empty() {
            let (entry, base) = self.s.init_queue.remove(0);
            self.call_guest(entry, [base, 1, 0], RET_INIT);
        } else {
            let a0 = if self.s.wow64 { self.s.wow64_ctx } else { PEB };
            self.call_guest(self.s.exe_entry, [a0, 0, 0], RET_EXIT);
        }
    }

    // Program memory where a failure needs no handling: a read gives 0, a write to
    // address 0 (an optional out-parameter left out) is skipped.
    pub(crate) fn r8(&mut self, a: u64) -> u8 { self.mmu.read8(a).unwrap_or(0) }
    pub(crate) fn r16(&mut self, a: u64) -> u16 { self.mmu.read16(a).unwrap_or(0) }
    pub(crate) fn r32(&mut self, a: u64) -> u32 { self.mmu.read32(a).unwrap_or(0) }
    pub(crate) fn r64(&mut self, a: u64) -> u64 { self.mmu.read64(a).unwrap_or(0) }
    pub(crate) fn w8(&mut self, a: u64, v: u8) { if a != 0 { let _ = self.mmu.write8(a, v); } }
    pub(crate) fn w16(&mut self, a: u64, v: u16) { if a != 0 { let _ = self.mmu.write16(a, v); } }
    pub(crate) fn w32(&mut self, a: u64, v: u32) { if a != 0 { let _ = self.mmu.write32(a, v); } }
    pub(crate) fn w64(&mut self, a: u64, v: u64) { if a != 0 { let _ = self.mmu.write64(a, v); } }
    /// The thread's Windows "last error" (TEB.LastErrorValue).
    pub(crate) fn set_last_error(&mut self, v: u32) { let a = self.cpu.gs_base + 0x68; self.w32(a, v); }

    pub fn arg(&mut self, i: usize) -> u64 {
        match i {
            0 => self.cpu.regs[1],
            1 => self.cpu.regs[2],
            2 => self.cpu.regs[8],
            3 => self.cpu.regs[9],
            // stack args: [rsp] = return address, then 32 bytes of shadow space
            _ => self.r64(self.cpu.regs[4] + 8 + 8 * i as u64),
        }
    }

    pub fn do_ret(&mut self, v: u64) {
        self.cpu.regs[0] = v;
        let rsp = self.cpu.regs[4];
        match self.mmu.read64(rsp) {
            Ok(r) => { self.cpu.rip = r; self.cpu.regs[4] = rsp + 8; }
            Err(_) => self.fault(rsp),
        }
    }

    /// A native module's answer for a name it has no case for.
    pub(crate) fn not_mine(&mut self) -> Option<u64> { self.not_mine = true; None }

    /// Asks the engine's own Windows modules in order: scheduler, files, kernel32, the rest
    /// of the Win32 API, ntdll. A module may also pass on a name it knows (a handle that is
    /// not its kind), so the next one gets the call. The first call of each thunk learns
    /// which modules have no case for its name at all; later calls skip those.
    fn native_chain(&mut self, idx: usize, name: &str) -> Option<u64> {
        if unsafe { WINE_DEBUG } == 2 { call_census(self, name); }
        const MODULES: [fn(&mut Engine, &str) -> Option<u64>; 6] = [sched::call, files::call, kernel32::call, winapi::call, ntsys::call, unixlib::call];
        const LEARNED: u8 = 0x80;
        let route = self.s.routes.get(idx).copied().unwrap_or(0);
        if route & LEARNED != 0 {
            for (i, module) in MODULES.iter().enumerate() {
                if route & (1 << i) != 0 { if let Some(v) = module(self, name) { return Some(v); } }
            }
            return None;
        }
        // first call: every module counts as knowing the name until it says otherwise
        let mut route = LEARNED | 0x3F;
        let mut answer = None;
        for (i, module) in MODULES.iter().enumerate() {
            self.not_mine = false;
            answer = module(self, name);
            if self.not_mine { route &= !(1 << i); }
            if answer.is_some() { break; }
        }
        if let Some(r) = self.s.routes.get_mut(idx) { *r = route; }
        answer
    }

    fn call_native(&mut self) {
        self.ins_rip = 0;
        match self.cpu.rip {
            RET_EXIT => { let c = self.cpu.regs[0] as u32; self.exit_process(c); return; }
            RET_INIT => { self.next_init(); return; }
            RET_FBINIT => { self.init_next(); return; }
            RET_INITTERM => { msvcrt::initterm_next(self); return; }
            RET_EH => { self.eh_next(); return; }
            RET_THREAD => { let c = self.cpu.regs[0] as u32; self.exit_thread(c); return; }
            RET_LOADLIB => {
                let (h, sp) = self.s.loadlib.pop().unwrap_or((0, self.cpu.regs[4]));
                self.cpu.regs[4] = sp;
                self.do_ret(h);
                return;
            }
            _ => {}
        }
        let idx = ((self.cpu.rip - THUNK_BASE) / 16) as usize;
        if let Some(&a) = self.s.thunk_fb.get(&idx) { self.cpu.rip = a; return; }
        let ra = self.r64(self.cpu.regs[4]);
        let k = self.s.recent_n % 16; self.s.recent[k] = (idx as u32, ra); self.s.recent_n += 1;
        if unsafe { TRACE } { if let Some((d, n)) = self.s.thunks.get(idx).cloned() {
            let t = self.cur_tid(); let p = self.cpu.regs[1];
            // first argument shown as text when it reads like a string
            let mut txt = String::new();
            if n.ends_with('W') { for i in 0..80 { match self.mmu.read16(p + 2 * i) { Ok(c) if c >= 32 && c < 0x7F => txt.push(c as u8 as char), _ => break } } }
            else if n.contains("OpenFile") || n.contains("CreateFile") || n.contains("Key") { let oa = self.cpu.regs[8]; txt = self.oa_name(oa); }
            else if n.ends_with('A') { for i in 0..80 { match self.mmu.read8(p + i) { Ok(c) if c >= 32 && c < 0x7F => txt.push(c as char), _ => break } } }
            console(&format!("[call] t{} {}!{} rcx={:x} rdx={:x} r8={:x} ra={:x} {}\n", t, d, n, p, self.cpu.regs[2], self.cpu.regs[8], ra, txt));
        } }
        let Some((dll, func)) = self.s.thunks.get(idx).cloned() else {
            self.set_error(format!("jump into the native area at 0x{:x}", self.cpu.rip));
            return;
        };
        let dll: &str = &dll;
        // ntdll's Rtl heap functions are answered by their kernel32 twins
        let name: &str = match winefb::alias(&func) { Some(n) if dll == "ntdll.dll" => n, _ => &func };
        let r = match name {
            // Wine-side functions compiled from C to wasm
            "lstrlenA" => { self.s.bridge_calls += 1; Some(unsafe { wine_lstrlenA(self.arg(0)) } as u32 as u64) }
            "RtlComputeCrc32" => {
                self.s.bridge_calls += 1;
                let (a, b, c) = (self.arg(0) as u32, self.arg(1), self.arg(2) as i32);
                Some(unsafe { wine_RtlComputeCrc32(a, b, c) } as u64)
            }
            _ if name.starts_with("_beginthread") || name.starts_with("_endthread") => sched::call(self, name),
            _ if dll == "msvcrt.dll" => msvcrt::call(self, name),
            _ if dll == "oleaut32.dll" => oleaut::call(self, name),
            _ if dll.starts_with("vcruntime140") => msvcrt::call(self, name),
            // the window system's kernel side: console programs get "nothing there"
            _ if dll == "win32u.dll" => winsys::call(self, name),
            _ => self.native_chain(idx, name),
        };
        if self.s.error.is_some() || self.s.exited { return; }
        match r {
            Some(kernel32::NO_RETURN) | Some(sched::BLOCK) => {}
            Some(v) => {
                self.do_ret(v);
                if v == sched::USER_APC && !self.s.sched.th.is_empty() {
                    self.deliver_apc();
                }
            }
            None => {
                // not native: Wine's own version, run as program code
                let before = self.s.init_queue.len();
                match self.wine_fallback(dll, name, self.cpu.rip) {
                    Some(a) => {
                        self.s.thunk_fb.insert(idx, a);
                        // DLLs that came in for it start first
                        if self.s.init_queue.len() > before {
                            let pending: Vec<(u64, u64)> = self.s.init_queue.drain(before..).collect();
                            self.run_inits(pending, a, None);
                        } else { self.cpu.rip = a }
                    }
                    // a system call the engine does not answer fails as it does in Wine
                    // (STATUS_NOT_IMPLEMENTED) instead of stopping the program
                    None if dll == "ntdll.dll" && (name.starts_with("Nt") || name.starts_with("Zw")) => {
                        if self.s.stubbed.insert(name.to_string()) {
                            console(&format!("[engine] stub: {}!{} returns STATUS_NOT_IMPLEMENTED\n", dll, name));
                        }
                        self.do_ret(0xC000_0002);
                    }
                    None => self.set_error(format!("not implemented yet: {}!{}", dll, name)),
                }
            }
        }
    }
}

/// Host stand-ins for the wasm imports, so `cargo test` on the host can link.
/// The tests never reach them except through console output.
#[cfg(test)]
mod host_stubs {
    #[no_mangle]
    pub extern "C" fn host_console(_ptr: *const u8, _len: u32) {}
}

// ?winedebug=2: every 5 s, the native calls made since, most frequent first (what a stuck
// program is doing). Debug only.
fn call_census(e: &Engine, name: &str) {
    let pid = e.s.wpid;
    use std::cell::RefCell;
    thread_local! { static C: RefCell<(f64, std::collections::HashMap<String, u64>)> = RefCell::new((0.0, Default::default())); }
    C.with(|c| {
        let mut c = c.borrow_mut();
        *c.1.entry(format!("{}:{}", pid, name)).or_insert(0) += 1;
        let now = now_ms();
        if now - c.0 > 5000.0 {
            c.0 = now;
            let mut v: Vec<_> = c.1.drain().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1));
            let top: Vec<String> = v.iter().take(12).map(|(n, k)| format!("{} {}", n, k)).collect();
            console(&format!("[calls] {}\n", top.join(", ")));
            let (total, big, n) = e.s.files.mem_stats();
            let (sn, sb) = e.s.files.spilled();
            let open = e.s.fs.handles.values().filter(|h| matches!(h, files::Handle::File { .. })).count();
            console(&format!(
                "[mem] engine heap {} MB, files in memory {} MB, biggest {} ({} MB); spilled {} files {} MB (spill file {} MB), spill failures {}, big writes kept in memory {} (clean {}); open files {}\n",
                heap_used() as u64 >> 20, total >> 20, big, n >> 20, sn, sb >> 20, e.s.files.spill_end() >> 20, e.s.files.spill_fails.get(), e.s.files.big_refused.get(), e.s.files.big_clean.get(), open
            ));
        }
    });
}
