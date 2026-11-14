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

// fixed real RAM: the same rule as the tests page (half the lowest tab limit seen)
function poolMb() {
  let s = {};
  try { s = JSON.parse(localStorage.getItem('inksoc-limits')) || {}; } catch (e) {}
  const lim = s.minLimit || (s.calib && !s.calib.running && s.calib.mb) || 0;
  return lim ? Math.max(128, Math.min(2048, Math.floor(lim * 0.5 / 64) * 64)) : 256;
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

function consoleWindow(p, title) {
  const c = document.createElement('div');
  c.className = 'con';
  const n = progs.length;
  c.style.left = 12 + (n % 5) * 28 + 'px';
  c.style.top = 12 + (n % 5) * 28 + 'px';
  c.innerHTML = '<div class="t"><span></span><button title="Hide">–</button><button title="Stop">✕</button></div><table></table><pre></pre>';
  c.querySelector('span').textContent = title + ' — Console';
  const [hide, stop] = c.querySelectorAll('button');
  hide.onclick = () => setHidden(p, true);
  stop.onclick = () => close(p);
  // drag by the title bar
  const t = c.querySelector('.t');
  t.addEventListener('pointerdown', (e) => {
    if (e.target.tagName === 'BUTTON') return;
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
  if (!ctx || ctx.state !== 'running') return;
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

function run(files, exe, args, label, home) {
  // Files and Console step aside: the program gets the screen (they use no CPU while hidden)
  for (const q of progs) if (q.tool && !q.hidden) setHidden(q, true);
  const p = { id: nextId++, z: 0, hidden: false, conAway: false };
  progs.push(p);
  p.btn = document.createElement('button');
  p.btn.className = 'task';
  p.btn.textContent = label;
  p.btn.onclick = () => {
    if (p.hidden) { setHidden(p, false); raise(p); }
    else if (p.z === zTop) setHidden(p, true);
    else raise(p);
  };
  $('tasks').append(p.btn);
  p.con = consoleWindow(p, label);
  const out = p.con.querySelector('pre');
  raise(p);
  const w = p.w = new Worker('worker.js?v=62');
  w.onmessage = (ev) => {
    const d = ev.data;
    if (d.type === 'log') { out.textContent += d.text; out.scrollTop = out.scrollHeight; }
    else if (d.type === 'stats') {
      const gips = d.insns / d.ms / 1e6;
      p.con.querySelector('table').innerHTML = '<tr><td>' + fmt(d.insns / d.ms / 1000) + ' MIPS · ' + (d.jit ? 'JIT' : 'interpreter') + ' · ' + d.cores + (d.cores > 1 ? ' cores' : ' core') + (d.threads > 1 ? ' · ' + d.threads + ' threads' : '') + ' · RAM ' + fmt(d.resident * 4 / 1024) + '/' + fmt(d.frames * 4 / 1024) + ' MB' + (d.swapOut || d.swapIn ? ' · swap ' + fmt(d.swapOut * 4 / 1024) + ' MB out, ' + fmt(d.swapIn * 4 / 1024) + ' MB in, ' + fmt(d.swapMs) + ' ms on ' + (d.opfs ? 'OPFS' : 'RAM') : '') + '</td></tr>';
      $('speed').textContent = d.cores + (d.cores > 1 ? ' cores' : ' core') + ' · ' + (gips >= 1 ? gips.toFixed(1) + ' GIPS' : fmt(gips * 1000) + ' MIPS');
    } else if (d.type === 'frame') frame(p, d);
    else if (d.type === 'audio') play(p, d);
    else if (d.type === 'done') {
      soundStop(p.id + '');
      listDrive();
      // a program with windows that printed nothing: no console to show at the end either
      const quiet = p.cv && !out.textContent.trim();
      out.innerHTML += d.ok ? '<b class="ok">\nFinished (exit code 0).</b>' : '<b class="bad">\n' + (d.error ? 'Stopped: ' + d.error : 'Exit code ' + d.exitCode) + '</b>';
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
    jit: $('jit').checked, cores: +$('cores').value, strong: new URLSearchParams(location.search).get('strong') !== '0', opt: new URLSearchParams(location.search).get('opt') !== '0', winedebug: +new URLSearchParams(location.search).get('winedebug') || 0, winetrace: new URLSearchParams(location.search).get('winetrace') || '' },
  files ? files.map((f) => f.data).filter(Boolean) : []);
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
const seen = { keys: 0, taps: 0 }; // for Diagnostics: the page gets keyboard and touch input
const at = (e) => { const r = desk.getBoundingClientRect(); return [Math.round((e.clientX - r.left) / scale), Math.round((e.clientY - r.top) / scale)]; };
const send = (p, k, x, y, d) => { if (p && p.w) p.w.postMessage({ type: 'win', k, x, y, d }); };
function hit(x, y) {
  if (x < 0 || y < 0 || x >= W || y >= H) return null;
  const list = progs.filter((p) => p.cv && !p.hidden && p.px).sort((a, b) => b.z - a.z);
  for (const p of list) if (p.px[(y * W + x) * 4 + 3]) return p;
  return null;
}
desk.addEventListener('pointerdown', (e) => {
  seen.taps++;
  if (e.target !== desk) return;
  const [x, y] = at(e);
  const p = hit(x, y);
  closePops();
  if (!p) { iconTap(e); return; }
  held = p; focus = p;
  raise(p);
  desk.setPointerCapture(e.pointerId);
  send(p, 1, x, y, 0); send(p, 2, x, y, e.button);
  e.preventDefault();
});
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
const padTarget = () => focus && focus.cv ? focus : progs.filter((p) => p.cv && !p.hidden).sort((a, b) => b.z - a.z)[0];
for (const k of document.querySelectorAll('#pad .k')) {
  const vk = +k.dataset.vk, ch = /^[A-Z]$/.test(k.textContent) ? k.textContent.toLowerCase().codePointAt(0) : vk === 27 ? 27 : 0;
  let held = null;
  k.addEventListener('pointerdown', (e) => { e.preventDefault(); soundOn(); k.setPointerCapture(e.pointerId); held = padTarget(); if (held) send(held, 4, 0, 0, vk | (ch << 16)); k.classList.add('down'); });
  const up = () => { if (held) send(held, 5, 0, 0, vk); held = null; k.classList.remove('down'); };
  k.addEventListener('pointerup', up);
  k.addEventListener('pointercancel', up);
  k.addEventListener('contextmenu', (e) => e.preventDefault());
}
// the 🎮 button shows on touch screens; the controls stay hidden until it is tapped
if (matchMedia('(pointer: coarse)').matches) $('padbtn').hidden = false;
$('padbtn').onclick = () => { const on = $('pad').hidden; $('pad').hidden = !on; $('padbtn').classList.toggle('on', on); };
// a real keyboard got connected and used: the on-screen controls step aside (🎮 brings them back)
addEventListener('keydown', (e) => { if (e.target.tagName !== 'INPUT' && !$('pad').hidden) { $('pad').hidden = true; $('padbtn').classList.remove('on'); } }, true);
addEventListener('keydown', key(4));
addEventListener('keyup', key(5));

/* ---------- Start menu, settings and info ---------- */
const pops = ['menu', 'setp', 'infop'];
function closePops() { for (const id of pops) $(id).hidden = true; }
function togglePop(id) { const h = $(id).hidden; closePops(); $(id).hidden = !h; return h; }
$('start').onclick = (e) => { e.stopPropagation(); if (togglePop('menu')) { listDrive(); $('find').value = ''; showMenu(); } };
$('info').onclick = (e) => { e.stopPropagation(); togglePop('infop'); };
document.addEventListener('click', (e) => { if (!e.target.closest('.pop')) closePops(); });
$('find').oninput = () => showMenu();
$('find').onkeydown = (e) => { if (e.key === 'Enter') { const b = $('installed').querySelector('button'); if (b) b.click(); } };
const note = (t) => { $('status').textContent = t; };

// the clock in the bar
const tick = () => { $('clock').textContent = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }); };
tick();
setInterval(tick, 15000);

// File Explorer, Command Prompt, Diagnostics: tools.js loads the first time one is opened
let toolsReady = null;
function loadTools() {
  toolsReady = toolsReady || new Promise((ok, fail) => { const s = document.createElement('script'); s.src = 'tools.js?v=2'; s.onload = ok; s.onerror = fail; document.head.append(s); });
  return toolsReady;
}
async function tool(name, arg) {
  closePops();
  if (name === 'settings') { $('setp').hidden = false; listDrive(); return; }
  await loadTools();
  ({ files: openFiles, console: openConsole, diag: openDiag })[name](arg);
}
for (const b of document.querySelectorAll('[data-tool]')) b.onclick = (e) => { e.stopPropagation(); tool(b.dataset.tool); };

// Shut down: every program stops and keeps its files on drive C:
$('shutdown').onclick = () => {
  closePops();
  for (const p of progs.slice()) close(p);
  note('All programs stopped; drive C: saved');
};

/* ---------- opening a file that is on drive C: (by its Windows path) ---------- */
const folderOf = (path) => path.slice(0, path.lastIndexOf('\\'));
const baseName = (path) => path.split('\\').pop();
function runPath(path, label) {
  run([], path, $('args').value.trim(), '🪟 ' + (label || baseName(path).replace(/\.exe$/i, '')), folderOf(path));
}
// a Windows Installer package: Windows' own msiexec installs it
async function installMsi(path) {
  const data = await fetch('bin/wine/wine-msiexec.exe?v=1').then((r) => r.arrayBuffer());
  run([{ name: 'msiexec.exe', data }], 'msiexec.exe', '/i "' + path + '"' + ($('args').value.trim() ? ' ' + $('args').value.trim() : ''), '📦 ' + baseName(path).replace(/\.msi$/i, ''), folderOf(path));
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
      if (h.kind === 'directory') { await walk(h, p, depth + 1); continue; }
      const f = await h.getFile();
      bytes += f.size;
      const win = p.replace(/^C/, 'C:');
      const top = p.split('\\')[1] || '';
      if (/\\desktop\\[^\\]+$/i.test(win) && /\.(lnk|exe)$/i.test(name)) onDesk.push({ path: win, file: f });
      if (/\.lnk$/i.test(name) && /\\(start menu|desktop)\\/i.test(win) && !skipLink.test(name)) links.push({ path: win, file: f });
      const deep = /^program files/i.test(top) ? 3 : 2; // C:\Program Files\App\app.exe or C:\App\app.exe
      if (depth === deep && /\.exe$/i.test(name) && !skip.test(name) && (deep === 3 || !system.test(top))) exes.push(win);
    }
  };
  if (root) {
    try { await walk(await root.getDirectoryHandle('C'), 'C', 1); } catch (e) { /* nothing installed yet */ }
    for await (const [, h] of root.entries()) if (h.kind === 'file') bytes += (await h.getFile()).size;
  }
  $('csize').textContent = bytes ? fmt(bytes / 1048576) + ' MB' : 'empty';
  // a shortcut names its program better than the .exe file does
  const byTarget = new Map();
  for (const l of links) {
    const target = await lnkTarget(l.file);
    if (target && !skip.test(baseName(target))) byTarget.set(target.toLowerCase(), { name: baseName(l.path).replace(/\.lnk$/i, ''), path: target });
  }
  for (const path of exes) if (!byTarget.has(path.toLowerCase())) byTarget.set(path.toLowerCase(), { name: baseName(path).replace(/\.exe$/i, ''), path });
  programs = [...byTarget.values()].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
  desktopItems = onDesk;
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
    b.textContent = '🪟 ' + p.name;
    b.title = p.path;
    b.onclick = () => { closePops(); runPath(p.path, p.name); };
    return b;
  }) : [none]));
}

