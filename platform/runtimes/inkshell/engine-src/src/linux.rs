// Linux programs on the same CPU: a static x86-64 ELF loader and the Linux
// system calls (the `syscall` instruction). Files live in a small in-memory
// tree (/bin, /root, /tmp, ...). One process for now: no fork yet.

use crate::Engine;
use std::collections::{BTreeMap, BTreeSet};

const STACK_TOP: u64 = 0x7FFF_F000_0000;
const STACK_SIZE: u64 = 8 << 20;
const MMAP_BASE: u64 = 0x10_0000_0000;
const PAGE: u64 = 4096;
const BLOCKED: i64 = i64::MIN + 1; // the call waits (and runs again when woken)
const NORET: i64 = i64::MIN + 2; // registers already set (execve)

// errno
const ENOENT: i64 = 2; const EBADF: i64 = 9; const ENOMEM: i64 = 12; const EEXIST: i64 = 17;
const ENOTDIR: i64 = 20; const EISDIR: i64 = 21; const EINVAL: i64 = 22; const ENOTTY: i64 = 25;
const ESPIPE: i64 = 29; const ERANGE: i64 = 34; const ENOSYS: i64 = 38; const ENOTEMPTY: i64 = 39;
const ECHILD: i64 = 10; const EPIPE: i64 = 32; const ENOEXEC: i64 = 8;

pub enum Node { File(Vec<u8>), Dir, Link(String) }

#[derive(Clone)]
pub enum Fd {
    Tty,
    File { path: String, pos: u64, append: bool },
    Dir { path: String, pos: usize },
    Pipe { id: u32, w: bool },
    Sock { id: u32 },
    Dev(u8), // 0 /dev/zero, 1 /dev/urandom
    Unix { id: u32 },  // a local stream socket (AF_UNIX, or TCP on 127.0.0.1)
    Event { id: u32 }, // eventfd
    Epoll { id: u32 },
    Pty { id: u32, master: bool }, // /dev/ptmx and its /dev/pts/N
    Timer { id: u32 }, // timerfd
}

/// What a waiting Linux thread waits for.
#[derive(Clone)]
pub enum Cond { PipeRead(u32), Child { parent: u32, pid: i64 }, Stdin, Until, Socks(Vec<u32>), Fds(Vec<(Fd, u16)>), Futex { addr: u64 }, Signal, SigWait(u64) }

/// A process that is not running right now keeps its state here.
pub struct Saved {
    fds: BTreeMap<i32, Fd>, cloexec: BTreeSet<i32>, cwd: String,
    brk_start: u64, brk: u64, mmap_next: u64, exe: String, ids: (u32, u32, u32),
    space: crate::mmu::Space, jit: crate::jit::Jit,
}
pub struct Proc { ppid: u32, saved: Option<Saved>, zombie: Option<i32>, pgid: u32, ctty: u32 } // ctty: controlling pty (0 = the page's terminal)

#[derive(Default)]
pub struct Lx {
    pub fs: BTreeMap<String, Node>,
    // the running process
    pub pid: u32,
    pub fds: BTreeMap<i32, Fd>,
    pub cloexec: BTreeSet<i32>,
    pub cwd: String,
    pub brk_start: u64,
    pub brk: u64,
    pub mmap_next: u64,
    pub exe: String,
    pub ids: (u32, u32, u32), // real uid, gid, effective uid
    pub dirty: bool, // the file tree changed since the page last saved it
    // every process
    pub procs: BTreeMap<u32, Proc>,
    pub next_pid: u32,
    pub pipes: BTreeMap<u32, std::collections::VecDeque<u8>>,
    pub next_pipe: u32,
    pub stdin: Vec<u8>,
    pub stdin_eof: bool,
    pub lflag: u32, // terminal mode (ICANON 2, ECHO 8)
    pub termios: Option<Vec<u8>>,
    pub winsize: (u16, u16), // rows, columns
    pub written: BTreeSet<String>,
    pub unknown: BTreeSet<u64>,
    want: Option<Cond>,
    // network: sockets go out through the page (wisp tunnel on our server)
    pub socks: BTreeMap<u32, Sock>,
    pub next_sock: u32,
    pub net_out: Vec<u8>,
    pub dns: Vec<String>,
    // threads: scheduler thread id -> (Linux thread id, address cleared at exit)
    tids: BTreeMap<u32, (u32, u64)>,
    // signals: handlers per process (handler, flags, restorer, mask), pending per process,
    // blocked per thread, the mask to restore after rt_sigsuspend
    acts: BTreeMap<u32, Vec<[u64; 4]>>,
    pend: BTreeMap<u32, u64>,
    masks: BTreeMap<u32, u64>,
    susp: BTreeMap<u32, u64>,
    fg: u32, // foreground process group of the terminal (0 = not set)
    // local sockets, eventfd, epoll
    us: BTreeMap<u32, Us>,
    us_names: BTreeMap<String, u32>,
    next_us: u32,
    evs: BTreeMap<u32, (u64, bool, bool)>, // count, semaphore, non-blocking
    eps: BTreeMap<u32, BTreeMap<i32, (u32, u64)>>,
    next_ev: u32,
    ptys: BTreeMap<u32, Pty>,
    // owners and permissions (path -> mode bits, uid, gid); paths not listed: root, 755/644
    meta: BTreeMap<String, (u32, u32, u32)>,
    umask: u32,
    // descriptors passed with SCM_RIGHTS, waiting in a local socket's buffer (by pipe id)
    // (stream position, descriptors); reads stop where descriptors are attached, as on Linux
    passed: BTreeMap<u32, std::collections::VecDeque<(u64, Vec<Fd>)>>,
    pread: BTreeMap<u32, u64>, // bytes read so far from a local socket's buffer (by pipe id)
    us_got: Option<Vec<Fd>>, // descriptors the last local socket read reached
    pipe_nb: BTreeSet<(u32, bool)>, // pipe ends opened O_NONBLOCK
    // MAP_SHARED file mappings (pid, address, length, file, offset): written back at munmap/msync
    shared: Vec<(u32, u64, u64, String, u64)>,
    timers: BTreeMap<u32, Tmr>, // alarm/setitimer, timer_create and timerfd timers (by id)
    alarms: BTreeMap<u32, u32>, // each process's ITIMER_REAL timer (pid -> timer id)
    next_tm: u32,
}

/// A timer: at `next` (host ms; 0 = disarmed) it signals `sig` to `pid`, or (fd) counts an
/// expiration a timerfd read returns; `every` > 0 rearms it.
#[derive(Clone, Default)]
pub struct Tmr { pid: u32, sig: u32, fd: bool, nb: bool, real: bool, next: f64, every: f64, count: u64 }

/// A pseudo-terminal: what the master writes goes through the line discipline to the slave's input;
/// what the slave writes goes (with output processing) to the master.
pub struct Pty { inq: std::collections::VecDeque<u8>, line: Vec<u8>, eofs: u32, out: std::collections::VecDeque<u8>, termios: Vec<u8>, ws: (u16, u16), fg: u32, nb: (bool, bool), opened: bool } // opened: a slave was opened once

fn default_termios() -> Vec<u8> {
    let mut t = vec![0u8; 36];
    t[0..4].copy_from_slice(&0x500u32.to_le_bytes()); // ICRNL | IXON
    t[4..8].copy_from_slice(&0x5u32.to_le_bytes()); // OPOST | ONLCR
    t[8..12].copy_from_slice(&0xBFu32.to_le_bytes()); // CS8 | CREAD | B38400
    t[12..16].copy_from_slice(&0x8A3Bu32.to_le_bytes()); // ISIG | ICANON | ECHO...
    t[17..28].copy_from_slice(&[3, 0x1C, 0x7F, 0x15, 4, 0, 1, 0, 0x11, 0x13, 0x1A]); // ^C ^\ DEL ^U ^D, VMIN 1, ^Q ^S ^Z
    t
}

/// A local stream socket. Connected pairs talk through two pipe buffers.
pub struct Us { st: UsSt, nb: bool, name: String, port: u16, shut_wr: bool }
pub enum UsSt { New, Listen(std::collections::VecDeque<u32>), Conn { rx: u32, tx: u32, peer: u32 } }

/// A socket. TCP data comes and goes through the page; UDP only answers DNS (here, with made-up addresses
/// 198.18.x.y that the page turns back into names when connecting).
#[derive(Default)]
pub struct Sock { udp: bool, nb: bool, open: bool, closed: bool, failed: bool, port: u16, rx: std::collections::VecDeque<u8>, dgrams: std::collections::VecDeque<(Vec<u8>, [u8; 16])>, peer: [u8; 16] }
const EAGAIN: i64 = 11; const ENOTCONN: i64 = 107; const ECONNREFUSED: i64 = 111; const EOPNOTSUPP: i64 = 95; const EAFNOSUPPORT: i64 = 97; const ENETUNREACH: i64 = 101;


fn align(v: u64) -> u64 { (v + PAGE - 1) & !(PAGE - 1) }

/// Splits a command line into words ("..." and '...' keep spaces).
pub fn split_args(cmd: &str) -> Vec<String> {
    let (mut out, mut cur, mut q, mut any) = (Vec::new(), String::new(), None::<char>, false);
    for c in cmd.chars() {
        match (q, c) {
            (Some(x), c) if c == x => q = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => { q = Some(c); any = true; }
            (None, c) if c.is_whitespace() => { if any || !cur.is_empty() { out.push(std::mem::take(&mut cur)); any = false; } }
            (None, c) => cur.push(c),
        }
    }
    if any || !cur.is_empty() { out.push(cur); }
    out
}

impl Lx {
    /// Absolute, normalized path (no symlinks followed).
    fn abs(&self, p: &str) -> String {
        let full = if p.starts_with('/') { p.to_string() } else { format!("{}/{}", self.cwd, p) };
        let mut parts: Vec<&str> = Vec::new();
        for s in full.split('/') {
            match s { "" | "." => {} ".." => { parts.pop(); } s => parts.push(s) }
        }
        format!("/{}", parts.join("/"))
    }
    /// Follows symlinks (in every component); `last` = follow the final one too.
    pub fn resolve(&self, p: &str, last: bool) -> String {
        let mut path = self.abs(p);
        for _ in 0..16 {
            let parts: Vec<String> = path.split('/').filter(|s| !s.is_empty()).map(String::from).collect();
            let mut cur = String::new();
            let mut changed = false;
            for (k, s) in parts.iter().enumerate() {
                cur = format!("{}/{}", cur, s);
                if k + 1 == parts.len() && !last { break; }
                if let Some(Node::Link(t)) = self.fs.get(&cur) {
                    let base = cur.rsplit_once('/').map(|x| x.0).unwrap_or("");
                    let target = if t.starts_with('/') { t.clone() } else { format!("{}/{}", base, t) };
                    let rest: Vec<&str> = parts[k + 1..].iter().map(|s| s.as_str()).collect();
                    path = self.abs(&format!("{}/{}", target, rest.join("/")));
                    changed = true;
                    break;
                }
            }
            if !changed { return if path.is_empty() { "/".into() } else { path }; }
        }
        path
    }
    pub fn mkdirs(&mut self, path: &str) {
        let mut cur = String::new();
        for s in path.split('/').filter(|s| !s.is_empty()) {
            cur = format!("{}/{}", cur, s);
            self.fs.entry(cur.clone()).or_insert(Node::Dir);
        }
    }
    pub fn put(&mut self, path: &str, data: Vec<u8>) {
        if let Some((dir, _)) = path.rsplit_once('/') { self.mkdirs(dir); }
        self.fs.insert(path.to_string(), Node::File(data));
    }
    fn is_dir(&self, p: &str) -> bool { p == "/" || matches!(self.fs.get(p), Some(Node::Dir)) }
    fn children(&self, dir: &str) -> Vec<String> {
        let pre = if dir == "/" { "/".to_string() } else { format!("{}/", dir) };
        self.fs.range(pre.clone()..).take_while(|(k, _)| k.starts_with(&pre))
            .filter(|(k, _)| !k[pre.len()..].contains('/')).map(|(k, _)| k[pre.len()..].to_string()).collect()
    }
    /// Unpacks a tar archive (ustar, GNU long names, pax paths) at /.
    pub fn untar(&mut self, t: &[u8]) {
        let num = |b: &[u8]| -> usize {
            if b[0] & 0x80 != 0 { return b[1..].iter().fold(0usize, |a, &c| (a << 8) | c as usize); }
            let s = String::from_utf8_lossy(b); usize::from_str_radix(s.trim_matches(|c: char| c == '\0' || c == ' '), 8).unwrap_or(0)
        };
        let cstr = |b: &[u8]| String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]).to_string();
        let (mut o, mut long, mut longlink) = (0usize, None::<String>, None::<String>);
        while o + 512 <= t.len() {
            let h = &t[o..o + 512];
            if h.iter().all(|&c| c == 0) { break; }
            let size = num(&h[124..136]);
            let ty = h[156];
            let data = &t[(o + 512).min(t.len())..(o + 512 + size).min(t.len())];
            o += 512 + (size + 511) / 512 * 512;
            let mut name = cstr(&h[0..100]);
            if &h[257..262] == b"ustar" { let pre = cstr(&h[345..500]); if !pre.is_empty() { name = format!("{}/{}", pre, name); } }
            let mut link = cstr(&h[157..257]);
            match ty {
                b'L' => { long = Some(cstr(data)); continue; }
                b'K' => { longlink = Some(cstr(data)); continue; }
                b'x' => {
                    for rec in String::from_utf8_lossy(data).lines() {
                        if let Some((_, kv)) = rec.split_once(' ') {
                            if let Some(p) = kv.strip_prefix("path=") { long = Some(p.to_string()); }
                            if let Some(p) = kv.strip_prefix("linkpath=") { longlink = Some(p.to_string()); }
                        }
                    }
                    continue;
                }
                b'g' => continue,
                _ => {}
            }
            if let Some(n) = long.take() { name = n; }
            if let Some(n) = longlink.take() { link = n; }
            let path = self.abs(&format!("/{}", name));
            if path == "/" { continue; }
            if matches!(ty, b'5' | b'0' | 0 | b'7' | b'1') {
                let (m, u, g) = (num(&h[100..108]) as u32 & 0o7777, num(&h[108..116]) as u32, num(&h[116..124]) as u32);
                let dflt = if ty == b'5' { 0o755 } else if path.starts_with("/bin/") || path.starts_with("/usr/bin/") || path.starts_with("/sbin/") || path.starts_with("/usr/sbin/") { 0o755 } else { 0o644 };
                if m != 0 && (m != dflt || u != 0 || g != 0) { self.meta.insert(path.clone(), (m, u, g)); } else { self.meta.remove(&path); }
            }
            if let Some((dir, _)) = path.rsplit_once('/') { self.mkdirs(dir); }
            match ty {
                b'5' => { self.mkdirs(&path); }
                b'2' => { self.fs.insert(path, Node::Link(link)); }
                b'1' => { let src = self.abs(&format!("/{}", link)); if let Some(Node::File(d)) = self.fs.get(&src) { let d = d.clone(); self.fs.insert(path, Node::File(d)); } }
                b'0' | 0 | b'7' => { self.fs.insert(path, Node::File(data.to_vec())); }
                _ => {}
            }
        }
    }
    fn new_fd(&mut self, f: Fd) -> i32 {
        let mut n = 0;
        while self.fds.contains_key(&n) { n += 1; }
        self.fds.insert(n, f);
        n
    }
}

fn lx(e: &mut Engine) -> &mut Lx { e.s.lx.as_mut().unwrap() }

impl Engine {
    /// Loads a static ELF and builds the Linux start stack.
    pub fn linux_boot(&mut self, name: &str, cmd: &str) -> Result<(), String> {
        let mut l = Lx { umask: 0o022, cwd: "/root".into(), mmap_next: MMAP_BASE, pid: 1, next_pid: 100, lflag: 0x8A3B, winsize: (30, 100), ..Default::default() };
        l.procs.insert(1, Proc { ppid: 0, saved: None, zombie: None, pgid: 1, ctty: 0 });
        for d in ["/bin", "/usr/bin", "/sbin", "/tmp", "/root", "/etc", "/dev", "/dev/pts", "/proc/self"] { l.mkdirs(d); }
        l.fs.insert("/etc/passwd".into(), Node::File(b"root:x:0:0:root:/root:/bin/sh\n".to_vec()));
        l.fs.insert("/etc/group".into(), Node::File(b"root:x:0:\n".to_vec()));
        l.fs.insert("/etc/hostname".into(), Node::File(b"inksoc\n".to_vec()));
        l.fs.insert("/dev/null".into(), Node::File(Vec::new()));
        for d in ["zero", "urandom", "random", "tty", "console", "ptmx"] { l.fs.insert(format!("/dev/{}", d), Node::File(Vec::new())); }
        // the page's files go to /root; an ELF also goes to /bin (busybox: every applet links to it)
        let nm = |e: &Engine, k: &str| e.s.names.get(k).cloned().unwrap_or_else(|| k.to_string());
        let files: Vec<(String, Vec<u8>)> = self.s.files.iter().map(|(k, v)| (nm(self, k), v.clone())).collect();
        let bytes = self.s.files.get(name).cloned().ok_or_else(|| format!("{} not found", name))?;
        let name = nm(self, name);
        // a .tar is a root file system (e.g. Alpine's minirootfs): it is unpacked at /
        let mut files = files;
        files.sort_by_key(|(k, _)| (!k.starts_with("rootfs"), k.clone())); // the root file system first, overlays after
        for (k, v) in files { if k.ends_with(".tar") { l.untar(&v); } else if k != name { l.put(&format!("/root/{}", k), v); } }
        l.fs.insert("/etc/resolv.conf".into(), Node::File(b"nameserver 10.0.2.3\n".to_vec()));
        let exe = format!("/bin/{}", name);
        l.put(&exe, bytes.clone());
        l.fds.insert(0, Fd::Tty); l.fds.insert(1, Fd::Tty); l.fds.insert(2, Fd::Tty);
        l.fs.insert("/proc/self/exe".into(), Node::Link(exe.clone()));
        if name == "busybox" || name.starts_with("busybox") {
            for a in ["sh", "ash", "ls", "cat", "echo", "mkdir", "rm", "cp", "mv", "pwd", "grep", "sed", "awk", "wc", "head", "tail", "sort", "uniq", "touch", "find", "xargs", "vi", "env", "uname", "date", "sleep", "true", "false", "test", "expr", "printf", "md5sum", "sha256sum", "tar", "gzip", "gunzip", "ps", "free", "df", "du", "which", "clear", "seq", "tr", "cut", "basename", "dirname", "yes", "hexdump", "od", "diff", "less", "more", "ln", "chmod", "stat"] {
                l.fs.insert(format!("/bin/{}", a), Node::Link(exe.clone()));
            }
        }
        self.s.lx = Some(Box::new(l));
        // processes share the swap: pages of any process can move out to OPFS
        self.sched_init();
        self.s.sched.th[0].pid = 1;
        let mut args = split_args(cmd);
        if args.is_empty() { args.push(name.clone()); }
        let env: Vec<String> = ["HOME=/root", "PATH=/bin:/usr/bin:/sbin", "TERM=xterm-256color", "USER=root", "SHELL=/bin/sh", "PWD=/root", "LANG=C.UTF-8"].iter().map(|s| s.to_string()).collect();
        self.load_elf(&bytes, &exe, &args, &env)
    }

