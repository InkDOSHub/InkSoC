/* InkSoC desktop: each program runs in its own engine worker. Its screen is a
   full-desktop canvas, transparent outside its windows, stacked with the others;
   console output goes to a console window. */
'use strict';
const $ = (id) => document.getElementById(id);
const desk = $('desk');
const fmt = (n) => Math.round(n).toLocaleString('en-US');
const W = 1024, H = 768;
const progs = [];
let zTop = 10, nextId = 1;

// recommended real RAM for each program: half the lowest tab limit the memory calibration saw
function recommendedMb() {
  let s = {};
  try { s = JSON.parse(localStorage.getItem('inksoc-limits')) || {}; } catch (e) {}
  const lim = s.minLimit || (s.calib && !s.calib.running && s.calib.mb) || 0;
  return lim ? Math.max(128, Math.min(2048, Math.floor(lim * 0.5 / 64) * 64)) : 256;
}
// the real RAM a program gets: the fixed amount chosen in Settings, or the recommended one
function poolMb() { return prefs.ramMb || recommendedMb(); }

/* ---------- error log: program errors, page errors and page crashes ----------
   Kept in localStorage 'inksoc-log' line by line, so it survives a crash of the tab. System Monitor shows it. */
const LOG_KEY = 'inksoc-log', ALIVE_KEY = 'inksoc-alive', LOG_MAX = 400;
const errlog = [];
try { errlog.push(...(JSON.parse(localStorage.getItem(LOG_KEY)) || [])); } catch (e) { /* private window */ }
function logError(src, text) {
  errlog.push({ at: Date.now(), src, text: String(text).slice(0, 2000) });
  if (errlog.length > LOG_MAX) errlog.splice(0, errlog.length - LOG_MAX);
  try { localStorage.setItem(LOG_KEY, JSON.stringify(errlog)); } catch (e) { /* full or private */ }
}
function clearLog() {
  errlog.length = 0;
  try { localStorage.removeItem(LOG_KEY); } catch (e) { /* private window */ }
}
// lines of a program's output that are errors (Wine's "err:", engine stops, exceptions)
const ERR_LINE = /\berr:|\berror\b|exception|unhandled|panic|unreachable|fatal|crash|access violation|out of memory/i;
addEventListener('error', (e) => logError('page', (e.message || 'error') + (e.filename ? ' (' + e.filename.split('/').pop() + ':' + e.lineno + ')' : '')));
addEventListener('unhandledrejection', (e) => logError('page', 'unhandled: ' + (e.reason && (e.reason.stack || e.reason.message) || e.reason)));
// crash detection: a mark that a normal close removes. Found at start = the last session died
// (tab killed for memory, or the page froze); it says what was running then.
try {
  const last = JSON.parse(localStorage.getItem(ALIVE_KEY));
  let calib = false;
  try { calib = !!(JSON.parse(localStorage.getItem('inksoc-limits')) || {}).calib?.running; } catch (e) { /* none */ }
  if (last) logError(calib ? 'calibration' : 'crash', (calib ? 'Expected: the memory calibration filled the tab until the browser reloaded it. ' : '') + 'The page stopped unexpectedly around ' + new Date(last.at).toLocaleString() + ' (the browser closed or reloaded the tab).' + (last.progs.length ? ' Running: ' + last.progs.join(', ') + '.' : ' No program was running.') + (last.ram ? ' Program RAM in use: ' + last.ram + ' MB.' : '') + (last.heap ? ' Page memory: ' + last.heap + ' MB.' : ''));
} catch (e) { /* none */ }
function aliveMark() {
  const ram = progs.reduce((a, q) => a + (q.w && q.stats ? q.stats.resident * 4 / 1024 : 0), 0);
  const heap = performance.memory ? performance.memory.usedJSHeapSize / 1048576 : 0;
  try { localStorage.setItem(ALIVE_KEY, JSON.stringify({ at: Date.now(), progs: progs.filter((q) => q.w).map((q) => q.label), ram: Math.round(ram), heap: Math.round(heap) })); } catch (e) { /* private window */ }
}
setInterval(aliveMark, 3000);
addEventListener('pagehide', () => { try { localStorage.removeItem(ALIVE_KEY); } catch (e) { /* private window */ } });
addEventListener('pageshow', aliveMark);

// console output, as Wine's console: plain text, only the last part kept (an endless log never slows the page)
const OUT_MAX = 200000;
function appendOut(out, text) {
  if (!text) return;
  out.append(text);
  out.chars = (out.chars || 0) + text.length;
  while (out.chars > OUT_MAX && out.firstChild) {
    out.chars -= out.firstChild.textContent.length;
    out.firstChild.remove();
  }
  if (!out.pending) { out.pending = true; requestAnimationFrame(() => { out.pending = false; out.scrollTop = out.scrollHeight; }); }
}

