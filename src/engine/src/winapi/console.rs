// Console input: the lines typed in the page's Command Prompt window.
//
// The page turns typing on for a program it runs as a console (cmd.exe): after that,
// reading the console input handle waits for a line instead of failing at once, the way
// Windows' console host hands a program what the user typed. Every other program keeps
// the old answer (no input, end of file), so installers never wait for a keyboard.

use super::*;
use crate::sched::W;

/// The console input handle (GetStdHandle(STD_INPUT_HANDLE)).
const STDIN: u64 = 3;

pub(super) fn call(e: &mut Engine, name: &str) -> Option<u64> {
    if !e.s.con_live {
        return None;
    }
    let a0 = e.arg(0);
    Some(match name {
        "ReadConsoleW" => read_console(e, a0, true),
        "ReadConsoleA" => read_console(e, a0, false),
        "ReadFile" if a0 == STDIN => read_console(e, a0, false),
        "GetNumberOfConsoleInputEvents" if a0 == STDIN => {
            let n = e.s.con_in.len() as u32;
            let out = e.arg(1);
            e.w32(out, n);
            1
        }
        _ => return None,
    })
}

/// ReadConsoleW/ReadConsoleA(handle, buffer, chars, read*, control) and ReadFile(handle, buffer,
/// bytes, read*, overlapped): one typed line, ending in "\r\n", or a wait until there is one.
fn read_console(e: &mut Engine, h: u64, wide: bool) -> u64 {
    let (buf, n, done) = (e.arg(1), e.arg(2) as u32 as usize, e.arg(3));
    if h != STDIN {
        e.set_last_error(6); // ERROR_INVALID_HANDLE
        return 0;
    }
    if e.s.con_in.is_empty() {
        // the call runs again when a line arrives
        return e.wait_on(W::ConIn, u64::MAX);
    }
    let line = take_line(&mut e.s.con_in, n, wide);
    if wide {
        let w: Vec<u16> = line.encode_utf16().collect();
        for (i, c) in w.iter().enumerate() {
            e.w16(buf + 2 * i as u64, *c);
        }
        if done != 0 {
            e.w32(done, w.len() as u32);
        }
    } else {
        let _ = e.mmu.write_bytes(buf, line.as_bytes());
        if done != 0 {
            e.w32(done, line.len() as u32);
        }
    }
    1
}

/// The next typed line (or as much of it as fits in `room` characters or bytes), with the
/// page's "\n" turned into the "\r\n" Windows programs expect. What does not fit stays queued.
fn take_line(queue: &mut Vec<u8>, room: usize, wide: bool) -> String {
    let end = queue.iter().position(|&b| b == b'\n').map_or(queue.len(), |i| i + 1);
    let mut text = String::from_utf8_lossy(&queue[..end]).into_owned();
    if text.ends_with('\n') && !text.ends_with("\r\n") {
        text.pop();
        text.push_str("\r\n");
    }
    let size = |s: &str| if wide { s.encode_utf16().count() } else { s.len() };
    let mut cut = text.len();
    while cut > 0 && size(&text[..cut]) > room.max(1) {
        cut -= 1;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
    }
    let used = if cut == text.len() { end } else { cut.min(end) };
    queue.drain(..used);
    text.truncate(cut);
    text
}

/// Text typed in the page's Command Prompt (UTF-8, lines end in "\n"). Called once with
/// nothing (n = 0) when the program starts, which turns console input on for this run.
#[no_mangle]
pub extern "C" fn win_stdin(n: u32) {
    let e = crate::eng();
    let _g = crate::lock::lock();
    let b = unsafe { crate::take_buf() };
    e.s.con_live = true;
    e.s.con_in.extend_from_slice(&b[..(n as usize).min(b.len())]);
}
