# Wine 11.0 DLLs

These are the x86-64 PE builds of Wine 11.0's DLLs, taken unchanged except for
stripped debug symbols (`x86_64-w64-mingw32-strip`) from WineHQ's package
`wine-stable-amd64 11.0.0.0~noble-1`, plus `l_intl.nls` (the case tables).
Each file is renamed `wine-<name>.dll`.

InkSoC runs them as program code: any Windows function the engine does not
implement itself comes from these DLLs. Wine's ntdll system calls are answered
by the engine (`src/engine/src/ntsys.rs`, `sched.rs`), playing the part of
Wine's Unix side.

Wine is free software under the GNU Lesser General Public License 2.1 or later
(https://www.winehq.org/license). The source for this exact version is the
Wine
11.0 release (https://dl.winehq.org/wine/source/11.0/).

`roots.der` is not part of Wine: it is Mozilla's list of trusted root certificates
(as packaged by Debian's `ca-certificates`), one DER certificate after another.
The engine hands it to crypt32 the way Wine's Unix side hands over the system's
roots, so programs that check HTTPS certificates with CryptoAPI trust the same
authorities as a browser.

Loaded on demand: `wine-*.dll` files that are not in `list.json` (for example `wtsapi32`,
`msvcr120`, `dinput8`, `rsaenh`) are fetched the first time a program needs one (worker.js
`lazyFetch`, engine `want_file`), so they cost nothing until used. This includes the older
Visual C++ runtimes (msvcr70-110, msvcp60-110, vcomp, atl), msxml3/6, vbscript, jscript,
scrrun, cryptsp/cryptbase/cryptnet/cryptui, XAudio2 2.7-2.9, d3dx9_36 and d3dcompiler_43.