// the program screens are scaled to fit the desktop
let scale = 1;
function fit() {
  scale = Math.min(desk.clientWidth / W, desk.clientHeight / H);
  for (const p of progs) if (p.cv) { p.cv.style.width = W * scale + 'px'; p.cv.style.height = H * scale + 'px'; }
}
addEventListener('resize', fit);

function raise(p) {
  p.z = ++zTop;
  if (p.cv) p.cv.style.zIndex = p.z;
  if (p.con) p.con.style.zIndex = p.z + (p.cv ? 0.5 : 0);
  for (const q of progs) q.btn.classList.toggle('on', q === p);
}
function setHidden(p, h) {
  p.hidden = h;
  if (p.cv) p.cv.hidden = h;
  if (p.con) p.con.hidden = h || p.conAway;
}

// the shell's icons (Papirus, icons/): an <img> with a text after it
function icon(name) {
  const i = document.createElement('img');
  i.className = 'ic';
  i.src = 'icons/' + name + '.svg';
  i.alt = '';
  return i;
}
// a symbolic (one colour) Linux icon from icons/sym, drawn in the text colour: the shell's buttons and marks
function sym(name, cls) { return '<i class="sym' + (cls ? ' ' + cls : '') + '" style="--s:url(icons/sym/' + name + '-symbolic.svg)"></i>'; }
function labelled(el, name, text) { el.replaceChildren(icon(name), document.createTextNode(text)); }

function consoleWindow(p, title) {
  const c = document.createElement('div');
  c.className = 'con';
  const n = progs.length;
  c.style.left = 12 + (n % 5) * 28 + 'px';
  c.style.top = 12 + (n % 5) * 28 + 'px';
  c.innerHTML = '<div class="t"><span></span><button title="Hide">' + sym('window-minimize') + '</button><button title="Fill the screen">' + sym('window-maximize') + '</button><button title="Close">' + sym('window-close') + '</button></div><pre></pre>';
  c.querySelector('span').textContent = title;
  const [hide, max, stop] = c.querySelectorAll('.t button');
  hide.onclick = () => setHidden(p, true);
  stop.onclick = () => close(p);
  // maximize: fill the desktop, again: back to the size it had
  let before = null;
  max.onclick = () => {
    max.innerHTML = sym(before ? 'window-maximize' : 'window-restore');
    if (before) { Object.assign(c.style, before); before = null; return; }
    before = { left: c.style.left, top: c.style.top, width: c.style.width, height: c.style.height };
    Object.assign(c.style, { left: '0px', top: '0px', width: desk.clientWidth + 'px', height: desk.clientHeight + 'px' });
  };
  // resize by any edge or corner: thin invisible strips around the window (pointer events, so a finger works)
  for (const side of ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw']) {
    const g = document.createElement('div');
    g.className = 'edge ' + side;
    g.addEventListener('pointerdown', (e) => {
      e.stopPropagation();
      e.preventDefault();
      raise(p);
      g.setPointerCapture(e.pointerId);
      const x0 = e.clientX, y0 = e.clientY;
      const l0 = c.offsetLeft, t0 = c.offsetTop, w0 = c.offsetWidth, h0 = c.offsetHeight;
      g.onpointermove = (m) => {
        const dx = m.clientX - x0, dy = m.clientY - y0;
        if (side.includes('e')) c.style.width = Math.max(220, Math.min(desk.clientWidth - l0, w0 + dx)) + 'px';
        if (side.includes('s')) c.style.height = Math.max(140, Math.min(desk.clientHeight - t0, h0 + dy)) + 'px';
        if (side.includes('w')) {
          const w = Math.max(220, w0 - Math.max(dx, -l0));
          c.style.left = l0 + w0 - w + 'px'; c.style.width = w + 'px';
        }
        if (side.includes('n')) {
          const h = Math.max(140, h0 - Math.max(dy, -t0));
          c.style.top = t0 + h0 - h + 'px'; c.style.height = h + 'px';
        }
        before = null;
      };
      g.onpointerup = g.onpointercancel = () => { g.onpointermove = null; };
    });
    c.append(g);
  }
  // drag by the title bar
  const t = c.querySelector('.t');
  t.addEventListener('pointerdown', (e) => {
    if (e.target.closest('button')) return; // a tap on the symbol inside a button must reach the button
    raise(p);
    t.setPointerCapture(e.pointerId);
    const x0 = e.clientX - c.offsetLeft, y0 = e.clientY - c.offsetTop;
    t.onpointermove = (m) => {
      c.style.left = Math.max(-c.offsetWidth + 60, Math.min(desk.clientWidth - 60, m.clientX - x0)) + 'px';
      c.style.top = Math.max(0, Math.min(desk.clientHeight - 30, m.clientY - y0)) + 'px';
    };
    t.onpointerup = () => { t.onpointermove = null; };
  });
  c.addEventListener('pointerdown', () => raise(p));
  desk.append(c);
  // a phone: the window starts inside the screen
  if (c.offsetLeft + c.offsetWidth > desk.clientWidth) c.style.left = Math.max(0, desk.clientWidth - c.offsetWidth) + 'px';
  return c;
}