    /// Maps the PT_LOAD segments of an ELF at `base` (0 for fixed programs).
    /// Returns (entry, phdr, phent, phnum, end, interpreter path).
    fn map_elf(&mut self, b: &[u8], base: u64) -> Result<(u64, u64, u64, u64, u64, Option<String>), String> {
        let u16_ = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]) as u64;
        let u32_ = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as u64;
        let u64_ = |o: usize| u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        if b.len() < 64 || &b[0..4] != b"\x7fELF" || b[4] != 2 || u16_(18) != 62 { return Err("not an x86-64 ELF program".into()); }
        let base = if u16_(16) == 3 { base } else { 0 };
        let (phoff, phent, phnum) = (u64_(32) as usize, u16_(54) as usize, u16_(56) as usize);
        let (mut end, mut phdr, mut interp) = (0u64, 0u64, None);
        for k in 0..phnum {
            let p = phoff + k * phent;
            if p + 56 > b.len() { return Err("bad ELF header".into()); }
            match u32_(p) {
                1 => {
                    let (off, va, fsz, msz) = (u64_(p + 8), u64_(p + 16) + base, u64_(p + 32), u64_(p + 40));
                    let start = va & !(PAGE - 1);
                    let stop = align(va + msz);
                    // segments may share a page: only add what is not mapped yet
                    let mut a = start;
                    while a < stop {
                        if self.mmu.region_of(a).is_none() {
                            let mut z = a;
                            while z < stop && self.mmu.region_of(z).is_none() { z += PAGE; }
                            self.mmu.add_region(a, z - a);
                            a = z;
                        } else { a += PAGE; }
                    }
                    let data = b.get(off as usize..(off + fsz) as usize).ok_or("ELF segment outside the file")?;
                    self.mmu.write_bytes(va, data).map_err(|_| "out of memory")?;
                    if off == 0 && phdr == 0 { phdr = va + phoff as u64; }
                    end = end.max(stop);
                }
                3 => {
                    let (off, sz) = (u64_(p + 8) as usize, u64_(p + 32) as usize);
                    let s = b.get(off..off + sz).ok_or("bad interpreter path")?;
                    interp = Some(String::from_utf8_lossy(s).trim_end_matches('\0').to_string());
                }
                6 => phdr = u64_(p + 16) + base,
                _ => {}
            }
        }
        Ok((u64_(24) + base, phdr, phent as u64, phnum as u64, end, interp))
    }

    /// Maps an ELF program into the (empty) current address space and builds its start stack.
    /// A dynamic program also gets its loader (e.g. /lib/ld-musl-x86_64.so.1), which starts first.
    fn load_elf(&mut self, bytes: &[u8], exe: &str, args: &[String], env: &[String]) -> Result<(), String> {
        lx(self).exe = exe.to_string();
        let (entry, phdr, phent, phnum, end, interp) = self.map_elf(bytes, 0x5555_5555_4000)?;
        let (mut start_ip, mut ld_base) = (entry, 0u64);
        if let Some(ip) = interp {
            let l = lx(self);
            let q = l.resolve(&ip, true);
            let d = match l.fs.get(&q) { Some(Node::File(d)) => d.clone(), _ => return Err(format!("needs {} (its dynamic loader), which is not installed", ip)) };
            ld_base = 0x7F00_0000_0000;
            let r = self.map_elf(&d, ld_base)?;
            start_ip = r.0;
        }
        let l = lx(self);
        l.brk_start = end; l.brk = end; l.mmap_next = MMAP_BASE;

        // stack: argc, argv, envp, auxv, then the strings
        self.mmu.add_region(STACK_TOP - STACK_SIZE, STACK_SIZE);
        let mut sp = STACK_TOP - 0x100;
        let mut put = |e: &mut Engine, s: &[u8]| -> u64 { sp -= s.len() as u64 + 1; let _ = e.mmu.write_bytes(sp, s); e.w8(sp + s.len() as u64, 0); sp };
        let argp: Vec<u64> = args.iter().map(|a| put(self, a.as_bytes())).collect();
        let envp: Vec<u64> = env.iter().map(|a| put(self, a.as_bytes())).collect();
        let plat = put(self, b"x86_64");
        let rnd = put(self, b"0123456789abcdef");
        let exe_p = put(self, exe.as_bytes());
        let aux: [(u64, u64); 15] = [(3, phdr), (4, phent), (5, phnum), (6, PAGE), (7, ld_base), (8, 0), (9, entry),
            (11, 0), (12, 0), (13, 0), (14, 0), (16, 0x178bfbff), (17, 100), (25, rnd), (31, exe_p)];
        let words = 1 + argp.len() + 1 + envp.len() + 1 + 2 * (aux.len() + 2);
        let mut p = (sp - 8 * words as u64) & !15;
        let start = p;
        let mut w = |e: &mut Engine, v: u64| { e.w64(p, v); p += 8; };
        w(self, argp.len() as u64);
        for a in argp { w(self, a); }
        w(self, 0);
        for a in envp { w(self, a); }
        w(self, 0);
        for (k, v) in aux { w(self, k); w(self, v); }
        w(self, 15); w(self, plat);
        w(self, 0); w(self, 0);
        self.cpu.regs = [0; 16];
        self.cpu.fs_base = 0;
        self.cpu.regs[4] = start;
        self.cpu.rip = start_ip;
        Ok(())
    }
}

fn ret(e: &mut Engine, v: i64) { e.cpu.regs[0] = v as u64; }

fn stat_buf(e: &mut Engine, a: u64, mode: u32, size: u64, ino: u64) -> i64 { stat_buf_own(e, a, mode, size, ino, (0, 0)) }

/// Permission bits and owner of a path.
fn perm(l: &Lx, p: &str, dir: bool) -> (u32, u32, u32) {
    l.meta.get(p).copied().unwrap_or(if dir { (0o755, 0, 0) } else if p.starts_with("/dev/") { (0o666, 0, 0) } else if p.starts_with("/bin/") || p.starts_with("/usr/bin/") || p.starts_with("/sbin/") || p.starts_with("/usr/sbin/") { (0o755, 0, 0) } else { (0o644, 0, 0) })
}

fn stat_buf_own(e: &mut Engine, a: u64, mode: u32, size: u64, ino: u64, own: (u32, u32)) -> i64 {
    let mut b = [0u8; 144];
    b[28..32].copy_from_slice(&own.0.to_le_bytes());
    b[32..36].copy_from_slice(&own.1.to_le_bytes());
    b[0..8].copy_from_slice(&1u64.to_le_bytes());
    b[8..16].copy_from_slice(&ino.to_le_bytes());
    b[16..24].copy_from_slice(&1u64.to_le_bytes());
    b[24..28].copy_from_slice(&mode.to_le_bytes());
    b[48..56].copy_from_slice(&size.to_le_bytes());
    b[56..64].copy_from_slice(&4096u64.to_le_bytes());
    b[64..72].copy_from_slice(&((size + 511) / 512).to_le_bytes());
    let t = (crate::epoch_ms() / 1000.0) as u64;
    for o in [72, 88, 104] { b[o..o + 8].copy_from_slice(&t.to_le_bytes()); }
    if e.mmu.write_bytes(a, &b).is_err() { return -14; }
    0
}

fn stat_path(e: &mut Engine, path: &str, a: u64, follow: bool) -> i64 {
    let l = lx(e);
    let p = l.resolve(path, follow);
    // terminals: the same inode as fstat of an open one (ttyname compares them)
    let ino = if let Some(n) = p.strip_prefix("/dev/pts/").and_then(|n| n.parse::<u64>().ok()) { 8000 + n } else if p == "/dev/console" || p == "/dev/tty" { 5 } else { p.bytes().fold(1469598103934665603u64, |h, c| (h ^ c as u64).wrapping_mul(1099511628211)) };
    let (mode, size) = match l.fs.get(&p) {
        _ if p == "/" => (0o040000, 4096),
        Some(Node::Dir) => (0o040000, 4096),
        Some(Node::File(d)) => (if p.starts_with("/dev/") { 0o020000 } else if l.us_names.contains_key(&p) { 0o140000 } else { 0o100000 }, d.len() as u64),
        Some(Node::Link(t)) => (0o120777, t.len() as u64),
        None => return -ENOENT,
    };
    let (bits, uid, gid) = if mode == 0o120777 { (0, 0, 0) } else { perm(l, &p, mode == 0o040000) };
    stat_buf_own(e, a, mode | bits, size, ino, (uid, gid))
}

/// /proc files made when opened (memory, CPU, uptime) so free, top and htop have something to read.
fn proc_file(e: &mut Engine, p: &str) {
    let total_kb = e.mmu.sh.target_bytes >> 10;
    let frames_kb = (e.mmu.frames_total() * 4) as u64;
    let used_kb = (e.mmu.resident() * 4) as u64;
    let up = unsafe { crate::host_now_ms() } / 1000.0;
    let text = match p {
        "/proc/meminfo" => format!("MemTotal:       {total_kb} kB\nMemFree:        {f} kB\nMemAvailable:   {f} kB\nBuffers:        0 kB\nCached:         0 kB\nSwapCached:     0 kB\nSwapTotal:      0 kB\nSwapFree:       0 kB\nShmem:          0 kB\nSReclaimable:   0 kB\n", f = total_kb.saturating_sub(used_kb)),
        "/proc/cpuinfo" => "processor\t: 0\nvendor_id\t: InkSoC\nmodel name\t: InkSoC x86-64 (WebAssembly JIT)\ncpu MHz\t\t: 1000.000\ncpu cores\t: 1\nflags\t\t: fpu tsc cx8 cmov mmx fxsr sse sse2 lm\n\n".to_string(),
        "/proc/uptime" => format!("{:.2} 0.00\n", up),
        "/proc/loadavg" => "0.00 0.00 0.00 1/1 1\n".to_string(),
        "/proc/version" => "Linux version 6.6.0-inksoc (InkSoC engine in WebAssembly) #1 SMP\n".to_string(),
        "/proc/stat" => format!("cpu  {u} 0 0 0 0 0 0 0 0 0\ncpu0 {u} 0 0 0 0 0 0 0 0 0\nbtime 0\nprocesses 1\nprocs_running 1\n", u = (up * 100.0) as u64),
        _ => return,
    };
    let _ = frames_kb;
    let l = lx(e);
    l.mkdirs("/proc");
    l.fs.insert(p.to_string(), Node::File(text.into_bytes()));
}

fn open(e: &mut Engine, path: &str, flags: u64, mode: u64) -> i64 {
    let l = lx(e);
    let p = l.resolve(path, true);
    let p = if p.starts_with("/proc/") { let q = p.clone(); proc_file(e, &q); q } else { p };
    let l = lx(e);
    let exists = p == "/" || l.fs.contains_key(&p);
    if l.is_dir(&p) {
        if flags & 3 != 0 { return -EISDIR; }
        let fd = l.new_fd(Fd::Dir { path: p, pos: 0 });
        if flags & 0x80000 != 0 { l.cloexec.insert(fd); }
        return fd as i64;
    }
    if flags & 0x10000 != 0 { return if exists { -ENOTDIR } else { -ENOENT }; }
    if p == "/dev/ptmx" || p.starts_with("/dev/pts/") || p == "/dev/tty" {
        let me = l.pid;
        let ctty = l.procs.get(&me).map_or(0, |x| x.ctty);
        let (id, master) = if p == "/dev/ptmx" {
            let id = (0..).find(|k| !l.ptys.contains_key(k) || !held(l, &|f| matches!(f, Fd::Pty { id: i, .. } if *i == *k))).unwrap();
            l.ptys.insert(id, Pty { inq: Default::default(), line: Vec::new(), eofs: 0, out: Default::default(), termios: default_termios(), ws: (24, 80), fg: 0, nb: (false, false), opened: false });
            l.fs.insert(format!("/dev/pts/{}", id), Node::File(Vec::new()));
            (id, true)
        } else if p == "/dev/tty" {
            if ctty == 0 { let fd = l.new_fd(Fd::Tty); if flags & 0x80000 != 0 { l.cloexec.insert(fd); } return fd as i64; }
            (ctty - 1, false)
        } else {
            let Ok(n) = p[9..].parse::<u32>() else { return -ENOENT };
            if !l.ptys.contains_key(&n) { return -5; } // EIO
            // a session leader without a terminal gets this one (unless O_NOCTTY)
            if flags & 0x100 == 0 && ctty == 0 && l.procs.get(&me).map_or(false, |x| x.pgid == me) { if let Some(x) = l.procs.get_mut(&me) { x.ctty = n + 1; } }
            (n, false)
        };
        if let Some(t) = l.ptys.get_mut(&id) { if master { t.nb.0 = flags & 0x800 != 0; } else { t.nb.1 = flags & 0x800 != 0; t.opened = true; } }
        let fd = l.new_fd(Fd::Pty { id, master });
        if flags & 0x80000 != 0 { l.cloexec.insert(fd); }
        return fd as i64;
    }
    if p == "/dev/console" { return l.new_fd(Fd::Tty) as i64; }
    if p == "/dev/zero" || p == "/dev/urandom" || p == "/dev/random" { let fd = l.new_fd(Fd::Dev(if p == "/dev/zero" { 0 } else { 1 })); if flags & 0x80000 != 0 { l.cloexec.insert(fd); } return fd as i64; }
    if !exists {
        if flags & 0x40 == 0 { return -ENOENT; }
        let dir = p.rsplit_once('/').map(|x| x.0).unwrap_or("/");
        if !l.is_dir(if dir.is_empty() { "/" } else { dir }) { return -ENOENT; }
        l.fs.insert(p.clone(), Node::File(Vec::new()));
        l.meta.insert(p.clone(), (mode as u32 & 0o7777 & !l.umask, l.ids.2, l.ids.1));
    } else if flags & 0xC0 == 0xC0 {
        return -EEXIST;
    }
    if flags & 0x200 != 0 && flags & 3 != 0 { if let Some(Node::File(d)) = l.fs.get_mut(&p) { d.clear(); } }
    if flags & 3 != 0 && !p.starts_with("/dev/") { l.written.insert(p.clone()); }
    let fd = l.new_fd(Fd::File { path: p, pos: 0, append: flags & 0x400 != 0 });
    if flags & 0x80000 != 0 { l.cloexec.insert(fd); }
    fd as i64
}

fn path_at(e: &mut Engine, dirfd: u64, a: u64) -> String {
    let s = e.read_cstr(a, 4096);
    if s.starts_with('/') || dirfd as i32 == -100 { return s; }
    match lx(e).fds.get(&(dirfd as i32)) { Some(Fd::Dir { path, .. }) => format!("{}/{}", path, s), _ => s }
}

fn do_read(e: &mut Engine, fd: i32, buf: u64, n: u64) -> i64 {
    let l = lx(e);
    match l.fds.get_mut(&fd) {
        Some(Fd::Pipe { id, .. }) => {
            let id = *id;
            if l.pipes.get(&id).map_or(true, |b| b.is_empty()) {
                if has_end(l, id, true) { if l.pipe_nb.contains(&(id, false)) { return -EAGAIN; } l.want = Some(Cond::PipeRead(id)); return BLOCKED; }
                return 0;
            }
            let b = l.pipes.get_mut(&id).unwrap();
            let k = (n as usize).min(b.len());
            let d: Vec<u8> = b.drain(..k).collect();
            if e.mmu.write_bytes(buf, &d).is_err() { return -14; }
            k as i64
        }
        Some(Fd::Sock { id }) => { let id = *id; sock_recv(e, id, buf, n, false).0 }
        Some(Fd::Unix { id }) => { let id = *id; us_read(e, id, buf, n, false) }
        Some(Fd::Event { id }) => { let id = *id; ev_read(e, id, buf, n) }
        Some(Fd::Timer { id }) => { let id = *id; tfd_read(e, id, buf, n) }
        Some(Fd::Epoll { .. }) => -EINVAL,
        Some(Fd::Pty { id, master }) => { let (id, m) = (*id, *master); pty_read(e, id, m, buf, n) }
        Some(Fd::Dev(k)) => {
            let k = *k; let n = n.min(1 << 20) as usize;
            let mut d = vec![0u8; n];
            if k == 1 { let mut x = (crate::now_ms() * 1000.0) as u64 | 1; for c in d.iter_mut() { x ^= x << 13; x ^= x >> 7; x ^= x << 17; *c = x as u8; } }
            if e.mmu.write_bytes(buf, &d).is_err() { return -14; }
            n as i64
        }
        Some(Fd::Tty) => {
            if l.stdin.is_empty() {
                if l.stdin_eof { l.stdin_eof = false; return 0; }
                l.want = Some(Cond::Stdin);
                return BLOCKED;
            }
            let k = (n as usize).min(l.stdin.len());
            let d: Vec<u8> = l.stdin.drain(..k).collect();
            if e.mmu.write_bytes(buf, &d).is_err() { return -14; }
            k as i64
        }
        Some(Fd::File { path, pos, .. }) => {
            let (path, p) = (path.clone(), *pos);
            let d = match l.fs.get(&path) { Some(Node::File(d)) => d, _ => return -EBADF };
            let s = (p as usize).min(d.len());
            let k = (n as usize).min(d.len() - s);
            let chunk = d[s..s + k].to_vec();
            if let Some(Fd::File { pos, .. }) = l.fds.get_mut(&fd) { *pos += k as u64; }
            if e.mmu.write_bytes(buf, &chunk).is_err() { return -14; }
            k as i64
        }
        Some(Fd::Dir { .. }) => -EISDIR,
        None => -EBADF,
    }
}

fn do_write(e: &mut Engine, fd: i32, buf: u64, n: u64) -> i64 {
    let mut d = vec![0u8; n.min(1 << 24) as usize];
    if e.mmu.read_bytes(buf, &mut d).is_err() { return -14; }
    write_data(e, fd, &d)
}

fn write_data(e: &mut Engine, fd: i32, d: &[u8]) -> i64 {
    let n = d.len() as u64;
    let l = lx(e);
    match l.fds.get_mut(&fd) {
        Some(Fd::Tty) => { crate::console(&String::from_utf8_lossy(&d)); n as i64 }
        Some(Fd::Sock { id }) => { let id = *id; sock_send(l, id, d, None) }
        Some(Fd::Unix { id }) => { let id = *id; us_write(l, id, d) }
        Some(Fd::Timer { .. }) => -EINVAL,
        Some(Fd::Event { id }) => {
            if d.len() < 8 { return -EINVAL; }
            let v = u64::from_le_bytes(d[..8].try_into().unwrap());
            if let Some(x) = l.evs.get_mut(id) { x.0 = x.0.saturating_add(v); }
            8
        }
        Some(Fd::Epoll { .. }) => -EINVAL,
        Some(Fd::Pty { id, master }) => {
            let (id, m) = (*id, *master);
            if m { let sigs = pty_input(l, id, d); for (g, sig) in sigs { pty_signal(e, id, g, sig); } n as i64 } else { pty_output(l, id, d) }
        }
        Some(Fd::Dev(_)) => n as i64,
        Some(Fd::Pipe { id, .. }) => {
            let id = *id;
            if !has_end(l, id, false) { return -EPIPE; }
            l.pipes.entry(id).or_default().extend(d.iter());
            n as i64
        }
        Some(Fd::File { path, pos, append }) => {
            let (path, p, app) = (path.clone(), *pos, *append);
            if path == "/dev/null" { return n as i64; }
            let f = match l.fs.get_mut(&path) { Some(Node::File(f)) => f, _ => return -EBADF };
            let at = if app { f.len() } else { p as usize };
            if f.len() < at + d.len() { f.resize(at + d.len(), 0); }
            f[at..at + d.len()].copy_from_slice(&d);
            if let Some(Fd::File { pos, .. }) = l.fds.get_mut(&fd) { *pos = (at + d.len()) as u64; }
            n as i64
        }
        Some(Fd::Dir { .. }) => -EISDIR,
        None => -EBADF,
    }
}

