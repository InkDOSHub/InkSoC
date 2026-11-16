// Unix libraries: the Unix-side halves of Wine DLLs.
//
// A Wine DLL with a Unix half (opengl32.dll, winepulse.drv) asks ntdll for its "unix funcs" handle
// once (NtQueryVirtualMemory, class MemoryWineUnixFuncs), then calls
// __wine_unix_call_dispatcher(handle, code, params) for each function. The engine plays
// the Unix side: the dispatcher points at a native thunk, and each handle names one of
// the engine's modules.

use crate::status::*;
use crate::Engine;

/// NtQueryVirtualMemory's class for a module's unix funcs handle (1001: the same for a
/// 32-bit DLL under WoW64, whose calls then carry 32-bit structures).
pub const MEMORY_WINE_UNIX_FUNCS: u32 = 1000;
pub const MEMORY_WINE_UNIX_WOW64_FUNCS: u32 = 1001;

const OPENGL: u64 = 1;
const AUDIO: u64 = 2;
const WS2: u64 = 3;
const CRYPT32: u64 = 4;

/// The variable ntdll exports for the dispatcher: a pointer to the native thunk.
pub fn data_export(e: &mut Engine, dll: &str, name: &str) -> Option<u64> {
    if dll != "ntdll.dll" || name != "__wine_unix_call_dispatcher" {
        return None;
    }
    if e.s.unix_dispatcher == 0 {
        let a = e.heap_alloc(8, true);
        let t = e.thunk("ntdll.dll", "__wine_unix_call");
        e.w64(a, t);
        e.s.unix_dispatcher = a;
    }
    Some(e.s.unix_dispatcher)
}

/// The unix funcs handle of the module at `base`, or None when it has no Unix side here.
/// A 32-bit DLL gets one only where the engine reads 32-bit structures (net.rs follows the
/// process: a WoW64 process hands over 32-bit layouts).
pub fn handle_of(e: &Engine, base: u64, wow64: bool) -> Option<u64> {
    // the engine's own modules, or a DLL that a loader (WoW64's 32-bit one) mapped as a section
    let name = match e.s.modules.iter().find(|m| m.base == base) {
        Some(m) => m.name.clone(),
        None => e.s.image_files.get(&base)?.rsplit(['\\', '/']).next()?.to_ascii_lowercase(),
    };
    let h = match name.trim_start_matches("wine32-").trim_start_matches("wine-") {
        "opengl32.dll" => OPENGL,
        "winepulse.drv" => AUDIO,
        "ws2_32.dll" => WS2,
        "crypt32.dll" => CRYPT32,
        _ => return None,
    };
    if wow64 && !matches!(h, WS2 | CRYPT32) {
        return None;
    }
    Some(h)
}

/// __wine_unix_call(handle, code, params).
pub fn call(e: &mut Engine, name: &str) -> Option<u64> {
    if name != "__wine_unix_call" {
        return e.not_mine();
    }
    let (handle, code, params) = (e.arg(0), e.arg(1) as u32, e.arg(2));
    if handle == OPENGL || handle == AUDIO {
        if let Some(r) = e.to_core0() {
            return Some(r);
        }
    }
    Some(match handle {
        OPENGL => e.opengl_call(code, params),
        AUDIO => e.audio_call(code, params),
        WS2 => e.ws2_call(code, params),
        CRYPT32 => crypt32_call(e, code, params),
        _ => NOT_IMPLEMENTED,
    })
}

/// crypt32's Unix side: process attach and detach, and the trusted root certificates
/// (roots.der: Mozilla's list, one DER certificate after another). No PFX import.
fn crypt32_call(e: &mut Engine, code: u32, params: u64) -> u64 {
    match code {
        0 | 1 => OK, // process_attach, process_detach
        6 => enum_root_certs(e, params),
        _ => NOT_IMPLEMENTED,
    }
}

/// (buffer, size, needed*): the next root certificate; when it does not fit, only its size
/// (the caller grows the buffer and asks again).
fn enum_root_certs(e: &mut Engine, params: u64) -> u64 {
    let ps = if e.s.wow64 { 4 } else { 8 };
    let (buf, size, needed) = (e.rptr(params), e.r32(params + ps) as usize, e.rptr(params + 2 * ps));
    let off = e.s.root_off;
    let cert = match e.s.files.get("roots.der") {
        Some(d) => der_len(&d[off.min(d.len())..]).map(|n| d[off..off + n].to_vec()),
        None => None,
    };
    let Some(cert) = cert else {
        e.s.root_off = 0;
        return 0x8000_001A; // STATUS_NO_MORE_ENTRIES
    };
    e.w32(needed, cert.len() as u32);
    if cert.len() > size {
        return OK;
    }
    let _ = e.mmu.write_bytes(buf, &cert);
    e.s.root_off = off + cert.len();
    OK
}

/// The whole length of the DER element at the start of `d` (header included).
pub fn der_len(d: &[u8]) -> Option<usize> {
    if d.len() < 2 || d[0] != 0x30 {
        return None;
    }
    let (n, h) = match d[1] {
        l if l < 0x80 => (l as usize, 2),
        l => {
            let k = (l & 0x7f) as usize;
            if k == 0 || k > 3 || d.len() < 2 + k {
                return None;
            }
            (d[2..2 + k].iter().fold(0usize, |a, &b| a << 8 | b as usize), 2 + k)
        }
    };
    (h + n <= d.len()).then_some(h + n)
}