// sound: each program's audio streams play through one Web Audio context. Browsers start
// it only after a tap or click, so every tap on the page tries to wake it.
const sound = { ctx: null, streams: new Map() };
function soundOn() {
  try {
    if (!sound.ctx) sound.ctx = new (self.AudioContext || self.webkitAudioContext)();
    if (sound.ctx.state === 'suspended') sound.ctx.resume();
  } catch (e) {}
}
document.addEventListener('pointerdown', soundOn, true);
document.addEventListener('keydown', soundOn, true);
function soundStop(key) {
  for (const [k, s] of sound.streams) {
    if (k !== key && !k.startsWith(key + ':')) continue;
    for (const n of s.nodes) try { n.stop(); } catch (e) {}
    sound.streams.delete(k);
  }
}
function play(p, d) {
  const key = p.id + ':' + d.id;
  if (d.stop) { soundStop(key); return; }
  const ctx = sound.ctx;
  if (!ctx || ctx.state !== 'running' || !prefs.sound) return;
  let s = sound.streams.get(key);
  if (!s) sound.streams.set(key, s = { next: 0, nodes: new Set() });
  const frames = d.data.length / d.ch, b = ctx.createBuffer(d.ch, frames, d.rate);
  for (let c = 0; c < d.ch; c++) {
    const out = b.getChannelData(c);
    for (let i = 0; i < frames; i++) out[i] = d.data[i * d.ch + c];
  }
  const n = ctx.createBufferSource();
  n.buffer = b;
  n.connect(ctx.destination);
  // a gap (the program fell behind): start again a little ahead, so blocks join smoothly
  const t = s.next > ctx.currentTime ? s.next : ctx.currentTime + 0.05;
  n.start(t);
  s.next = t + b.duration;
  s.nodes.add(n);
  n.onended = () => s.nodes.delete(n);
}

function close(p) {
  soundStop(p.id + '');
  // ask the program to stop so its files are kept on drive C:; force it after 5 s
  if (p.w) { const w = p.w; w.onmessage = (ev) => { if (ev.data.type === 'done') { w.terminate(); listDrive(); } }; w.postMessage({ type: 'stop' }); setTimeout(() => w.terminate(), 5000); }
  p.w = null;
  if (p.cv) p.cv.remove();
  if (p.con) p.con.remove();
  p.btn.remove();
  progs.splice(progs.indexOf(p), 1);
  if (focus === p) focus = null;
}

