<p align="center"><img src=".github/icon.svg" width="96" alt="InkMind icon"></p>

<h1 align="center">InkMind</h1>

<p align="center">AI chat that runs on your own device: small open models (Qwen, Llama), private, and offline after the first download.</p>

<p align="center"><b><a href="https://hub.inkdos.workers.dev/InkMind/">Open InkMind</a></b> · part of <a href="https://github.com/InkDOSHub/inkdoshub.github.io">InkDOS Hub</a> · MIT license</p>

<p align="center"><img src=".github/prints/InkMind-iphone.png" width="28%" alt="InkMind on iPhone"> <img src=".github/prints/InkMind-ipad.png" width="68%" alt="InkMind on a larger screen"></p>

## What it does

- Chat with small GGUF models: **Qwen 2.5 0.5B**, **Llama 3.2 1B** and **Qwen 2.5 1.5B**, all running inside the browser with [wllama](https://github.com/ngxson/wllama) (llama.cpp compiled to WebAssembly, single or multithreaded).
- The model is downloaded once from Hugging Face (through the Hub's Worker, because Safari fails the direct download) and kept in the browser storage (OPFS). After that, nothing leaves the device.
- Attach a document (PDF, Word, text) and ask about it: its text is extracted locally with the Hub's PDF and ZIP readers.
- In the Hub's Windows app the same page uses native llama.cpp, so larger models run fast.
- Experimental: speed depends on the device; on iPad and iPhone the smaller models are the comfortable ones.

## Files

| Path | What |
|---|---|
| `index.html`, `app.js` | The chat |
| `download-worker.js` | Model download worker (written to OPFS a piece at a time) |
| `extract.js` | Text extraction from attached files |
| `wllama/` | wllama 2.3.7 (2.4+ needs Memory64, which Safari lacks) |

## Part of InkDOS Hub

InkMind is one of the apps of [InkDOS Hub](https://hub.inkdos.workers.dev/), a free, open-source home screen of tools for iPad, iPhone and PC. It is published by GitHub Pages at `inkdoshub.github.io/InkMind/`, on the same site as the Hub, so it shares the Hub's top bar, theme and language files (`/assets/`). The public address `hub.inkdos.workers.dev/InkMind/` is the Hub's Cloudflare Worker, which serves these files with cross-origin isolation (multithreaded WebAssembly).

Other InkDOS Hub repositories: [Hub](https://github.com/InkDOSHub/inkdoshub.github.io) · [InkSoC](https://github.com/InkDOSHub/InkSoC) · [InkShell](https://github.com/InkDOSHub/InkShell) · [InkCam](https://github.com/InkDOSHub/InkCam) · [InkMind](https://github.com/InkDOSHub/InkMind) · [InkArcade](https://github.com/InkDOSHub/InkArcade) · [InkBench](https://github.com/InkDOSHub/InkBench)

## License

Code made here: [MIT](LICENSE). Third party: wllama (MIT, `wllama/LICENSE`). Models keep their own licenses (Qwen: Apache-2.0; Llama 3.2: Llama 3.2 Community License).
