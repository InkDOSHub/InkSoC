# Third-party licenses: InkSoC

Code written for this project is MIT ([LICENSE](LICENSE)). Components below keep their own licenses. Rows marked **REVIEW** need a human decision (see InkOS autopilot run, 2026-11-20).

| Component | Location | License | Status |
|---|---|---|---|
| Wine builds | `bin/wine*`, `wine.wasm` | LGPL-2.1 | separate binaries, OK |
| Wine Tahoma bitmap font | `src/engine/fonts/tahoma.bin` (from Wine `fonts/tahoma.sfd` via `mkfont.py`), `include_bytes!` in the engine (`inksoc.wasm`) | LGPL-2.1-or-later (Wine; Bitstream Vera origin) | kept for the Windows GDI; source in repo, rebuildable. InkShell excludes it (cargo feature `tahoma` off) |
| InkSoC engine | `src/engine/` | MIT (`Cargo.toml`, `lib.rs` header) | OK |
| Wine msvcrt sources | `src/wine/` | LGPL-2.1 | **REVIEW**: LGPL source inside the MIT source tree; keep as a clearly separated module |
| 7-Zip `7za.exe` | `bin/7za.exe` | LGPL-2.1 + unRAR restriction (`bin/7za-license.txt`) | **REVIEW**: unRAR clause is not permissive |
| BusyBox | per README | GPL-2.0 | **REVIEW**: must stay a separate downloaded artifact |
| xterm.js | `vendor/` | MIT | OK |