function run(files, exe, args, label, home, opts = {}) {
  // Files and Console step aside: the program gets the screen (they use no CPU while hidden)
  for (const q of progs) if (q.tool && !q.hidden) setHidden(q, true);
  const p = { id: nextId++, z: 0, hidden: false, conAway: false };
  progs.push(p);
  p.btn = document.createElement('button');
  p.btn.className = 'task';
  labelled(p.btn, opts.icon || 'application-x-executable', label);
  p.btn.onclick = () => {
    if (p.hidden) { setHidden(p, false); raise(p); }
    else if (p.z === zTop) setHidden(p, true);
    else raise(p);
  };
  $('tasks').append(p.btn);
  p.con = consoleWindow(p, label);
  labelled(p.con.querySelector('.t span'), opts.icon || 'application-x-executable', label);
  const out = p.con.querySelector('pre');
  if (opts.typing) typingLine(p, out);
  raise(p);
  p.label = label;
  p.icon = opts.icon || 'application-x-executable';
  p.errLines = 0;
  const w = p.w = new Worker('worker.js?v=87');
  w.onerror = (e) => { logError(label, 'engine worker failed: ' + (e.message || 'unknown error')); appendOut(out, '\nThe engine stopped: ' + (e.message || 'unknown error') + '\n'); };
  w.onmessage = (ev) => {
    const d = ev.data;
    if (d.type === 'log') {
      const text = d.text.replace(/\r/g, '');
      appendOut(out, text);
      if (p.errLines < 200 && ERR_LINE.test(text)) for (const l of text.split('\n')) if (l.trim() && ERR_LINE.test(l) && p.errLines++ < 200) logError(label, l.trim());
    }
    else if (d.type === 'stats') {
      const gips = d.insns / d.ms / 1e6;
      // for System Monitor: the speed over the last second, not since the start
      const now = performance.now();
      if (!p.lastAt || now - p.lastAt >= 1000) {
        if (p.lastAt && d.ms > p.lastMs) p.mips = (d.insns - p.lastIns) / (d.ms - p.lastMs) / 1000;
        p.lastAt = now; p.lastIns = d.insns; p.lastMs = d.ms;
      }
      p.stats = d; // shown by System Monitor; the console stays plain text
      lastSpeed = d.cores + (d.cores > 1 ? ' cores' : ' core') + ' · ' + (gips >= 1 ? gips.toFixed(1) + ' GIPS' : fmt(gips * 1000) + ' MIPS');
    } else if (d.type === 'frame') frame(p, d);
    else if (d.type === 'audio') play(p, d);
    else if (d.type === 'done') {
      soundStop(p.id + '');
      listDrive();
      // a program with windows that printed nothing: no console to show at the end either
      const quiet = p.cv && !out.textContent.trim();
      if (!d.ok) logError(label, d.error ? 'Stopped: ' + d.error : 'Exit code ' + d.exitCode);
      const end = document.createElement('b');
      end.className = d.ok ? 'ok' : 'bad';
      end.textContent = d.ok ? '\nFinished (exit code 0).' : '\n' + (d.error ? 'Stopped: ' + d.error : 'Exit code ' + d.exitCode);
      out.append(end);
      for (const f of d.out || []) {
        const a = document.createElement('a');
        a.href = URL.createObjectURL(new Blob([f.data]));
        a.download = f.name;
        a.textContent = `\nDownload ${f.name} (${fmt(f.data.length)} bytes)`;
        out.append(a);
      }
      out.scrollTop = out.scrollHeight;
      w.terminate();
      p.w = null;
      if (p.cv) { p.cv.remove(); p.cv = null; }
      if (quiet && d.ok) { close(p); return; }
      p.conAway = false;
      setHidden(p, false);
      raise(p);
    }
  };
  w.postMessage({ type: 'exec', files, exe, cmd: (/\s/.test(exe) ? '"' + exe + '"' : exe) + (args ? ' ' + args : ''), drive: true, home, poolMb: poolMb(), targetMb: 8192,
    jit: prefs.jit, cores: opts.typing ? 1 : coresFor(), typing: !!opts.typing, strong: new URLSearchParams(location.search).get('strong') !== '0', opt: new URLSearchParams(location.search).get('opt') !== '0', winedebug: +new URLSearchParams(location.search).get('winedebug') || 0, winetrace: new URLSearchParams(location.search).get('winetrace') || '' },
  files ? files.map((f) => f.data).filter(Boolean) : []);
}

// a console the user types in (Command Prompt): the line below the output; Enter sends it to
// the program's console input, and it shows in the output the way a console echoes typing
function typingLine(p, out) {
  const line = document.createElement('input');
  line.className = 'cline';
  line.autocapitalize = 'off';
  line.autocomplete = 'off';
  line.spellcheck = false;
  out.after(line);
  const history = [];
  let at = 0;
  line.onkeydown = (e) => {
    if (e.key === 'ArrowUp' && at > 0) { line.value = history[--at]; e.preventDefault(); return; }
    if (e.key === 'ArrowDown') { at = Math.min(at + 1, history.length); line.value = history[at] || ''; e.preventDefault(); return; }
    if (e.key !== 'Enter' || !p.w) return;
    const text = line.value;
    line.value = '';
    if (text.trim()) { history.push(text); at = history.length; }
    appendOut(out, text + '\n');
    p.w.postMessage({ type: 'stdin', text: text + '\n' });
  };
  p.con.addEventListener('pointerup', () => { if (!getSelection().toString()) line.focus(); });
  setTimeout(() => line.focus(), 0);
}

// a program's screen: shown the first time it draws a window; its console steps aside
function frame(p, d) {
  if (!p.cv) {
    p.cv = document.createElement('canvas');
    p.cv.width = W; p.cv.height = H;
    desk.append(p.cv);
    p.ctx = p.cv.getContext('2d');
    p.conAway = true; p.con.hidden = true; // its log returns if it stops with an error
    fit();
    raise(p);
    focus = p;
  }
  p.px = new Uint8ClampedArray(d.data);
  p.ctx.putImageData(new ImageData(p.px, d.w, d.h), 0, 0);
}

