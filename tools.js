/* InkSoC shell tools: File Explorer (drive C:), Command Prompt (Wine's cmd.exe) and
   Diagnostics. Both are plain page code over the drive's storage (OPFS folder "inksoc-c/C"):
   this file loads the first time one is opened, and they use no CPU while they wait, so a
   program keeps the whole engine. They step aside (hide) when a program starts. */
'use strict';

/* ---------- drive C: paths ---------- */
const driveRoot = async () => (await (await navigator.storage.getDirectory()).getDirectoryHandle('inksoc-c', { create: true })).getDirectoryHandle('C', { create: true });
// "C:\a\b" or "a\b" (from cwd) -> ['a', 'b']; ".." and "." work
function pathParts(cwd, p) {
  p = (p || '').replace(/\//g, '\\').trim().replace(/^"|"$/g, '');
  let parts = /^[a-z]:/i.test(p) ? [] : cwd.slice();
  p = p.replace(/^[a-z]:/i, '');
  if (p.startsWith('\\')) parts = [];
  for (const s of p.split('\\')) {
    if (!s || s === '.') continue;
    if (s === '..') parts.pop();
    else parts.push(s);
  }
  return parts;
}
const showPath = (parts) => 'C:\\' + parts.join('\\');
// the folder at parts (null when it is not there); names match whatever their case
async function dirAt(parts, create) {
  let d = await driveRoot();
  for (const s of parts) {
    const n = await realName(d, s);
    try { d = await d.getDirectoryHandle(n || s, { create: !!create }); } catch (e) { return null; }
  }
  return d;
}
async function realName(dir, name) {
  const low = name.toLowerCase();
  for await (const k of dir.keys()) if (k.toLowerCase() === low) return k;
  return null;
}
// the entry at parts: { dir (its folder), name, h (handle) } or null
async function entryAt(parts) {
  if (!parts.length) return null;
  const dir = await dirAt(parts.slice(0, -1));
  if (!dir) return null;
  const name = await realName(dir, parts[parts.length - 1]);
  if (!name) return null;
  let h;
  try { h = await dir.getFileHandle(name); } catch (e) { h = await dir.getDirectoryHandle(name); }
  return { dir, name, h };
}
async function listDir(dir) {
  const out = [];
  for await (const [name, h] of dir.entries()) out.push({ name, h, dir: h.kind === 'directory', size: h.kind === 'file' ? (await h.getFile()).size : 0 });
  return out.sort((a, b) => (b.dir - a.dir) || a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
}
async function copyFile(h, toDir, name) {
  const w = await (await toDir.getFileHandle(name, { create: true })).createWritable();
  await (await h.getFile()).stream().pipeTo(w);
}
const size = (n) => (n >= 1048576 ? (n / 1048576).toFixed(1) + ' MB' : n >= 1024 ? Math.ceil(n / 1024) + ' KB' : n + ' B');

/* ---------- a tool window (looks like a console window, has a bar button) ---------- */
function toolWindow(label, cls, ic) {
  const p = { id: nextId++, z: 0, hidden: false, conAway: false, tool: true };
  progs.push(p);
  p.btn = document.createElement('button');
  p.btn.className = 'task';
  labelled(p.btn, ic, label);
  p.btn.onclick = () => {
    if (p.hidden) { setHidden(p, false); raise(p); }
    else if (p.z === zTop) setHidden(p, true);
    else raise(p);
  };
  $('tasks').append(p.btn);
  p.con = consoleWindow(p, label);
  p.con.classList.add(cls);
  labelled(p.con.querySelector('span'), ic, label);
  p.con.querySelector('table').remove();
  if (cls === 'win') p.con.style.left = Math.max(0, Math.min(110 + (progs.length % 5) * 28, desk.clientWidth - p.con.offsetWidth)) + 'px'; // the desktop icons stay visible (on a phone it fits the screen)
  raise(p);
  return p;
}

/* ---------- File Explorer ---------- */
// one window; opening it again (from an icon or "explorer" in the prompt) goes to that folder
function openFiles(start) {
  const old = progs.find((q) => q.tool === 'files');
  if (old) { setHidden(old, false); raise(old); if (start) old.go(pathParts([], start)); return; }
  const p = toolWindow('File Explorer', 'win', 'system-file-manager');
  p.tool = 'files';
  const pre = p.con.querySelector('pre');
  const head = document.createElement('div');
  head.className = 'fbar';
  head.innerHTML = '<button title="Back">←</button><button title="Forward">→</button><button title="Up">↑</button><input type="text" spellcheck="false" autocomplete="off"><label title="Copy files from this device into this folder">Copy here…<input type="file" multiple hidden></label>';
  pre.before(head);
  const body = document.createElement('div');
  body.className = 'body';
  const side = document.createElement('div');
  side.className = 'side';
  const list = document.createElement('div');
  list.className = 'flist';
  body.append(side, list);
  pre.replaceWith(body);
  const [back, fwd, up] = head.querySelectorAll('button');
  const addr = head.querySelector('input[type=text]');
  const places = [['drive-harddisk', 'Local Disk (C:)', ''], ['user-desktop', 'Desktop', 'InkSoC\\Desktop'], ['folder-download', 'Downloads', 'InkSoC\\Downloads'], ['folder-documents', 'Documents', 'InkSoC\\Documents'], ['folder', 'Program Files', 'Program Files'], ['folder', 'Program Files (x86)', 'Program Files (x86)'], ['folder', 'Windows', 'Windows']];
  side.replaceChildren(...places.map(([ic, label, path]) => { const d = document.createElement('div'); labelled(d, ic, label); d.dataset.path = path; d.onclick = () => go(pathParts([], 'C:\\' + path)); return d; }));
  let cwd = [], past = [], ahead = [], sel = null;
  function go(parts, how) {
    if (how !== 'history') { past.push(cwd); ahead = []; }
    cwd = parts;
    show();
  }
  p.go = go;
  back.onclick = () => { if (past.length) { ahead.push(cwd); go(past.pop(), 'history'); } };
  fwd.onclick = () => { if (ahead.length) { past.push(cwd); go(ahead.pop(), 'history'); } };
  up.onclick = () => go(cwd.slice(0, -1));
  addr.onkeydown = async (e) => { if (e.key === 'Enter') { const parts = pathParts(cwd, addr.value); if (await dirAt(parts)) go(parts); else addr.value = showPath(cwd); } };
  // files from this device are copied into the open folder: by "Copy here…" or by dropping them
  const bring = async (files) => { await copyIn(files, showPath(cwd), await dirAt(cwd, true)); show(); };
  head.querySelector('input[type=file]').onchange = async (ev) => { await bring([...ev.target.files]); ev.target.value = ''; };
  p.con.addEventListener('dragover', (e) => { e.preventDefault(); p.con.classList.add('drop'); });
  p.con.addEventListener('dragleave', () => p.con.classList.remove('drop'));
  p.con.addEventListener('drop', (e) => { e.preventDefault(); e.stopPropagation(); p.con.classList.remove('drop'); if (e.dataTransfer.files.length) bring([...e.dataTransfer.files]); });
  let ask = null;
  async function show() {
    addr.value = showPath(cwd);
    labelled(p.con.querySelector('.t span'), cwd.length ? 'folder' : 'drive-harddisk', cwd.length ? cwd[cwd.length - 1] : 'Local Disk (C:)');
    back.disabled = !past.length;
    fwd.disabled = !ahead.length;
    up.disabled = !cwd.length;
    for (const d of side.children) d.classList.toggle('on', d.dataset.path.toLowerCase() === cwd.join('\\').toLowerCase());
    const dir = await dirAt(cwd);
    if (!dir) { cwd = []; return show(); }
    const items = await listDir(dir);
    list.replaceChildren(...items.map((it) => {
      const row = document.createElement('div');
      row.className = 'frow';
      const kind = it.dir ? 'folder' : /\.(exe|lnk)$/i.test(it.name) ? 'application-x-executable' : /\.msi$/i.test(it.name) ? 'application-x-msi' : /\.zip$/i.test(it.name) ? 'application-zip' : 'text-x-generic';
      row.innerHTML = '<span class="n"></span><span class="z"></span><button title="Save a copy on this device">⬇</button><button title="Delete">🗑</button>';
      labelled(row.querySelector('.n'), kind, it.name);
      row.querySelector('.z').textContent = it.dir ? '' : size(it.size);
      const [dl, del] = row.querySelectorAll('button');
      if (it.dir) dl.remove();
      else dl.onclick = async () => { const a = document.createElement('a'); a.href = URL.createObjectURL(await it.h.getFile()); a.download = it.name; a.click(); setTimeout(() => URL.revokeObjectURL(a.href), 60000); };
      // delete: a second tap within 4 seconds confirms
      del.onclick = async () => {
        if (ask !== del) { ask = del; del.textContent = 'Sure?'; setTimeout(() => { if (ask === del) { ask = null; del.textContent = '🗑'; } }, 4000); return; }
        ask = null;
        try { await dir.removeEntry(it.name, { recursive: true }); } catch (e) { del.textContent = '⚠'; del.title = 'In use by a running program'; return; }
        show();
        listDrive();
      };
      // a tap selects, a double tap opens (a folder, or the file: .exe runs, .msi installs, .zip unpacks)
      let last = 0;
      row.querySelector('.n').onclick = async () => {
        for (const r of list.children) r.classList.toggle('sel', r === row);
        if (Date.now() - last > 500) { last = Date.now(); return; }
        last = 0;
        if (it.dir) go([...cwd, it.name]);
        else openPath(showPath([...cwd, it.name]), await it.h.getFile());
      };
      return row;
    }));
    if (!items.length) list.innerHTML = '<div class="empty">This folder is empty. Drop files here or use "Copy here…".</div>';
  }
  p.refresh = show;
  go(start ? pathParts([], start) : []);
}

/* ---------- Diagnostics: does the shell work with this browser? ---------- */
function openDiag() {
  const old = progs.find((q) => q.tool === 'diag');
  if (old) { setHidden(old, false); raise(old); old.refresh(); return; }
  const p = toolWindow('Diagnostics', 'win', 'utilities-system-monitor');
  p.tool = 'diag';
  p.con.classList.add('diag');
  const pre = p.con.querySelector('pre');
  const list = document.createElement('div');
  list.className = 'flist';
  pre.replaceWith(list);
  const row = (name) => {
    const r = document.createElement('div');
    r.className = 'chk';
    r.innerHTML = '<b>…</b><div><span></span><small></small></div>';
    r.querySelector('span').textContent = name;
    list.append(r);
    return (ok, text) => { r.querySelector('b').textContent = ok === null ? '–' : ok ? '✅' : '❌'; r.querySelector('small').textContent = text; };
  };
  async function check() {
    list.replaceChildren();
    // drive C: lives in OPFS: write a file, read it back
    const store = row('Drive C: storage (OPFS)');
    try {
      const d = await (await navigator.storage.getDirectory()).getDirectoryHandle('inksoc-diag', { create: true });
      const h = await d.getFileHandle('probe', { create: true });
      const w = await h.createWritable();
      await w.write('inksoc');
      await w.close();
      const back = await (await h.getFile()).text();
      store(back === 'inksoc', back === 'inksoc' ? 'files are written and read back' : 'read back something else');
    } catch (e) { store(false, 'not available: ' + (e.message || e)); }
    // the drive stays after a reload: a mark left by the last check must still be there
    const kept = row('Drive C: kept after a reload');
    try {
      const d = await (await navigator.storage.getDirectory()).getDirectoryHandle('inksoc-diag', { create: true });
      const h = await d.getFileHandle('mark', { create: true });
      const before = await (await h.getFile()).text();
      let loadedAt = 0;
      try { loadedAt = performance.timeOrigin; } catch (e) { /* old browser */ }
      const [at, page] = before.split(' ').map(Number);
      if (!at) kept(null, 'first check: reload the page and run Diagnostics again');
      else if (page === loadedAt) kept(null, 'reload the page and run Diagnostics again to check');
      else kept(true, 'a mark from ' + new Date(at).toLocaleString() + ' was still there');
      if (!at || page !== loadedAt) { const w = await h.createWritable(); await w.write(Date.now() + ' ' + loadedAt); await w.close(); }
    } catch (e) { kept(false, e.message || String(e)); }
    // the browser may clear site data when space runs short, unless storage is persistent
    const room = row('Space for drive C:');
    try {
      const est = await navigator.storage.estimate();
      const persisted = navigator.storage.persisted ? await navigator.storage.persisted() : false;
      room(est.quota - est.usage > 1 << 30, size(est.usage) + ' used of ' + size(est.quota) + (persisted ? ', kept by the browser' : ', the browser may clear it when space runs short'));
    } catch (e) { room(null, 'the browser does not say'); }
    // several cores need shared memory, which needs an isolated page
    const cores = row('Several cores (shared memory)');
    cores(self.crossOriginIsolated, self.crossOriginIsolated ? (navigator.hardwareConcurrency || '?') + ' cores in this device; programs can use up to 4' : 'this address gives 1 core; hub.inkdos.workers.dev gives several');
    // network: programs' sockets go through the wisp tunnel on our Cloudflare worker
    const net = row('Internet for programs (tunnel)');
    const url = /\.workers\.dev$/.test(location.hostname) ? `wss://${location.host}/net/wisp/` : 'wss://hub.inkdos.workers.dev/net/wisp/';
    await new Promise((res) => {
      let ws;
      const end = (ok, text) => { clearTimeout(t); try { ws.close(); } catch (e) { /* closed */ } net(ok, text); res(); };
      const t = setTimeout(() => end(false, 'no answer in 8 s'), 8000);
      try { ws = new WebSocket(url); ws.onopen = () => end(true, 'tunnel open'); ws.onerror = () => end(false, 'the tunnel did not open'); } catch (e) { end(false, e.message); }
    });
    // windows are drawn on a canvas; 3D games need WebGL2 or WebGPU
    const gfx = row('Graphics (2D windows, 3D games)');
    const c = document.createElement('canvas');
    const gl2 = !!c.getContext('webgl2');
    gfx(gl2 || !!navigator.gpu, '2D canvas ok · WebGL2 ' + (gl2 ? 'yes' : 'no') + ' · WebGPU ' + (navigator.gpu ? 'yes' : 'no'));
    // sound starts only after a tap; tapping "Check again" counts
    const snd = row('Sound');
    soundOn();
    snd(sound.ctx ? sound.ctx.state === 'running' : false, sound.ctx ? 'audio ' + sound.ctx.state + (sound.ctx.state === 'running' ? '' : ': tap the page once') : 'no Web Audio in this browser');
    // input reaches the programs
    const input = row('Touch, mouse and keyboard');
    input(seen.taps > 0 || seen.keys > 0, (matchMedia('(pointer: coarse)').matches ? 'touch screen' : 'mouse') + ' · ' + seen.taps + ' tap(s) on the desktop · ' + seen.keys + ' key press(es) so far' + (seen.keys ? '' : ' (press a key to check the keyboard)'));
    // files from outside: the browser hands them over by picking or dropping
    const pick = row('Files from this device');
    pick(!!(window.File && window.DataTransfer), 'pick with "Copy here…" in File Explorer or drop on the desktop; they land in Downloads');
    // the engine itself: a small Windows program runs in a worker and finishes
    const engine = row('Engine runs a Windows program');
    try {
      const t0 = performance.now();
      const done = await new Promise((res) => {
        const w = new Worker('worker.js?v=65');
        const stop = setTimeout(() => { w.terminate(); res({ ok: false, error: 'no answer in 60 s' }); }, 60000);
        w.onmessage = (ev) => { if (ev.data.type === 'done') { clearTimeout(stop); w.terminate(); res(ev.data); } };
        w.onerror = (e) => { clearTimeout(stop); res({ ok: false, error: e.message }); };
        w.postMessage({ type: 'exec', exe: 'stage1.exe', cmd: 'stage1.exe', poolMb: 128, targetMb: 1024, jit: true, cores: 1, strong: true, opt: true });
      });
      engine(!!done.ok, done.ok ? 'stage1.exe finished in ' + Math.round(performance.now() - t0) + ' ms' : 'stopped: ' + (done.error || 'exit code ' + done.exitCode));
    } catch (e) { engine(false, e.message || String(e)); }
    const again = document.createElement('button');
    again.textContent = 'Check again';
    again.style.cssText = 'margin:10px 0;padding:6px 14px;border-radius:8px;border:1px solid #ccc;background:#f4f4f4;font:13px system-ui;cursor:pointer';
    again.onclick = check;
    list.append(again);
  }
  p.refresh = check;
  check();
}

/* ---------- Command Prompt: Wine's own cmd.exe ---------- */
// cmd.exe and the usual command-line tools of C:\Windows\System32 (xcopy, findstr, reg...)
// run in the engine like any program; what is typed goes to cmd's console input.
let system32 = null;
async function openConsole() {
  system32 = system32 || fetch('bin/system32/list.json?v=1').then((r) => r.json()).then((names) => Promise.all(names.map(async (name) => ({ name, data: await fetch('bin/system32/' + name + '?v=1').then((r) => r.arrayBuffer()) }))));
  const files = (await system32).map((f) => ({ name: f.name, data: f.data.slice(0) }));
  run(files, 'cmd.exe', '', 'Command Prompt', '', { typing: true, icon: 'utilities-terminal' });
}
