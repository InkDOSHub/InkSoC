<p align="center"><img src=".github/icon.svg" width="96" alt="InkCam icon"></p>

<h1 align="center">InkCam</h1>

<p align="center">Use the iPhone camera as a PC webcam: up to 4K, paired by QR code, no account, video straight between your devices.</p>

<p align="center"><b><a href="https://hub.inkdos.workers.dev/InkCam/">Open InkCam</a></b> · part of <a href="https://github.com/InkDOSHub/inkdoshub.github.io">InkDOS Hub</a> · MIT license</p>

<p align="center"><img src=".github/prints/InkCam-iphone.png" width="28%" alt="InkCam on iPhone"> <img src=".github/prints/InkCam-ipad.png" width="68%" alt="InkCam on a larger screen"></p>

## What it does

- **Host** (the phone): opens the camera and sends it. **Guest** (the PC): receives the picture.
- Pairing by QR code or short code with [PeerJS](https://github.com/peers/peerjs); the video goes directly between the two devices over WebRTC (the public PeerJS server and Google's STUN are used only to find each other).
- Quality chosen on the PC (720p up to 4K, 30/60 fps), with automatic restart when the delay builds up.
- **Use it in Zoom, Teams, Meet:** with OBS Studio (Browser Source + Virtual Camera), or with the small Windows app in [`windows/`](windows/), which receives the video natively (GStreamer) and exposes it as the "InkDOS Hub Webcam" virtual camera (softcam).

## Files

| Path | What |
|---|---|
| `index.html`, `app.js` | The web app (Host and Guest) |
| `lib/` | PeerJS 1.5.5 and the QR code generator |
| `windows/` | Windows receiver (C#/.NET, WebView2, GStreamer, softcam) |
| `.github/workflows/webcam-windows.yml` | Builds the Windows app and publishes `InkDOSHub-Webcam-Windows.zip` to the [webcam-windows release](https://github.com/InkDOSHub/InkCam/releases/tag/webcam-windows) |

## Part of InkDOS Hub

InkCam is one of the apps of [InkDOS Hub](https://hub.inkdos.workers.dev/), a free, open-source home screen of tools for iPad, iPhone and PC. It is published by GitHub Pages at `inkdoshub.github.io/InkCam/`, on the same site as the Hub, so it shares the Hub's top bar, theme and language files (`/assets/`). The public address `hub.inkdos.workers.dev/InkCam/` is the Hub's Cloudflare Worker, which serves these files with cross-origin isolation (multithreaded WebAssembly).

Other InkDOS Hub repositories: [Hub](https://github.com/InkDOSHub/inkdoshub.github.io) · [InkSoC](https://github.com/InkDOSHub/InkSoC) · [InkShell](https://github.com/InkDOSHub/InkShell) · [InkCam](https://github.com/InkDOSHub/InkCam) · [InkMind](https://github.com/InkDOSHub/InkMind) · [InkArcade](https://github.com/InkDOSHub/InkArcade) · [InkBench](https://github.com/InkDOSHub/InkBench)

## License

Code made here: [MIT](LICENSE). Third party: PeerJS (MIT, `lib/`, see `UPSTREAM-LICENSE.txt` and `UPSTREAM-SOURCE.txt`); the Windows build downloads softcam (MIT) and GStreamer (LGPL) at build time.