/* ---------- mouse and keyboard: the topmost program with a window under the pointer ---------- */
let focus = null, held = null;
const seen = { keys: 0, taps: 0 }; // for System Monitor: the page gets keyboard and touch input
const at = (e) => { const r = desk.getBoundingClientRect(); return [Math.round((e.clientX - r.left) / scale), Math.round((e.clientY - r.top) / scale)]; };
const send = (p, k, x, y, d) => { if (p && p.w) p.w.postMessage({ type: 'win', k, x, y, d }); };
function hit(x, y) {
  if (x < 0 || y < 0 || x >= W || y >= H) return null;
  const list = progs.filter((p) => p.cv && !p.hidden && p.px).sort((a, b) => b.z - a.z);
  for (const p of list) if (p.px[(y * W + x) * 4 + 3]) return p;
  return null;
}
// a shell window (console, File Explorer) under a program's window: the program is on top
// when it was raised later, so the touch goes to it and not to the window below
const zOf = (el) => { const w = el.closest && el.closest('.con'); return w && w.offsetParent ? (parseFloat(w.style.zIndex) || 0) : -1; };
desk.addEventListener('pointerdown', (e) => {
  seen.taps++;
  const [x, y] = at(e);
  const p = hit(x, y);
  if (e.target !== desk && !(p && p.z > zOf(e.target))) return;
  if (e.target !== desk) e.stopPropagation();
  closePops();
  if (!p) { iconTap(e); return; }
  held = p; focus = p;
  raise(p);
  desk.setPointerCapture(e.pointerId);
  send(p, 1, x, y, 0); send(p, 2, x, y, e.button);
  e.preventDefault();
}, true);
desk.addEventListener('pointermove', (e) => {
  const [x, y] = at(e);
  const p = held || hit(x, y);
  if (p) send(p, 1, x, y, 0);
});
desk.addEventListener('pointerup', (e) => {
  if (!held) return;
  const [x, y] = at(e);
  send(held, 3, x, y, e.button);
  held = null;
});
desk.addEventListener('contextmenu', (e) => e.preventDefault());
const key = (kind) => (e) => {
  seen.keys++;
  if (!focus || !focus.cv || e.target.tagName === 'INPUT') return;
  const special = { Enter: 13, Backspace: 8, Tab: 9, Escape: 27 };
  const ch = kind === 4 ? (e.key.length === 1 ? e.key.codePointAt(0) : special[e.key] || 0) : 0;
  send(focus, kind, 0, 0, (e.keyCode & 0xFF) | (ch << 16));
  e.preventDefault();
};
// on-screen game controls: each button holds its key down while touched (several at once)
// they are switched on in the top menu > Controls
$('padon').onchange = () => { $('pad').hidden = !$('padon').checked; sendPad(); };
// a real keyboard got connected and used: the on-screen controls step aside (the top menu brings them back)
addEventListener('keydown', (e) => { if (e.target.tagName !== 'INPUT' && !$('pad').hidden) { $('pad').hidden = true; $('padon').checked = false; } }, true);
addEventListener('keydown', key(4));
addEventListener('keyup', key(5));

/* ---------- Start menu, settings and info ---------- */
const pops = ['menu', 'infop'];
function closePops() { for (const id of pops) $(id).hidden = true; }
function togglePop(id) { const h = $(id).hidden; closePops(); $(id).hidden = !h; return h; }
$('start').onclick = (e) => { e.stopPropagation(); if (togglePop('menu')) { listDrive(); $('find').value = ''; showMenu(); } };
$('more').onclick = (e) => { e.stopPropagation(); togglePop('infop'); };
// the top menu has two tabs: Info and Controls
for (const b of document.querySelectorAll('.tabs button')) {
  b.onclick = () => {
    for (const x of document.querySelectorAll('.tabs button')) x.classList.toggle('on', x === b);
    for (const pane of document.querySelectorAll('[data-pane]')) pane.hidden = pane.dataset.pane !== b.dataset.tab;
  };
}
document.addEventListener('click', (e) => { if (!e.target.closest('.pop')) closePops(); });
$('find').oninput = () => showMenu();
$('find').onkeydown = (e) => { if (e.key === 'Enter') { const b = $('installed').querySelector('button'); if (b) b.click(); } };
const note = (t) => { $('status').textContent = t; };
let lastSpeed = '', driveBytes = 0; // shown in Settings
// the Recycle Bin: deleted files and folders wait in C:\$Recycle.Bin; this list (OPFS
// "inksoc-trash.json") remembers where each came from: [{ id, name, from, at, dir, size }]
let trashFull = false;
async function readTrash() {
  try { return JSON.parse(await (await (await (await navigator.storage.getDirectory()).getFileHandle('inksoc-trash.json')).getFile()).text()) || []; } catch (e) { return []; }
}
async function writeTrash(list) {
  const w = await (await (await navigator.storage.getDirectory()).getFileHandle('inksoc-trash.json', { create: true })).createWritable();
  await w.write(JSON.stringify(list));
  await w.close();
  trashFull = list.length > 0;
  showIcons();
}

// the clock in the bar
const tick = () => { $('clock').textContent = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: prefs.seconds ? '2-digit' : undefined, hour12: !prefs.clock24 }); };
tick();
setInterval(tick, 1000);

// File Explorer, Command Prompt, System Monitor: tools.js loads the first time one is opened
let toolsReady = null;
function loadTools() {
  toolsReady = toolsReady || new Promise((ok, fail) => { const s = document.createElement('script'); s.src = 'tools.js?v=15'; s.onload = ok; s.onerror = fail; document.head.append(s); });
  return toolsReady;
}
async function tool(name, arg) {
  closePops();
  await loadTools();
  ({ files: openFiles, console: openConsole, diag: openDiag, settings: openSettings })[name](arg);
}
for (const b of document.querySelectorAll('[data-tool]')) b.onclick = (e) => { e.stopPropagation(); tool(b.dataset.tool); };