/// The `syscall` instruction (rip already points after it).
pub fn syscall(e: &mut Engine) {
    let _g = crate::lock::lock();
    let r = e.cpu.regs;
    let (nr, a0, a1, a2, a3, a4) = (r[0], r[7], r[6], r[2], r[10], r[8]);
    let _ = a4;
    e.cpu.regs[1] = e.cpu.rip;
    e.cpu.regs[11] = 0x202;
    #[cfg(feature = "prof")]
    crate::console(&format!("[sys pid {} {} {:x} {:x} {:x}]\n", lx(e).pid, nr, a0, a1, a2));
    // a signal the thread can take now: its handler runs first; the call then restarts
    // (SA_RESTART, or a call that never waits) or returns EINTR
    let tid = e.cur_tid();
    let sig = { let l = lx(e); fire_timers(l); deliverable(l, l.pid, tid) };
    if sig != 0 && nr != 15 && { let l = lx(e); let pid = l.pid; act(l, pid, sig)[0] == 0 && !dfl_ignore(sig) && pid != 1 } {
        exit_proc(e, sig as i32); // a pending signal without a handler (a timer's SIGALRM): its default action
        return;
    }
    if sig != 0 && nr != 15 {
        let flags = { let l = lx(e); act(l, l.pid, sig)[1] };
        let slow = matches!(nr, 0 | 7 | 23 | 34 | 35 | 43 | 45 | 47 | 61 | 128 | 130 | 202 | 230 | 232 | 270 | 271 | 281 | 288 | 441);
        let rip = e.cpu.rip;
        if matches!(nr, 34 | 130) || (slow && flags & SA_RESTART == 0) { deliver(e, sig, (-EINTR) as u64, rip); } else { deliver(e, sig, nr, rip - 2); }
        return;
    }
    if matches!(nr, 2 | 257 | 76 | 77 | 82 | 264 | 316 | 83 | 258 | 84 | 87 | 263 | 86 | 88 | 265 | 266) || (matches!(nr, 1 | 18 | 20) && matches!(lx(e).fds.get(&(a0 as i32)), Some(Fd::File { .. }))) { lx(e).dirty = true; }
    let v: i64 = match nr {
        0 => do_read(e, a0 as i32, a1, a2),
        22 | 293 => { // pipe / pipe2
            let l = lx(e);
            let id = l.next_pipe; l.next_pipe += 1;
            l.pipes.insert(id, Default::default());
            let r = l.new_fd(Fd::Pipe { id, w: false });
            let w = l.new_fd(Fd::Pipe { id, w: true });
            if nr == 293 && a1 & 0x80000 != 0 { l.cloexec.insert(r); l.cloexec.insert(w); }
            if nr == 293 && a1 & 0x800 != 0 { l.pipe_nb.insert((id, false)); l.pipe_nb.insert((id, true)); }
            e.w32(a0, r as u32); e.w32(a0 + 4, w as u32);
            0
        }
        56 if a0 & 0x100 != 0 && a0 & 0x4000 == 0 => clone_thread(e, a0, a1, a2, a3, a4), // a thread (CLONE_VM)
        56 | 57 | 58 => fork(e),
        59 | 322 => {
            let path = if nr == 59 { e.read_cstr(a0, 4096) } else { path_at(e, a0, a1) };
            let (av, ev) = if nr == 59 { (a1, a2) } else { (a2, a3) };
            let strs = |e: &mut Engine, mut p: u64| { let mut v = Vec::new(); while p != 0 && v.len() < 4096 { let s = e.r64(p); if s == 0 { break; } v.push(e.read_cstr(s, 1 << 17)); p += 8; } v };
            let args = strs(e, av); let env = strs(e, ev);
            execve(e, &path, args, env)
        }
        61 => { if a3 != 0 { let _ = e.mmu.write_bytes(a3, &[0u8; 144]); } wait4(e, a0 as i32 as i64, a1, a2) }
        60 if is_thread(e) => { thread_exit(e); NORET }
        60 | 231 => { exit_proc(e, ((a0 & 0xFF) << 8) as i32); NORET }
        39 => lx(e).pid as i64,
        186 => cur_linux_tid(e) as i64, // gettid
        110 => { let l = lx(e); l.procs.get(&l.pid).map_or(0, |p| p.ppid) as i64 }
        62 => { // kill: a process, a process group (0 = mine, < -1), or everybody (-1)
            let pid = a0 as i32;
            if a1 > 64 { return ret(e, -EINVAL); }
            let l = lx(e);
            let me = l.pid;
            let my_g = l.procs.get(&me).map_or(me, |p| p.pgid);
            let alive = |l: &Lx, f: &dyn Fn(u32, &Proc) -> bool| -> Vec<u32> { l.procs.iter().filter(|(k, p)| p.zombie.is_none() && f(**k, p)).map(|(k, _)| *k).collect() };
            let t = if pid > 0 { alive(l, &|k, _| k == pid as u32) } else if pid == 0 { alive(l, &|_, p| p.pgid == my_g) } else if pid == -1 { alive(l, &|k, _| k != 1 && k != me) } else { alive(l, &|_, p| p.pgid == (-pid) as u32) };
            if t.is_empty() { -3 } else { let mut dead = false; for k in t { dead |= signal(e, k, a1 as u32); } if dead { NORET } else { 0 } }
        }
        1 => do_write(e, a0 as i32, a1, a2),
        2 => { let p = e.read_cstr(a0, 4096); open(e, &p, a1, a2) }
        257 => { let p = path_at(e, a0, a1); open(e, &p, a2, a3) }
        85 => { let p = e.read_cstr(a0, 4096); open(e, &p, 0x241, a1) } // creat
        3 => { let l = lx(e); l.cloexec.remove(&(a0 as i32)); if l.fds.remove(&(a0 as i32)).is_some() { 0 } else { -EBADF } }
        4 | 6 => { let p = e.read_cstr(a0, 4096); stat_path(e, &p, a1, nr == 4) }
        262 => { // newfstatat
            let p = path_at(e, a0, a1);
            if p.is_empty() && a3 & 0x1000 != 0 { fstat(e, a0 as i32, a2) } else { stat_path(e, &p, a2, a3 & 0x100 == 0) }
        }
        5 => fstat(e, a0 as i32, a1),
        8 => { // lseek
            let l = lx(e);
            let size = match l.fds.get(&(a0 as i32)) { Some(Fd::File { path, .. }) => match l.fs.get(path) { Some(Node::File(d)) => d.len() as i64, _ => 0 }, Some(Fd::Tty | Fd::Pipe { .. } | Fd::Sock { .. } | Fd::Dev(_) | Fd::Unix { .. } | Fd::Event { .. } | Fd::Epoll { .. } | Fd::Pty { .. } | Fd::Timer { .. }) => return ret(e, -ESPIPE), Some(Fd::Dir { .. }) => 0, None => return ret(e, -EBADF) };
            match l.fds.get_mut(&(a0 as i32)) {
                Some(Fd::File { pos, .. }) => {
                    let np = match a2 { 0 => a1 as i64, 1 => *pos as i64 + a1 as i64, 2 => size + a1 as i64, _ => -1 };
                    if np < 0 { -EINVAL } else { *pos = np as u64; np }
                }
                Some(Fd::Dir { pos, .. }) => { *pos = a1 as usize; a1 as i64 }
                _ => -EINVAL,
            }
        }
        9 => mmap(e, a0, a1, a2, a3, r[8] as i32, r[9]),
        26 => { sync_shared(e, a0, align(a1), false); 0 } // msync
        10 | 28 => 0, // mprotect, madvise
        11 => { sync_shared(e, a0, align(a1), true); e.mmu.drop_pages(a0, a0 + align(a1)); 0 }
        25 => -ENOMEM, // mremap: callers fall back to mmap + copy
        12 => { // brk
            let l = lx(e);
            let (cur, start) = (l.brk, l.brk_start);
            if a0 > cur && a0 < start + (1 << 30) {
                let (from, to) = (align(cur), align(a0));
                if to > from { e.mmu.add_region(from, to - from); }
                lx(e).brk = a0;
                a0 as i64
            } else if a0 >= start && a0 <= cur { lx(e).brk = a0; a0 as i64 } else { cur as i64 }
        }
        13 => { // rt_sigaction
            let sig = a0 as usize;
            if sig == 0 || sig > 64 || ((sig == 9 || sig == 19) && a1 != 0) { return ret(e, -EINVAL); }
            let pid = lx(e).pid;
            let cur = act(lx(e), pid, sig as u32);
            if a2 != 0 { for k in 0..4 { e.w64(a2 + 8 * k as u64, cur[k]); } }
            if a1 != 0 {
                let n = [e.r64(a1), e.r64(a1 + 8), e.r64(a1 + 16), e.r64(a1 + 24)];
                let l = lx(e);
                l.acts.entry(pid).or_insert_with(|| vec![[0; 4]; 65])[sig] = n;
                if n[0] == 1 || (n[0] == 0 && dfl_ignore(sig as u32)) { if let Some(p) = l.pend.get_mut(&pid) { *p &= !bit(sig as u32); } }
            }
            0
        }
        14 => { // rt_sigprocmask
            let l = lx(e);
            let cur = l.masks.get(&tid).copied().unwrap_or(0);
            let n = if a1 != 0 { Some(e.r64(a1)) } else { None };
            if a2 != 0 { e.w64(a2, cur); }
            if let Some(s) = n {
                let m = match a0 { 0 => cur | s, 1 => cur & !s, 2 => s, _ => return ret(e, -EINVAL) };
                lx(e).masks.insert(tid, m & !(bit(9) | bit(19)));
            }
            0
        }
        15 => { sigreturn(e, tid); NORET }
        127 => { let l = lx(e); let p = l.pend.get(&l.pid).copied().unwrap_or(0); e.w64(a0, p); 0 } // rt_sigpending
        130 => { // rt_sigsuspend: waits with a temporary mask until a signal is handled (then EINTR)
            let m = e.r64(a0);
            let l = lx(e);
            let cur = l.masks.get(&tid).copied().unwrap_or(0);
            l.susp.entry(tid).or_insert(cur);
            l.masks.insert(tid, m & !(bit(9) | bit(19)));
            l.want = Some(Cond::Signal);
            BLOCKED
        }
        34 => { lx(e).want = Some(Cond::Signal); BLOCKED } // pause
        128 => { // rt_sigtimedwait: takes a pending signal of the set without its handler
            let set = e.r64(a0);
            let l = lx(e);
            let pid = l.pid;
            let p = l.pend.get(&pid).copied().unwrap_or(0) & set;
            if p != 0 {
                let sig = p.trailing_zeros() + 1;
                *l.pend.get_mut(&pid).unwrap() &= !bit(sig);
                if a1 != 0 { let _ = e.mmu.write_bytes(a1, &[0u8; 128]); e.w32(a1, sig); }
                sig as i64
            } else {
                let ms = if a2 == 0 { None } else { Some(e.r64(a2) as f64 * 1000.0 + e.r64(a2 + 8) as f64 / 1e6) };
                if ms == Some(0.0) { -EAGAIN } else { e.cpu.rip -= 2; e.block_linux(Cond::SigWait(set), ms); NORET }
            }
        }
        131 => { if a1 != 0 { let _ = e.mmu.write_bytes(a1, &[0u8; 24]); e.w32(a1 + 8, 2); } 0 } // sigaltstack: none (SS_DISABLE)
        16 => ioctl(e, a0 as i32, a1, a2),
        40 => { // sendfile
            let (out, inp, offp, n) = (a0 as i32, a1 as i32, a2, a3.min(1 << 20));
            let buf = 0x7FFF_EFF0_0000u64; // scratch at the bottom of the stack region
            let _ = buf;
            let l = lx(e);
            let (path, pos) = match l.fds.get(&inp) { Some(Fd::File { path, pos, .. }) => (path.clone(), *pos), _ => return ret(e, -EINVAL) };
            let at = if offp != 0 { e.r64(offp) } else { pos };
            let d = match lx(e).fs.get(&path) { Some(Node::File(d)) => { let s = (at as usize).min(d.len()); d[s..(s + n as usize).min(d.len())].to_vec() } _ => return ret(e, -EBADF) };
            let k = write_data(e, out, &d);
            if k > 0 {
                if offp != 0 { e.w64(offp, at + k as u64); } else if let Some(Fd::File { pos, .. }) = lx(e).fds.get_mut(&inp) { *pos += k as u64; }
            }
            k
        }
        17 => { let fd = a0 as i32; let k = do_pread(e, fd, a1, a2, a3); k }
        19 | 20 => { // readv / writev
            let mut tot = 0i64;
            for k in 0..a2.min(1024) {
                let (b, n) = (e.r64(a1 + 16 * k), e.r64(a1 + 16 * k + 8));
                if n == 0 { continue; }
                let got = if nr == 19 { do_read(e, a0 as i32, b, n) } else { do_write(e, a0 as i32, b, n) };
                if got < 0 { if tot == 0 { tot = got; } break; }
                tot += got;
                if (got as u64) < n { break; }
            }
            tot
        }
        21 | 269 => { // access / faccessat
            let p = if nr == 21 { e.read_cstr(a0, 4096) } else { path_at(e, a0, a1) };
            let l = lx(e); let q = l.resolve(&p, true);
            if q == "/" || l.fs.contains_key(&q) { 0 } else { -ENOENT }
        }
        7 | 271 if poll_fds(e, a0, a1, nr, a2) => NORET,
        23 | 270 => select(e, nr, a0, a1, a2, a3, a4),
        213 | 291 => { // epoll_create / epoll_create1
            let l = lx(e);
            l.next_ev += 1; let id = l.next_ev;
            l.eps.insert(id, BTreeMap::new());
            let fd = l.new_fd(Fd::Epoll { id });
            if nr == 291 && a0 & 0x80000 != 0 { l.cloexec.insert(fd); }
            fd as i64
        }
        233 => epoll_ctl(e, a0 as i32, a1, a2 as i32, a3),
        232 | 281 | 441 => {
            let ms = if nr == 441 { if a3 == 0 { -1 } else { (e.r64(a3) * 1000 + e.r64(a3 + 8) / 1_000_000) as i64 } } else { a3 as i32 as i64 };
            epoll_wait(e, a0 as i32, a1, a2, ms)
        }
        284 | 290 => { // eventfd / eventfd2
            let l = lx(e);
            l.next_ev += 1; let id = l.next_ev;
            let fl = if nr == 290 { a1 } else { 0 };
            l.evs.insert(id, (a0 as u32 as u64, fl & 1 != 0, fl & 0x800 != 0));
            let fd = l.new_fd(Fd::Event { id });
            if fl & 0x80000 != 0 { l.cloexec.insert(fd); }
            fd as i64
        }
        41..=55 | 288 => net_call(e, nr, a0, a1, a2, a3, a4, r[9]),
        7 | 271 => { for k in 0..a1.min(64) { let ev = e.r16(a0 + 8 * k + 4); e.w16(a0 + 8 * k + 6, ev & 5); } a1 as i64 } // a huge poll: everything is ready
        24 => 0, // sched_yield
        165 | 166 => -1, // mount / umount2: EPERM, as for a program without the right to mount
        73 => 0, // flock: one machine, nobody else holds the lock
        37 => { // alarm: seconds left on the previous one
            let pid = lx(e).pid;
            let id = alarm_timer(lx(e), pid);
            let (left, _) = tm_get(lx(e), id);
            tm_set(lx(e), id, a0 as f64 * 1000.0, 0.0);
            ((left + 999.0) / 1000.0) as i64
        }
        36 | 38 => { // getitimer / setitimer: ITIMER_REAL (others are accepted and never fire)
            let (newv, old) = if nr == 36 { (0, a1) } else { (a1, a2) };
            let id = if a0 == 0 { let pid = lx(e).pid; Some(alarm_timer(lx(e), pid)) } else { None };
            let (left, every) = id.map_or((0.0, 0.0), |id| tm_get(lx(e), id));
            if old != 0 { for (k, v) in [every, left].iter().enumerate() { e.w64(old + 16 * k as u64, (*v / 1000.0) as u64); e.w64(old + 16 * k as u64 + 8, ((*v % 1000.0) * 1000.0) as u64); } }
            if newv != 0 && id.is_some() {
                let tv = |e: &mut Engine, a: u64| e.r64(a) as f64 * 1000.0 + e.r64(a + 8) as f64 / 1000.0;
                let (every, val) = (tv(e, newv), tv(e, newv + 16));
                tm_set(lx(e), id.unwrap(), val, every);
            }
            0
        }
        222 => { // timer_create: a signal (SIGALRM unless the sigevent names one) or nothing
            let (sig, notify) = if a1 == 0 { (14, 0) } else { (e.r32(a1 + 8), e.r32(a1 + 12)) };
            let l = lx(e);
            l.next_tm += 1; let id = l.next_tm;
            let real = a0 == 0 || a0 == 5 || a0 == 8;
            l.timers.insert(id, Tmr { pid: l.pid, sig: if notify == 1 { 0 } else { sig }, real, ..Default::default() });
            e.w32(a2, id); 0
        }
        223 | 286 => { // timer_settime / timerfd_settime (flags & 1 = absolute time)
            let id = if nr == 223 { Some(a0 as u32) } else { match lx(e).fds.get(&(a0 as i32)) { Some(Fd::Timer { id }) => Some(*id), _ => None } };
            match id.filter(|i| lx(e).timers.contains_key(i)) {
                None => -EINVAL,
                Some(id) => {
                    let (left, every) = tm_get(lx(e), id);
                    if a3 != 0 { put_ts(e, a3, every); put_ts(e, a3 + 16, left); }
                    let ts = |e: &mut Engine, a: u64| e.r64(a) as f64 * 1000.0 + e.r64(a + 8) as f64 / 1e6;
                    let (every, mut val) = (ts(e, a2), ts(e, a2 + 16));
                    if a1 & 1 != 0 && val > 0.0 { let real = lx(e).timers[&id].real; val = (val - if real { crate::epoch_ms() } else { crate::now_ms() }).max(0.001); }
                    if nr == 286 { if let Some(t) = lx(e).timers.get_mut(&id) { t.count = 0; } }
                    tm_set(lx(e), id, val, every);
                    0
                }
            }
        }
        224 | 287 => { // timer_gettime / timerfd_gettime
            let id = if nr == 224 { Some(a0 as u32) } else { match lx(e).fds.get(&(a0 as i32)) { Some(Fd::Timer { id }) => Some(*id), _ => None } };
            match id.filter(|i| lx(e).timers.contains_key(i)) { None => -EINVAL, Some(id) => { let (left, every) = tm_get(lx(e), id); put_ts(e, a1, every); put_ts(e, a1 + 16, left); 0 } }
        }
        225 => 0, // timer_getoverrun
        226 => { if lx(e).timers.remove(&(a0 as u32)).is_some() { 0 } else { -EINVAL } } // timer_delete
        283 => { // timerfd_create
            let l = lx(e);
            l.next_tm += 1; let id = l.next_tm;
            let real = a0 == 0 || a0 == 5 || a0 == 8;
            l.timers.insert(id, Tmr { pid: l.pid, fd: true, nb: a1 & 0x800 != 0, real, ..Default::default() });
            let fd = l.new_fd(Fd::Timer { id });
            if a1 & 0x80000 != 0 { l.cloexec.insert(fd); }
            fd as i64
        }
        32 => { let l = lx(e); match l.fds.get(&(a0 as i32)).cloned() { Some(f) => l.new_fd(f) as i64, None => -EBADF } }
        33 | 292 => { // dup2 / dup3
            let l = lx(e);
            let Some(f) = l.fds.get(&(a0 as i32)).cloned() else { return ret(e, -EBADF) };
            if a0 != a1 { l.fds.insert(a1 as i32, f); if nr == 292 && a2 & 0x80000 != 0 { l.cloexec.insert(a1 as i32); } else { l.cloexec.remove(&(a1 as i32)); } }
            a1 as i64
        }
        35 => { // nanosleep
            let ms = e.r64(a0) as f64 * 1000.0 + e.r64(a0 + 8) as f64 / 1e6;
            if ms <= 0.0 { 0 } else { lx(e).want = Some(Cond::Until); e.cpu.rip -= 2; e.block_linux(Cond::Until, Some(ms)); lx(e).want = None; NORET }
        }
        102 => lx(e).ids.0 as i64, // getuid
        107 => lx(e).ids.2 as i64, // geteuid
        104 | 108 => lx(e).ids.1 as i64, // getgid / getegid
        105 => { let l = lx(e); if l.ids.2 == 0 { l.ids.0 = a0 as u32; } l.ids.2 = a0 as u32; 0 } // setuid
        113 | 117 => { let l = lx(e); if a0 as i32 != -1 { l.ids.0 = a0 as u32; } if a1 as i32 != -1 { l.ids.2 = a1 as u32; } 0 } // setreuid / setresuid
        106 | 114 | 119 => { let l = lx(e); let v = if nr == 106 { a0 } else if a1 as i32 != -1 { a1 } else { a0 }; if v as i32 != -1 { l.ids.1 = v as u32; } 0 } // setgid / setregid / setresgid
        118 | 120 => { let i = lx(e).ids; let v = if nr == 118 { [i.0, i.2, i.2] } else { [i.1; 3] }; for (p, x) in [a0, a1, a2].into_iter().zip(v) { if p != 0 { e.w32(p, x); } } 0 }
        116 | 115 => 0, // setgroups / getgroups (none)
        122 | 123 => lx(e).ids.0 as i64, // setfsuid / setfsgid
        109 => { // setpgid
            let l = lx(e);
            let p = if a0 == 0 { l.pid } else { a0 as u32 };
            let g = if a1 == 0 { p } else { a1 as u32 };
            match l.procs.get_mut(&p) { Some(x) => { x.pgid = g; 0 } None => -3 }
        }
        112 => { let l = lx(e); let me = l.pid; if let Some(x) = l.procs.get_mut(&me) { x.pgid = me; x.ctty = 0; } me as i64 } // setsid
        121 => { let l = lx(e); let p = if a0 == 0 { l.pid } else { a0 as u32 }; l.procs.get(&p).map_or(-3, |x| x.pgid as i64) } // getpgid
        111 => { let l = lx(e); l.procs.get(&l.pid).map_or(l.pid, |x| x.pgid) as i64 } // getpgrp
        124 => 1, // getsid
        435 => -ENOSYS, // clone3: callers fall back to clone
        247 => -ECHILD, // waitid
        200 | 234 => { // tkill / tgkill: the signal goes to the thread's process
            let (t, sig) = if nr == 234 { (a0 as u32, a2 as u32) } else { (a1 as u32, a1 as u32) };
            let sig = if nr == 200 { a1 as u32 } else { sig };
            let pid = if nr == 234 { t } else { tid_pid(e, a0 as u32) };
            if sig > 64 { -EINVAL } else if !lx(e).procs.get(&pid).map_or(false, |p| p.zombie.is_none()) { -3 } else if signal(e, pid, sig) { NORET } else { 0 }
        }
        63 => { // uname
            let mut b = [0u8; 390];
            for (k, s) in ["Linux", "inksoc", "6.1.0-inksoc", "#1 SMP InkSoC", "x86_64", "(none)"].iter().enumerate() { b[65 * k..65 * k + s.len()].copy_from_slice(s.as_bytes()); }
            if e.mmu.write_bytes(a0, &b).is_err() { -14 } else { 0 }
        }
        72 if (a1 == 3 || a1 == 4) && matches!(lx(e).fds.get(&(a0 as i32)), Some(Fd::Sock { .. } | Fd::Unix { .. } | Fd::Event { .. } | Fd::Pty { .. } | Fd::Pipe { .. })) => { // F_GETFL / F_SETFL: O_NONBLOCK
            let l = lx(e);
            let f = l.fds.get(&(a0 as i32)).cloned().unwrap();
            let acc = match f { Fd::Pipe { w, .. } => if w { 1 } else { 0 }, _ => 2 };
            if a1 == 4 { set_nb(l, &f, a2 & 0x800 != 0); 0 } else { acc | if get_nb(l, &f) { 0x800 } else { 0 } }
        }
        72 => { // fcntl
            let l = lx(e);
            let fd = a0 as i32;
            match (a1, l.fds.get(&fd).cloned()) {
                (_, None) => -EBADF,
                (0 | 1030, Some(f)) => { let n = l.new_fd_from(a2 as i32, f); if a1 == 1030 { l.cloexec.insert(n); } n as i64 }
                (1, _) => l.cloexec.contains(&fd) as i64,
                (2, _) => { if a2 & 1 != 0 { l.cloexec.insert(fd); } else { l.cloexec.remove(&fd); } 0 }
                (3, Some(Fd::Pipe { w, .. })) => w as i64,
                (3, _) => 2,
                _ => 0,
            }
        }
        74 | 75 | 162 => 0, // fsync / fdatasync / sync
        157 => { if a0 == 16 { let _ = e.mmu.write_bytes(a1, b"inksoc\0"); } 0 } // prctl: names and flags accepted (PR_GET_NAME answers)
        285 => { // fallocate: grows the file (mode 0)
            let l = lx(e);
            if let Some(Fd::File { path, .. }) = l.fds.get(&(a0 as i32)) { let p = path.clone(); if a1 & 1 == 0 { if let Some(Node::File(d)) = l.fs.get_mut(&p) { let end = (a2 + a3) as usize; if d.len() < end { d.resize(end, 0); } } } 0 } else { -ESPIPE }
        }
        77 => { let l = lx(e); if let Some(Fd::File { path, .. }) = l.fds.get(&(a0 as i32)) { let p = path.clone(); if let Some(Node::File(d)) = l.fs.get_mut(&p) { d.resize(a1 as usize, 0); } } 0 }
        76 => { let p = e.read_cstr(a0, 4096); let l = lx(e); let q = l.resolve(&p, true); if let Some(Node::File(d)) = l.fs.get_mut(&q) { d.resize(a1 as usize, 0); 0 } else { -ENOENT } }
        78 | 217 => getdents(e, a0 as i32, a1, a2),
        79 => { // getcwd
            let c = lx(e).cwd.clone();
            if c.len() + 1 > a1 as usize { -ERANGE } else { let _ = e.mmu.write_bytes(a0, c.as_bytes()); e.w8(a0 + c.len() as u64, 0); c.len() as i64 + 1 }
        }
        80 => { let p = e.read_cstr(a0, 4096); let l = lx(e); let q = l.resolve(&p, true); if l.is_dir(&q) { l.cwd = q; 0 } else if l.fs.contains_key(&q) { -ENOTDIR } else { -ENOENT } }
        81 => { let l = lx(e); match l.fds.get(&(a0 as i32)) { Some(Fd::Dir { path, .. }) => { l.cwd = path.clone(); 0 } _ => -ENOTDIR } }
        82 | 264 | 316 => { // rename / renameat / renameat2
            let (f, t) = if nr == 82 { (e.read_cstr(a0, 4096), e.read_cstr(a1, 4096)) } else { (path_at(e, a0, a1), path_at(e, a2, a3)) };
            let l = lx(e); let (f, t) = (l.resolve(&f, false), l.resolve(&t, false));
            let keys: Vec<String> = l.fs.keys().filter(|k| **k == f || k.starts_with(&format!("{}/", f))).cloned().collect();
            if keys.is_empty() { -ENOENT } else { for k in keys { let n = l.fs.remove(&k).unwrap(); let nk = format!("{}{}", t, &k[f.len()..]); l.written.remove(&k); if let Some(m) = l.meta.remove(&k) { l.meta.insert(nk.clone(), m); } if matches!(n, Node::File(_)) { l.written.insert(nk.clone()); } l.fs.insert(nk, n); } 0 }
        }
        83 | 258 => { // mkdir / mkdirat
            let (p, m) = if nr == 83 { (e.read_cstr(a0, 4096), a1) } else { (path_at(e, a0, a1), a2) };
            let l = lx(e); let q = l.resolve(&p, false);
            if l.fs.contains_key(&q) { -EEXIST } else { l.meta.insert(q.clone(), (m as u32 & 0o7777 & !l.umask, l.ids.2, l.ids.1)); l.fs.insert(q, Node::Dir); 0 }
        }
        84 | 87 | 263 => { // rmdir / unlink / unlinkat
            let (p, dir) = if nr == 263 { (path_at(e, a0, a1), a2 & 0x200 != 0) } else { (e.read_cstr(a0, 4096), nr == 84) };
            let l = lx(e); let q = l.resolve(&p, false);
            match l.fs.get(&q) {
                None => -ENOENT,
                Some(Node::Dir) if !dir => -EISDIR,
                Some(Node::Dir) if !l.children(&q).is_empty() => -ENOTEMPTY,
                Some(Node::Dir) => { l.fs.remove(&q); 0 }
                Some(_) if dir => -ENOTDIR,
                Some(_) => { l.fs.remove(&q); l.written.remove(&q); l.us_names.remove(&q); l.meta.remove(&q); 0 }
            }
        }
        86 | 88 | 265 | 266 => { // link / symlink / linkat / symlinkat
            let (t, n) = match nr { 88 => (e.read_cstr(a0, 4096), e.read_cstr(a1, 4096)), 266 => (e.read_cstr(a0, 4096), path_at(e, a1, a2)), 86 => (e.read_cstr(a0, 4096), e.read_cstr(a1, 4096)), _ => (path_at(e, a0, a1), path_at(e, a2, a3)) };
            let l = lx(e); let q = l.resolve(&n, false);
            if l.fs.contains_key(&q) { -EEXIST } else if nr == 88 || nr == 266 { l.fs.insert(q, Node::Link(t)); 0 } else {
                let src = l.resolve(&t, true);
                match l.fs.get(&src) { Some(Node::File(d)) => { let d = d.clone(); l.fs.insert(q, Node::File(d)); 0 } _ => -ENOENT }
            }
        }
        89 | 267 => { // readlink / readlinkat
            let (p, b, n) = if nr == 89 { (e.read_cstr(a0, 4096), a1, a2) } else { (path_at(e, a0, a1), a2, a3) };
            if let Some(k) = p.strip_prefix("/proc/self/fd/").and_then(|k| k.parse::<i32>().ok()) { // what a descriptor is (ttyname, ls -l)
                let t = match lx(e).fds.get(&k) {
                    None => return ret(e, -EBADF),
                    Some(Fd::Tty) => "/dev/console".to_string(),
                    Some(Fd::Pty { id, master }) => if *master { "/dev/ptmx".into() } else { format!("/dev/pts/{}", id) },
                    Some(Fd::File { path, .. }) | Some(Fd::Dir { path, .. }) => path.clone(),
                    Some(Fd::Pipe { id, .. }) => format!("pipe:[{}]", id),
                    Some(Fd::Sock { id }) | Some(Fd::Unix { id }) => format!("socket:[{}]", id),
                    Some(Fd::Dev(k)) => if *k == 0 { "/dev/zero".into() } else { "/dev/urandom".into() },
                    Some(Fd::Event { .. }) => "anon_inode:[eventfd]".into(),
                    Some(Fd::Timer { .. }) => "anon_inode:[timerfd]".into(),
                    Some(Fd::Epoll { .. }) => "anon_inode:[eventpoll]".into(),
                };
                let k = t.len().min(n as usize); let _ = e.mmu.write_bytes(b, &t.as_bytes()[..k]); return ret(e, k as i64);
            }
            let l = lx(e); let q = l.resolve(&p, false);
            match l.fs.get(&q) { Some(Node::Link(t)) => { let t = t.clone(); let k = t.len().min(n as usize); let _ = e.mmu.write_bytes(b, &t.as_bytes()[..k]); k as i64 } Some(_) => -EINVAL, None => -ENOENT }
        }
        90 | 91 | 268 | 92 | 93 | 94 | 260 => { // chmod, fchmod, fchmodat, chown, fchown, lchown, fchownat
            let p = match nr {
                91 | 93 => match lx(e).fds.get(&(a0 as i32)) { Some(Fd::File { path, .. }) | Some(Fd::Dir { path, .. }) => path.clone(), Some(_) => return ret(e, 0), None => return ret(e, -EBADF) },
                268 | 260 => path_at(e, a0, a1),
                _ => e.read_cstr(a0, 4096),
            };
            let l = lx(e);
            let q = l.resolve(&p, nr != 94 && !(nr == 260 && a4 & 0x100 != 0));
            let dir = l.is_dir(&q);
            if q != "/" && !l.fs.contains_key(&q) { return ret(e, -ENOENT); }
            let mut m = perm(l, &q, dir);
            let me = l.ids.2;
            if matches!(nr, 90 | 91 | 268) {
                if me != 0 && me != m.1 { return ret(e, -1); } // EPERM
                m.0 = (if nr == 268 { a2 } else { a1 }) as u32 & 0o7777;
            } else {
                let (u, g) = if nr == 260 { (a2, a3) } else { (a1, a2) };
                if me != 0 && (u as i32 != -1 && u as u32 != m.1) { return ret(e, -1); }
                if u as i32 != -1 { m.1 = u as u32; }
                if g as i32 != -1 { m.2 = g as u32; }
            }
            l.meta.insert(q, m);
            0
        }
        95 => { let l = lx(e); let o = l.umask; l.umask = a0 as u32 & 0o777; o as i64 } // umask
        280 | 235 | 132 => { // utimensat, utimes, utime: times are not kept, but the file must exist (touch creates it then)
            let p = if nr == 280 { if a1 == 0 { return ret(e, 0); } path_at(e, a0, a1) } else { e.read_cstr(a0, 4096) };
            let l = lx(e); let q = l.resolve(&p, true);
            if q == "/" || l.fs.contains_key(&q) { 0 } else { -ENOENT }
        }
        96 => { // gettimeofday
            let ms = crate::epoch_ms();
            if a0 != 0 { e.w64(a0, (ms / 1000.0) as u64); e.w64(a0 + 8, ((ms % 1000.0) * 1000.0) as u64); }
            0
        }
        97 | 302 => { if nr == 302 && a3 != 0 { e.w64(a3, u64::MAX); e.w64(a3 + 8, u64::MAX); } 0 } // getrlimit / prlimit64
        98 => { let _ = e.mmu.write_bytes(a1, &[0u8; 144]); 0 } // getrusage
        99 => { // sysinfo: the memory programs see (target) and how much of it is in use
            let total = e.mmu.sh.target_bytes; let used = (e.mmu.resident() as u64) * 4096;
            let up = (unsafe { crate::host_now_ms() } / 1000.0) as u64;
            let mut b = [0u8; 112];
            b[0..8].copy_from_slice(&up.to_le_bytes());
            b[32..40].copy_from_slice(&total.to_le_bytes()); b[40..48].copy_from_slice(&total.saturating_sub(used).to_le_bytes());
            b[80..82].copy_from_slice(&(lx(e).procs.len() as u16).to_le_bytes());
            b[104..108].copy_from_slice(&1u32.to_le_bytes()); // mem_unit
            let _ = e.mmu.write_bytes(a0, &b); 0
        }
        100 => { let _ = e.mmu.write_bytes(a0, &[0u8; 32]); 0 } // times
        137 | 138 => { let mut b = [0u8; 120]; b[0..8].copy_from_slice(&0xEF53u64.to_le_bytes()); b[8..16].copy_from_slice(&4096u64.to_le_bytes()); b[16..24].copy_from_slice(&(1u64 << 20).to_le_bytes()); b[24..32].copy_from_slice(&(1u64 << 19).to_le_bytes()); b[32..40].copy_from_slice(&(1u64 << 19).to_le_bytes()); b[56..64].copy_from_slice(&255u64.to_le_bytes()); let _ = e.mmu.write_bytes(a1, &b); 0 } // statfs
        158 => { // arch_prctl
            match a0 { 0x1002 => { e.cpu.fs_base = a1; 0 } 0x1003 => { e.w64(a1, e.cpu.fs_base); 0 } 0x1001 => { e.cpu.gs_base = a1; 0 } _ => -EINVAL }
        }
        201 => { let t = (crate::epoch_ms() / 1000.0) as i64; if a0 != 0 { e.w64(a0, t as u64); } t }
        202 => futex(e, a0, a1, a2, a3, r[8], r[9]),
        203 | 204 => { if nr == 204 { e.w64(a2, 1); } if nr == 204 { 8 } else { 0 } } // sched_setaffinity / getaffinity
        218 => { let t = cur_linux_tid(e); lx(e).tids.insert(tid, (t, a0)); t as i64 } // set_tid_address
        228 => { // clock_gettime
            let ms = if a0 == 0 || a0 == 5 || a0 == 8 { crate::epoch_ms() } else { crate::now_ms() };
            e.w64(a1, (ms / 1000.0) as u64); e.w64(a1 + 8, ((ms % 1000.0) * 1e6) as u64);
            0
        }
        229 => { e.w64(a1, 0); e.w64(a1 + 8, 1); 0 }
        230 => { // clock_nanosleep (relative)
            let ms = e.r64(a2) as f64 * 1000.0 + e.r64(a2 + 8) as f64 / 1e6;
            if ms <= 0.0 || a1 & 1 != 0 { 0 } else { e.cpu.rip -= 2; e.block_linux(Cond::Until, Some(ms)); NORET }
        }
        273 | 334 => 0, // set_robust_list / rseq
        318 => { // getrandom
            let mut s = (crate::now_ms() * 1000.0) as u64 | 1;
            for k in 0..a1.min(1 << 20) { s ^= s << 13; s ^= s >> 7; s ^= s << 17; e.w8(a0 + k, s as u8); }
            a1 as i64
        }
        332 => { // statx
            let p = path_at(e, a0, a1);
            let tmp = 0u64;
            let _ = tmp;
            statx(e, &p, a0 as i32, a2, r[8])
        }
        _ => {
            if lx(e).unknown.insert(nr) { crate::console(&format!("[inksoc] Linux system call {} is not available yet\n", nr)); }
            -ENOSYS
        }
    };
    if v == BLOCKED {
        let c = lx(e).want.take().unwrap_or(Cond::Until);
        e.cpu.regs[0] = nr; // runs again from the start when woken
        e.cpu.rip -= 2;
        e.block_linux(c, None);
        return;
    }
    lx(e).want = None;
    if v == NORET { return; }
    // writing to a pipe or socket nobody reads: SIGPIPE (MSG_NOSIGNAL for send calls)
    if v == -EPIPE && matches!(nr, 1 | 18 | 20 | 40 | 44 | 46) && !(nr == 44 && a3 & 0x4000 != 0) && !(nr == 46 && a2 & 0x4000 != 0) {
        let pid = lx(e).pid;
        if signal(e, pid, 13) { return; }
    }
    ret(e, v);
    let sig = { let l = lx(e); deliverable(l, l.pid, tid) };
    if sig != 0 { let rip = e.cpu.rip; deliver(e, sig, v as u64, rip); }
}

