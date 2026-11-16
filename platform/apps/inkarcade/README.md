<p align="center"><img src=".github/icon.svg" width="96" alt="InkArcade icon"></p>

<h1 align="center">InkArcade</h1>

<p align="center">Play your own retro games in the browser: PS1, SNES, N64, NES and Game Boy Advance, with touch controls.</p>

<p align="center"><b><a href="https://hub.inkdos.workers.dev/InkArcade/">Open InkArcade</a></b> · part of <a href="https://github.com/InkDOSHub/inkdoshub.github.io">InkDOS Hub</a> · MIT license</p>

<p align="center"><img src=".github/prints/InkArcade-iphone.png" width="28%" alt="InkArcade on iPhone"> <img src=".github/prints/InkArcade-ipad.png" width="68%" alt="InkArcade on a larger screen"></p>

## What it does

- **Open game**: pick a ROM from your device; the right console is chosen by the file type.
- Consoles: PlayStation (PS1), Super Nintendo, Nintendo 64, NES and Game Boy Advance, through [EmulatorJS](https://github.com/EmulatorJS/EmulatorJS) cores (libretro) running in WebAssembly.
- **PS1 BIOS** (optional): add your own BIOS file for better PS1 compatibility.
- On-screen gamepad on touch screens, saves kept in the browser.
- No games are included: use only games you own.

## Files

| Path | What |
|---|---|
| `index.html`, `app.js` | The app (game picker, BIOS, settings) |
| `ejs/` | EmulatorJS loader, cores and translations |

## Part of InkDOS Hub

InkArcade is one of the apps of [InkDOS Hub](https://hub.inkdos.workers.dev/), a free, open-source home screen of tools for iPad, iPhone and PC. It is published by GitHub Pages at `inkdoshub.github.io/InkArcade/`, on the same site as the Hub, so it shares the Hub's top bar, theme and language files (`/assets/`). The public address `hub.inkdos.workers.dev/InkArcade/` is the Hub's Cloudflare Worker, which serves these files with cross-origin isolation (multithreaded WebAssembly).

Other InkDOS Hub repositories: [Hub](https://github.com/InkDOSHub/inkdoshub.github.io) · [InkSoC](https://github.com/InkDOSHub/InkSoC) · [InkShell](https://github.com/InkDOSHub/InkShell) · [InkCam](https://github.com/InkDOSHub/InkCam) · [InkMind](https://github.com/InkDOSHub/InkMind) · [InkArcade](https://github.com/InkDOSHub/InkArcade) · [InkBench](https://github.com/InkDOSHub/InkBench)

## License

Code made here: [MIT](LICENSE). Third party: EmulatorJS (GPL-3.0, `ejs/LICENSE.txt`) and its libretro cores (each under its own license).
