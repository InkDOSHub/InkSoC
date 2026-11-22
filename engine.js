/* InkSoC engine page: no desktop of its own. It is one window of InkOS (or a plain tab):
   the program's screen fills the page, scaled to fit, and a console program (cmd.exe,
   or any program before it draws a window) shows as a text console.
   What it runs:
   - ?run=C:\path\app.exe        a program already on drive C:
   - ?file=1                     wait for a file from InkOS (assets/inkos-bridge.js turns the
                                 postMessage into an "inkos:file" event): .exe runs, .msi
                                 installs, .zip unpacks, .lnk follows its target
   - ?list=1                     only send InkOS the installed programs (inksoc-programs)
   - nothing                     Command Prompt (cmd.exe)
   Each program runs in its own engine worker (worker.js); a second file sent to the same
   window runs next to the first one, stacked on the same screen. */
'use strict';
const $ = (id) => document.getElementById(id);
const screenEl = $('screen');
const conEl = $('con');
const fmt = (n) => Math.round(n).toLocaleString('en-US');
const W = 1024, H = 768; // the engine's screen (worker.js sends frames of this size)
const progs = [];
let zTop = 10, nextId = 1;
const params = new URLSearchParams(location.search);

// recommended real RAM for each program: half the lowest tab limit the memory calibration saw
// (the calibration lives in InkShell and in older InkSoC pages; same origin, same key)
function recommendedMb() {
  let s = {};
  try { s = JSON.parse(localStorage.getItem('inksoc-limits')) || {}; } catch (e) { /* private window */ }
  const lim = s.minLimit || (s.calib && !s.calib.running && s.calib.mb) || 0;
  return lim ? Math.max(128, Math.min(2048, Math.floor(lim * 0.5 / 64) * 64)) : 256;
}
function poolMb() { return prefs.ramMb || recommendedMb(); }

/* ---------- error log (localStorage 'inksoc-log'), kept for bug reports ---------- */
const LOG_KEY = 'inksoc-log', LOG_MAX = 400;
const errlog = [];
try { errlog.push(...(JSON.parse(localStorage.getItem(LOG_KEY)) || [])); } catch (e) { /* private window */ }
function logError(src, text) {
  errlog.push({ at: Date.now(), src, text: String(text).slice(0, 2000) });
  if (errlog.length > LOG_MAX) errlog.splice(0, errlog.length - LOG_MAX);
  try { localStorage.setItem(LOG_KEY, JSON.stringify(errlog)); } catch (e) { /* full or private */ }
}
const ERR_LINE = /\berr:|\berror\b|exception|unhandled|panic|unreachable|fatal|crash|access violation|out of memory/i;
addEventListener('error', (e) => logError('page', (e.message || 'error') + (e.filename ? ' (' + e.filename.split('/').pop() + ':' + e.lineno + ')' : '')));
addEventListener('unhandledrejection', (e) => logError('page', 'unhandled: ' + (e.reason && (e.reason.stack || e.reason.message) || e.reason)));

/* ---------- the InkOS window around this page ---------- */
// Same origins as assets/inkos-bridge.js: InkOS production and its branch previews.
const PARENT_OK = (o) => o === location.origin || o === 'https://inkos-5k7.pages.dev' || /^https:\/\/[a-z0-9-]+\.inkos-5k7\.pages\.dev$/.test(o);
const parentOrigin = (() => {
  if (window.parent === window) return null;
  try {
    let o = location.ancestorOrigins && location.ancestorOrigins[0];
    if (!o && document.referrer) o = new URL(document.referrer).origin;
    return o && PARENT_OK(o) ? o : null;
  } catch (e) { return null; }
})();
// the window's title follows the program in front
function setTitle(text) {
  document.title = text ? text + ' · InkSoC' : 'InkSoC';
  if (parentOrigin) try { window.parent.postMessage({ v: 1, type: 'inkos-title', title: text || 'InkSoC' }, parentOrigin); } catch (e) { /* closed */ }
}