impl Lx {
    fn new_fd_from(&mut self, min: i32, f: Fd) -> i32 {
        let mut n = min.max(0);
        while self.fds.contains_key(&n) { n += 1; }
        self.fds.insert(n, f);
        n
    }
}

fn do_pread(e: &mut Engine, fd: i32, buf: u64, n: u64, off: u64) -> i64 {
    let l = lx(e);
    let path = match l.fds.get(&fd) { Some(Fd::File { path, .. }) => path.clone(), _ => return -ESPIPE };
    let d = match l.fs.get(&path) { Some(Node::File(d)) => d, _ => return -EBADF };
    let s = (off as usize).min(d.len());
    let k = (n as usize).min(d.len() - s);
    let chunk = d[s..s + k].to_vec();
    if e.mmu.write_bytes(buf, &chunk).is_err() { return -14; }
    k as i64
}

fn fstat(e: &mut Engine, fd: i32, a: u64) -> i64 {
    let l = lx(e);
    match l.fds.get(&fd) {
        Some(Fd::Pipe { id, .. }) => { let id = *id as u64; stat_buf(e, a, 0o010600, 0, 1000 + id) }
        Some(Fd::Tty) => stat_buf(e, a, 0o020620, 0, 5),
        Some(Fd::Pty { id, .. }) => { let id = *id as u64; stat_buf(e, a, 0o020620, 0, 8000 + id) }
        Some(Fd::Sock { id }) | Some(Fd::Unix { id }) => { let id = *id as u64; stat_buf(e, a, 0o140777, 0, 5000 + id) }
        Some(Fd::Event { id }) | Some(Fd::Epoll { id }) | Some(Fd::Timer { id }) => { let id = *id as u64; stat_buf(e, a, 0o600, 0, 9000 + id) }
        Some(Fd::Dev(k)) => { let k = *k as u64; stat_buf(e, a, 0o020666, 0, 7 + k) }
        Some(Fd::File { path, .. }) | Some(Fd::Dir { path, .. }) => { let p = path.clone(); stat_path(e, &p, a, true) }
        None => -EBADF,
    }
}

fn statx(e: &mut Engine, path: &str, dirfd: i32, flags: u64, buf: u64) -> i64 {
    let mut st = [0u8; 144];
    let tmp = 0x7FFF_EFFF_F000u64; // scratch inside the stack region
    let _ = st;
    let r = if path.is_empty() && flags & 0x1000 != 0 { fstat(e, dirfd, tmp) } else { stat_path(e, path, tmp, flags & 0x100 == 0) };
    if r < 0 { return r; }
    let _ = e.mmu.read_bytes(tmp, &mut st);
    let g = |o: usize| u64::from_le_bytes(st[o..o + 8].try_into().unwrap());
    let mut b = [0u8; 256];
    b[0..4].copy_from_slice(&0x7FFu32.to_le_bytes());
    b[4..8].copy_from_slice(&4096u32.to_le_bytes());
    b[16..20].copy_from_slice(&1u32.to_le_bytes());
    b[28..30].copy_from_slice(&(u32::from_le_bytes(st[24..28].try_into().unwrap()) as u16).to_le_bytes());
    b[20..28].copy_from_slice(&st[28..36]); // uid, gid
    b[32..40].copy_from_slice(&g(8).to_le_bytes());
    b[40..48].copy_from_slice(&g(48).to_le_bytes());
    b[48..56].copy_from_slice(&g(64).to_le_bytes());
    for o in [64, 80, 96, 112] { b[o..o + 8].copy_from_slice(&g(72).to_le_bytes()); }
    if e.mmu.write_bytes(buf, &b).is_err() { -14 } else { 0 }
}

fn ioctl(e: &mut Engine, fd: i32, req: u64, a: u64) -> i64 {
    let tty = matches!(lx(e).fds.get(&fd), Some(Fd::Tty));
    if !lx(e).fds.contains_key(&fd) { return -EBADF; }
    if req == 0x5421 || req == 0x541B { // FIONBIO / FIONREAD
        let l = lx(e);
        let f = l.fds.get(&fd).cloned().unwrap();
        if req == 0x5421 { let on = e.r32(a) != 0; set_nb(lx(e), &f, on); return 0; }
        let n = match &f {
            Fd::Tty => l.stdin.len(),
            Fd::Pipe { id, .. } => l.pipes.get(id).map_or(0, |b| b.len()),
            Fd::Sock { id } => l.socks.get(id).map_or(0, |s| s.rx.len()),
            Fd::Unix { id } => match l.us.get(id).map(|u| &u.st) { Some(UsSt::Conn { rx, .. }) => l.pipes.get(rx).map_or(0, |b| b.len()), _ => 0 },
            Fd::Pty { id, master } => l.ptys.get(id).map_or(0, |t| if *master { t.out.len() } else { t.inq.len() }),
            _ => 0,
        };
        e.w32(a, n as u32);
        return 0;
    }
    if let Some(Fd::Pty { id, master }) = lx(e).fds.get(&fd).cloned() { return pty_ioctl(e, id, master, req, a); }
    if !tty { return -ENOTTY; }
    match req {
        0x5401 => { // TCGETS: what the program set last (a cooked terminal at first)
            let t = lx(e).termios.clone().unwrap_or_else(|| {
                let mut t = vec![0u8; 36];
                t[0..4].copy_from_slice(&0x500u32.to_le_bytes()); // ICRNL | IXON
                t[4..8].copy_from_slice(&0x5u32.to_le_bytes()); // OPOST | ONLCR
                t[8..12].copy_from_slice(&0xBFu32.to_le_bytes()); // CS8 | CREAD | B38400
                t[12..16].copy_from_slice(&0x8A3Bu32.to_le_bytes()); // ISIG | ICANON | ECHO...
                t[17..28].copy_from_slice(&[3, 0x1C, 0x7F, 0x15, 4, 0, 1, 0, 0x11, 0x13, 0x1A]); // ^C ^\ DEL ^U ^D, VMIN 1, ^Q ^S ^Z
                t
            });
            let lf = lx(e).lflag; e.w32(a + 12, lf);
            if e.mmu.write_bytes(a, &t[..12]).is_err() { -14 } else { let _ = e.mmu.write_bytes(a + 16, &t[16..]); 0 }
        }
        0x5402..=0x5404 => {
            let mut t = vec![0u8; 36];
            let _ = e.mmu.read_bytes(a, &mut t);
            let l = lx(e);
            l.lflag = u32::from_le_bytes(t[12..16].try_into().unwrap());
            l.termios = Some(t);
            0
        }
        0x5413 => { let (rows, cols) = lx(e).winsize; let w: [u16; 4] = [rows, cols, 0, 0]; for (k, v) in w.iter().enumerate() { e.w16(a + 2 * k as u64, *v); } 0 }
        0x5414 | 0x540E | 0x5422 | 0x540B | 0x5409 | 0x540A => 0, // TIOCSWINSZ, TIOCSCTTY, TIOCNOTTY, TCFLSH, TCSBRK, TCXONC
        0x5411 => { e.w32(a, 0); 0 } // TIOCOUTQ
        0x540F => { let l = lx(e); let g = if l.fg != 0 { l.fg } else { l.procs.get(&l.pid).map_or(l.pid, |p| p.pgid) }; e.w32(a, g); 0 } // TIOCGPGRP
        0x5410 => { let g = e.r32(a); lx(e).fg = g; 0 } // TIOCSPGRP
        _ => -EINVAL,
    }
}