// Shut down: every program stops and keeps its files on drive C:
$('shutdown').onclick = () => {
  closePops();
  const busy = progs.some((p) => p.w);
  for (const p of progs.slice()) close(p);
  // a dark screen, like a computer that is off; programs get up to 5 s to save drive C: first
  const msg = $('off').querySelector('small');
  $('off').querySelector('button').hidden = busy;
  msg.textContent = busy ? 'Saving drive C:…' : 'Every program stopped and drive C: is saved.';
  $('off').hidden = false;
  if (busy) setTimeout(() => { msg.textContent = 'Every program stopped and drive C: is saved.'; $('off').querySelector('button').hidden = false; }, 5200);
};

/* ---------- opening a file that is on drive C: (by its Windows path) ---------- */
const folderOf = (path) => path.slice(0, path.lastIndexOf('\\'));
const baseName = (path) => path.split('\\').pop();
function runPath(path, label) {
  run([], path, '', label || baseName(path).replace(/\.exe$/i, ''), folderOf(path));
}
// a Windows Installer package: Windows' own msiexec installs it
async function installMsi(path) {
  const data = await fetch('bin/wine/wine-msiexec.exe?v=1').then((r) => r.arrayBuffer());
  run([{ name: 'msiexec.exe', data }], 'msiexec.exe', '/i "' + path + '"', baseName(path).replace(/\.msi$/i, ''), folderOf(path), { icon: 'application-x-msi' });
}
// what a double tap does in File Explorer or on the desktop
async function openPath(path, file) {
  if (/\.exe$/i.test(path)) return runPath(path);
  if (/\.msi$/i.test(path)) return installMsi(path);
  if (/\.zip$/i.test(path)) return unzipToDrive(file);
  if (/\.lnk$/i.test(path)) { const t = await lnkTarget(file); if (t) return runPath(t, baseName(path).replace(/\.lnk$/i, '')); note('This shortcut points to nothing on drive C:'); return; }
  // any other file: the browser opens it
  const a = document.createElement('a');
  a.href = URL.createObjectURL(file);
  a.download = baseName(path);
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 60000);
}

/* ---------- drive C: (OPFS folder "inksoc-c", kept apart from the swap file) ---------- */
const HOME = 'C:\\InkSoC'; // the user's profile folder (USERPROFILE in the engine)
async function driveDir(path, create) {
  let d = await (await navigator.storage.getDirectory()).getDirectoryHandle('inksoc-c', { create });
  for (const s of ('C' + path.slice(2)).split('\\').filter(Boolean)) d = await d.getDirectoryHandle(s, { create });
  return d;
}
// files from outside the shell (dropped or picked) are copied into a folder of drive C:
async function copyIn(files, path, into) {
  const dir = into || await driveDir(path, true);
  for (const f of files) {
    note('Copying ' + f.name + '…');
    const name = f.name.replace(/ \(\d+\)(?=\.[^.]+$)/, ''); // "7z (1).dll" -> "7z.dll"
    const w = await (await dir.getFileHandle(name, { create: true })).createWritable();
    await f.stream().pipeTo(w);
  }
  note(files.length + ' file(s) copied to ' + path);
  listDrive();
}
// dropping files on the desktop: they go to Downloads, which opens in File Explorer
desk.addEventListener('dragover', (e) => { e.preventDefault(); desk.classList.add('drop'); });
desk.addEventListener('dragleave', () => desk.classList.remove('drop'));
desk.addEventListener('drop', async (e) => {
  e.preventDefault();
  desk.classList.remove('drop');
  if (e.target.closest && e.target.closest('.con')) return; // a window takes its own drops
  const files = [...e.dataTransfer.files];
  if (!files.length) return;
  await copyIn(files, HOME + '\\Downloads');
  tool('files', HOME + '\\Downloads');
});

// unpacks a .zip to C:\<folder> in a worker, then starts its program when it has just one
function unzipToDrive(file) {
  return new Promise((done) => {
    const w = new Worker('unzip.js?v=1');
    note('Unpacking ' + file.name + '…');
    w.onmessage = async (ev) => {
      const d = ev.data;
      if (d.progress !== undefined) { note('Unpacking ' + file.name + ' · ' + Math.floor((100 * d.progress) / Math.max(1, d.total)) + '%'); return; }
      w.terminate();
      if (d.error) { note(file.name + ': ' + d.error); done(); return; }
      const found = (await listDrive()).filter((p) => p.path.toLowerCase().startsWith('c:\\' + d.done.toLowerCase() + '\\'));
      note('Unpacked to C:\\' + d.done);
      if (found.length === 1) runPath(found[0].path);
      done();
    };
    w.postMessage({ file });
  });
}