/* ---------- installed programs ----------
   The shortcuts installers leave in the Start Menu and Desktop folders, plus the .exe files in
   the folders of Program Files (and C:\<app> from a .zip). InkOS gets the list (inksoc-programs)
   and shows it in its Apps and in the CMD/EXE menu; opening one is ?run=<path>. */
const skipExe = /^(unins|uninst|setup|install|update|crash)/i;
const skipLink = /(uninstall|readme|help|website|homepage|license|manual)/i;
const systemDir = /^(windows|programdata|users|inksoc|program files( \(x86\))?)$/i;
async function listPrograms() {
  let root;
  try { root = await (await navigator.storage.getDirectory()).getDirectoryHandle('inksoc-c'); } catch (e) { return []; }
  const exes = [], links = [];
  const walk = async (dir, path, depth) => {
    for await (const [name, h] of dir.entries()) {
      const p = path + '\\' + name;
      if (h.kind === 'directory') { if (name !== '$Recycle.Bin' && depth < 6) await walk(h, p, depth + 1); continue; }
      const win = p.replace(/^C/, 'C:');
      const top = p.split('\\')[1] || '';
      if (/\.lnk$/i.test(name) && /\\(start menu|desktop)\\/i.test(win) && !skipLink.test(name)) links.push({ path: win, file: await h.getFile() });
      const inPF = /^program files/i.test(top);
      const sub = /\\(program|bin)$/i.test(path);
      const ok = inPF ? depth === 3 || (depth === 4 && sub) : depth === 2 && !systemDir.test(top);
      if (ok && /\.exe$/i.test(name) && !skipExe.test(name)) exes.push(win);
    }
  };
  try { await walk(await root.getDirectoryHandle('C'), 'C', 1); } catch (e) { return []; }
  // a shortcut names its program better than the .exe file does
  const byTarget = new Map();
  for (const l of links) {
    const target = await lnkTarget(l.file);
    if (target && !skipExe.test(baseName(target))) byTarget.set(target.toLowerCase(), { name: baseName(l.path).replace(/\.lnk$/i, ''), path: target });
  }
  for (const path of exes) if (!byTarget.has(path.toLowerCase())) byTarget.set(path.toLowerCase(), { name: baseName(path).replace(/\.exe$/i, ''), path });
  return [...byTarget.values()].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })).slice(0, 200);
}
let knownPrograms = null;
async function postPrograms() {
  const list = await listPrograms();
  if (parentOrigin) try { window.parent.postMessage({ v: 1, type: 'inksoc-programs', programs: list }, parentOrigin); } catch (e) { /* closed */ }
  const before = knownPrograms;
  knownPrograms = list;
  return before ? list.filter((p) => !before.some((q) => q.path.toLowerCase() === p.path.toLowerCase())) : [];
}
// after an installer: a button for each new program, in the console
function offerPrograms(p, list) {
  if (!list.length) return;
  const box = document.createElement('div');
  box.className = 'progs';
  const t = document.createElement('span');
  t.textContent = 'Installed (also in InkOS Apps): ';
  box.append(t);
  for (const it of list) {
    const b = document.createElement('button');
    b.type = 'button';
    b.textContent = it.name;
    b.title = it.path;
    b.onclick = () => runPath(it.path, it.name);
    box.append(b);
  }
  p.out.after(box);
  p.extra = box;
}

/* ---------- status line (copying, unpacking, errors) ---------- */
let noteTimer = 0;
function note(t) {
  const n = $('note');
  n.textContent = t;
  n.hidden = !t;
  clearTimeout(noteTimer);
  if (t) noteTimer = setTimeout(() => { n.hidden = true; }, 4000);
}

// console output, as Wine's console: plain text, only the last part kept
const OUT_MAX = 200000;
function appendOut(out, text) {
  if (!text) return;
  out.append(text);
  out.chars = (out.chars || 0) + text.length;
  while (out.chars > OUT_MAX && out.firstChild) {
    out.chars -= out.firstChild.textContent.length;
    out.firstChild.remove();
  }
  if (!out.pending) { out.pending = true; requestAnimationFrame(() => { out.pending = false; conEl.scrollTop = conEl.scrollHeight; }); }
}

