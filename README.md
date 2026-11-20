<p align="center"><img src=".github/icon.svg" width="96" alt="InkSoC icon"></p>

<p align="center"><b>The engine that runs Windows programs in InkOS, on iPad, iPhone and PC: our own x86-64 engine (JIT, WebAssembly) with Wine.</b></p>

<p align="center"><b><a href="https://inkos-5k7.pages.dev/?open=cmdexe">Open in InkOS</a></b> · part of <a href="https://github.com/InkDOSHub/inkdoshub.github.io">InkDOS Hub</a> · MIT license (Wine: LGPL)</p>

<p align="center"><img src=".github/prints/InkSoC-iphone.png" width="26%" alt="WineMine in an InkOS window on iPhone"> <img src=".github/prints/InkSoC-ipad.png" width="70%" alt="Command Prompt and WineMine in InkOS windows"></p>

# InkSoC

Our own engine to run Windows `.exe` files in the browser, WebKit first (iPhone/iPad).
A Rust engine compiled to WebAssembly runs the x86-64 (and 32-bit, WoW64) code and plays
the part of the kernel that Wine runs on: Wine's DLLs (in `bin/`) are the rest of Windows.

It has no desktop of its own: [InkOS](https://github.com/InkDOSHub/InkOS) is the desktop.
In InkOS, **CMD/EXE** opens Command Prompt, an `.exe`, `.msi` or `.lnk` from the Finder runs in
a window of its own that fits the program's window, and installed programs show in Apps under
"Programas Windows".

Addresses: https://hub.inkdos.workers.dev/InkSoC/ (multi-core) and https://inkdoshub.github.io/InkSoC/.
Opened on its own, the page starts Command Prompt at once. Until 2026-11-14 it lived in the Hub
repository as `apps/inksoc/`.

## Repository layout

| Path | What |
|---|---|
| `index.html`, `engine.js` | The engine page: no desktop of its own. InkOS shows it in a window; the program's screen fills it (zoomed to its windows), a console program shows as text. `?run=C:\path.exe` runs a program on drive C:, `?file=1` waits for a file from InkOS, nothing opens Command Prompt (cmd.exe) |
| `worker.js` | Core 0: loads the engine, runs the program, owns the screen and files |
| `core.js`, `sharedmem.js`, `swapio.js` | Extra cores on shared memory, and the swap file worker |
| `gl.js`, `gl-funcs.js` | OpenGL on WebGL 2 (`gl-funcs.js` is generated: do not edit) |
| `pad.js`, `prefs.js`, `unzip.js` | On-screen gamepad (the controller button on touch screens), saved settings, .zip install onto drive C: |
| `inksoc.wasm`, `inksoc-mt.wasm`, `wine.wasm` | Built engine (single and multi-core) and Wine's msvcrt |
| `bin/` | Wine DLLs (`wine/` 64-bit, `wine32/` 32-bit), `system32/` tools such as `cmd.exe`, 7-Zip, WineMine (`winemine.exe`, a quick test) |
| `src/engine/` | The engine (Rust). Map of the code: `docs/graph/SUMMARY.md` |
| `src/wine/` | Build of Wine's msvcrt for the engine |
| `docs/PROTOCOL.md` | The messages between this page and InkOS |
| `.github/workflows/engine.yml` | Builds both engines on every pull request and publishes them from `main` |

The Linux terminal is [InkShell](https://github.com/InkDOSHub/InkShell): it keeps its own copy of
this engine with a patch (`linux.rs` is shared).

## Engine (`src/engine/src`)

The Windows code is organized by area, like Wine: one Rust function per Windows API, with its name.

- CPU: `cpu.rs` (interpreter), `sse.rs`, `x87.rs` + `f80.rs` (exact 80-bit FPU), `jit.rs`.
- Memory and processes: `mmu.rs` (64-bit page table, fixed pool of real RAM, swap to OPFS),
  `heap.rs`, `pe.rs` (PE loader), `proc.rs` (child processes), `wow64.rs` (32-bit programs),
  `sched.rs` (threads), `lock.rs` (engine lock with several cores).
- Wine's Unix side: `ntsys/` (NT system calls), `winsys/` (NtUser*), `wingdi/` (NtGdi*),
  `opengl/`, `audio.rs`, `net.rs` (sockets), `registry.rs`, `npipe.rs`, `unixlib.rs`.
- Windows API written here: `kernel32.rs`, `msvcrt.rs`, `oleaut.rs`, `winapi/`, `eh.rs` (C++ exceptions),
  `hash.rs`; anything else falls through to Wine (`winefb.rs`).
- Files: `files.rs`, `store.rs` (drive C:, kept in the browser).
- `linux.rs`: Linux system calls, used by InkShell (which keeps its own patch on top of this engine).

More detail on a few parts:

- `src/engine/src/jit.rs`: JIT. Hot x86-64 blocks (40 visits) become small wasm
  modules that the page compiles and adds to the engine's function table.
  Registers live in wasm locals, flags are lazy (a cmp + jcc becomes one wasm
  compare), a jump back to the block start is a wasm loop, and memory uses the
  same software TLB inline. Instructions it does not translate run through the
  interpreter one at a time. `setPref('jit', false)` in the page's console turns it off.
- `src/wine/`: Wine's msvcrt (string functions, printf, ctype; Wine 10.0, LGPL)
  compiled to `wine.wasm`. The C files are compiled as x86-64 Windows code (so
  structs and `va_list` match the program exactly), retargeted to wasm64, and every
  load/store is routed through the engine's page table (`wasm-opt
  --instrument-memory`, then `--memory64-lowering`). Program pointers are used as
  is, even when their page is in swap. `glue.c` gives it a fixed "C" locale and
  the entry point `ink_call`. Wine's own data and stack sit at 1-4 MB of the
  engine's memory (the engine is linked with `--global-base=4194304`).
- `src/engine/src/sched.rs`: program threads. Each thread is a saved CPU state;
  the engine switches every 200,000 instructions or when a thread waits
  (WaitFor*, Sleep, critical sections, SRW locks, events, mutexes, semaphores).
  When every thread waits on a timeout, `run` returns 3 and the page waits.

## Build

```
cd src/engine && cargo build --release --target wasm32-unknown-unknown && cp target/wasm32-unknown-unknown/release/inksoc.wasm ../../
src/wine/build.sh   # needs clang/llc/wasm-ld 18, binaryen, git (fetches Wine 10.0)
sh src/engine/build-mt.sh   # multi-core engine: nightly Rust with rust-src
```

On `main`, `.github/workflows/engine.yml` builds both engines and commits them; a pull request
only checks that they build.

Own programs: build with `-D__USE_MINGW_ANSI_STDIO=0` so printf comes from msvcrt.dll
(mingw's own printf uses x87 instructions, not supported yet).

## Debugging

- `?winedebug=1`: Wine's errors; `2`: warnings too, plus process starts and exits and, every
  5 s, the engine calls made since (what a stuck program is doing); `3` with
  `&winetrace=channel,…`: Wine's TRACE lines of those channels. All of it goes to the page's
  console text.

## Tests

The engine page is checked by InkOS's end-to-end tests (`tests/e2e/` in InkOS), which run
`cmd.exe` and WineMine in InkOS windows.

## Known limits

- An NSIS installer (Steam's) can stop responding on its last page, so its files are not saved
  (drive C: is written when the program ends).
- Wizard pages can leave pixels of the previous page behind.
- Programs built on Chromium (CEF), such as the Steam client, are out of reach.

## Part of InkDOS Hub

Other InkDOS Hub repositories: [Hub](https://github.com/InkDOSHub/inkdoshub.github.io) · [InkSoC](https://github.com/InkDOSHub/InkSoC) · [InkShell](https://github.com/InkDOSHub/InkShell) · [InkCam](https://github.com/InkDOSHub/InkCam) · [InkMind](https://github.com/InkDOSHub/InkMind) · [InkArcade](https://github.com/InkDOSHub/InkArcade) · [InkBench](https://github.com/InkDOSHub/InkBench)

## License

Code made here (engine, page, tools): [MIT](LICENSE). Third party: Wine builds in `bin/wine*` (LGPL-2.1, see their READMEs), 7-Zip `7za.exe` (`bin/7za-license.txt`), BusyBox (GPL-2.0).
