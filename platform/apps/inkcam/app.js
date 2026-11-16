/* InkDOS Hub Webcam (test version).

   The iPhone (Host) sends the camera to the PC (Guest) over WebRTC, directly from one device to the other.
   Pairing uses the public PeerJS server (lib/peerjs.min.js): the Guest registers as
   "inkdoshub-webcam-<code>" and shows a QR code with ?host=<code>; the Host calls it.
   On the PC, the image becomes a Windows webcam through OBS (browser source with ?guest=<code>&obs=1 and
   "Start virtual camera").

   Addresses: ?host=<code> opens the Host; ?guest=<code> opens the Guest with that code; &obs=1 shows only
   the image. */
(function () {
  "use strict";
  var t = window.InkIdioma ? InkIdioma.t : function (s, p) {
    return p ? s.replace(/\{(\w+)\}/g, function (m, k) { return k in p ? p[k] : m; }) : s;
  };
  var $ = function (id) { return document.getElementById(id); };
  var PREFIX = "inkdoshub-webcam-";
  var KEY_CODE = "inkdoshub-webcam-codigo";
  var KEY_HOST = "inkdoshub-webcam-host";
  var LETTERS = "abcdefghjkmnpqrstuvwxyz23456789";
  var BASE = location.origin + location.pathname;
  var QUALITIES = {
    "2160p60": { w: 3840, h: 2160, fps: 60, bps: 40e6 },
    "2160p30": { w: 3840, h: 2160, fps: 30, bps: 25e6 },
    "1080p60": { w: 1920, h: 1080, fps: 60, bps: 15e6 },
    "1080p30": { w: 1920, h: 1080, fps: 30, bps: 8e6 },
    "720p60": { w: 1280, h: 720, fps: 60, bps: 6e6 },
    "720p30": { w: 1280, h: 720, fps: 30, bps: 4e6 },
  };
  var q = new URLSearchParams(location.search);
  // window.obsstudio exists inside the OBS browser source: there it shows only the image, even without &obs=1
  // Windows app (windows/webcam/): the page runs inside it, in a WebView2
  var app = !!(window.chrome && window.chrome.webview);
  var obs = q.get("obs") === "1" || !!window.obsstudio || app;
  if (obs) document.body.classList.add("obs");
  if (app) document.body.classList.add("app-webcam");
  // Windows app (always the native receiver): the app itself receives the video (GStreamer); the page only shows the QR code and the status
  var native = app && q.get("nativo") === "1";
  if (native) document.body.classList.add("nativo");

  function load(k) { try { return localStorage.getItem(k); } catch (_) { return null; } }
  function store(k, v) { try { localStorage.setItem(k, v); } catch (_) {} }
  function clean(c) { return String(c || "").toLowerCase().replace(/[^a-z0-9]/g, "").slice(0, 10); }
  function newCode() {
    var s = "", a = new Uint32Array(10);
    crypto.getRandomValues(a);
    for (var i = 0; i < 10; i++) s += LETTERS[a[i] % LETTERS.length];
    return s;
  }
  function show(id) {
    ["escolha", "guest", "host"].forEach(function (x) { $(x).hidden = x !== id; });
    $("btroca").hidden = id === "escolha" || obs;
  }
  function mbps(b) { return (b / 1e6).toFixed(1) + " Mbps"; }

  /* ---------- SDP: codec preferido e limite de bitrate ---------- */
  function videoSection(lines) {
    var first = -1, end = lines.length;
    for (var i = 0; i < lines.length; i++) {
      if (lines[i].indexOf("m=video") === 0) first = i;
      else if (first >= 0 && lines[i].indexOf("m=") === 0) { end = i; break; }
    }
    return [first, end];
  }
  function tuneSdp(sdp, codec, bps) {
    var lines = sdp.split("\r\n"), s = videoSection(lines), first = s[0], end = s[1];
    if (first < 0) return sdp;
    var chosen = [], rtx = [], fmtp = {};
    for (var i = first; i < end; i++) {
      var f = /^a=fmtp:(\d+) (.*)$/.exec(lines[i]);
      if (f) fmtp[f[1]] = f[2];
    }
    for (i = first; i < end; i++) {
      var m = /^a=rtpmap:(\d+) ([\w-]+)\//.exec(linhas[i]);
      if (m && codec && m[2].toUpperCase() === codec) chosen.push(m[1]);
    }
    if (codec === "H264") {
      // first the High profile with packetization-mode=1, which gives more quality per bit
      var note = function (pt) {
        var p = fmtp[pt] || "";
        return (/packetization-mode=1/.test(p) ? 2 : 0) + (/profile-level-id=64/i.test(p) ? 1 : 0);
      };
      chosen.sort(function (a, b) { return note(b) - note(a); });
    }
    Object.keys(fmtp).forEach(function (pt) {
      var a = /apt=(\d+)/.exec(fmtp[pt]);
      if (a && chosen.indexOf(a[1]) >= 0) rtx.push(pt);
    });
    if (chosen.length) {
      var parts = lines[first].split(" "), header = parts.slice(0, 3), list = parts.slice(3);
      var front = chosen.concat(rtx);
      lines[first] = header.concat(front, list.filter(function (p) { return front.indexOf(p) < 0; })).join(" ");
    }
    if (bps) {
      for (i = end - 1; i > first; i--) if (/^b=(AS|TIAS):/.test(lines[i])) { lines.splice(i, 1); end--; }
      // b= comes right after c= (or the m= line)
      var pos = first + 1;
      for (i = first + 1; i < end; i++) if (lines[i].indexOf("c=") === 0) { pos = i + 1; break; }
      lines.splice(pos, 0, "b=AS:" + Math.round(bps / 1000), "b=TIAS:" + Math.round(bps));
      end += 2;
      // WebRTC starts at ~300 kbps and ramps up slowly; this makes it start close to the requested quality
      var begin = "x-google-start-bitrate=" + Math.round(bps / 2000) + ";x-google-max-bitrate=" + Math.round(bps / 1000);
      lines[first].split(" ").slice(3).forEach(function (pt) {
        var found = false;
        for (var j = first; j < end; j++) {
          if (lines[j].indexOf("a=fmtp:" + pt + " ") === 0) {
            found = true;
            if (!/apt=/.test(lines[j]) && !/x-google-/.test(lines[j])) lines[j] += ";" + begin;
          }
        }
        if (!found) for (j = first; j < end; j++) if (lines[j].indexOf("a=rtpmap:" + pt + " ") === 0 && !/(red|ulpfec|flexfec)/i.test(lines[j])) { lines.splice(j + 1, 0, "a=fmtp:" + pt + " " + begin); end++; break; }
      });
    }
    return lines.join("\r\n");
  }

  /* ---------- Statistics ---------- */
  function readStats(pc, kind, prev, cb) {
    pc.getStats().then(function (r) {
      var rtp = null, codec = "", par = null, types = {};
      r.forEach(function (s) {
        if (s.type === kind && s.kind === "video") rtp = s;
        if (s.type === "candidate-pair" && (s.nominated || s.selected) && s.state === "succeeded") par = s;
        types[s.id] = s;
      });
      if (!rtp) return cb(null);
      if (rtp.codecId && types[rtp.codecId]) codec = types[rtp.codecId].mimeType.replace("video/", "");
      var bytes = kind === "outbound-rtp" ? rtp.bytesSent : rtp.bytesReceived, now = rtp.timestamp;
      var rate = prev && prev.t ? (bytes - prev.b) * 8 / ((now - prev.t) / 1000) : 0;
      var network = "";
      if (par) {
        var loc = types[par.localCandidateId], rem = types[par.remoteCandidateId];
        var tp = [loc && loc.candidateType, rem && rem.candidateType];
        network = tp.indexOf("relay") >= 0 ? "relay" : tp.indexOf("srflx") >= 0 || tp.indexOf("prflx") >= 0 ? "internet (P2P)" : "local network";
        if (par.currentRoundTripTime) network += " · " + Math.round(par.currentRoundTripTime * 1000) + " ms";
      }
      cb({ rtp: rtp, codec: codec, rate: rate, network: network, b: bytes, t: now });
    }).catch(function () { cb(null); });
  }

  /* ---------- Guest (PC) ---------- */
  function guest(code) {
    show("guest");
    code = clean(code);
    if (!/^[a-z0-9]{10}$/.test(code)) code = clean(load(KEY_CODE)); // the Windows app opens without a code: use the saved one
    if (!/^[a-z0-9]{10}$/.test(code)) code = newCode();
    store(KEY_CODE, code);
    if (q.get("guest") !== code) history.replaceState(null, "", "?guest=" + code + (obs ? "&obs=1" : "") + (native ? "&nativo=1" : ""));
    var video = $("vguest"), peer = null, call = null, prev = null, timer = 0, reconnectTimer = 0, closing = false, statsVisible = false, restarting = false;
    var hostLink = BASE + "?host=" + code;
    $("gcodigo").textContent = code;
    $("linkobs").value = BASE + "?guest=" + code + "&obs=1";
    var qr = qrcode(0, "M");
    qr.addData(hostLink);
    qr.make();
    $("qr").innerHTML = qr.createSvgTag({ cellSize: 4, margin: 2, scalable: true });
    function status(s) { $("gestado").textContent = s; }
    var pump = app && !native ? appPump(video, function () { return format; }, requestQuality) : null;
    /* Windows app: asks the iPhone for the same resolution and fps as the camera chosen in the app, so the frame
       arrives at the right size and takes the fast path (no resizing). */
    function requestQuality() {
      var target = pump && pump.target();
      if (target) notifyHost({ type: "inkdoshub-quality", w: target.w, h: target.h, fps: target.fps });
    }
    function notifyHost(msg) {
      var c = call;
      if (!c || !peer || !c.peer) return;
      try {
        var conn = peer.connect(c.peer, { reliable: true });
        conn.on("open", function () {
          try { conn.send(msg); } catch (_) {}
          setTimeout(function () { try { conn.close(); } catch (_) {} }, 1000);
        });
      } catch (_) {}
    }
    if (app) $("linkobs").closest("details").hidden = true;

    function connect() {
      if (closing) return;
      peer = new Peer(PREFIX + code, { debug: 0 });
      peer.on("open", function () { clearTimeout(reconnectTimer); reconnectTimer = 0; status(t("Ready. Waiting for the iPhone…")); });
      peer.on("disconnected", function () {
        status(t("Connecting to the pairing server…"));
        clearTimeout(reconnectTimer);
        function tryReconnect() {
          var p = peer;
          if (closing || !p || p.destroyed) return;
          if (!p.disconnected) { reconnectTimer = 0; return; }
          try { p.reconnect(); } catch (_) {}
          reconnectTimer = setTimeout(tryReconnect, 3000);
        }
        reconnectTimer = setTimeout(tryReconnect, 1000);
      });
      peer.on("error", function (e) {
        if (e.type === "unavailable-id") {
          status(t("This code is open in another window (another tab or OBS). Trying again…"));
          peer.destroy();
          setTimeout(connect, 5000);
        } else if (e.type === "network" || e.type === "server-error" || e.type === "socket-error" || e.type === "socket-closed") {
          status(t("No connection to the pairing server. Trying again…"));
          if (!peer.destroyed) { peer.destroy(); setTimeout(connect, 4000); }
        }
      });
      peer.on("call", receive);
    }

    // Safari/WebKit can receive and decode WebRTC frames without starting the <video> element.
    // Reattach the MediaStream and try to play on media events, not just once.
    function startVideo(stream, reiniciar) {
      video.autoplay = true;
      video.muted = true;
      video.playsInline = true;
      function play() {
        if (!stream || video.srcObject !== stream) return;
        var p = video.play();
        if (p && p.catch) p.catch(function () {});
      }
      video.onloadedmetadata = play;
      video.onloadeddata = play;
      video.oncanplay = play;
      if (reiniciar) {
        try { video.pause(); } catch (_) {}
        video.srcObject = null;
        requestAnimationFrame(function () {
          video.srcObject = stream;
          play();
          requestAnimationFrame(play);
        });
      } else {
        video.srcObject = stream;
        play();
      }
    }

    function receive(c) {
      // A public peer ID does not authenticate the caller: reject concurrent calls and
      // treat all received metadata as untrusted data.
      if (call) {
        var previous = call;
        var currentStatus = previous.peerConnection && previous.peerConnection.connectionState;
        // A call stuck in "disconnected" after airplane mode is also stale:
        // accept the Host's new call so the reconnection can complete.
        // (or a restart requested by this page to clear the buffer delay)
        if (currentStatus !== "connected" || restarting) {
          restarting = false;
          call = null;
          try { previous.close(); } catch (_) {}
        } else { try { c.close(); } catch (_) {} return; }
      }
      var received = c.metadata && typeof c.metadata === "object" ? c.metadata : {};
      var codecs = ["H264", "H265", "VP9", "AV1", "VP8"];
      var codec = typeof received.codec === "string" ? received.codec.toUpperCase() : "VP8";
      var views = ["preencher", "ajustar", "gira90", "gira270"];
      var meta = {
        codec: codecs.indexOf(codec) >= 0 ? codec : "VP8",
        bps: typeof received.bps === "number" && isFinite(received.bps) && received.bps >= 1000000 ? Math.min(40000000, Math.floor(received.bps)) : 0,
        view: views.indexOf(received.view) >= 0 ? received.view : "preencher"
      };
      call = c;
      $("bparartransmissao").hidden = false;
      c.answer(undefined, { sdpTransform: function (sdp) { return tuneSdp(sdp, meta.codec, meta.bps); } });
      status(t("Connecting to the iPhone…"));
      format = meta.view;
      applyFormat();
      c.on("stream", function (s) {
        if (call !== c) return;
        startVideo(s, false);
        requestQuality();
        $("parear").hidden = true;
        $("bparartransmissao").hidden = false;
        // less delay: Chrome plays the frame as soon as it arrives
        try { c.peerConnection.getReceivers().forEach(function (r) { if ("jitterBufferTarget" in r) r.jitterBufferTarget = 0; else r.playoutDelayHint = 0; }); } catch (_) {}
        clearInterval(timer);
        prev = null;
        timer = setInterval(stats, 1000);
      });
      function end() {
        if (call !== c) return;
        noCall(t("The iPhone disconnected. Waiting for it again…"));
      }
      c.on("close", end);
      c.on("error", end);
      var pc = c.peerConnection;
      if (pc) pc.addEventListener("connectionstatechange", function () {
        if (pc.connectionState === "failed") { c.close(); end(); }
      });
    }

    /* End of the call (iPhone left or the PC stopped): shows the QR code again and the camera shows "waiting". */
    function noCall(msg) {
      call = null;
      clearInterval(timer);
      video.srcObject = null;
      $("parear").hidden = false;
      $("bparartransmissao").hidden = true;
      $("gstats").hidden = true;
      status(msg);
      if (pump) pump.wait();
    }

    /* Image format on the PC: fill 16:9 (crops), whole, or rotated 90° to one side. */
    var format = "preencher";
    function applyFormat() {
      video.className = format;
      var rotates = format === "gira90" || format === "gira270", box = $("guest");
      video.style.width = rotates ? box.clientHeight + "px" : "";
      video.style.height = rotates ? box.clientWidth + "px" : "";
      video.style.transform = rotates ? "translate(-50%,-50%) rotate(" + (format === "gira90" ? 90 : -90) + "deg)" : "";
    }
    addEventListener("resize", applyFormat);

    function stats() {
      if (!call || !call.peerConnection) return;
      var c = call;
      readStats(call.peerConnection, "inbound-rtp", prev, function (s) {
        if (!s) return;
        var a = prev && prev.rtp;
        prev = s;
        var r = s.rtp, delay = 0, stalled = false;
        // buffer delay NOW (in the last second), not the average since the start
        if (a && r.jitterBufferEmittedCount > a.jitterBufferEmittedCount)
          delay = Math.round((r.jitterBufferDelay - a.jitterBufferDelay) / (r.jitterBufferEmittedCount - a.jitterBufferEmittedCount) * 1000);
        // packets arriving without any frame leaving the buffer: the PC is behind (counts as delay)
        else if (a && r.packetsReceived > a.packetsReceived) stalled = true;
        watchDelay(c, stalled ? Infinity : delay);
        if (app) try { chrome.webview.postMessage("BUF " + (stalled ? -1 : delay)); } catch (_) {}
        $("gstats").textContent = (r.frameWidth || "?") + "×" + (r.frameHeight || "?") + " · " + Math.round(r.framesPerSecond || 0) + " fps\n" +
          s.codec + (r.decoderImplementation ? " (" + r.decoderImplementation + ")" : "") + " · " + mbps(s.rate) + "\n" +
          t("Buffer") + ": " + delay + " ms · " + t("lost") + ": " + (r.packetsLost || 0) + "\n" + s.network;
        $("gstats").hidden = !statsVisible;
      });
    }

    /* Chrome's buffer grows endlessly when the PC cannot decode at pace (the heavier the
       quality, the faster) and the video falls seconds behind. If it passes 0.5 s for 3 readings in a row,
       it redoes the call: the iPhone reconnects by itself and the delay goes back to ~30 ms. At most every 10 s. */
    var overLimit = 0, lastRestart = 0;
    function watchDelay(c, ms) {
      if (c !== call) return;
      overLimit = ms > 500 ? overLimit + 1 : 0; // (Infinity: nenhum quadro saiu do buffer neste segundo)
      if (overLimit < 3 || Date.now() - lastRestart < 10000) return;
      overLimit = 0;
      lastRestart = Date.now();
      status(t("Video fell behind; reconnecting to catch up…"));
      restarting = true;
      setTimeout(function () { restarting = false; }, 10000);
      notifyHost({ type: "inkdoshub-restart" });
    }

    function diagnose() {
      var lines = [];
      var tracks = video.srcObject && video.srcObject.getVideoTracks ? video.srcObject.getVideoTracks() : [];
      var tr = tracks[0] || null;
      var pc = call && call.peerConnection;
      function line(k, v) { lines.push(k + ": " + (v === undefined || v === null || v === "" ? "—" : v)); }
      line("Hora", new Date().toLocaleString());
      line("Navegador", navigator.userAgent);
      line("Page visibility", document.visibilityState);
      line("Tela cheia", !!document.fullscreenElement);
      line("WebRTC chamada", call ? "presente" : "ausente");
      line("PeerConnection", pc ? pc.connectionState : "ausente");
      line("ICE", pc ? pc.iceConnectionState : "ausente");
      line("Video attached to stream", !!video.srcObject);
      line("Stream ativo", video.srcObject ? video.srcObject.active : false);
      line("Video tracks", tracks.length);
      line("Track", tr ? tr.readyState + ", muted=" + tr.muted + ", enabled=" + tr.enabled : "ausente");
      line("Video element", "readyState=" + video.readyState + ", paused=" + video.paused + ", networkState=" + video.networkState);
      line("Rendered size", video.videoWidth + "×" + video.videoHeight);
      line("Video time", Number(video.currentTime || 0).toFixed(2) + " s");
      line("Last media error", video.error ? ("code=" + video.error.code + " " + video.error.message) : "none");
      if (!pc) {
        $("diagnostico").textContent = lines.join("\n") + "\n\nNo PeerConnection: check that the iPhone tapped Start and that the code matches.";
        return;
      }
      pc.getStats().then(function (rel) {
        var inbound = null, codec = "", map = {};
        rel.forEach(function (s) { map[s.id] = s; });
        rel.forEach(function (s) {
          if (s.type === "inbound-rtp" && (s.kind === "video" || s.mediaType === "video")) inbound = s;
        });
        line("Video stats received", !!inbound);
        if (inbound) {
          var c = inbound.codecId && map[inbound.codecId];
          codec = c && c.mimeType ? c.mimeType : "desconhecido";
          line("Codec recebido", codec);
          line("Pacotes recebidos", inbound.packetsReceived);
          line("Pacotes perdidos", inbound.packetsLost);
          line("Bytes recebidos", inbound.bytesReceived);
          line("Frames recebidos", inbound.framesReceived);
          line("Frames decodificados", inbound.framesDecoded);
          line("Frames descartados", inbound.framesDropped);
          line("Received resolution", (inbound.frameWidth || "?") + "×" + (inbound.frameHeight || "?"));
          line("FPS recebido", inbound.framesPerSecond);
        }
        $("diagnostico").textContent = lines.join("\n") + "\n\nLeitura rápida:\n- Bytes/frames não aumentam: problema de envio ou rede/WebRTC.\n- Frames recebidos aumentam, mas decodificados ficam em 0: possível incompatibilidade de codec/decodificador do XeOS.\n- Frames decodificados aumentam, mas a imagem continua preta até tela cheia: provável falha de composição/renderização de vídeo do navegador; o botão Refresh video tenta forçar uma atualização.";
      }).catch(function (e) {
        $("diagnostico").textContent = lines.join("\n") + "\n\nFalha ao ler WebRTC stats: " + (e && (e.message || e.name) || e);
      });
    }
    $("bparartransmissao").onclick = function () {
      var c = call, signal = peer && c && c.peer ? peer.connect(c.peer, { reliable: true }) : null, finished = false;
      function finish() {
        if (finished) return;
        finished = true;
        clearTimeout(signalLimit);
        if (signal) { try { signal.close(); } catch (_) {} }
        if (c) { try { c.close(); } catch (_) {} }
        video.pause();
        noCall(t("Transmission stopped by the PC."));
      }
      var signalLimit = setTimeout(finish, 1500);
      if (signal) {
        signal.on("open", function () {
          try { signal.send({ type: "inkdoshub-stop" }); } catch (_) {}
          setTimeout(finish, 150);
        });
        signal.on("error", finish);
      } else finish();
    };
    $("bdiagnostico").onclick = function () {
      var panel = $("diagnostico");
      panel.hidden = !panel.hidden;
      if (!panel.hidden) diagnose();
    };
    $("brefrescar").onclick = function () {
      // Recovers the media element without ending the WebRTC call.
      var stream = video.srcObject;
      if (stream) startVideo(stream, true);
      video.style.transform = "translateZ(0)";
      video.style.willChange = "transform";
      requestAnimationFrame(function () {
        video.style.opacity = "0.99";
        requestAnimationFrame(function () {
          video.style.opacity = "1";
          video.style.willChange = "";
          if (!$("diagnostico").hidden) diagnose();
        });
      });
    };
    $("bstats").onclick = function () { statsVisible = !statsVisible; $("gstats").hidden = !statsVisible || !call; };
    /* Windows app: the in-window preview costs GPU and memory (heavy at 4K) and does not change what
       goes to the virtual camera. Off by default; the button turns it on and the choice is saved. */
    if (app) {
      var KEY_PREVIEW = "inkdoshub-webcam-previa";
      var preview = load(KEY_PREVIEW) === "1";
      var showPreview = function () {
        video.style.visibility = preview ? "" : "hidden";
        $("bprevia").textContent = preview ? t("Hide preview") : t("Show preview");
      };
      $("bprevia").hidden = false;
      $("bprevia").onclick = function () { preview = !preview; store(KEY_PREVIEW, preview ? "1" : "0"); showPreview(); };
      showPreview();
    }
    $("btela").onclick = function () {
      var el = $("guest");
      if (document.fullscreenElement) document.exitFullscreen();
      else if (el.requestFullscreen) el.requestFullscreen().catch(function () {});
    };
    $("copiar").onclick = function () {
      var i = $("linkobs");
      i.select();
      (navigator.clipboard ? navigator.clipboard.writeText(i.value) : Promise.reject()).then(function () {
        $("copiar").textContent = t("Copied");
      }, function () { document.execCommand && document.execCommand("copy"); });
    };
    $("novocodigo").onclick = function () {
      store(KEY_CODE, newCode());
      location.href = BASE + "?guest=" + load(KEY_CODE) + (native ? "&nativo=1" : "");
    };
    addEventListener("pagehide", function () {
      closing = true;
      clearInterval(timer);
      if (call) { try { call.close(); } catch (_) {} call = null; }
      video.srcObject = null;
      if (pump) pump.parar();
      if (peer) { peer.destroy(); peer = null; }
    });
    if (!native) { connect(); return; }
    // native receiver: the app connects with this code and sends the status back
    chrome.webview.addEventListener("message", function (e) {
      var m = typeof e.data === "string" ? e.data : "";
      if (m.indexOf("nat:estado:") === 0) status(t(m.slice(11)));
      else if (m === "nat:on") $("parear").classList.add("ligado");   // connected: the QR code disappears, the status stays
      else if (m === "nat:off") $("parear").classList.remove("ligado");
    });
    chrome.webview.postMessage("NATIVO " + code);
  }

  /* ---------- Windows app: sends frames to the virtual camera ----------
     The app (windows/webcam/Program.cs) provides a shared buffer the size of the camera.

     Fast path (almost always): reads the decoded frames straight from the WebRTC track
     (MediaStreamTrackProcessor), without going through <video> or canvas, and copies each frame already in
     BGRX to the buffer (VideoFrame.copyTo). The pace is the frames' own, not a timer, so a
     hidden tab or one covered by Zoom does not slow down. While the app is still delivering the previous
     frame, new ones are dropped: no queue forms, so the delay does not grow.

     Slow path (only when the frame is not the camera size, is portrait or rotated): draws the
     frame onto a canvas in the chosen format and copies the pixels (RGBA).

     Messages to the app: "frame:bgrx" or "frame:rgba"; the app replies "ok" once it has sent it to the
     camera. Every 5 s "PERFJS ..." goes to the app log. */
  function appPump(video, formatoAtual, aoMudarQualidade) {
    // the buffer may have 2 slots (double buffer): while the app delivers one, the page copies into the other
    var mem = null, slots = [], free = [], W = 0, H = 0, fps = 30, reader = null, currentTrack = null, last = 0;
    var canvas = document.createElement("canvas"), ctx = null, bgrxOk = true;
    var perf = { ini: performance.now(), rapido: 0, lento: 0, descartados: 0, copia: 0, ack: 0, envio: 0 };
    var hasProcessor = typeof window.MediaStreamTrackProcessor === "function" && typeof window.VideoFrame === "function";

    function report() {
      var now = performance.now(), s = (now - perf.ini) / 1000;
      if (s < 5) return;
      var n = perf.rapido + perf.lento;
      chrome.webview.postMessage("PERFJS camera=" + W + "x" + H + "@" + fps + " sent_fps=" + (n / s).toFixed(1) +
        " fast=" + perf.rapido + " slow=" + perf.lento + " dropped=" + perf.descartados +
        " copy_avg_ms=" + (n ? perf.copia / n : 0).toFixed(2) + " ack_avg_ms=" + (n ? perf.ack / n : 0).toFixed(2) +
        " processor=" + (hasProcessor ? 1 : 0) + " bgrx=" + (bgrxOk ? 1 : 0) + " " + (perf.info || ""));
      perf = { ini: now, rapido: 0, lento: 0, descartados: 0, copia: 0, ack: 0, envio: 0, info: perf.info };
    }
    function takeSlot() {
      for (var i = 0; i < free.length; i++) if (free[i]) { free[i] = false; return i; }
      return -1;
    }
    function notify(kind, slot, t0) {
      perf.copia += performance.now() - t0;
      perf.envio = performance.now();
      chrome.webview.postMessage("frame:" + (kind.indexOf(":") > 0 ? kind : kind + ":" + slot));
    }

    chrome.webview.addEventListener("sharedbufferreceived", function (e) {
      var d = e.additionalData || {};
      mem = e.getBuffer();
      W = d.w; H = d.h; fps = d.fps || 30;
      var n = Math.max(1, Math.min(2, d.slots || 1));
      slots = []; free = [];
      for (var i = 0; i < n; i++) { slots.push(new Uint8Array(mem, i * W * H * 4, W * H * 4)); free.push(true); }
      canvas.width = W; canvas.height = H;
      ctx = canvas.getContext("2d", { alpha: false, willReadFrequently: true });
      if (!video.srcObject) wait();
      start(video.srcObject);
      if (aoMudarQualidade) aoMudarQualidade();
    });
    chrome.webview.addEventListener("message", function (e) {
      var m = typeof e.data === "string" ? /^ok(?::(\d))?$/.exec(e.data) : null;
      if (!m) return;
      if (perf.envio) { perf.ack += performance.now() - perf.envio; perf.envio = 0; }
      if (m[1] === undefined) for (var i = 0; i < free.length; i++) free[i] = true;
      else if (+m[1] < free.length) free[+m[1]] = true;
      report();
    });

    /* Delivers a frame: a WebRTC VideoFrame (which this function closes) or, without the processor, the
       <video> itself. */
    function deliver(fonte, largura, altura) {
      var frame = fonte instanceof window.VideoFrame ? fonte : null;
      function close() { if (frame) { try { frame.close(); } catch (_) {} } }
      if (!slots.length || !ctx) { close(); return; }
      // limits to the fps of the chosen camera (the iPhone may send more)
      // (clock with a "slot" per period: tolerates the natural variation between frames without needless drops)
      var now = performance.now(), period = 1000 / fps;
      if (now < last - 2) { perf.descartados++; close(); return; }
      var slot = takeSlot();
      if (slot < 0) { perf.descartados++; close(); return; }
      last = Math.max(last + period, now - period / 2);
      var buf = slots[slot];
      var f = formatoAtual(), t0 = performance.now();
      var g = f === "gira90" ? 90 : f === "gira270" ? 270 : 0;
      var vr = frame && frame.visibleRect, rot = null;
      if (frame) rot = typeof frame.rotation === "number" ? frame.rotation : (vr && vr.width === largura ? 0 : null);
      perf.info = "rot=" + rot + " fmt=" + f + " raw=" + (vr ? vr.width + "x" + vr.height : "-") + " disp=" + largura + "x" + altura;
      function copy(kind, layout) {
        frame.copyTo(buf, { format: "BGRX", layout: layout }).then(function () {
          close();
          perf.rapido++;
          notify(kind, slot, t0);
        }, function () {
          close();
          bgrxOk = false; // this Chrome does not convert to BGRX: use the canvas from now on
          free[slot] = true;
        });
      }
      if (frame && bgrxOk && rot === 0 && g === 0 && largura === W && altura === H) {
        // fast path: Chrome converts to BGRX and copies straight into the app memory
        copy("bgrx", [{ offset: 0, stride: W * 4 }]);
        return;
      }
      if (frame && bgrxOk && vr && rot !== null && !frame.flip && vr.width * vr.height <= W * H) {
        // raw frame (any size and rotation): the app rotates, crops and scales when converting for the camera
        var rotation = (rot + g) % 360;
        copy("raw:" + slot + ":" + vr.width + ":" + vr.height + ":" + rotation + ":" + (f === "ajustar" ? 1 : 0), [{ offset: 0, stride: vr.width * 4 }]);
        return;
      }
      try {
        draw(fonte, largura, altura, f);
        buf.set(ctx.getImageData(0, 0, W, H).data);
      } finally { close(); }
      perf.lento++;
      notify("rgba", slot, t0);
    }
    function draw(fonte, vw, vh, f) {
      var rotates = f === "gira90" || f === "gira270";
      var sw = rotates ? vh : vw, sh = rotates ? vw : vh;
      var esc = f === "ajustar" ? Math.min(W / sw, H / sh) : Math.max(W / sw, H / sh);
      ctx.fillStyle = "#000";
      ctx.fillRect(0, 0, W, H);
      ctx.save();
      ctx.translate(W / 2, H / 2);
      if (rotates) ctx.rotate((f === "gira90" ? 90 : -90) * Math.PI / 180);
      ctx.drawImage(fonte, -vw * esc / 2, -vh * esc / 2, vw * esc, vh * esc);
      ctx.restore();
    }

    /* Reads the frames of the video track as soon as the iPhone connects. */
    function start(stream) {
      var track = stream && stream.getVideoTracks ? stream.getVideoTracks()[0] : null;
      if (!track || track === currentTrack || !slots.length) return;
      stop();
      currentTrack = track;
      if (!hasProcessor) return byVideo(track);
      var r;
      try { r = new MediaStreamTrackProcessor({ track: track, maxBufferSize: 1 }).readable.getReader(); }
      catch (_) { hasProcessor = false; return byVideo(track); }
      reader = r;
      (function next() {
        r.read().then(function (res) {
          if (res.done || reader !== r) { if (res.value) res.value.close(); return; }
          var q = res.value;
          try { deliver(q, q.displayWidth, q.displayHeight); } catch (_) { try { q.close(); } catch (e) {} for (var i = 0; i < free.length; i++) free[i] = true; }
          next();
        }, function () {});
      })();
    }
    // without MediaStreamTrackProcessor: one frame per new <video> frame
    function byVideo(track) {
      function frame() {
        if (currentTrack !== track) return;
        if (video.videoWidth) deliver(video, video.videoWidth, video.videoHeight);
        if (video.requestVideoFrameCallback) video.requestVideoFrameCallback(frame);
        else setTimeout(frame, 1000 / fps);
      }
      frame();
    }
    function stop() {
      currentTrack = null;
      if (reader) { var r = reader; reader = null; try { r.cancel(); } catch (_) {} }
    }
    // <video> gets the track a little after the call: check from time to time
    setInterval(function () { if (video.srcObject) start(video.srcObject); }, 500);

    // no iPhone: the camera shows a notice instead of freezing on the last frame
    function wait() {
      if (!ctx || !slots.length) return;
      stop();
      ctx.fillStyle = "#000";
      ctx.fillRect(0, 0, W, H);
      ctx.fillStyle = "#fff";
      ctx.font = Math.round(H / 22) + "px -apple-system, Segoe UI, sans-serif";
      ctx.textAlign = "center";
      ctx.fillText(t("Waiting for the iPhone…"), W / 2, H / 2);
      slots[0].set(ctx.getImageData(0, 0, W, H).data);
      free[0] = false;
      chrome.webview.postMessage("frame:rgba:0");
    }
    return {
      wait: wait,
      target: function () { return W ? { w: W, h: H, fps: fps } : null; },
      parar: function () { stop(); slots = []; free = []; }
    };
  }

  /* ---------- Host (iPhone) ---------- */
  function host(code) {
    show("host");
    var cfg = {};
    try { cfg = JSON.parse(load(KEY_HOST) || "{}") || {}; } catch (_) {}
    var initialCode = clean(code), savedCode = clean(cfg.codigo);
    $("hcodigo").value = /^[a-z0-9]{10}$/.test(initialCode) ? initialCode : (/^[a-z0-9]{10}$/.test(savedCode) ? savedCode : "");
    if (cfg.qualidade && QUALITIES[cfg.qualidade]) $("qualidade").value = cfg.qualidade;
    // H.264 is encoded by the iPhone hardware and decoded by the PC GPU (version 3 brings everyone back to it)
    if (cfg.codecVersion === 3 && cfg.codec) $("codec").value = cfg.codec;
    else $("codec").value = "H264";
    if (cfg.formato) $("formato").value = cfg.formato;
    var video = $("vhost"), stream = null, peer = null, call = null, on = false, attempt = 0, reconnectTimer = 0, destFailures = 0, prev = null, timer = 0, lock = null;
    var closing = false;
    var savedCamera = cfg.camera || "environment";
    $("camera").value = savedCamera;

    function status(s) { $("hestado").textContent = s; }
    function save() {
      store(KEY_HOST, JSON.stringify({ codigo: clean($("hcodigo").value), qualidade: $("qualidade").value, camera: $("camera").value, codec: $("codec").value, codecVersion: 3, formato: $("formato").value }));
    }
    function which() { return QUALITIES[$("qualidade").value] || QUALITIES["1080p30"]; }

    function openCamera() {
      var Q = which(), cam = $("camera").value;
      var v = { width: { ideal: Q.w }, height: { ideal: Q.h }, frameRate: { ideal: Q.fps } };
      if (cam === "environment" || cam === "user") v.facingMode = { ideal: cam };
      else v.deviceId = { exact: cam };
      return navigator.mediaDevices.getUserMedia({ audio: false, video: v }).then(function (s) {
        var tr = s.getVideoTracks()[0];
        try { tr.contentHint = "motion"; } catch (_) {}
        // only reopens if the camera that dropped is still the current one (Safari also reports "ended" when the
        // page itself stops the old camera while switching, which turned into a switching loop every ~2 s)
        tr.addEventListener("ended", function () { if (on && stream && stream.getVideoTracks()[0] === tr) switchCamera(); });
        listCameras();
        return s;
      });
    }
    function listCameras() {
      if (!navigator.mediaDevices.enumerateDevices) return;
      navigator.mediaDevices.enumerateDevices().then(function (list) {
        var cams = list.filter(function (d) { return d.kind === "videoinput" && d.label; });
        if (!cams.length) return;
        var sel = $("camera"), current = sel.value;
        sel.innerHTML = "";
        [["environment", t("Back camera")], ["user", t("Front camera")]].concat(cams.map(function (d) { return [d.deviceId, d.label]; })).forEach(function (o) {
          var op = document.createElement("option");
          op.value = o[0];
          op.textContent = o[1];
          sel.appendChild(op);
        });
        sel.value = current;
        if (sel.value !== current) sel.value = "environment";
      }).catch(function () {});
    }
    function stopStream() { if (stream) stream.getTracks().forEach(function (x) { x.stop(); }); stream = null; }

    function tuneSender() {
      if (!call || !call.peerConnection) return;
      var Q = which();
      call.peerConnection.getSenders().forEach(function (snd) {
        if (!snd.track || snd.track.kind !== "video") return;
        var p = snd.getParameters();
        if (!p.encodings || !p.encodings.length) p.encodings = [{}];
        p.encodings[0].maxBitrate = Q.bps;
        p.encodings[0].maxFramerate = Q.fps;
        p.encodings[0].scaleResolutionDownBy = 1;
        // with little bandwidth, lowers fps and not resolution: the PC always receives the camera size
        try { p.degradationPreference = "maintain-resolution"; } catch (_) {}
        snd.setParameters(p).catch(function () {});
      });
    }

    function stopLocal(mensagem) {
      on = false;
      clearTimeout(attempt);
      clearTimeout(reconnectTimer);
      reconnectTimer = 0;
      clearInterval(timer);
      var c = call;
      call = null;
      if (c) { try { c.close(); } catch (_) {} }
      if (peer) { peer.destroy(); peer = null; }
      stopStream();
      video.srcObject = null;
      if (lock) { try { lock.release().catch(function () {}); } catch (_) {} }
      lock = null;
      $("hstats").hidden = true;
      $("iniciar").textContent = t("Start");
      $("iniciar").classList.remove("parar");
      status(mensagem || t("Stopped."));
    }

    function start() {
      if (!on || closing) return;
      var dest = PREFIX + clean($("hcodigo").value), Q = which();
      if (!peer || peer.destroyed) {
        peer = new Peer({ debug: 0 });
        peer.on("open", function () { clearTimeout(reconnectTimer); reconnectTimer = 0; start(); });
        peer.on("connection", function (conn) {
          conn.on("data", function (msg) {
            if (conn.peer !== PREFIX + clean($("hcodigo").value) || !msg || typeof msg !== "object") return;
            if (msg.type === "inkdoshub-stop") stopLocal(t("Transmission stopped by the PC."));
            else if (msg.type === "inkdoshub-restart" && on) { clearTimeout(attempt); start(); } // the PC asked for a new call (accumulated delay)
            else if (msg.type === "inkdoshub-quality") {
              // the Windows app asks for its camera resolution (e.g. 1080p60); only accepts the listed ones
              var key = (msg.h >= 2160 ? "2160p" : msg.h >= 1080 ? "1080p" : "720p") + (msg.fps >= 60 ? "60" : "30");
              if (QUALITIES[key] && $("qualidade").value !== key) {
                $("qualidade").value = key;
                switchCamera();
              }
            }
          });
        });
        peer.on("disconnected", function () {
          clearTimeout(reconnectTimer);
          status(t("Connecting to the pairing server…"));
          function tryReconnect() {
            var p = peer;
            if (closing || !on || !p || p.destroyed) return;
            if (!p.disconnected) { reconnectTimer = 0; return; }
            try { p.reconnect(); } catch (_) {}
            reconnectTimer = setTimeout(tryReconnect, 3000);
          }
          reconnectTimer = setTimeout(tryReconnect, 1000);
        });
        peer.on("error", function (e) {
          if (e.type === "peer-unavailable") {
            status(t("The PC was not found. Check that the Guest page is open with code {code}. Trying again…", { code: clean($("hcodigo").value) }));
          } else if (e.type === "network" || e.type === "server-error" || e.type === "socket-error" || e.type === "socket-closed") status(t("No connection to the pairing server. Trying again…"));
          else status(t("Error: {msg}", { msg: e.type || e.message }));
          again();
        });
        status(t("Connecting to the pairing server…"));
        return;
      }
      if (!peer.open) return;
      if (call) { try { call.close(); } catch (_) {} }
      status(t("Calling the PC…"));
      var c = peer.call(dest, stream, {
        metadata: { codec: $("codec").value, bps: Q.bps, view: $("formato").value },
        sdpTransform: function (sdp) { return tuneSdp(sdp, $("codec").value, 0); },
      });
      call = c;
      if (!c) return again();
      var pc = c.peerConnection;
      pc.addEventListener("connectionstatechange", function () {
        if (call !== c) return;
        var st = pc.connectionState;
        if (st === "connected") {
          attempt = 0;
          destFailures = 0;
          status(t("Sending to the PC. Keep Safari open with the screen on."));
          tuneSender();
          clearInterval(timer);
          prev = null;
          timer = setInterval(stats, 1000);
        } else if (st === "failed") {
          status(t("The connection failed. If the devices are on different networks, put both on the same Wi-Fi. Trying again…"));
          again();
        } else if (st === "disconnected") {
          status(t("Connection lost. Waiting…"));
          setTimeout(function () { if (call === c && pc.connectionState === "disconnected") again(); }, 5000);
        }
      });
      c.on("close", function () {
        if (call === c && on) again();
      });
    }
    function again() {
      if (!on) return;
      if (call) { var c = call; call = null; try { c.close(); } catch (_) {} }
      clearInterval(timer);
      $("hstats").hidden = true;
      clearTimeout(attempt);
      attempt = setTimeout(start, 3000);
    }

    function stats() {
      if (!call || !call.peerConnection) return;
      readStats(call.peerConnection, "outbound-rtp", prev, function (s) {
        if (!s) return;
        prev = s;
        var tr0 = stream && stream.getVideoTracks()[0], cfgs = tr0 ? tr0.getSettings() : {}, r = s.rtp;
        var caps = tr0 && tr0.getCapabilities ? tr0.getCapabilities() : {};
        var maxFps = caps.frameRate && caps.frameRate.max ? " (max " + Math.round(caps.frameRate.max) + ")" : "";
        $("hstats").textContent = t("Camera") + ": " + (cfgs.width || "?") + "×" + (cfgs.height || "?") + " · " + Math.round(cfgs.frameRate || 0) + " fps" + maxFps + "\n" +
          t("Sent") + ": " + (r.frameWidth || "?") + "×" + (r.frameHeight || "?") + " · " + Math.round(r.framesPerSecond || 0) + " fps\n" +
          s.codec + (r.encoderImplementation ? " (" + r.encoderImplementation + ")" : "") + " · " + mbps(s.rate) +
          (r.qualityLimitationReason && r.qualityLimitationReason !== "none" ? "\n" + t("Limited by") + ": " + r.qualityLimitationReason : "") + "\n" + s.network;
        $("hstats").hidden = false;
      });
    }

    function keepScreenOn() {
      if (!navigator.wakeLock || document.visibilityState !== "visible") return;
      navigator.wakeLock.request("screen").then(function (l) { lock = l; }).catch(function () {});
    }

    function switchCamera() {
      save();
      if (!on) return;
      var old = stream;
      stream = null;
      if (old) old.getTracks().forEach(function (x) { x.stop(); }); // the iPhone opens one camera at a time
      openCamera().then(function (s) {
        stream = s;
        video.srcObject = s;
        var tr = s.getVideoTracks()[0];
        if (call && call.peerConnection) {
          call.peerConnection.getSenders().forEach(function (snd) { if (snd.track === null || (snd.track && snd.track.kind === "video")) snd.replaceTrack(tr); });
          tuneSender();
        }
      }).catch(function (e) { status(t("Could not open the camera: {msg}", { msg: e.name || e.message })); });
    }

    $("iniciar").onclick = function () {
      if (on) {
        stopLocal(t("Stopped."));
        return;
      }
      if (!/^[a-z0-9]{10}$/.test(clean($("hcodigo").value))) { status(t("Type the 10-character code shown on the PC.")); $("hcodigo").focus(); return; }
      $("hcodigo").value = clean($("hcodigo").value);
      if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) { status(t("This browser cannot open the camera here.")); return; }
      save();
      keepScreenOn();
      status(t("Opening the camera…"));
      openCamera().then(function (s) {
        stream = s;
        video.srcObject = s;
        video.play().catch(function () {});
        on = true;
        $("iniciar").textContent = t("Stop");
        $("iniciar").classList.add("parar");
        start();
      }).catch(function (e) {
        status(e && e.name === "NotAllowedError" ? t("Camera access was denied. Allow it in Safari (aA › Website Settings) and try again.") : t("Could not open the camera: {msg}", { msg: e.name || e.message }));
      });
    };
    $("qualidade").onchange = switchCamera;
    $("camera").onchange = switchCamera;
    $("codec").onchange = function () { save(); if (on) again(); };
    $("formato").onchange = function () { save(); if (on) again(); };
    $("hcodigo").onchange = function () { save(); if (on) again(); };
    document.addEventListener("visibilitychange", function () {
      if (document.visibilityState !== "visible" || !on) return;
      keepScreenOn();
      // iOS turns the camera off when Safari leaves the foreground; on return, opens it again
      var tr = stream && stream.getVideoTracks()[0];
      if (!tr || tr.readyState === "ended" || tr.muted) switchCamera();
    });
    addEventListener("pagehide", function () { closing = true; stopLocal(); });
  }

  /* ---------- Escolha ---------- */
  $("ehost").onclick = function () { history.replaceState(null, "", "?host="); host(""); };
  $("eguest").onclick = function () { guest(""); };
  $("btroca").onclick = function () { location.href = BASE; };
  if (app) guest(q.get("guest"));
  else if (q.has("host")) host(q.get("host"));
  else if (q.has("guest")) guest(q.get("guest"));
  else show("escolha");
})();