/* ---------- the screen: program canvases scaled to fit, centred ---------- */
// The view follows the programs' windows: the part of the 1024x768 screen they cover fills
// the InkOS window (up to 2x), so a small dialog is not a speck in a big black frame.
let scale = 1, ox = 0, oy = 0, view = { x: 0, y: 0, w: W, h: H }, viewAt = 0;
function fit() {
  const w = screenEl.clientWidth, h = screenEl.clientHeight;
  scale = Math.min(w / view.w, h / view.h, 2) || 1;
  ox = Math.round((w - view.w * scale) / 2 - view.x * scale);
  oy = Math.round((h - view.h * scale) / 2 - view.y * scale);
  for (const p of progs) if (p.cv) Object.assign(p.cv.style, { width: W * scale + 'px', height: H * scale + 'px', left: ox + 'px', top: oy + 'px' });
}
addEventListener('resize', fit);

// what the page shows: the front program's screen, or its console when it has none
function show() {
  const front = progs.slice().sort((a, b) => b.z - a.z)[0];
  for (const p of progs) {
    p.out.hidden = p !== front;
    if (p.extra) p.extra.hidden = p !== front;
    if (p.line) p.line.hidden = p !== front || !p.w;
  }
  const gui = front && front.cv && front.conAway;
  conEl.hidden = !front || gui;
  screenEl.classList.toggle('gui', !!gui);
  $('padbtn').hidden = !gui || !matchMedia('(pointer: coarse)').matches;
  setTitle(front ? front.label : '');
}
function raise(p) {
  p.z = ++zTop;
  if (p.cv) p.cv.style.zIndex = p.z;
  show();
}