fn getdents(e: &mut Engine, fd: i32, buf: u64, n: u64) -> i64 {
    let l = lx(e);
    let (path, pos) = match l.fds.get(&fd) { Some(Fd::Dir { path, pos }) => (path.clone(), *pos), Some(_) => return -ENOTDIR, None => return -EBADF };
    let mut names = vec![".".to_string(), "..".to_string()];
    names.extend(l.children(&path));
    let mut out = Vec::new();
    let mut k = pos;
    while k < names.len() {
        let nm = &names[k];
        let full = if path == "/" { format!("/{}", nm) } else { format!("{}/{}", path, nm) };
        let ty = match l.fs.get(&full) { Some(Node::Dir) => 4u8, Some(Node::Link(_)) => 10, Some(Node::File(_)) => 8, None => 4 };
        let rec = (19 + nm.len() + 1 + 7) & !7;
        if out.len() + rec > n as usize { break; }
        let ino = full.bytes().fold(1469598103934665603u64, |h, c| (h ^ c as u64).wrapping_mul(1099511628211));
        out.extend_from_slice(&ino.to_le_bytes());
        out.extend_from_slice(&((k + 1) as u64).to_le_bytes());
        out.extend_from_slice(&(rec as u16).to_le_bytes());
        out.push(ty);
        out.extend_from_slice(nm.as_bytes());
        out.resize(out.len() + rec - 19 - nm.len(), 0);
        k += 1;
    }
    if let Some(Fd::Dir { pos, .. }) = l.fds.get_mut(&fd) { *pos = k; }
    if out.is_empty() && k < names.len() { return -EINVAL; }
    if e.mmu.write_bytes(buf, &out).is_err() { return -14; }
    out.len() as i64
}

fn mmap(e: &mut Engine, addr: u64, len: u64, _prot: u64, flags: u64, fd: i32, off: u64) -> i64 {
    let len = align(len.max(1));
    let a = if flags & 0x10 != 0 { // MAP_FIXED
        e.mmu.drop_pages(addr, addr + len);
        let mut p = addr & !(PAGE - 1);
        while p < addr + len { if e.mmu.region_of(p).is_none() { e.mmu.add_region(p, PAGE); } p += PAGE; }
        addr
    } else {
        if e.mmu.sh.committed + len > e.mmu.sh.target_bytes { return -ENOMEM; }
        let l = lx(e);
        let a = l.mmap_next;
        l.mmap_next += len + PAGE;
        e.mmu.add_region(a, len);
        a
    };
    if flags & 0x20 == 0 && fd >= 0 { // file mapping: copy the bytes in
        let l = lx(e);
        let path = match l.fds.get(&fd) { Some(Fd::File { path, .. }) => path.clone(), _ => return -EBADF };
        let d = match l.fs.get(&path) { Some(Node::File(d)) => d, _ => return -EBADF };
        let s = (off as usize).min(d.len());
        let k = (len as usize).min(d.len() - s);
        let chunk = d[s..s + k].to_vec();
        if flags & 1 != 0 { let pid = l.pid; l.shared.push((pid, a, len, path, off)); } // MAP_SHARED
        if e.mmu.write_bytes(a, &chunk).is_err() { return -ENOMEM; }
    }
    a as i64
}

/// Copies what the process wrote into its MAP_SHARED file mappings in [a, a+n) back to the files
/// (only up to each file's length, as on Linux); `unmap` also forgets those ranges.
fn sync_shared(e: &mut Engine, a: u64, n: u64, unmap: bool) {
    let pid = lx(e).pid;
    let all = std::mem::take(&mut lx(e).shared);
    let mut keep = Vec::new();
    for (p, ma, ml, path, off) in all {
        let (s, t) = (a.max(ma), (a + n).min(ma + ml));
        if p != pid || s >= t { keep.push((p, ma, ml, path, off)); continue; }
        let flen = match lx(e).fs.get(&path) { Some(Node::File(d)) => d.len() as u64, _ => 0 };
        let (fs, ft) = (off + (s - ma), (off + (t - ma)).min(flen));
        if fs < ft {
            let mut buf = vec![0u8; (ft - fs) as usize];
            if e.mmu.read_bytes(s, &mut buf).is_ok() {
                let l = lx(e);
                if let Some(Node::File(d)) = l.fs.get_mut(&path) { d[fs as usize..ft as usize].copy_from_slice(&buf); l.written.insert(path.clone()); }
            }
        }
        if !unmap { keep.push((p, ma, ml, path, off)); continue; }
        if ma < s { keep.push((p, ma, s - ma, path.clone(), off)); }
        if t < ma + ml { keep.push((p, t, ma + ml - t, path, off + (t - ma))); }
    }
    lx(e).shared.extend(keep);
}

/// Text typed in the terminal, for programs reading stdin.
#[no_mangle]
pub extern "C" fn linux_stdin(n: u32) {
    let e = crate::eng();
    let b = unsafe { crate::take_buf() };
    if let Some(l) = e.s.lx.as_mut() { l.stdin.extend_from_slice(&b[..(n as usize).min(b.len())]); }
}

/// Files the Linux program created or changed: same download path as Windows.
pub fn written(e: &Engine) -> Vec<(String, Vec<u8>)> {
    let Some(l) = e.s.lx.as_ref() else { return Vec::new() };
    l.written.iter().filter_map(|p| match l.fs.get(p) { Some(Node::File(d)) => Some((p.trim_start_matches("/root/").to_string(), d.clone())), _ => None }).collect()
}

/// Is there an open read end (`w` false) or write end (`w` true) of pipe `id` in any process?
fn has_end(l: &Lx, id: u32, w: bool) -> bool {
    held(l, &|f| matches!(f, Fd::Pipe { id: i, w: x } if *i == id && *x == w))
}

/// Does any process hold a descriptor like this?
fn held(l: &Lx, is: &dyn Fn(&Fd) -> bool) -> bool {
    l.fds.values().any(|f| is(f)) || l.procs.values().any(|p| p.saved.as_ref().map_or(false, |s| s.fds.values().any(|f| is(f))))
}
/// Is local socket `id` still open: held by a process, or a connection waiting in a listener's queue?
fn us_held(l: &Lx, id: u32) -> bool {
    held(l, &|f| matches!(f, Fd::Unix { id: i } if *i == id)) || l.us.values().any(|u| matches!(&u.st, UsSt::Listen(q) if q.contains(&id)))
}

/// (readable, writable, hung up) for poll, select and epoll.
fn fd_state(l: &Lx, f: &Fd) -> (bool, bool, bool) {
    match f {
        Fd::Tty => (!l.stdin.is_empty() || l.stdin_eof, true, false),
        Fd::Pipe { id, w: false } => { let open = has_end(l, *id, true); (l.pipes.get(id).map_or(false, |b| !b.is_empty()) || !open, false, !open) }
        Fd::Pipe { id, w: true } => { let open = has_end(l, *id, false); (false, open, !open) }
        Fd::Sock { id } => match l.socks.get(id) {
            Some(s) => (!s.rx.is_empty() || !s.dgrams.is_empty() || s.closed || s.failed, s.udp || (s.open && !s.closed), s.closed || s.failed),
            None => (true, false, true),
        },
        Fd::Unix { id } => match l.us.get(id).map(|u| &u.st) {
            Some(UsSt::Listen(q)) => (!q.is_empty(), false, false),
            Some(UsSt::Conn { rx, peer, .. }) => {
                let alive = us_held(l, *peer);
                let eof = !alive || l.us.get(peer).map_or(true, |p| p.shut_wr);
                (l.pipes.get(rx).map_or(false, |b| !b.is_empty()) || eof, alive, !alive)
            }
            _ => (false, false, true),
        },
        Fd::Event { id } => (l.evs.get(id).map_or(false, |v| v.0 > 0), true, false),
        Fd::Timer { id } => (l.timers.get(id).map_or(false, |t| t.count > 0), false, false),
        Fd::Pty { id, master } => {
            let other = pty_held(l, *id, !*master) || (*master && !l.ptys.get(id).map_or(false, |t| t.opened));
            match l.ptys.get(id) {
                Some(t) if *master => (!t.out.is_empty() || !other, true, !other),
                Some(t) => (!t.inq.is_empty() || t.eofs > 0 || !other, other, !other),
                None => (true, false, true),
            }
        }
        _ => (true, true, false),
    }
}

/// Can a Linux thread waiting on `c` go on?
pub fn ready(e: &mut Engine, c: &Cond, pid: u32, tid: u32) -> bool {
    let l = lx(e);
    fire_timers(l);
    if deliverable(l, pid, tid) != 0 { return true; } // a signal to handle: the call returns or restarts
    match c {
        Cond::PipeRead(id) => l.pipes.get(id).map_or(true, |b| !b.is_empty()) || !has_end(l, *id, true),
        Cond::Child { parent, pid } => {
            let kids: Vec<(&u32, &Proc)> = l.procs.iter().filter(|(k, p)| p.ppid == *parent && (*pid <= 0 || **k as i64 == *pid)).collect();
            kids.is_empty() || kids.iter().any(|(_, p)| p.zombie.is_some())
        }
        Cond::Stdin => !l.stdin.is_empty() || l.stdin_eof,
        Cond::Until => false,
        Cond::Socks(ids) => ids.iter().any(|i| l.socks.get(i).map_or(true, |s| !s.rx.is_empty() || !s.dgrams.is_empty() || s.closed)),
        Cond::Fds(v) => v.iter().any(|(f, m)| { let (r, w, h) = fd_state(l, f); (r && m & 0x41 != 0) || (w && m & 0x104 != 0) || h }),
        Cond::Futex { .. } | Cond::Signal => false,
        Cond::SigWait(set) => l.pend.get(&pid).map_or(false, |p| p & set != 0),
    }
}

/// What a Linux call returns when its wait times out.
pub fn timed_out(c: &Cond) -> u64 {
    match c { Cond::Futex { .. } => (-110i64) as u64, Cond::SigWait(_) => (-EAGAIN) as u64, _ => 0 }
}

/// Makes process `pid` the running one (its memory, files and compiled code).
pub fn enter(e: &mut Engine, pid: u32) {
    if pid == 0 || lx(e).pid == pid { return; }
    let space = e.mmu.take_space();
    let jit = std::mem::take(&mut e.jit);
    let on = jit.on;
    let l = lx(e);
    let cur = l.pid;
    let saved = Saved {
        fds: std::mem::take(&mut l.fds), cloexec: std::mem::take(&mut l.cloexec), cwd: std::mem::take(&mut l.cwd),
        brk_start: l.brk_start, brk: l.brk, mmap_next: l.mmap_next, exe: std::mem::take(&mut l.exe), ids: l.ids, space, jit,
    };
    match l.procs.get_mut(&cur) { Some(p) => p.saved = Some(saved), None => e.mmu.free_saved(saved.space) }
    let l = lx(e);
    let Some(s) = l.procs.get_mut(&pid).and_then(|p| p.saved.take()) else { return };
    l.pid = pid;
    l.fds = s.fds; l.cloexec = s.cloexec; l.cwd = s.cwd; l.brk_start = s.brk_start; l.brk = s.brk; l.mmap_next = s.mmap_next; l.exe = s.exe; l.ids = s.ids;
    e.mmu.put_space(s.space);
    e.jit = s.jit;
    e.jit.on = on;
}

/// fork: the child gets a copy of the memory, the files and the registers (rax 0).
fn fork(e: &mut Engine) -> i64 {
    e.materialize();
    let Some(space) = e.mmu.clone_space() else { return -ENOMEM };
    let jit = e.jit.clone();
    let l = lx(e);
    let pid = l.next_pid; l.next_pid += 1;
    let saved = Saved {
        fds: l.fds.clone(), cloexec: l.cloexec.clone(), cwd: l.cwd.clone(),
        brk_start: l.brk_start, brk: l.brk, mmap_next: l.mmap_next, exe: l.exe.clone(), ids: l.ids, space, jit,
    };
    let me = l.pid;
    let (pgid, ctty) = l.procs.get(&me).map_or((me, 0), |p| (p.pgid, p.ctty));
    l.procs.insert(pid, Proc { ppid: me, saved: Some(saved), zombie: None, pgid, ctty });
    if let Some(a) = l.acts.get(&me).cloned() { l.acts.insert(pid, a); }
    let tid = e.cur_tid();
    let mask = lx(e).masks.get(&tid).copied().unwrap_or(0);
    let mut cpu = e.cpu.clone();
    cpu.regs[0] = 0;
    let t = e.spawn_linux(cpu, pid);
    lx(e).masks.insert(t, mask);
    pid as i64
}

/// The running process ends with wait status `status`.
fn exit_proc(e: &mut Engine, status: i32) {
    let l = lx(e);
    let pid = l.pid;
    if pid == 1 {
        e.s.exit_code = ((status >> 8) & 0xFF) as u32 | if status & 0x7F != 0 { 128 + (status & 0x7F) as u32 } else { 0 };
        e.s.exited = true;
        return;
    }
    l.fds.clear(); l.cloexec.clear();
    for p in l.procs.values_mut() { if p.ppid == pid { p.ppid = 1; } }
    if let Some(p) = l.procs.get_mut(&pid) { p.zombie = Some(status); }
    gone(e, pid);
    e.mmu.free_space();
    { let on = e.jit.on; e.jit = Default::default(); e.jit.on = on; }
    e.end_linux(pid);
}

fn kill_proc(e: &mut Engine, pid: u32, sig: i32) {
    let l = lx(e);
    let Some(p) = l.procs.get_mut(&pid) else { return };
    p.zombie = Some(sig);
    let saved = p.saved.take();
    for q in l.procs.values_mut() { if q.ppid == pid { q.ppid = 1; } }
    if let Some(s) = saved { e.mmu.free_saved(s.space); }
    gone(e, pid);
    e.end_linux(pid);
}

/// A process ended: its threads' tables go, and a parent that handles SIGCHLD gets it.
fn gone(e: &mut Engine, pid: u32) {
    let ts = e.linux_threads(pid);
    let l = lx(e);
    for t in ts { l.tids.remove(&t); l.masks.remove(&t); l.susp.remove(&t); }
    l.acts.remove(&pid); l.pend.remove(&pid);
    l.timers.retain(|_, t| t.pid != pid); l.alarms.remove(&pid);
    let pp = l.procs.get(&pid).map_or(0, |p| p.ppid);
    if pp != 0 && act(l, pp, 17)[0] > 1 { *l.pend.entry(pp).or_default() |= bit(17); }
}

fn wait4(e: &mut Engine, pid: i64, status: u64, opts: u64) -> i64 {
    let l = lx(e);
    let me = l.pid;
    let kids: Vec<u32> = l.procs.iter().filter(|(k, p)| p.ppid == me && (pid <= 0 || **k as i64 == pid)).map(|(k, _)| *k).collect();
    if kids.is_empty() { return -ECHILD; }
    if let Some(&k) = kids.iter().find(|k| l.procs[k].zombie.is_some()) {
        let p = l.procs.remove(&k).unwrap();
        if let Some(s) = p.saved { e.mmu.free_saved(s.space); }
        if status != 0 { e.w32(status, p.zombie.unwrap() as u32); }
        return k as i64;
    }
    if opts & 1 != 0 { return 0; } // WNOHANG
    l.want = Some(Cond::Child { parent: me, pid });
    BLOCKED
}

/// execve: the running process becomes another program (same pid and open files).
fn execve(e: &mut Engine, path: &str, mut args: Vec<String>, env: Vec<String>) -> i64 {
    let l = lx(e);
    let mut p = l.resolve(path, true);
    let mut data = match l.fs.get(&p) { Some(Node::File(d)) => d.clone(), Some(Node::Dir) => return -13, _ => return -ENOENT };
    if data.starts_with(b"#!") { // script: run its interpreter
        let line = String::from_utf8_lossy(&data[2..data.iter().position(|&c| c == b'\n').unwrap_or(data.len()).min(256)]).trim().to_string();
        let mut it = line.splitn(2, char::is_whitespace);
        let interp = it.next().unwrap_or("").to_string();
        let mut na = vec![interp.clone()];
        if let Some(x) = it.next() { na.push(x.trim().to_string()); }
        na.push(path.to_string());
        na.extend(args.into_iter().skip(1));
        args = na;
        p = l.resolve(&interp, true);
        data = match l.fs.get(&p) { Some(Node::File(d)) => d.clone(), _ => return -ENOENT };
    }
    if !data.starts_with(b"\x7fELF") { return -ENOEXEC; }
    if args.is_empty() { args.push(p.clone()); }
    // set-uid programs (no file modes here): these run with effective uid 0, as on Alpine
    let base = path.rsplit('/').next().unwrap_or("");
    if matches!(base, "su" | "passwd" | "login" | "ping" | "ping6" | "traceroute" | "crontab" | "doas" | "sudo" | "newgrp" | "chsh" | "chfn" | "gpasswd") { l.ids.2 = 0; }
    // the old program goes away
    let cx: Vec<i32> = l.cloexec.iter().copied().collect();
    for fd in cx { l.fds.remove(&fd); }
    l.cloexec.clear();
    let me = l.pid;
    if let Some(a) = l.acts.get_mut(&me) { for x in a.iter_mut() { if x[0] != 1 { *x = [0; 4]; } } }
    e.mmu.free_space();
    { let on = e.jit.on; e.jit = Default::default(); e.jit.on = on; }
    match e.load_elf(&data, &p, &args, &env) {
        Ok(()) => NORET,
        Err(m) => { crate::console(&format!("{}: {}\n", path, m)); exit_proc(e, 127 << 8); NORET }
    }
}

// ---------------------------------------------------------------- pseudo-terminals

fn pty_held(l: &Lx, id: u32, master: bool) -> bool { held(l, &|f| matches!(f, Fd::Pty { id: i, master: m } if *i == id && *m == master)) }

/// The master typed `d`: line editing, echo and Ctrl+C, Ctrl+\ and Ctrl+Z as the terminal settings say.
/// Returns the signals to send (to the terminal's foreground group).
fn pty_input(l: &mut Lx, id: u32, d: &[u8]) -> Vec<(u32, u32)> {
    let mut sigs = Vec::new();
    let Some(t) = l.ptys.get_mut(&id) else { return sigs };
    for &c0 in d {
        let r = |o: usize| u32::from_le_bytes(t.termios[o..o + 4].try_into().unwrap());
        let (iflag, oflag, lflag) = (r(0), r(4), r(12));
        let cc: Vec<u8> = t.termios[17..36].to_vec();
        let echo = lflag & 8 != 0;
        let mut c = c0;
        if c == b'\r' && iflag & 0x80 != 0 { continue; } // IGNCR
        if c == b'\r' && iflag & 0x100 != 0 { c = b'\n'; } else if c == b'\n' && iflag & 0x40 != 0 { c = b'\r'; } // ICRNL / INLCR
        if lflag & 1 != 0 && c != 0 && (c == cc[0] || c == cc[1] || c == cc[10]) { // ISIG
            let sig = if c == cc[0] { 2 } else if c == cc[1] { 3 } else { 20 };
            if echo { t.out.extend([b'^', c + 64, b'\r', b'\n']); }
            t.line.clear();
            sigs.push((t.fg, sig));
            continue;
        }
        if lflag & 2 != 0 { // ICANON
            if c == cc[2] || c == 8 { if t.line.pop().is_some() && echo { t.out.extend(b"\x08 \x08"); } continue; }
            if c == cc[3] { if echo { for _ in 0..t.line.len() { t.out.extend(b"\x08 \x08"); } } t.line.clear(); continue; }
            if c == cc[4] { if t.line.is_empty() { t.eofs += 1; } else { let ln = std::mem::take(&mut t.line); t.inq.extend(ln); } continue; }
            t.line.push(c);
            if echo || (c == b'\n' && lflag & 0x40 != 0) { if c == b'\n' && oflag & 5 == 5 { t.out.extend(b"\r\n"); } else { t.out.push_back(c); } }
            if c == b'\n' { let ln = std::mem::take(&mut t.line); t.inq.extend(ln); }
        } else {
            t.inq.push_back(c);
            if echo { if c == b'\n' && oflag & 5 == 5 { t.out.extend(b"\r\n"); } else { t.out.push_back(c); } }
        }
    }
    sigs
}