/* ---------- desktop icons: This PC, Command Prompt, Downloads, then the Desktop folder ---------- */
function showIcons() {
  const items = [
    { icon: '🖥️', name: 'This PC', open: () => tool('files', 'C:') },
    { icon: '⬛', name: 'Command Prompt', open: () => tool('console') },
    { icon: '📥', name: 'Downloads', open: () => tool('files', HOME + '\\Downloads') },
    ...desktopItems.map((d) => ({ icon: '🪟', name: baseName(d.path).replace(/\.(lnk|exe)$/i, ''), open: () => openPath(d.path, d.file) })),
  ];
  $('icons').replaceChildren(...items.map((it) => {
    const el = document.createElement('div');
    el.className = 'ico';
    el.innerHTML = '<i></i><span></span>';
    el.firstChild.textContent = it.icon;
    el.lastChild.textContent = it.name;
    el.open = it.open;
    return el;
  }));
}
// icons sit under the program screens, so the desktop finds them by position:
// a tap selects, a second tap on the same icon opens it (like a double click)
let lastIcon = null, lastTap = 0;
function iconTap(e) {
  const el = [...$('icons').children].find((c) => { const r = c.getBoundingClientRect(); return e.clientX >= r.left && e.clientX < r.right && e.clientY >= r.top && e.clientY < r.bottom; });
  for (const c of $('icons').children) c.classList.toggle('sel', c === el);
  if (el && el === lastIcon && Date.now() - lastTap < 500) { el.classList.remove('sel'); lastIcon = null; el.open(); return; }
  lastIcon = el;
  lastTap = Date.now();
}

// erase: a second tap within 4 seconds confirms
let eraseAsk = 0;
$('cerase').onclick = async (e) => {
  e.stopPropagation();
  if (Date.now() - eraseAsk > 4000) { eraseAsk = Date.now(); $('cerase').textContent = 'Tap again'; setTimeout(() => { $('cerase').textContent = 'Erase'; }, 4000); return; }
  eraseAsk = 0;
  $('cerase').textContent = 'Erase';
  try { await (await navigator.storage.getDirectory()).removeEntry('inksoc-c', { recursive: true }); } catch (err) { /* already empty */ }
  listDrive();
};
// keep drive C: when the browser runs short of space
if (navigator.storage && navigator.storage.persist) navigator.storage.persist().catch(() => {});
// the usual folders of the user's profile are there from the start
(async () => {
  for (const f of ['Desktop', 'Documents', 'Downloads']) try { await driveDir(HOME + '\\' + f, true); } catch (e) { /* no OPFS here */ }
  listDrive();
})();

// default: up to 4 cores where the page is isolated (shared memory), 1 elsewhere
$('cores').value = String(self.crossOriginIsolated ? Math.max(1, Math.min(navigator.hardwareConcurrency || 2, 4)) : 1);
fit();