/* ---------- sound: one Web Audio context, woken by the first tap or key ---------- */
const sound = { ctx: null, streams: new Map() };
function soundOn() {
  try {
    if (!sound.ctx) sound.ctx = new (self.AudioContext || self.webkitAudioContext)();
    if (sound.ctx.state === 'suspended') sound.ctx.resume();
  } catch (e) { /* no audio */ }
}
document.addEventListener('pointerdown', soundOn, true);
document.addEventListener('keydown', soundOn, true);
function soundStop(key) {
  for (const [k, s] of sound.streams) {
    if (k !== key && !k.startsWith(key + ':')) continue;
    for (const n of s.nodes) try { n.stop(); } catch (e) { /* ended */ }
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
  const t = s.next > ctx.currentTime ? s.next : ctx.currentTime + 0.05;
  n.start(t);
  s.next = t + b.duration;
  s.nodes.add(n);
  n.onended = () => s.nodes.delete(n);
}

/* ---------- running a program ---------- */
function run(files, exe, args, label, home, opts = {}) {
  const p = { id: nextId++, z: 0, conAway: false, label };
  progs.push(p);
  p.out = document.createElement('pre');
  conEl.append(p.out);
  const w = p.w = new Worker('worker.js?v=105');
  if (opts.typing) typingLine(p);
  raise(p);
  w.onerror = (e) => { logError(label, 'engine worker failed: ' + (e.message || 'unknown error')); appendOut(p.out, '\nThe engine stopped: ' + (e.message || 'unknown error') + '\n'); };
  w.onmessage = (ev) => {
    const d = ev.data;
    if (d.type === 'log') {
      const text = d.text.replace(/\r/g, '');
      appendOut(p.out, text);
      if (ERR_LINE.test(text)) for (const l of text.split('\n')) if (l.trim() && ERR_LINE.test(l)) logError(label, l.trim());
    } else if (d.type === 'stats') p.stats = d;
    else if (d.type === 'frame') frame(p, d);
    else if (d.type === 'audio') play(p, d);
    else if (d.type === 'done') done(p, d);
  };
  w.postMessage({ type: 'exec', files, exe, cmd: (/\s/.test(exe) ? '"' + exe + '"' : exe) + (args ? ' ' + args : ''), drive: true, home, poolMb: poolMb(), targetMb: 8192,
    jit: prefs.jit, cores: opts.typing ? 1 : coresFor(), typing: !!opts.typing, strong: params.get('strong') !== '0', opt: params.get('opt') !== '0', winedebug: +params.get('winedebug') || 0, maxout: +params.get('maxout') || 0, winetrace: params.get('winetrace') || '' },
  files ? files.map((f) => f.data).filter(Boolean) : []);
  return p;
}
function done(p, d) {
  soundStop(p.id + '');
  const quiet = p.cv && !p.out.textContent.trim();
  if (!d.ok) logError(p.label, d.error ? 'Stopped: ' + d.error : 'Exit code ' + d.exitCode);
  const end = document.createElement('b');
  end.className = d.ok ? 'ok' : 'bad';
  end.textContent = d.ok ? '\nFinished (exit code 0).' : '\n' + (d.error ? 'Stopped: ' + d.error : 'Exit code ' + d.exitCode);
  p.out.append(end);
  for (const f of d.out || []) {
    const a = document.createElement('a');
    a.href = URL.createObjectURL(new Blob([f.data]));
    a.download = f.name;
    a.textContent = `\nDownload ${f.name} (${fmt(f.data.length)} bytes)`;
    p.out.append(a);
  }
  p.w.terminate();
  p.w = null;
  if (p.cv) { p.cv.remove(); p.cv = null; }
  p.conAway = false;
  // a program with windows that printed nothing and ended well leaves nothing to show
  postPrograms().then((fresh) => offerPrograms(p, fresh)).catch(() => {});
  if (quiet && d.ok && progs.length > 1) { close(p); return; }
  show();
}
function close(p) {
  soundStop(p.id + '');
  if (p.w) { const w = p.w; w.onmessage = (ev) => { if (ev.data.type === 'done') w.terminate(); }; w.postMessage({ type: 'stop' }); setTimeout(() => w.terminate(), 5000); }
  p.w = null;
  if (p.cv) p.cv.remove();
  p.out.remove();
  if (p.line) p.line.remove();
  if (p.extra) p.extra.remove();
  progs.splice(progs.indexOf(p), 1);
  if (focus === p) focus = null;
  show();
}
// closing the InkOS window (or the tab): programs get the stop message, so drive C: is saved
addEventListener('pagehide', () => { for (const p of progs) if (p.w) p.w.postMessage({ type: 'stop' }); });

// Command Prompt: the line below the output; Enter sends it to the program's console input
function typingLine(p) {
  const line = p.line = document.createElement('input');
  line.className = 'cline';
  line.autocapitalize = 'off';
  line.autocomplete = 'off';
  line.spellcheck = false;
  line.setAttribute('aria-label', 'Command');
  conEl.append(line);
  const history = [];
  let at = 0;
  line.onkeydown = (e) => {
    if (e.key === 'ArrowUp' && at > 0) { line.value = history[--at]; e.preventDefault(); return; }
    if (e.key === 'ArrowDown') { at = Math.min(at + 1, history.length); line.value = history[at] || ''; e.preventDefault(); return; }
    if (e.key !== 'Enter' || !p.w) return;
    const text = line.value;
    line.value = '';
    if (text.trim()) { history.push(text); at = history.length; }
    appendOut(p.out, text + '\n');
    p.w.postMessage({ type: 'stdin', text: text + '\n' });
  };
  setTimeout(() => line.focus(), 0);
}
conEl.addEventListener('pointerup', () => {
  const p = progs.slice().sort((a, b) => b.z - a.z)[0];
  if (p && p.line && !p.line.hidden && !getSelection().toString()) p.line.focus();
});

// a program's screen: shown the first time it draws a window; its console steps aside
function frame(p, d) {
  if (!p.cv) {
    p.cv = document.createElement('canvas');
    p.cv.width = W; p.cv.height = H;
    screenEl.append(p.cv);
    p.ctx = p.cv.getContext('2d');
    p.conAway = true; // its log returns if it stops
    fit();
    raise(p);
    focus = p;
  }
  p.px = new Uint8ClampedArray(d.data);
  p.ctx.putImageData(new ImageData(p.px, d.w, d.h), 0, 0);
  if (!held && performance.now() - viewAt > 400) { viewAt = performance.now(); follow(); }
}
// the box around every window on the screen (sampled every 4 pixels), with a small margin
function follow() {
  let x0 = W, y0 = H, x1 = -1, y1 = -1;
  for (const p of progs) {
    if (!p.cv || !p.px) continue;
    const px = p.px;
    for (let y = 0; y < H; y += 4) {
      const row = y * W * 4;
      for (let x = 0; x < W; x += 4) {
        if (!px[row + x * 4 + 3]) continue;
        if (x < x0) x0 = x;
        if (x > x1) x1 = x;
        if (y < y0) y0 = y;
        if (y > y1) y1 = y;
      }
    }
  }
  const next = x1 < 0 ? { x: 0, y: 0, w: W, h: H } : (() => {
    const x = Math.max(0, x0 - 8), y = Math.max(0, y0 - 8);
    return { x, y, w: Math.min(W, x1 + 12) - x, h: Math.min(H, y1 + 12) - y };
  })();
  // small moves (a window redrawing its edge) do not shake the view
  if (Math.abs(next.x - view.x) + Math.abs(next.y - view.y) + Math.abs(next.w - view.w) + Math.abs(next.h - view.h) < 16) return;
  view = next;
  fit();
  sizeParent();
}
// ask InkOS to fit its window to the programs' windows (it ignores this once the user resizes)
let sentSize = '';
function sizeParent() {
  if (!parentOrigin || (view.w === W && view.h === H)) return;
  const key = view.w + 'x' + view.h;
  if (key === sentSize) return;
  sentSize = key;
  try { window.parent.postMessage({ v: 1, type: 'inkos-size', width: view.w, height: view.h }, parentOrigin); } catch (e) { /* closed */ }
}

/* ---------- mouse, touch and keyboard: the topmost program with a window under the pointer ---------- */
let focus = null, held = null;
const at = (e) => { const r = screenEl.getBoundingClientRect(); return [Math.round((e.clientX - r.left - ox) / scale), Math.round((e.clientY - r.top - oy) / scale)]; };
const send = (p, k, x, y, d) => { if (p && p.w) p.w.postMessage({ type: 'win', k, x, y, d }); };
function hit(x, y) {
  if (x < 0 || y < 0 || x >= W || y >= H) return null;
  const list = progs.filter((p) => p.cv && p.px).sort((a, b) => b.z - a.z);
  for (const p of list) if (p.px[(y * W + x) * 4 + 3]) return p;
  return null;
}
screenEl.addEventListener('pointerdown', (e) => {
  const [x, y] = at(e);
  const p = hit(x, y);
  if (!p) return;
  held = p; focus = p;
  if (p.z !== zTop) raise(p);
  screenEl.setPointerCapture(e.pointerId);
  send(p, 1, x, y, 0); send(p, 2, x, y, e.button);
  e.preventDefault();
});
screenEl.addEventListener('pointermove', (e) => {
  const [x, y] = at(e);
  const p = held || hit(x, y);
  if (p) send(p, 1, x, y, 0);
});
screenEl.addEventListener('pointerup', (e) => {
  if (!held) return;
  const [x, y] = at(e);
  send(held, 3, x, y, e.button);
  held = null;
});
// the program gets the right button; the page shows no menu of its own
screenEl.addEventListener('contextmenu', (e) => e.preventDefault());
const key = (kind) => (e) => {
  if (!focus || !focus.cv || e.target.tagName === 'INPUT') return;
  const special = { Enter: 13, Backspace: 8, Tab: 9, Escape: 27 };
  const ch = kind === 4 ? (e.key.length === 1 ? e.key.codePointAt(0) : special[e.key] || 0) : 0;
  send(focus, kind, 0, 0, (e.keyCode & 0xFF) | (ch << 16));
  e.preventDefault();
};
addEventListener('keydown', key(4));
addEventListener('keyup', key(5));
// a real keyboard got used: the on-screen controls step aside
addEventListener('keydown', (e) => { if (e.target.tagName !== 'INPUT' && !$('pad').hidden) { $('pad').hidden = true; sendPad(); } }, true);
// touch screens: the controller button shows the on-screen gamepad (pad.js) over a program's screen
$('padbtn').onclick = () => { $('pad').hidden = !$('pad').hidden; sendPad(); };

/* ---------- drive C: (OPFS folder "inksoc-c", kept apart from the swap file) ---------- */
const HOME = 'C:\\InkSoC'; // the user's profile folder (USERPROFILE in the engine)
const folderOf = (path) => path.slice(0, path.lastIndexOf('\\'));
const baseName = (path) => path.split('\\').pop();
async function driveDir(path, create) {
  let d = await (await navigator.storage.getDirectory()).getDirectoryHandle('inksoc-c', { create });
  for (const s of ('C' + path.slice(2)).split('\\').filter(Boolean)) d = await d.getDirectoryHandle(s, { create });
  return d;
}
// files from outside (InkOS, a drop) are copied into a folder of drive C:
async function copyIn(files, path) {
  const dir = await driveDir(path, true);
  for (const f of files) {
    note('Copying ' + f.name + '…');
    const name = f.name.replace(/ \(\d+\)(?=\.[^.]+$)/, ''); // "7z (1).dll" -> "7z.dll"
    const w = await (await dir.getFileHandle(name, { create: true })).createWritable();
    await f.stream().pipeTo(w);
  }
  note('');
}
async function driveFile(path) {
  const dir = await driveDir(folderOf(path), false);
  return (await dir.getFileHandle(baseName(path))).getFile();
}

function runPath(path, label, args) {
  return run([], path, args || '', label || baseName(path).replace(/\.exe$/i, ''), folderOf(path));
}
// ?args=...: extra command-line arguments for the file opened with ?file=1 or ?run= (an
// installer's silent switch: /S for NSIS, /qb for Windows Installer), used by the tests
const openArgs = params.get('args') || '';
// a Windows Installer package: Windows' own msiexec installs it
async function installMsi(path) {
  const data = await fetch('bin/wine/wine-msiexec.exe?v=1').then((r) => r.arrayBuffer());
  run([{ name: 'msiexec.exe', data }], 'msiexec.exe', '/i "' + path + '"' + (openArgs ? ' ' + openArgs : ''), baseName(path).replace(/\.msi$/i, ''), folderOf(path));
}
// unpacks a .zip to C:\<folder> in a worker, then starts its program when it has just one
function unzipToDrive(file) {
  return new Promise((ok) => {
    const w = new Worker('unzip.js?v=1');
    note('Unpacking ' + file.name + '…');
    w.onmessage = async (ev) => {
      const d = ev.data;
      if (d.progress !== undefined) { note('Unpacking ' + file.name + ' · ' + Math.floor((100 * d.progress) / Math.max(1, d.total)) + '%'); return; }
      w.terminate();
      if (d.error) { note(file.name + ': ' + d.error); ok(); return; }
      note('Unpacked to C:\\' + d.done);
      const exes = await findExes('C:\\' + d.done);
      if (exes.length === 1) runPath(exes[0]);
      else if (!progs.length) startConsole('cd /d "C:\\' + d.done + '"');
      ok();
    };
    w.postMessage({ file });
  });
}
async function findExes(path) {
  const found = [];
  const walk = async (dir, p, depth) => {
    for await (const [name, h] of dir.entries()) {
      if (h.kind === 'directory') { if (depth < 2) await walk(h, p + '\\' + name, depth + 1); }
      else if (/\.exe$/i.test(name) && !/^(unins|uninst|setup|install|update|crash)/i.test(name)) found.push(p + '\\' + name);
    }
  };
  try { await walk(await driveDir(path, false), path, 0); } catch (e) { /* nothing there */ }
  return found;
}
// a Windows shortcut (.lnk): the first "X:\...\something.exe" path inside it
async function lnkTarget(file) {
  const b = new Uint8Array(await file.slice(0, 65536).arrayBuffer());
  const find = (t) => (t.match(/[a-z]:\\[^\0-\x1f"*?<>|]*?\.exe/i) || [])[0];
  return find(new TextDecoder('utf-16le').decode(b)) || find(new TextDecoder('latin1').decode(b)) || null;
}
// what opening a file of drive C: does
async function openPath(path, file) {
  if (/\.exe$/i.test(path)) return runPath(path, '', openArgs);
  if (/\.msi$/i.test(path)) return installMsi(path);
  if (/\.zip$/i.test(path)) return unzipToDrive(file || await driveFile(path));
  if (/\.lnk$/i.test(path)) {
    const t = await lnkTarget(file || await driveFile(path));
    if (t) return runPath(t, baseName(path).replace(/\.lnk$/i, ''));
    note('This shortcut points to nothing on drive C:');
    return;
  }
  // any other file: Command Prompt in its folder, where a program can open it
  if (!progs.length) startConsole('cd /d "' + folderOf(path) + '"');
}

// Command Prompt: Wine's cmd.exe with the tools of bin/system32 (7-Zip, BusyBox...)
let system32 = null;
async function startConsole(first) {
  system32 = system32 || fetch('bin/system32/list.json?v=1').then((r) => r.json()).then((names) => Promise.all(names.map(async (name) => ({ name, data: await fetch('bin/system32/' + name + '?v=1').then((r) => r.arrayBuffer()) }))));
  const files = (await system32).map((f) => ({ name: f.name, data: f.data.slice(0) }));
  const p = run(files, 'cmd.exe', '', 'Command Prompt', '', { typing: true });
  if (first) p.w.postMessage({ type: 'stdin', text: first + '\n' });
  return p;
}

// a file sent from InkOS: copy it to Downloads, then open it
window.addEventListener('inkos:file', (e) => {
  const file = e.detail;
  if (!(file instanceof File)) return;
  e.preventDefault();
  gotFile = true;
  (async () => {
    const dir = HOME + '\\Downloads';
    await copyIn([file], dir);
    const name = file.name.replace(/ \(\d+\)(?=\.[^.]+$)/, '');
    await openPath(dir + '\\' + name, file);
  })().catch((err) => { note(file.name + ': ' + (err && err.message || err)); if (!progs.length) startConsole(); });
});
// files dropped on the page (a plain tab on a computer): same as a file from InkOS
addEventListener('dragover', (e) => e.preventDefault());
addEventListener('drop', async (e) => {
  e.preventDefault();
  for (const f of e.dataTransfer.files) window.dispatchEvent(new CustomEvent('inkos:file', { detail: f, cancelable: true }));
});

/* ---------- start ---------- */
if (navigator.storage && navigator.storage.persist) navigator.storage.persist().catch(() => {});
let gotFile = false;
fit();
show();
(async () => {
  for (const f of [HOME + '\\Desktop', HOME + '\\Documents', HOME + '\\Downloads', 'C:\\Program Files', 'C:\\Program Files (x86)']) try { await driveDir(f, true); } catch (e) { /* no OPFS here */ }
  // ?list=1: InkOS asks only for the installed programs (a hidden frame); nothing runs
  if (params.has('list')) { await postPrograms().catch(() => {}); return; }
  postPrograms().catch(() => {});
  const path = params.get('run');
  if (path && /^[a-z]:\\/i.test(path)) { openPath(path).catch((err) => { note(baseName(path) + ': ' + (err && err.message || err)); startConsole(); }); return; }
  if (params.has('file')) {
    // InkOS sends the file once this page says it is ready; Command Prompt if none comes
    note('Waiting for the file…');
    setTimeout(() => { if (!gotFile && !progs.length) { note(''); startConsole(); } }, 8000);
    return;
  }
  startConsole();
})();

// a one-colour icon drawn in the text colour (pad.js uses it for its menu control); our own
// shapes, inline, so the page ships no icon theme
const SYMS = { 'open-menu': '<rect x="2" y="3" width="12" height="2" rx="1"/><rect x="2" y="7" width="12" height="2" rx="1"/><rect x="2" y="11" width="12" height="2" rx="1"/>' };
function sym(name, cls) {
  const svg = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16">' + (SYMS[name] || '') + '</svg>';
  return '<i class="sym' + (cls ? ' ' + cls : '') + '" style="--s:url(&quot;data:image/svg+xml,' + encodeURIComponent(svg) + '&quot;)"></i>';
}