/// The slave wrote `d`: output processing (OPOST|ONLCR turns \n into \r\n).
fn pty_output(l: &mut Lx, id: u32, d: &[u8]) -> i64 {
    if !pty_held(l, id, true) { return -5; } // EIO: nobody holds the master
    let Some(t) = l.ptys.get_mut(&id) else { return -5 };
    let oflag = u32::from_le_bytes(t.termios[4..8].try_into().unwrap());
    if oflag & 5 == 5 { for &c in d { if c == b'\n' { t.out.extend(b"\r\n"); } else { t.out.push_back(c); } } } else { t.out.extend(d.iter()); }
    d.len() as i64
}

fn pty_read(e: &mut Engine, id: u32, master: bool, buf: u64, n: u64) -> i64 {
    let l = lx(e);
    let other = pty_held(l, id, !master) || (master && !l.ptys.get(&id).map_or(false, |t| t.opened));
    let Some(t) = l.ptys.get_mut(&id) else { return -5 };
    let canon = u32::from_le_bytes(t.termios[12..16].try_into().unwrap()) & 2 != 0;
    let q = if master { &mut t.out } else { &mut t.inq };
    if q.is_empty() {
        if !master && t.eofs > 0 { t.eofs -= 1; return 0; }
        if !other { return if master { -5 } else { 0 }; } // the other side closed: EIO / end of input
        if (master && t.nb.0) || (!master && t.nb.1) { return -EAGAIN; }
        l.want = Some(Cond::Fds(vec![(Fd::Pty { id, master }, 1)]));
        return BLOCKED;
    }
    let mut k = (n as usize).min(q.len());
    if !master && canon { if let Some(nl) = q.iter().position(|&c| c == b'\n') { k = k.min(nl + 1); } }
    let d: Vec<u8> = q.drain(..k).collect();
    if e.mmu.write_bytes(buf, &d).is_err() { return -14; }
    k as i64
}

/// A signal from the terminal: to its foreground group, or (none set) to the processes it controls.
fn pty_signal(e: &mut Engine, id: u32, fg: u32, sig: u32) {
    let l = lx(e);
    let to: Vec<u32> = l.procs.iter().filter(|(_, p)| p.zombie.is_none() && if fg != 0 { p.pgid == fg } else { p.ctty == id + 1 }).map(|(k, _)| *k).collect();
    for pid in to { signal(e, pid, sig); }
}

fn pty_ioctl(e: &mut Engine, id: u32, master: bool, req: u64, a: u64) -> i64 {
    let me = lx(e).pid;
    let req = req & 0xFFFF_FFFF; // an int: some callers sign-extend it
    match req {
        0x5401 => { let t = lx(e).ptys.get(&id).map(|t| t.termios.clone()).unwrap_or_else(default_termios); if e.mmu.write_bytes(a, &t).is_err() { -14 } else { 0 } } // TCGETS
        0x5402..=0x5404 => { // TCSETS / TCSETSW / TCSETSF
            let mut b = vec![0u8; 36];
            let _ = e.mmu.read_bytes(a, &mut b);
            if let Some(t) = lx(e).ptys.get_mut(&id) {
                t.termios = b;
                if req == 0x5404 { t.inq.clear(); t.line.clear(); }
                // leaving line mode: what was typed so far becomes input
                if u32::from_le_bytes(t.termios[12..16].try_into().unwrap()) & 2 == 0 && !t.line.is_empty() { let ln = std::mem::take(&mut t.line); t.inq.extend(ln); }
            }
            0
        }
        0x5413 => { let (r, c) = lx(e).ptys.get(&id).map_or((24, 80), |t| t.ws); for (k, v) in [r, c, 0, 0].iter().enumerate() { e.w16(a + 2 * k as u64, *v); } 0 } // TIOCGWINSZ
        0x5414 => { // TIOCSWINSZ: SIGWINCH to the foreground group
            let (r, c) = (e.r16(a), e.r16(a + 2));
            let fg = { let l = lx(e); match l.ptys.get_mut(&id) { Some(t) => { let ch = t.ws != (r, c); t.ws = (r, c); if ch { Some(t.fg) } else { None } } None => None } };
            if let Some(g) = fg { pty_signal(e, id, g, 28); }
            0
        }
        0x540F => { let l = lx(e); let g = l.ptys.get(&id).map_or(0, |t| t.fg); let g = if g != 0 { g } else { l.procs.get(&me).map_or(me, |p| p.pgid) }; e.w32(a, g); 0 } // TIOCGPGRP
        0x5410 => { let g = e.r32(a); if let Some(t) = lx(e).ptys.get_mut(&id) { t.fg = g; } 0 } // TIOCSPGRP
        0x540E => { if let Some(p) = lx(e).procs.get_mut(&me) { p.ctty = id + 1; } 0 } // TIOCSCTTY
        0x5422 => { if let Some(p) = lx(e).procs.get_mut(&me) { if p.ctty == id + 1 { p.ctty = 0; } } 0 } // TIOCNOTTY
        0x5429 => { e.w32(a, me); 0 } // TIOCGSID
        0x80045430 => { e.w32(a, id); 0 } // TIOCGPTN
        0x40045431 => 0, // TIOCSPTLCK
        0x5441 if master => { let l = lx(e); if let Some(t) = l.ptys.get_mut(&id) { t.opened = true; } let fd = l.new_fd(Fd::Pty { id, master: false }); if a & 0x80000 != 0 { l.cloexec.insert(fd); } fd as i64 } // TIOCGPTPEER
        0x540B => { if let Some(t) = lx(e).ptys.get_mut(&id) { if a != 1 { t.inq.clear(); t.line.clear(); } if a != 0 { t.out.clear(); } } 0 } // TCFLSH
        0x5409 | 0x540A | 0x5408 => 0, // TCSBRK, TCXONC, TCSBRKP
        0x5411 => { e.w32(a, 0); 0 } // TIOCOUTQ
        _ => -EINVAL,
    }
}

// ---------------------------------------------------------------- threads and futexes

/// clone with CLONE_VM: a new thread of the running process (own stack, registers and TLS).
fn clone_thread(e: &mut Engine, flags: u64, stack: u64, ptid: u64, ctid: u64, tls: u64) -> i64 {
    e.materialize();
    let mut cpu = e.cpu.clone();
    cpu.regs[0] = 0;
    if stack != 0 { cpu.regs[4] = stack; }
    if flags & 0x80000 != 0 { cpu.fs_base = tls; } // CLONE_SETTLS
    let me = e.cur_tid();
    let l = lx(e);
    let pid = l.pid;
    let tid = l.next_pid; l.next_pid += 1;
    let mask = l.masks.get(&me).copied().unwrap_or(0);
    let id = e.spawn_linux(cpu, pid);
    let l = lx(e);
    l.tids.insert(id, (tid, if flags & 0x200000 != 0 { ctid } else { 0 })); // CLONE_CHILD_CLEARTID
    l.masks.insert(id, mask);
    if flags & 0x100000 != 0 { e.w32(ptid, tid); } // CLONE_PARENT_SETTID
    if flags & 0x1000000 != 0 { e.w32(ctid, tid); } // CLONE_CHILD_SETTID
    tid as i64
}

/// The running thread's Linux id (the first thread of a process has the process id).
fn cur_linux_tid(e: &mut Engine) -> u32 {
    let t = e.cur_tid();
    let l = lx(e);
    l.tids.get(&t).map_or(l.pid, |x| x.0)
}

fn is_thread(e: &mut Engine) -> bool { let pid = lx(e).pid; cur_linux_tid(e) != pid }

/// A Linux thread id to its process.
fn tid_pid(e: &mut Engine, tid: u32) -> u32 {
    let l = lx(e);
    if l.procs.contains_key(&tid) { return tid; }
    let Some(sid) = l.tids.iter().find(|(_, v)| v.0 == tid).map(|(k, _)| *k) else { return 0 };
    e.s.sched.th.iter().find(|t| t.id == sid).map_or(0, |t| t.pid)
}

/// exit() of a thread that is not the first one: only it ends (its tid word is cleared and woken).
fn thread_exit(e: &mut Engine) {
    let me = e.cur_tid();
    let l = lx(e);
    let clr = l.tids.remove(&me).map_or(0, |x| x.1);
    l.masks.remove(&me); l.susp.remove(&me);
    if clr != 0 { e.w32(clr, 0); futex_wake(e, clr, 1); }
    e.end_linux_thread();
}

fn futex_wake(e: &mut Engine, addr: u64, n: u64) -> i64 {
    let pid = lx(e).pid;
    let mut k = 0u64;
    e.wake_linux(&mut |p, c| if k < n && p == pid && matches!(c, Cond::Futex { addr: a } if *a == addr) { k += 1; Some(0) } else { None });
    k as i64
}

fn futex(e: &mut Engine, addr: u64, op: u64, val: u64, t: u64, addr2: u64, val3: u64) -> i64 {
    let cmd = op & 0x7F;
    match cmd {
        0 | 9 => { // WAIT / WAIT_BITSET (absolute time)
            if e.r32(addr) != val as u32 { return -EAGAIN; }
            let ms = if t == 0 { None } else {
                let m = e.r64(t) as f64 * 1000.0 + e.r64(t + 8) as f64 / 1e6;
                Some(if cmd == 9 { m - if op & 256 != 0 { crate::epoch_ms() } else { crate::now_ms() } } else { m })
            };
            if ms.map_or(false, |m| m <= 0.0) { return -110; } // ETIMEDOUT
            e.cpu.rip -= 2;
            e.block_linux(Cond::Futex { addr }, ms);
            NORET
        }
        1 | 10 => futex_wake(e, addr, val), // WAKE / WAKE_BITSET
        3 | 4 => { // REQUEUE / CMP_REQUEUE: wake `val`, move up to `t` waiters to addr2
            if cmd == 4 && e.r32(addr) != val3 as u32 { return -EAGAIN; }
            let pid = lx(e).pid;
            let (mut woke, mut moved) = (0u64, 0u64);
            e.wake_linux(&mut |p, c| {
                let Cond::Futex { addr: a } = c else { return None };
                if p != pid || *a != addr { return None; }
                if woke < val { woke += 1; return Some(0); }
                if moved < t { moved += 1; *a = addr2; }
                None
            });
            (woke + moved) as i64
        }
        5 => { // WAKE_OP: the operation on addr2, then wake on both
            let (opk, arg) = ((val3 >> 28) & 7, (val3 >> 12) & 0xFFF);
            let arg = if (val3 >> 28) & 8 != 0 { 1u32 << (arg & 31) } else { arg as u32 };
            let old = e.r32(addr2);
            let new = match opk { 0 => arg, 1 => old.wrapping_add(arg), 2 => old | arg, 3 => old & !arg, _ => old ^ arg };
            e.w32(addr2, new);
            let (cmp, carg) = ((val3 >> 24) & 15, (val3 & 0xFFF) as u32);
            let ok = match cmp { 0 => old == carg, 1 => old != carg, 2 => (old as i32) < carg as i32, 3 => (old as i32) <= carg as i32, 4 => (old as i32) > carg as i32, _ => (old as i32) >= carg as i32 };
            let mut n = futex_wake(e, addr, val);
            if ok { n += futex_wake(e, addr2, t); }
            n
        }
        _ => -ENOSYS,
    }
}

// ---------------------------------------------------------------- signals

const SA_RESTART: u64 = 0x1000_0000;
const SA_NODEFER: u64 = 0x4000_0000;
const SA_RESETHAND: u64 = 0x8000_0000;
const EINTR: i64 = 4;

fn bit(sig: u32) -> u64 { 1u64 << (sig - 1) }
fn act(l: &Lx, pid: u32, sig: u32) -> [u64; 4] { l.acts.get(&pid).map_or([0; 4], |a| a[sig as usize]) }
/// Signals whose default action is to do nothing here (child, continue, stops, urgent, window size).
fn dfl_ignore(sig: u32) -> bool { matches!(sig, 17 | 18 | 19 | 20 | 21 | 22 | 23 | 28) }

/// The signal thread `tid` of `pid` can take now (0 = none).
fn deliverable(l: &Lx, pid: u32, tid: u32) -> u32 {
    let p = l.pend.get(&pid).copied().unwrap_or(0) & !l.masks.get(&tid).copied().unwrap_or(0);
    if p == 0 { 0 } else { p.trailing_zeros() + 1 }
}

/// Sends `sig` to process `pid`: a handler runs at its next system call; the default action
/// ends it (the first process never ends this way). true = the running process ended.
fn signal(e: &mut Engine, pid: u32, sig: u32) -> bool {
    let l = lx(e);
    if sig == 0 || sig > 64 || !l.procs.get(&pid).map_or(false, |p| p.zombie.is_none()) { return false; }
    let h = act(l, pid, sig)[0];
    if sig != 9 && h == 1 { return false; } // ignored
    if sig != 9 && h > 1 { *l.pend.entry(pid).or_default() |= bit(sig); crate::lock::kick(); return false; }
    if dfl_ignore(sig) || pid == 1 { return false; }
    if pid == l.pid { exit_proc(e, sig as i32); true } else { kill_proc(e, pid, sig as i32); false }
}

/// Runs the handler of `sig` on the running thread: a Linux signal frame (return address = the
/// program's restorer, then ucontext and siginfo) on its stack. `rax`/`rip` = where it goes on.
fn deliver(e: &mut Engine, sig: u32, rax: u64, rip: u64) {
    let tid = e.cur_tid();
    let l = lx(e);
    let pid = l.pid;
    if let Some(p) = l.pend.get_mut(&pid) { *p &= !bit(sig); }
    let [h, flags, restorer, amask] = act(l, pid, sig);
    #[cfg(feature = "prof")]
    crate::console(&format!("[signal {} to pid {} handler {:x}]\n", sig, pid, h));
    if h <= 1 { return; }
    let cur = l.masks.get(&tid).copied().unwrap_or(0);
    let saved = l.susp.remove(&tid).unwrap_or(cur);
    let mut m = cur | amask;
    if flags & SA_NODEFER == 0 { m |= bit(sig); }
    l.masks.insert(tid, m & !(bit(9) | bit(19)));
    if flags & SA_RESETHAND != 0 { if let Some(a) = l.acts.get_mut(&pid) { a[sig as usize] = [0; 4]; } }
    let uid = l.ids.0;
    let r = e.cpu.regs;
    let frame = ((r[4] - 128 - 440) & !15) - 8;
    let (uc, info) = (frame + 8, frame + 8 + 304);
    let mut b = vec![0u8; 440];
    let mut put = |o: usize, v: u64| b[o..o + 8].copy_from_slice(&v.to_le_bytes());
    put(0, restorer);
    let mc = [r[8], r[9], r[10], r[11], r[12], r[13], r[14], r[15], r[7], r[6], r[5], r[3], r[2], rax, r[1], r[4], rip, 0x202, 0x33, 0, 0, saved];
    for (k, v) in mc.iter().enumerate() { put(8 + 40 + 8 * k, *v); }
    put(8 + 296, saved);
    b[312..316].copy_from_slice(&sig.to_le_bytes());
    b[328..332].copy_from_slice(&pid.to_le_bytes());
    b[332..336].copy_from_slice(&uid.to_le_bytes());
    let _ = e.mmu.write_bytes(frame, &b);
    e.cpu.regs[7] = sig as u64; e.cpu.regs[6] = info; e.cpu.regs[2] = uc; e.cpu.regs[0] = 0;
    e.cpu.regs[4] = frame;
    e.cpu.rip = h;
}

/// rt_sigreturn: back from a handler (registers and mask from the frame's ucontext).
fn sigreturn(e: &mut Engine, tid: u32) {
    let uc = e.cpu.regs[4];
    let mc = uc + 40;
    let g: Vec<u64> = (0..17).map(|k| e.r64(mc + 8 * k)).collect();
    for k in 0..8 { e.cpu.regs[8 + k] = g[k]; }
    e.cpu.regs[7] = g[8]; e.cpu.regs[6] = g[9]; e.cpu.regs[5] = g[10]; e.cpu.regs[3] = g[11];
    e.cpu.regs[2] = g[12]; e.cpu.regs[0] = g[13]; e.cpu.regs[1] = g[14]; e.cpu.regs[4] = g[15];
    e.cpu.rip = g[16];
    let m = e.r64(uc + 296);
    lx(e).masks.insert(tid, m & !(bit(9) | bit(19)));
}

/// A child process hit a fatal error: it ends with a signal and the others go on.
/// false = it is the first process (the whole run stops with the error).
pub fn crash(e: &mut Engine, m: &str) -> bool {
    let Some(l) = e.s.lx.as_ref() else { return false };
    let pid = l.pid;
    if pid <= 1 || e.cur_pid() != pid { return false; }
    let sig = if m.starts_with("access violation") { 11 } else { 4 };
    crate::console(&format!("[inksoc] {} (process {} stopped)\n", m, pid));
    exit_proc(e, sig);
    e.cpu.jit_limit = 0;
    true
}

/// Terminal mode for the page: bit 2 = line mode (the page edits the line), bit 8 = echo.
#[no_mangle]
pub extern "C" fn linux_lflag() -> u32 { crate::eng().s.lx.as_ref().map_or(0x8A3B, |l| l.lflag) }

#[no_mangle]
pub extern "C" fn linux_winsize(rows: u32, cols: u32) {
    let e = crate::eng();
    let Some(l) = e.s.lx.as_mut() else { return };
    l.winsize = (rows.clamp(2, 500) as u16, cols.clamp(10, 1000) as u16);
    // SIGWINCH for programs that redraw (only where a handler waits for it)
    let fg = if l.fg != 0 { l.fg } else { 1 };
    let to: Vec<u32> = l.procs.iter().filter(|(_, p)| p.zombie.is_none() && p.pgid == fg).map(|(k, _)| *k).collect();
    for pid in to { if act(l, pid, 28)[0] > 1 { *l.pend.entry(pid).or_default() |= bit(28); } }
    crate::lock::kick();
}

/// Ctrl+D: the next read of the terminal returns 0 (end of input).
#[no_mangle]
pub extern "C" fn linux_eof() { if let Some(l) = crate::eng().s.lx.as_mut() { l.stdin_eof = true; } }

/// Ctrl+C: stops every program started from the shell (the shell itself stays).
#[no_mangle]
pub extern "C" fn linux_interrupt() {
    let e = crate::eng();
    let Some(l) = e.s.lx.as_mut() else { return };
    l.stdin.clear();
    let fg = l.fg;
    let mut alive: Vec<u32> = l.procs.iter().filter(|(k, p)| p.zombie.is_none() && if fg != 0 { p.pgid == fg } else { **k != 1 }).map(|(k, _)| *k).collect();
    if alive.is_empty() && fg != 0 { alive = l.procs.iter().filter(|(k, p)| **k != 1 && p.zombie.is_none()).map(|(k, _)| *k).collect(); }
    for pid in alive {
        let l = lx(e);
        let waiting = l.pend.get(&pid).map_or(false, |p| p & bit(2) != 0);
        if waiting && pid != 1 { if pid == l.pid { exit_proc(e, 2); } else { kill_proc(e, pid, 2); } } else { signal(e, pid, 2); }
    }
    crate::lock::kick();
}

/// Adds a file from the page ("path\0data" in the buffer) to the file tree.
#[no_mangle]
pub extern "C" fn linux_put_file() {
    let e = crate::eng();
    let b = unsafe { crate::take_buf() };
    let nl = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    let name = String::from_utf8_lossy(&b[..nl]).into_owned();
    let Some(l) = e.s.lx.as_mut() else { return };
    // relative names go to the user's home (the Alpine terminal keeps the user name in /etc/inksoc/user)
    let home = match l.fs.get("/etc/inksoc/user") { Some(Node::File(d)) if !d.is_empty() => format!("/home/{}", String::from_utf8_lossy(d).trim()), _ => "/root".to_string() };
    let path = l.abs(&if name.starts_with('/') { name } else { format!("{}/{}", home, name) });
    l.put(&path, b.get(nl + 1..).unwrap_or(&[]).to_vec());
    l.dirty = true;
}

/// Reads a file of the tree (path in the buffer) for the page to download; its size, or -1.
#[no_mangle]
pub extern "C" fn linux_get_file() -> i32 {
    let e = crate::eng();
    let b = unsafe { crate::take_buf() };
    let Some(l) = e.s.lx.as_ref() else { return -1 };
    let p = l.resolve(&String::from_utf8_lossy(&b), true);
    match l.fs.get(&p) { Some(Node::File(d)) => crate::set_out(d.clone()) as i32, _ => -1 }
}

// ---------------------------------------------------------------- network

fn sock_send(l: &mut Lx, id: u32, d: &[u8], to: Option<[u8; 16]>) -> i64 {
    let Some(s) = l.socks.get_mut(&id) else { return -EBADF };
    if s.udp {
        let to = to.unwrap_or(s.peer);
        if u16::from_be_bytes([to[2], to[3]]) == 53 { if let Some(r) = dns_answer(&mut l.dns, d) { l.socks.get_mut(&id).unwrap().dgrams.push_back((r, to)); } }
        return d.len() as i64;
    }
    if s.failed { return -ECONNREFUSED; }
    if !s.open { return -ENOTCONN; }
    if s.closed { return -EPIPE; }
    net_event(&mut l.net_out, 2, id, d);
    d.len() as i64
}