// a Windows shortcut (.lnk): the first "X:\...\something.exe" path inside it, read as
// UTF-16 or as bytes (the LinkInfo block keeps the ANSI path, string data the Unicode one)
async function lnkTarget(file) {
  const b = new Uint8Array(await file.slice(0, 65536).arrayBuffer());
  const find = (t) => (t.match(/[a-z]:\\[^\0-\x1f"*?<>|]*?\.exe/i) || [])[0];
  return find(new TextDecoder('utf-16le').decode(b)) || find(new TextDecoder('latin1').decode(b)) || null;
}

// Installed programs: the shortcuts installers leave in the Start Menu and Desktop folders,
// plus the .exe files in the folders of Program Files (and C:\<app> from a .zip)
const skip = /^(unins|uninst|setup|install|update|crash)/i;
const skipLink = /(uninstall|readme|help|website|homepage|license|manual)/i;
const system = /^(windows|programdata|users|inksoc|program files( \(x86\))?)$/i;
let programs = [], desktopItems = [];
async function listDrive() {
  for (const q of progs) if (q.refresh && !q.hidden) q.refresh();
  let root;
  try { root = await (await navigator.storage.getDirectory()).getDirectoryHandle('inksoc-c'); } catch (e) { root = null; }
  let bytes = 0;
  const exes = [], links = [], onDesk = [];
  const walk = async (dir, path, depth) => {
    for await (const [name, h] of dir.entries()) {
      const p = path + '\\' + name;
      if (h.kind === 'directory') { if (name !== '$Recycle.Bin') await walk(h, p, depth + 1); continue; }
      const f = await h.getFile();
      bytes += f.size;
      const win = p.replace(/^C/, 'C:');
      const top = p.split('\\')[1] || '';
      if (/\\desktop\\[^\\]+$/i.test(win) && /\.(lnk|exe)$/i.test(name)) onDesk.push({ path: win, file: f });
      if (/\.lnk$/i.test(name) && /\\(start menu|desktop)\\/i.test(win) && !skipLink.test(name)) links.push({ path: win, file: f });
      // C:\Program Files\App\app.exe, C:\Program Files\App\program\app.exe (LibreOffice, and bin folders), or C:\App\app.exe
      const inPF = /^program files/i.test(top);
      const sub = /\\(program|bin)$/i.test(path);
      const ok = inPF ? depth === 3 || (depth === 4 && sub) : depth === 2 && !system.test(top);
      if (ok && /\.exe$/i.test(name) && !skip.test(name)) exes.push(win);
    }
  };
  if (root) {
    try { await walk(await root.getDirectoryHandle('C'), 'C', 1); } catch (e) { /* nothing installed yet */ }
    for await (const [, h] of root.entries()) if (h.kind === 'file') bytes += (await h.getFile()).size;
  }
  driveBytes = bytes;
  // a shortcut names its program better than the .exe file does
  const byTarget = new Map();
  for (const l of links) {
    const target = await lnkTarget(l.file);
    if (target && !skip.test(baseName(target))) byTarget.set(target.toLowerCase(), { name: baseName(l.path).replace(/\.lnk$/i, ''), path: target });
  }
  for (const path of exes) if (!byTarget.has(path.toLowerCase())) byTarget.set(path.toLowerCase(), { name: baseName(path).replace(/\.exe$/i, ''), path });
  programs = [...byTarget.values()].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
  desktopItems = onDesk;
  trashFull = (await readTrash()).length > 0;
  showMenu();
  showIcons();
  return programs;
}
function showMenu() {
  const q = $('find').value.trim().toLowerCase();
  const list = programs.filter((p) => !q || p.name.toLowerCase().includes(q));
  const none = document.createElement('div');
  none.className = 'none';
  none.textContent = programs.length ? 'No program matches' : 'None yet: install one from File Explorer';
  $('installed').replaceChildren(...(list.length ? list.map((p) => {
    const b = document.createElement('button');
    b.className = 'it';
    labelled(b, 'application-x-executable', p.name);
    b.title = p.path;
    b.onclick = () => { closePops(); runPath(p.path, p.name); };
    return b;
  }) : [none]));
}

/* ---------- context menu: right click, or touch and hold ---------- */
// items: [label, icon, action] or '-' for a line; an item without action shows disabled
function contextMenu(x, y, items) {
  closeMenu();
  const m = document.createElement('div');
  m.className = 'ctx';
  for (const it of items) {
    if (it === '-') { m.append(document.createElement('hr')); continue; }
    const [label, ic, act] = it;
    const b = document.createElement('button');
    labelled(b, ic, label);
    b.disabled = !act;
    b.onclick = (e) => { e.stopPropagation(); closeMenu(); act(); };
    m.append(b);
  }
  document.body.append(m);
  m.style.left = Math.max(4, Math.min(x, innerWidth - m.offsetWidth - 4)) + 'px';
  m.style.top = Math.max(4, Math.min(y, innerHeight - m.offsetHeight - 4)) + 'px';
  setTimeout(() => addEventListener('pointerdown', closeOutside, true), 0);
}
function closeOutside(e) { if (!e.target.closest('.ctx')) closeMenu(); }
function closeMenu() {
  for (const m of document.querySelectorAll('.ctx')) m.remove();
  removeEventListener('pointerdown', closeOutside, true);
}
// calls open(x, y) on a right click, or when a finger stays still on el for half a second;
// the tap that follows a long press does nothing else
function onHold(el, open) {
  el.addEventListener('contextmenu', (e) => { e.preventDefault(); e.stopPropagation(); open(e.clientX, e.clientY); });
  let timer = 0, x0 = 0, y0 = 0;
  el.addEventListener('pointerdown', (e) => {
    if (e.pointerType === 'mouse') return;
    x0 = e.clientX; y0 = e.clientY;
    clearTimeout(timer);
    timer = setTimeout(() => { el.dataset.held = '1'; open(x0, y0); }, 500);
  });
  el.addEventListener('pointermove', (e) => { if (Math.hypot(e.clientX - x0, e.clientY - y0) > 10) clearTimeout(timer); });
  el.addEventListener('pointerup', () => clearTimeout(timer));
  el.addEventListener('pointercancel', () => clearTimeout(timer));
  el.addEventListener('click', (e) => { if (el.dataset.held) { delete el.dataset.held; e.stopPropagation(); e.preventDefault(); } }, true);
}

/* ---------- desktop icons: This PC, Command Prompt, Downloads, then the Desktop folder ---------- */
function showIcons() {
  const items = [
    { icon: 'computer', name: 'This PC', open: () => tool('files', 'C:') },
    { icon: 'utilities-terminal', name: 'Command Prompt', open: () => tool('console') },
    { icon: 'folder-download', name: 'Downloads', open: () => tool('files', HOME + '\\Downloads') },
    { icon: trashFull ? 'user-trash-full' : 'user-trash', name: 'Recycle Bin', open: () => tool('files', 'C:\\$Recycle.Bin') },
    ...desktopItems.map((d) => ({ icon: 'application-x-executable', name: baseName(d.path).replace(/\.(lnk|exe)$/i, ''), open: () => openPath(d.path, d.file) })),
  ];
  $('icons').replaceChildren(...items.map((it) => {
    const el = document.createElement('div');
    el.className = 'ico';
    el.innerHTML = '<span></span>';
    el.prepend(icon(it.icon));
    el.lastChild.textContent = it.name;
    el.open = it.open;
    return el;
  }));
}
// icons sit under the program screens, so the desktop finds them by position:
// a tap selects, a second tap on the same icon opens it (like a double click)
let lastIcon = null, lastTap = 0;
const iconAt = (e) => [...$('icons').children].find((c) => { const r = c.getBoundingClientRect(); return e.clientX >= r.left && e.clientX < r.right && e.clientY >= r.top && e.clientY < r.bottom; });
// touch and hold, or right click, on an icon: Open; on the empty desktop: the usual places
let iconHold = 0;
function iconMenu(x, y) {
  const el = iconAt({ clientX: x, clientY: y });
  if (el) contextMenu(x, y, [['Open', 'folder', () => el.open()]]);
  else contextMenu(x, y, [['File Explorer', 'system-file-manager', () => tool('files')], ['Command Prompt', 'utilities-terminal', () => tool('console')], '-', ['Change wallpaper…', 'preferences-system', () => tool('settings')]]);
}
desk.addEventListener('contextmenu', (e) => { if (e.target === desk && !hit(...at(e))) iconMenu(e.clientX, e.clientY); });
function iconTap(e) {
  clearTimeout(iconHold);
  if (e.pointerType !== 'mouse') { const x = e.clientX, y = e.clientY; iconHold = setTimeout(() => iconMenu(x, y), 550); desk.addEventListener('pointerup', () => clearTimeout(iconHold), { once: true }); }
  const el = iconAt(e);
  for (const c of $('icons').children) c.classList.toggle('sel', c === el);
  if (el && el === lastIcon && Date.now() - lastTap < 500) { el.classList.remove('sel'); lastIcon = null; el.open(); return; }
  lastIcon = el;
  lastTap = Date.now();
}

// keep drive C: when the browser runs short of space
if (navigator.storage && navigator.storage.persist) navigator.storage.persist().catch(() => {});
// the usual folders of the user's profile are there from the start
(async () => {
  for (const f of [HOME + '\\Desktop', HOME + '\\Documents', HOME + '\\Downloads', 'C:\\Program Files', 'C:\\Program Files (x86)']) try { await driveDir(f, true); } catch (e) { /* no OPFS here */ }
  listDrive();
})();

fit();