/// Reads from a socket: (result, source address for UDP).
fn sock_recv(e: &mut Engine, id: u32, buf: u64, n: u64, dontwait: bool) -> (i64, Option<[u8; 16]>) {
    let l = lx(e);
    let Some(s) = l.socks.get_mut(&id) else { return (-EBADF, None) };
    if s.udp {
        let Some((d, from)) = s.dgrams.pop_front() else {
            if s.nb || dontwait { return (-EAGAIN, None); }
            l.want = Some(Cond::Socks(vec![id])); return (BLOCKED, None);
        };
        let k = d.len().min(n as usize);
        let _ = e.mmu.write_bytes(buf, &d[..k]);
        return (k as i64, Some(from));
    }
    if s.rx.is_empty() {
        if s.failed { return (-ECONNREFUSED, None); }
        if s.closed || !s.open { return (0, None); }
        if s.nb || dontwait { return (-EAGAIN, None); }
        l.want = Some(Cond::Socks(vec![id])); return (BLOCKED, None);
    }
    let k = (n as usize).min(s.rx.len());
    let d: Vec<u8> = s.rx.drain(..k).collect();
    if e.mmu.write_bytes(buf, &d).is_err() { return (-14, None); }
    (k as i64, None)
}

fn net_event(out: &mut Vec<u8>, op: u8, id: u32, d: &[u8]) {
    out.push(op); out.extend_from_slice(&id.to_le_bytes()); out.extend_from_slice(&(d.len() as u32).to_le_bytes()); out.extend_from_slice(d);
}

/// A DNS question answered here: an A record gets 198.18.x.y (the name is kept), other types get no answer.
fn dns_answer(names: &mut Vec<String>, q: &[u8]) -> Option<Vec<u8>> {
    if q.len() < 17 { return None; }
    let mut o = 12; let mut labels = Vec::new();
    while o < q.len() && q[o] != 0 { let n = q[o] as usize; labels.push(String::from_utf8_lossy(q.get(o + 1..o + 1 + n)?).to_string()); o += 1 + n; }
    o += 1;
    let qtype = u16::from_be_bytes([*q.get(o)?, *q.get(o + 1)?]);
    let end = o + 4;
    let name = labels.join(".").to_lowercase();
    let mut r = q.get(..end)?.to_vec();
    r[2] = 0x81; r[3] = 0x80; r[8..12].fill(0);
    if qtype == 1 && !name.is_empty() {
        let k = match names.iter().position(|n| *n == name) { Some(k) => k, None => { names.push(name); names.len() - 1 } } + 1;
        r[6] = 0; r[7] = 1;
        r.extend_from_slice(&[0xC0, 12, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 198, 18, (k >> 8) as u8, k as u8]);
    } else { r[6] = 0; r[7] = 0; }
    Some(r)
}

/// poll/ppoll with real readiness; it waits when nothing is ready.
fn poll_fds(e: &mut Engine, fds: u64, n: u64, nr: u64, t: u64) -> bool {
    let ms = if nr == 7 { t as i32 as i64 } else if t == 0 { -1 } else { (e.r64(t) * 1000 + e.r64(t + 8) / 1_000_000) as i64 };
    if n > 1024 { return false; }
    let (mut keys, mut nready) = (Vec::new(), 0i64);
    for k in 0..n {
        let at = fds + 8 * k;
        let fd = e.r32(at) as i32;
        let ev = e.r16(at + 4);
        if fd < 0 { e.w16(at + 6, 0); continue; }
        let l = lx(e);
        let Some(f) = l.fds.get(&fd).cloned() else { e.w16(at + 6, 0x20); nready += 1; continue }; // POLLNVAL
        let (rd, wr, hup) = fd_state(l, &f);
        let re = (if rd { 0x41 } else { 0 } | if wr { 0x104 } else { 0 }) & ev | if hup { 0x10 } else { 0 };
        e.w16(at + 6, re);
        if re != 0 { nready += 1; }
        keys.push((f, ev));
    }
    if nready > 0 || ms == 0 { ret(e, nready); return true; }
    e.cpu.rip -= 2;
    e.block_linux(Cond::Fds(keys), if ms < 0 { None } else { Some(ms as f64) });
    true
}

/// select / pselect6 with real readiness (the sets change only when the call returns something).
fn select(e: &mut Engine, nr: u64, n: u64, rp: u64, wp: u64, xp: u64, tp: u64) -> i64 {
    let n = n.min(1024);
    let words = ((n + 63) / 64) as usize;
    let get = |e: &mut Engine, p: u64| -> Vec<u64> { (0..words).map(|k| if p == 0 { 0 } else { e.r64(p + 8 * k as u64) }).collect() };
    let (ri, wi) = (get(e, rp), get(e, wp));
    let (mut ro, mut wo) = (vec![0u64; words], vec![0u64; words]);
    let (mut cnt, mut keys) = (0i64, Vec::new());
    for fd in 0..n as usize {
        let (wr_, ww) = (ri[fd / 64] >> (fd % 64) & 1 != 0, wi[fd / 64] >> (fd % 64) & 1 != 0);
        if !wr_ && !ww { continue; }
        let l = lx(e);
        let Some(f) = l.fds.get(&(fd as i32)).cloned() else { return -EBADF };
        let (r, w, h) = fd_state(l, &f);
        if wr_ && (r || h) { ro[fd / 64] |= 1 << (fd % 64); cnt += 1; }
        if ww && w { wo[fd / 64] |= 1 << (fd % 64); cnt += 1; }
        keys.push((f, if wr_ { 1 } else { 0 } | if ww { 4 } else { 0 }));
    }
    let ms: Option<f64> = if tp == 0 { None } else if nr == 23 { Some(e.r64(tp) as f64 * 1000.0 + e.r64(tp + 8) as f64 / 1000.0) } else { Some(e.r64(tp) as f64 * 1000.0 + e.r64(tp + 8) as f64 / 1e6) };
    if cnt > 0 || ms == Some(0.0) {
        for k in 0..words as u64 {
            if rp != 0 { e.w64(rp + 8 * k, ro[k as usize]); }
            if wp != 0 { e.w64(wp + 8 * k, wo[k as usize]); }
            if xp != 0 { e.w64(xp + 8 * k, 0); }
        }
        return cnt;
    }
    e.cpu.rip -= 2;
    e.block_linux(Cond::Fds(keys), ms);
    NORET
}

fn epoll_ctl(e: &mut Engine, ep: i32, op: u64, fd: i32, ev: u64) -> i64 {
    let l = lx(e);
    let Some(Fd::Epoll { id }) = l.fds.get(&ep).cloned() else { return if l.fds.contains_key(&ep) { -EINVAL } else { -EBADF } };
    if !l.fds.contains_key(&fd) { return -EBADF; }
    let (events, data) = if ev != 0 { (e.r32(ev), e.r64(ev + 4)) } else { (0, 0) };
    let set = lx(e).eps.entry(id).or_default();
    match op {
        1 => { if set.contains_key(&fd) { return -EEXIST; } set.insert(fd, (events, data)); 0 }
        2 => if set.remove(&fd).is_some() { 0 } else { -ENOENT },
        3 => match set.get_mut(&fd) { Some(x) => { *x = (events, data); 0 } None => -ENOENT },
        _ => -EINVAL,
    }
}

fn epoll_wait(e: &mut Engine, ep: i32, out: u64, max: u64, ms: i64) -> i64 {
    let l = lx(e);
    let Some(Fd::Epoll { id }) = l.fds.get(&ep).cloned() else { return -EBADF };
    if max == 0 { return -EINVAL; }
    let set: Vec<(i32, (u32, u64))> = l.eps.get(&id).map_or(Vec::new(), |s| s.iter().map(|(k, v)| (*k, *v)).collect());
    let (mut got, mut keys, mut closed) = (Vec::new(), Vec::new(), Vec::new());
    for (fd, (ev, data)) in set {
        let Some(f) = l.fds.get(&fd).cloned() else { closed.push(fd); continue };
        if ev & 0x2005 == 0 && ev & 0x18 == 0 { continue; } // disabled (EPOLLONESHOT after it fired)
        let (r, w, h) = fd_state(l, &f);
        let re = (if r { 0x41 } else { 0 } | if w { 0x104 } else { 0 } | if h { 0x2000 } else { 0 }) & ev | if h { 0x10 } else { 0 };
        keys.push((f, (ev & 0xFFFF) as u16));
        if re != 0 && (got.len() as u64) < max { got.push((fd, re, data, ev)); }
    }
    if let Some(s) = l.eps.get_mut(&id) {
        for fd in closed { s.remove(&fd); }
        for (fd, _, _, ev) in &got { if ev & (1 << 30) != 0 { if let Some(x) = s.get_mut(fd) { x.0 &= 0xFFFF_0000 & !0x2005; } } }
    }
    if !got.is_empty() || ms == 0 {
        for (k, (_, re, data, _)) in got.iter().enumerate() { e.w32(out + 12 * k as u64, *re); e.w64(out + 12 * k as u64 + 4, *data); }
        return got.len() as i64;
    }
    e.cpu.rip -= 2;
    e.block_linux(Cond::Fds(keys), if ms < 0 { None } else { Some(ms as f64) });
    NORET
}

/// The ITIMER_REAL timer of `pid` (alarm and setitimer share it).
fn alarm_timer(l: &mut Lx, pid: u32) -> u32 {
    if let Some(id) = l.alarms.get(&pid) { if l.timers.contains_key(id) { return *id; } }
    l.next_tm += 1; let id = l.next_tm;
    l.timers.insert(id, Tmr { pid, sig: 14, ..Default::default() });
    l.alarms.insert(pid, id);
    id
}

/// (ms left, interval ms) of a timer.
fn tm_get(l: &Lx, id: u32) -> (f64, f64) {
    l.timers.get(&id).map_or((0.0, 0.0), |t| (if t.next > 0.0 { (t.next - crate::now_ms()).max(0.0) } else { 0.0 }, t.every))
}

/// Arms a timer `ms` from now (0 disarms), then every `every` ms.
fn tm_set(l: &mut Lx, id: u32, ms: f64, every: f64) {
    if let Some(t) = l.timers.get_mut(&id) { t.next = if ms > 0.0 { crate::now_ms() + ms } else { 0.0 }; t.every = every; }
    crate::lock::kick();
}

fn put_ts(e: &mut Engine, a: u64, ms: f64) { e.w64(a, (ms / 1000.0) as u64); e.w64(a + 8, ((ms % 1000.0) * 1e6) as u64); }

/// Timers that are due: their signal goes pending (a handler runs at the next system call; the
/// default action ends the process there), a timerfd counts the expirations.
fn fire_timers(l: &mut Lx) {
    if l.timers.is_empty() { return; }
    let now = crate::now_ms();
    let mut sigs = Vec::new();
    l.timers.retain(|_, t| {
        if t.next <= 0.0 || now < t.next { return true; }
        let n = if t.every > 0.0 { ((now - t.next) / t.every) as u64 + 1 } else { 1 };
        t.next = if t.every > 0.0 { t.next + n as f64 * t.every } else { 0.0 };
        if t.fd { t.count += n; } else if t.sig != 0 { sigs.push((t.pid, t.sig)); }
        true
    });
    for (pid, sig) in sigs {
        if !l.procs.get(&pid).map_or(false, |p| p.zombie.is_none()) { continue; }
        if act(l, pid, sig)[0] != 1 { *l.pend.entry(pid).or_default() |= bit(sig); }
    }
}

/// Milliseconds until the next armed timer of a live process, or -1 (none): for the test harness,
/// which otherwise stops when every thread just waits.
#[no_mangle]
pub extern "C" fn linux_timer_ms() -> f64 {
    let e = crate::eng();
    let Some(l) = e.s.lx.as_ref() else { return -1.0 };
    let now = crate::now_ms();
    l.timers.values().filter(|t| t.next > 0.0).map(|t| (t.next - now).max(0.0)).fold(-1.0, |a: f64, b| if a < 0.0 { b } else { a.min(b) })
}

fn tfd_read(e: &mut Engine, id: u32, buf: u64, n: u64) -> i64 {
    if n < 8 { return -EINVAL; }
    let l = lx(e);
    fire_timers(l);
    let Some(t) = l.timers.get_mut(&id) else { return -EBADF };
    if t.count == 0 {
        if t.nb { return -EAGAIN; }
        l.want = Some(Cond::Fds(vec![(Fd::Timer { id }, 1)]));
        return BLOCKED;
    }
    let v = std::mem::take(&mut t.count);
    e.w64(buf, v);
    8
}

fn ev_read(e: &mut Engine, id: u32, buf: u64, n: u64) -> i64 {
    if n < 8 { return -EINVAL; }
    let l = lx(e);
    let Some(x) = l.evs.get_mut(&id) else { return -EBADF };
    if x.0 == 0 {
        if x.2 { return -EAGAIN; }
        l.want = Some(Cond::Fds(vec![(Fd::Event { id }, 1)]));
        return BLOCKED;
    }
    let v = if x.1 { 1 } else { x.0 };
    x.0 -= v;
    e.w64(buf, v);
    8
}

fn get_nb(l: &Lx, f: &Fd) -> bool {
    match f { Fd::Sock { id } => l.socks.get(id).map_or(false, |s| s.nb), Fd::Unix { id } => l.us.get(id).map_or(false, |u| u.nb), Fd::Event { id } => l.evs.get(id).map_or(false, |v| v.2), Fd::Timer { id } => l.timers.get(id).map_or(false, |t| t.nb), Fd::Pty { id, master } => l.ptys.get(id).map_or(false, |t| if *master { t.nb.0 } else { t.nb.1 }), Fd::Pipe { id, w } => l.pipe_nb.contains(&(*id, *w)), _ => false }
}
fn set_nb(l: &mut Lx, f: &Fd, on: bool) {
    match f {
        Fd::Sock { id } => if let Some(s) = l.socks.get_mut(id) { s.nb = on },
        Fd::Unix { id } => if let Some(u) = l.us.get_mut(id) { u.nb = on },
        Fd::Event { id } => if let Some(v) = l.evs.get_mut(id) { v.2 = on },
        Fd::Timer { id } => if let Some(t) = l.timers.get_mut(id) { t.nb = on },
        Fd::Pty { id, master } => if let Some(t) = l.ptys.get_mut(id) { if *master { t.nb.0 = on } else { t.nb.1 = on } },
        Fd::Pipe { id, w } => { if on { l.pipe_nb.insert((*id, *w)); } else { l.pipe_nb.remove(&(*id, *w)); } }
        _ => {}
    }
}

// ---------------------------------------------------------------- local sockets

fn us_new(l: &mut Lx, st: UsSt, nb: bool) -> u32 {
    l.next_us += 1;
    let id = l.next_us;
    l.us.insert(id, Us { st, nb, name: String::new(), port: 0, shut_wr: false });
    id
}

/// Reads from a connected local socket (EOF when the other end closed or shut down writing).
fn us_read(e: &mut Engine, id: u32, buf: u64, n: u64, dontwait: bool) -> i64 {
    let l = lx(e);
    let Some(u) = l.us.get(&id) else { return -EBADF };
    let (rx, peer, nb) = match u.st { UsSt::Conn { rx, peer, .. } => (rx, peer, u.nb), UsSt::Listen(_) => return -EINVAL, UsSt::New => return -ENOTCONN };
    if l.pipes.get(&rx).map_or(true, |b| b.is_empty()) {
        if !us_held(l, peer) || l.us.get(&peer).map_or(true, |p| p.shut_wr) { return 0; }
        if nb || dontwait { return -EAGAIN; }
        l.want = Some(Cond::Fds(vec![(Fd::Unix { id }, 1)]));
        return BLOCKED;
    }
    let pos = l.pread.get(&rx).copied().unwrap_or(0);
    let mut k = (n as usize).min(l.pipes[&rx].len());
    l.us_got = None;
    if let Some(q) = l.passed.get_mut(&rx) {
        while q.front().map_or(false, |x| x.0 < pos) { q.pop_front(); } // read without a control buffer: dropped
        if let Some(x) = q.front() { if x.0 == pos { l.us_got = q.pop_front().map(|x| x.1); } }
        if let Some(x) = q.front() { k = k.min((x.0 - pos) as usize); }
    }
    let b = l.pipes.get_mut(&rx).unwrap();
    let d: Vec<u8> = b.drain(..k).collect();
    *l.pread.entry(rx).or_default() += k as u64;
    if e.mmu.write_bytes(buf, &d).is_err() { return -14; }
    k as i64
}

fn us_write(l: &mut Lx, id: u32, d: &[u8]) -> i64 {
    let Some(u) = l.us.get(&id) else { return -EBADF };
    let UsSt::Conn { tx, peer, .. } = u.st else { return -ENOTCONN };
    if u.shut_wr || !us_held(l, peer) { return -EPIPE; }
    l.pipes.entry(tx).or_default().extend(d.iter());
    d.len() as i64
}

/// A local socket's address: AF_UNIX path (or "@name" for the abstract ones); TCP on this machine as "tcp:port".
fn unix_name(e: &mut Engine, a: u64, len: u64) -> Option<String> {
    if a == 0 || len < 3 { return None; }
    let mut b = vec![0u8; (len as usize).min(110)];
    let _ = e.mmu.read_bytes(a, &mut b);
    if u16::from_le_bytes([b[0], b[1]]) != 1 { return None; }
    let p = &b[2..];
    if p[0] == 0 { return Some(format!("@{}", String::from_utf8_lossy(&p[1..]).trim_end_matches('\0'))); }
    let end = p.iter().position(|&c| c == 0).unwrap_or(p.len());
    let s = String::from_utf8_lossy(&p[..end]).into_owned();
    Some(lx(e).abs(&s))
}

fn write_unix_addr(e: &mut Engine, a: u64, lenp: u64, u: &Us) {
    if a == 0 { return; }
    if u.port != 0 {
        let mut ad = [0u8; 16]; ad[0] = 2; ad[2..4].copy_from_slice(&u.port.to_be_bytes()); ad[4] = 127; ad[7] = 1;
        write_addr(e, a, lenp, &ad);
        return;
    }
    let mut b = vec![1u8, 0];
    if let Some(n) = u.name.strip_prefix('@') { b.push(0); b.extend_from_slice(n.as_bytes()); } else { b.extend_from_slice(u.name.as_bytes()); b.push(0); }
    let _ = e.mmu.write_bytes(a, &b);
    if lenp != 0 { e.w32(lenp, b.len() as u32); }
}

/// Socket calls on a local socket; None = not one (the network code handles it).
fn unix_call(e: &mut Engine, nr: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> Option<i64> {
    let fd = a0 as i32;
    let l = lx(e);
    if nr == 41 && a0 == 1 { // socket(AF_UNIX, ...)
        let ty = a1 & 0xF;
        if ty != 1 && ty != 5 { return Some(-EOPNOTSUPP); } // stream and seqpacket (as a stream)
        let id = us_new(l, UsSt::New, a1 & 0x800 != 0);
        let n = l.new_fd(Fd::Unix { id });
        if a1 & 0x80000 != 0 { l.cloexec.insert(n); }
        return Some(n as i64);
    }
    if nr == 53 { // socketpair
        if a0 != 1 { return Some(-EOPNOTSUPP); }
        let (p, q) = (l.next_pipe, l.next_pipe + 1); l.next_pipe += 2;
        l.pipes.insert(p, Default::default()); l.pipes.insert(q, Default::default());
        let nb = a1 & 0x800 != 0;
        let x = us_new(l, UsSt::New, nb); let y = us_new(l, UsSt::New, nb);
        l.us.get_mut(&x).unwrap().st = UsSt::Conn { rx: p, tx: q, peer: y };
        l.us.get_mut(&y).unwrap().st = UsSt::Conn { rx: q, tx: p, peer: x };
        let (f1, f2) = (l.new_fd(Fd::Unix { id: x }), l.new_fd(Fd::Unix { id: y }));
        if a1 & 0x80000 != 0 { l.cloexec.insert(f1); l.cloexec.insert(f2); }
        e.w32(a3, f1 as u32); e.w32(a3 + 4, f2 as u32);
        return Some(0);
    }
    // TCP on this machine: listen turns the socket into a local one; connect to 127.x finds it
    if let Some(Fd::Sock { id: sid }) = l.fds.get(&fd).cloned() {
        match nr {
            49 => { // bind: remember the port
                if a1 != 0 && a2 >= 4 { let fam = e.r16(a1); let port = u16::from_be(e.r16(a1 + 2)); if fam == 2 || fam == 10 { if let Some(s) = lx(e).socks.get_mut(&sid) { s.port = port; } } }
                return Some(0);
            }
            50 => {
                let l = lx(e);
                let (udp, nb, mut port) = l.socks.get(&sid).map_or((true, false, 0), |s| (s.udp, s.nb, s.port));
                if udp { return Some(-EOPNOTSUPP); }
                if port == 0 { port = 40000; while l.us_names.contains_key(&format!("tcp:{}", port)) { port += 1; } }
                let name = format!("tcp:{}", port);
                if let Some(&o) = l.us_names.get(&name) { if us_held(l, o) { return Some(-98); } } // EADDRINUSE
                let id = us_new(l, UsSt::Listen(Default::default()), nb);
                { let u = l.us.get_mut(&id).unwrap(); u.name = name.clone(); u.port = port; }
                l.us_names.insert(name, id);
                l.fds.insert(fd, Fd::Unix { id });
                return Some(0);
            }
            42 => {
                if a1 == 0 || a2 < 8 { return None; }
                let mut b = [0u8; 28];
                let _ = e.mmu.read_bytes(a1, &mut b[..(a2 as usize).min(28)]);
                let fam = u16::from_le_bytes([b[0], b[1]]);
                let v4 = if fam == 10 && b[8..18].iter().all(|&c| c == 0) && b[18] == 0xFF && b[19] == 0xFF { [b[20], b[21], b[22], b[23]] } else { [b[4], b[5], b[6], b[7]] };
                let local = (fam == 2 || fam == 10) && (v4[0] == 127 || v4 == [0, 0, 0, 0] || (fam == 10 && b[8..23].iter().all(|&c| c == 0) && b[23] <= 1));
                if !local { return None; }
                let ad = [0, 0, b[2], b[3]];
                let l = lx(e);
                let nb = l.socks.get(&sid).map_or(false, |s| s.nb);
                let id = us_new(l, UsSt::New, nb);
                l.fds.insert(fd, Fd::Unix { id });
                let port = u16::from_be_bytes([ad[2], ad[3]]);
                return Some(us_connect(lx(e), id, &format!("tcp:{}", port), false));
            }
            _ => return None,
        }
    }
    let Some(Fd::Unix { id }) = l.fds.get(&fd).cloned() else { return None };
    Some(match nr {
        49 => { // bind
            let Some(name) = unix_name(e, a1, a2) else { return Some(-EINVAL) };
            let l = lx(e);
            if l.us_names.get(&name).map_or(false, |&o| us_held(l, o)) || (!name.starts_with('@') && l.fs.contains_key(&name)) { return Some(-98); }
            if !name.starts_with('@') { l.fs.insert(name.clone(), Node::File(Vec::new())); }
            l.us.get_mut(&id).unwrap().name = name;
            0
        }
        50 => { // listen
            let l = lx(e);
            let u = l.us.get_mut(&id).unwrap();
            if !matches!(u.st, UsSt::New | UsSt::Listen(_)) { return Some(-EINVAL); }
            if u.name.is_empty() { u.name = format!("@inksoc-{}", id); }
            if matches!(u.st, UsSt::New) { u.st = UsSt::Listen(Default::default()); }
            let n = u.name.clone();
            l.us_names.insert(n, id);
            0
        }
        42 => { // connect
            let Some(name) = unix_name(e, a1, a2) else { return Some(-EINVAL) };
            us_connect(lx(e), id, &name, true)
        }
        43 | 288 => { // accept / accept4
            let l = lx(e);
            let u = l.us.get_mut(&id).unwrap();
            let nb = u.nb;
            let UsSt::Listen(q) = &mut u.st else { return Some(-EINVAL) };
            let Some(c) = q.pop_front() else {
                if nb { return Some(-EAGAIN); }
                l.want = Some(Cond::Fds(vec![(Fd::Unix { id }, 1)]));
                return Some(BLOCKED);
            };
            let flags = if nr == 288 { a3 } else { 0 };
            if let Some(x) = l.us.get_mut(&c) { x.nb = flags & 0x800 != 0; }
            let n = l.new_fd(Fd::Unix { id: c });
            if flags & 0x80000 != 0 { l.cloexec.insert(n); }
            let peer = match l.us.get(&c).map(|u| &u.st) { Some(UsSt::Conn { peer, .. }) => *peer, _ => 0 };
            let pu = l.us.get(&peer).map(|p| Us { st: UsSt::New, nb: false, name: p.name.clone(), port: if l.us[&c].port != 0 { 50000 + peer as u16 } else { 0 }, shut_wr: false });
            if let Some(pu) = pu { write_unix_addr(e, a1, a2, &pu); }
            n as i64
        }
        44 | 46 => { // sendto / sendmsg (with SCM_RIGHTS: the descriptors travel too)
            if nr == 46 {
                let (ctl, clen) = (e.r64(a1 + 32), e.r64(a1 + 40));
                let mut o = 0u64;
                while ctl != 0 && o + 16 <= clen {
                    let (len, lvl, ty) = (e.r64(ctl + o), e.r32(ctl + o + 8), e.r32(ctl + o + 12));
                    if len < 16 { break; }
                    if lvl == 1 && ty == 1 {
                        let fds: Vec<i32> = (0..(len - 16) / 4).map(|k| e.r32(ctl + o + 16 + 4 * k) as i32).collect();
                        let l = lx(e);
                        let v: Vec<Fd> = fds.iter().filter_map(|f| l.fds.get(f).cloned()).collect();
                        if let Some(UsSt::Conn { tx, .. }) = l.us.get(&id).map(|u| &u.st) {
                            let tx = *tx;
                            let at = l.pread.get(&tx).copied().unwrap_or(0) + l.pipes.get(&tx).map_or(0, |b| b.len() as u64);
                            l.passed.entry(tx).or_default().push_back((at, v));
                        }
                    }
                    o += (len + 7) & !7;
                }
            }
            let d = if nr == 44 {
                let mut d = vec![0u8; a2.min(1 << 24) as usize];
                let _ = e.mmu.read_bytes(a1, &mut d);
                d
            } else {
                let (iov, iovn) = (e.r64(a1 + 16), e.r64(a1 + 24));
                let mut d = Vec::new();
                for k in 0..iovn.min(64) {
                    let (b, n) = (e.r64(iov + 16 * k), e.r64(iov + 16 * k + 8).min(1 << 24));
                    let mut c = vec![0u8; n as usize]; let _ = e.mmu.read_bytes(b, &mut c); d.extend_from_slice(&c);
                }
                d
            };
            us_write(lx(e), id, &d)
        }
        45 | 47 => { // recvfrom / recvmsg
            if nr == 45 { us_read(e, id, a1, a2, a3 & 0x40 != 0) } else {
                let (iov, iovn) = (e.r64(a1 + 16), e.r64(a1 + 24));
                let (b, n) = (e.r64(iov), if iovn > 0 { e.r64(iov + 8) } else { 0 });
                let v = us_read(e, id, b, n, a2 & 0x40 != 0);
                if v >= 0 {
                    e.w32(a1 + 8, 0); e.w32(a1 + 48, 0);
                    let (ctl, clen) = (e.r64(a1 + 32), e.r64(a1 + 40));
                    let l = lx(e);
                    let got = if v > 0 && ctl != 0 { l.us_got.take() } else { l.us_got = None; None };
                    match got {
                        Some(fds) if clen >= 16 + 4 * fds.len() as u64 => {
                            let nums: Vec<i32> = fds.into_iter().map(|f| { let n = l.new_fd(f); if a2 & 0x4000_0000 != 0 { l.cloexec.insert(n); } n }).collect();
                            let len = 16 + 4 * nums.len() as u64;
                            e.w64(ctl, len); e.w32(ctl + 8, 1); e.w32(ctl + 12, 1);
                            for (k, n) in nums.iter().enumerate() { e.w32(ctl + 16 + 4 * k as u64, *n as u32); }
                            e.w64(a1 + 40, (len + 7) & !7);
                        }
                        Some(_) => { e.w64(a1 + 40, 0); e.w32(a1 + 48, 8); } // MSG_CTRUNC
                        None => e.w64(a1 + 40, 0),
                    }
                }
                v
            }
        }
        48 => { if a1 >= 1 { if let Some(u) = lx(e).us.get_mut(&id) { u.shut_wr = true; } } 0 } // shutdown
        54 => 0, // setsockopt
        55 => { // getsockopt: SO_ERROR 0, SO_TYPE stream, SO_PEERCRED this machine's user
            let (lvl, opt) = (a1, a2);
            let v: Vec<u8> = if lvl == 1 && opt == 17 { let l = lx(e); let mut b = l.pid.to_le_bytes().to_vec(); b.extend_from_slice(&l.ids.0.to_le_bytes()); b.extend_from_slice(&l.ids.1.to_le_bytes()); b } else if lvl == 1 && opt == 3 { 1u32.to_le_bytes().to_vec() } else { 0u32.to_le_bytes().to_vec() };
            if a3 != 0 { let _ = e.mmu.write_bytes(a3, &v); }
            if a4 != 0 { e.w32(a4, v.len() as u32); }
            0
        }
        51 | 52 => { // getsockname / getpeername
            let l = lx(e);
            let u = l.us.get(&id).unwrap();
            let who = if nr == 51 { id } else { match u.st { UsSt::Conn { peer, .. } => peer, _ => return Some(-ENOTCONN) } };
            let x = l.us.get(&who).map(|p| Us { st: UsSt::New, nb: false, name: p.name.clone(), port: p.port, shut_wr: false });
            match x { Some(x) => { write_unix_addr(e, a1, a2, &x); 0 } None => -ENOTCONN }
        }
        _ => { let _ = a5; -EOPNOTSUPP }
    })
}

/// Connects local socket `id` to the listener named `name` (the server end waits in its queue).
fn us_connect(l: &mut Lx, id: u32, name: &str, unix: bool) -> i64 {
    let Some(&srv) = l.us_names.get(name) else { return if unix && !l.fs.contains_key(name) && !name.starts_with('@') { -ENOENT } else { -ECONNREFUSED } };
    if !us_held(l, srv) { l.us_names.remove(name); return -ECONNREFUSED; }
    if !matches!(l.us.get(&id).map(|u| &u.st), Some(UsSt::New)) { return -106; } // EISCONN
    let port = l.us.get(&srv).map_or(0, |u| u.port);
    let (p, q) = (l.next_pipe, l.next_pipe + 1); l.next_pipe += 2;
    l.pipes.insert(p, Default::default()); l.pipes.insert(q, Default::default());
    let c = us_new(l, UsSt::Conn { rx: p, tx: q, peer: id }, false);
    { let u = l.us.get_mut(&c).unwrap(); u.name = name.to_string(); u.port = port; }
    { let u = l.us.get_mut(&id).unwrap(); u.st = UsSt::Conn { rx: q, tx: p, peer: c }; if port != 0 { u.port = 50000 + id as u16; } }
    if let Some(Us { st: UsSt::Listen(qq), .. }) = l.us.get_mut(&srv) { qq.push_back(c); }
    0
}

fn read_addr(e: &mut Engine, a: u64, len: u64) -> Option<[u8; 16]> {
    if a == 0 || len < 8 { return None; }
    let mut b = [0u8; 28];
    let _ = e.mmu.read_bytes(a, &mut b[..(len as usize).min(28)]);
    let mut out = [0u8; 16];
    match u16::from_le_bytes([b[0], b[1]]) {
        2 => { out[..8].copy_from_slice(&b[..8]); }
        10 => { // IPv6: only IPv4-mapped (::ffff:a.b.c.d)
            if b[8..18].iter().any(|&c| c != 0) || b[18] != 0xFF || b[19] != 0xFF { return Some([0xFF; 16]); }
            out[0] = 2; out[2] = b[2]; out[3] = b[3]; out[4..8].copy_from_slice(&b[20..24]);
        }
        _ => return Some([0xFF; 16]),
    }
    Some(out)
}

fn write_addr(e: &mut Engine, a: u64, lenp: u64, ad: &[u8; 16]) {
    if a == 0 { return; }
    let _ = e.mmu.write_bytes(a, ad);
    if lenp != 0 { e.w32(lenp, 16); }
}

fn net_call(e: &mut Engine, nr: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> i64 {
    if let Some(v) = unix_call(e, nr, a0, a1, a2, a3, a4, a5) { return v; }
    let fd = a0 as i32;
    let sid = match lx(e).fds.get(&fd) { Some(Fd::Sock { id }) => Some(*id), _ => None };
    match nr {
        41 => { // socket
            if a0 != 2 && a0 != 10 { return -EAFNOSUPPORT; }
            let ty = a1 & 0xF;
            if ty != 1 && ty != 2 { return -EOPNOTSUPP; }
            let l = lx(e);
            l.next_sock += 1;
            let id = l.next_sock;
            l.socks.insert(id, Sock { udp: ty == 2, nb: a1 & 0x800 != 0, ..Default::default() });
            let n = l.new_fd(Fd::Sock { id });
            if a1 & 0x80000 != 0 { l.cloexec.insert(n); }
            n as i64
        }
        53 => -EOPNOTSUPP, // socketpair
        288 => -EOPNOTSUPP, // accept4
        _ if sid.is_none() => if lx(e).fds.contains_key(&fd) { -88 } else { -EBADF }, // ENOTSOCK
        42 => { // connect
            let id = sid.unwrap();
            let Some(ad) = read_addr(e, a1, a2) else { return -EINVAL };
            if ad[0] == 0xFF { return -ENETUNREACH; }
            let l = lx(e);
            let s = l.socks.get_mut(&id).unwrap();
            s.peer = ad;
            if s.udp { return 0; }
            if s.open { return -106; } // EISCONN
            s.open = true;
            let port = u16::from_be_bytes([ad[2], ad[3]]);
            let host = if ad[4] == 198 && ad[5] == 18 {
                let k = ((ad[6] as usize) << 8 | ad[7] as usize).wrapping_sub(1);
                l.dns.get(k).cloned().unwrap_or_default()
            } else { format!("{}.{}.{}.{}", ad[4], ad[5], ad[6], ad[7]) };
            net_event(&mut l.net_out, 1, id, format!("{}:{}", host, port).as_bytes());
            0
        }
        44 | 46 => { // sendto / sendmsg
            let id = sid.unwrap();
            let (d, to) = if nr == 44 {
                let mut d = vec![0u8; a2.min(1 << 20) as usize];
                let _ = e.mmu.read_bytes(a1, &mut d);
                (d, read_addr(e, a4, a5))
            } else {
                let (name, nlen, iov, iovn) = (e.r64(a1), e.r32(a1 + 8) as u64, e.r64(a1 + 16), e.r64(a1 + 24));
                let mut d = Vec::new();
                for k in 0..iovn.min(64) {
                    let (b, n) = (e.r64(iov + 16 * k), e.r64(iov + 16 * k + 8).min(1 << 20));
                    let mut c = vec![0u8; n as usize]; let _ = e.mmu.read_bytes(b, &mut c); d.extend_from_slice(&c);
                }
                (d, read_addr(e, name, nlen))
            };
            if to.map_or(false, |t| t[0] == 0xFF) { return -ENETUNREACH; }
            sock_send(lx(e), id, &d, to)
        }
        45 | 47 => { // recvfrom / recvmsg
            let id = sid.unwrap();
            if nr == 45 {
                let (v, from) = sock_recv(e, id, a1, a2, a3 & 0x40 != 0);
                if let Some(f) = from { write_addr(e, a4, a5, &f); }
                v
            } else {
                let (name, iov, iovn) = (e.r64(a1), e.r64(a1 + 16), e.r64(a1 + 24));
                let (b, n) = (e.r64(iov), if iovn > 0 { e.r64(iov + 8) } else { 0 });
                let (v, from) = sock_recv(e, id, b, n, a2 & 0x40 != 0);
                if let Some(f) = from { if name != 0 { let _ = e.mmu.write_bytes(name, &f); e.w32(a1 + 8, 16); } }
                if v >= 0 { e.w64(a1 + 40, 0); e.w32(a1 + 48, 0); }
                v
            }
        }
        48 | 49 | 54 => 0, // shutdown / bind / setsockopt
        50 => -EOPNOTSUPP, // listen
        43 => -EOPNOTSUPP, // accept
        51 | 52 => { // getsockname / getpeername
            let id = sid.unwrap();
            let ad = if nr == 52 { lx(e).socks.get(&id).map_or([0; 16], |s| s.peer) } else { let mut a = [0u8; 16]; a[0] = 2; a[4] = 10; a[7] = 15; a };
            if nr == 52 && ad[0] == 0 { return -ENOTCONN; }
            write_addr(e, a1, a2, &ad);
            0
        }
        55 => { // getsockopt: SO_ERROR and the rest read as 0
            let id = sid.unwrap();
            let err = if lx(e).socks.get(&id).map_or(false, |s| s.failed) { ECONNREFUSED } else { 0 };
            if a3 != 0 { e.w32(a3, err as u32); }
            if a4 != 0 { e.w32(a4, 4); }
            0
        }
        _ => -ENOSYS,
    }
}

/// The page collects what the sockets want to send: [op u8][id u32][len u32][data]...
/// (1 connect "host:port", 2 data, 3 close). Sockets no process holds any more are closed here.
#[no_mangle]
pub extern "C" fn linux_net_poll() -> i32 {
    let Some(l) = crate::eng().s.lx.as_mut() else { return 0 };
    let held = |l: &Lx, id: u32| {
        let is = |f: &Fd| matches!(f, Fd::Sock { id: i } if *i == id);
        l.fds.values().any(is) || l.procs.values().any(|p| p.saved.as_ref().map_or(false, |s| s.fds.values().any(is)))
    };
    let gone: Vec<u32> = l.socks.keys().copied().filter(|&id| !held(l, id)).collect();
    for id in gone {
        let s = l.socks.remove(&id).unwrap();
        if !s.udp && s.open && !s.closed { net_event(&mut l.net_out, 3, id, &[]); }
    }
    if l.net_out.is_empty() { return 0; }
    let v = std::mem::take(&mut l.net_out);
    crate::set_out(v) as i32
}

/// The page hands data to socket `id`: kind 1 = bytes (in the buffer), 2 = closed by the other side, 3 = could not connect.
#[no_mangle]
pub extern "C" fn linux_net_in(id: u32, kind: u32) {
    let b = unsafe { crate::take_buf() };
    let Some(l) = crate::eng().s.lx.as_mut() else { return };
    let Some(s) = l.socks.get_mut(&id) else { return };
    match kind { 1 => s.rx.extend(b), 2 => s.closed = true, _ => s.failed = true }
}

/// Has the file tree changed since the last call? (the page then saves it with `linux_save_tar`)
#[no_mangle]
pub extern "C" fn linux_dirty() -> u32 { crate::eng().s.lx.as_mut().map_or(0, |l| std::mem::take(&mut l.dirty) as u32) }

/// The whole file tree as a tar (without /proc, /dev and /tmp), in out_ptr(); its size.
#[no_mangle]
pub extern "C" fn linux_save_tar() -> u32 {
    let Some(l) = crate::eng().s.lx.as_ref() else { return 0 };
    let mut t: Vec<u8> = Vec::new();
    fn hdr(t: &mut Vec<u8>, name: &str, ty: u8, size: usize, link: &str, mode: u32, own: (u32, u32)) {
        if name.len() > 99 { // GNU long name
            let mut d = name.as_bytes().to_vec(); d.push(0);
            hdr(t, "././@LongLink", b'L', d.len(), "", 0o644, (0, 0));
            t.extend_from_slice(&d); t.resize((t.len() + 511) / 512 * 512, 0);
        }
        let mut h = [0u8; 512];
        let n = name.as_bytes(); h[..n.len().min(99)].copy_from_slice(&n[..n.len().min(99)]);
        h[100..108].copy_from_slice(format!("{:07o}\0", mode).as_bytes());
        h[108..116].copy_from_slice(format!("{:07o}\0", own.0 & 0o777_7777).as_bytes()); h[116..124].copy_from_slice(format!("{:07o}\0", own.1 & 0o777_7777).as_bytes());
        h[124..136].copy_from_slice(format!("{:011o}\0", size).as_bytes());
        h[136..148].copy_from_slice(b"00000000000\0");
        h[156] = ty;
        let lk = link.as_bytes(); h[157..157 + lk.len().min(99)].copy_from_slice(&lk[..lk.len().min(99)]);
        h[257..263].copy_from_slice(b"ustar\0"); h[263..265].copy_from_slice(b"00");
        h[148..156].fill(b' ');
        let sum: u32 = h.iter().map(|&c| c as u32).sum();
        h[148..156].copy_from_slice(format!("{:06o}\0 ", sum).as_bytes());
        t.extend_from_slice(&h);
    }
    for (p, n) in l.fs.iter() {
        if p.starts_with("/proc") || p.starts_with("/dev") || p.starts_with("/tmp/") { continue; }
        let name = &p[1..];
        match n {
            Node::Dir => { let (m, u, g) = perm(l, p, true); hdr(&mut t, &format!("{}/", name), b'5', 0, "", m, (u, g)) }
            Node::Link(k) => {
                if k.len() > 99 { let mut d = k.as_bytes().to_vec(); d.push(0); hdr(&mut t, "././@LongLink", b'K', d.len(), "", 0o644, (0, 0)); t.extend_from_slice(&d); t.resize((t.len() + 511) / 512 * 512, 0); }
                hdr(&mut t, name, b'2', 0, k, 0o777, (0, 0))
            }
            Node::File(d) => { let (m, u, g) = perm(l, p, false); hdr(&mut t, name, b'0', d.len(), "", m, (u, g)); t.extend_from_slice(d); t.resize((t.len() + 511) / 512 * 512, 0); }
        }
    }
    t.extend_from_slice(&[0u8; 1024]);
    crate::set_out(t)
}
