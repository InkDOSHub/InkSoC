/* InkSoC shell tools: File Explorer (drive C:), Command Prompt (Wine's cmd.exe) and
   System Monitor. Both are plain page code over the drive's storage (OPFS folder "inksoc-c/C"):
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
  if (cls === 'win') p.con.style.left = Math.max(0, Math.min(110 + (progs.length % 5) * 28, desk.clientWidth - p.con.offsetWidth)) + 'px'; // the desktop icons stay visible (on a phone it fits the screen)
  raise(p);
  return p;
}

/* ---------- copying, moving and the Recycle Bin (drive C: in OPFS) ---------- */
// a file or a whole folder, copied into toDir as name
async function copyTree(h, toDir, name) {
  if (h.kind === 'file') return copyFile(h, toDir, name);
  const d = await toDir.getDirectoryHandle(name, { create: true });
  for await (const [n, c] of h.entries()) await copyTree(c, d, n);
}
// a name not used yet in dir: "a.txt", then "a (2).txt", "a (3).txt"...
async function freeName(dir, name) {
  const dot = name.lastIndexOf('.'), stem = dot > 0 ? name.slice(0, dot) : name, ext = dot > 0 ? name.slice(dot) : '';
  for (let k = 1; ; k++) {
    const n = k === 1 ? name : `${stem} (${k})${ext}`;
    if (!(await realName(dir, n))) return n;
  }
}
const TRASH = '$Recycle.Bin';
async function toTrash(dir, name, from) {
  const h = await dir.getFileHandle(name).catch(() => dir.getDirectoryHandle(name));
  const bin = await dirAt([TRASH], true);
  const id = Date.now().toString(36) + Math.random().toString(36).slice(2, 6);
  await copyTree(h, bin, id);
  await dir.removeEntry(name, { recursive: true });
  const list = await readTrash();
  list.push({ id, name, from, at: Date.now(), dir: h.kind === 'directory', size: h.kind === 'file' ? (await h.getFile()).size : 0 });
  await writeTrash(list);
}
async function restoreTrash(item) {
  const bin = await dirAt([TRASH], true);
  const h = await bin.getFileHandle(item.id).catch(() => bin.getDirectoryHandle(item.id));
  const to = await dirAt(item.from, true);
  await copyTree(h, to, await freeName(to, item.name));
  await bin.removeEntry(item.id, { recursive: true });
  await writeTrash((await readTrash()).filter((t) => t.id !== item.id));
}
async function deleteTrash(items) {
  const bin = await dirAt([TRASH], true);
  for (const t of items) await bin.removeEntry(t.id, { recursive: true }).catch(() => {});
  const gone = new Set(items.map((t) => t.id));
  await writeTrash((await readTrash()).filter((t) => !gone.has(t.id)));
}
// Copy / Cut in one Explorer window, Paste in any folder
let clip = null; // { parts (the item's path), cut }

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
  head.innerHTML = '<button title="Back">' + sym('go-previous') + '</button><button title="Forward">' + sym('go-next') + '</button><button title="Up">' + sym('go-up') + '</button><input type="text" spellcheck="false" autocomplete="off"><label title="Copy files from this device into this folder">Copy here…<input type="file" multiple hidden></label><button class="tbtn" hidden>Empty Recycle Bin</button>';
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
  const empty = head.querySelector('.tbtn');
  const addr = head.querySelector('input[type=text]');
  const picker = head.querySelector('input[type=file]');
  const copyLabel = head.querySelector('label');
  const places = [['drive-harddisk', 'Local Disk (C:)', ''], ['user-desktop', 'Desktop', 'InkSoC\\Desktop'], ['folder-download', 'Downloads', 'InkSoC\\Downloads'], ['folder-documents', 'Documents', 'InkSoC\\Documents'], ['folder', 'Program Files', 'Program Files'], ['folder', 'Program Files (x86)', 'Program Files (x86)'], ['user-trash', 'Recycle Bin', TRASH]];
  side.replaceChildren(...places.map(([ic, label, path]) => { const d = document.createElement('div'); labelled(d, ic, label); d.dataset.path = path; d.onclick = () => go(pathParts([], 'C:\\' + path)); return d; }));
  let cwd = [], past = [], ahead = [];
  const inTrash = () => cwd.length === 1 && cwd[0] === TRASH;
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
  picker.onchange = async (ev) => { await bring([...ev.target.files]); ev.target.value = ''; };
  p.con.addEventListener('dragover', (e) => { e.preventDefault(); p.con.classList.add('drop'); });
  p.con.addEventListener('dragleave', () => p.con.classList.remove('drop'));
  p.con.addEventListener('drop', (e) => { e.preventDefault(); e.stopPropagation(); p.con.classList.remove('drop'); if (e.dataTransfer.files.length && !inTrash()) bring([...e.dataTransfer.files]); });
  empty.onclick = async () => { await deleteTrash(await readTrash()); show(); };
  const changed = () => { show(); listDrive(); };

  // the empty part of the folder: New folder, Paste, Copy here…, Refresh
  onHold(list, (x, y) => {
    if (inTrash()) return contextMenu(x, y, [['Empty Recycle Bin', 'user-trash', () => empty.click()], ['Refresh', 'folder', show]]);
    contextMenu(x, y, [
      ['New folder', 'folder', newFolder],
      ['Paste', 'text-x-generic', clip && paste],
      ['Copy files from this device…', 'folder-download', () => picker.click()],
      '-',
      ['Refresh', 'folder', show],
    ]);
  });
  async function newFolder() {
    const dir = await dirAt(cwd, true);
    const name = await freeName(dir, 'New folder');
    await dir.getDirectoryHandle(name, { create: true });
    await show();
    const row = [...list.children].find((r) => r.dataset.name === name);
    if (row) row.rename();
  }
  async function paste() {
    const from = await entryAt(clip.parts);
    if (!from) { clip = null; return note('The copied item is no longer there'); }
    const dir = await dirAt(cwd, true);
    // a folder into itself would never end
    if (!clip.cut || clip.parts.join('\\').toLowerCase() !== [...cwd, from.name].join('\\').toLowerCase()) {
      if (from.h.kind === 'directory' && (cwd.join('\\') + '\\').toLowerCase().startsWith(clip.parts.join('\\').toLowerCase() + '\\')) return note('A folder cannot go inside itself');
      await copyTree(from.h, dir, await freeName(dir, from.name));
      if (clip.cut) { await from.dir.removeEntry(from.name, { recursive: true }); clip = null; }
    }
    changed();
  }

  async function show() {
    addr.value = showPath(cwd);
    labelled(p.con.querySelector('.t span'), inTrash() ? 'user-trash' : cwd.length ? 'folder' : 'drive-harddisk', inTrash() ? 'Recycle Bin' : cwd.length ? cwd[cwd.length - 1] : 'Local Disk (C:)');
    back.disabled = !past.length;
    fwd.disabled = !ahead.length;
    up.disabled = !cwd.length;
    empty.hidden = !inTrash();
    copyLabel.hidden = inTrash();
    for (const d of side.children) d.classList.toggle('on', d.dataset.path.toLowerCase() === cwd.join('\\').toLowerCase());
    if (inTrash()) return showTrash();
    const dir = await dirAt(cwd);
    if (!dir) { cwd = []; return show(); }
    const items = (await listDir(dir)).filter((it) => !(cwd.length === 0 && it.name === TRASH));
    list.replaceChildren(...items.map((it) => fileRow(dir, it)));
    if (!items.length) list.innerHTML = '<div class="empty">This folder is empty. Drop files here or use "Copy here…".</div>';
  }
  function fileRow(dir, it) {
    const row = document.createElement('div');
    row.className = 'frow';
    row.dataset.name = it.name;
    const path = [...cwd, it.name];
    const kind = it.dir ? 'folder' : /\.(exe|lnk)$/i.test(it.name) ? 'application-x-executable' : /\.msi$/i.test(it.name) ? 'application-x-msi' : /\.zip$/i.test(it.name) ? 'application-zip' : 'text-x-generic';
    row.innerHTML = '<span class="n"></span><span class="z"></span>';
    labelled(row.querySelector('.n'), kind, it.name);
    row.querySelector('.z').textContent = it.dir ? '' : size(it.size);
    const open = async () => { if (it.dir) go(path); else openPath(showPath(path), await it.h.getFile()); };
    const save = async () => { const a = document.createElement('a'); a.href = URL.createObjectURL(await it.h.getFile()); a.download = it.name; a.click(); setTimeout(() => URL.revokeObjectURL(a.href), 60000); };
    // rename in place: Enter keeps the new name, Esc or leaving the field cancels
    row.rename = () => {
      const n = row.querySelector('.n'), input = document.createElement('input');
      input.value = it.name;
      n.replaceChildren(icon(kind), input);
      input.focus();
      input.select();
      let done = false;
      const end = async (keep) => {
        if (done) return;
        done = true;
        const to = input.value.trim().replace(/[\\/:*?"<>|]/g, '');
        if (keep && to && to !== it.name && !(await realName(dir, to))) { await copyTree(it.h, dir, to); await dir.removeEntry(it.name, { recursive: true }); return changed(); }
        show();
      };
      input.onkeydown = (e) => { e.stopPropagation(); if (e.key === 'Enter') end(true); if (e.key === 'Escape') end(false); };
      input.onblur = () => end(true);
    };
    // a tap selects, a double tap opens (a folder, or the file: .exe runs, .msi installs, .zip unpacks)
    let last = 0;
    row.onclick = () => {
      for (const r of list.children) r.classList.toggle('sel', r === row);
      if (Date.now() - last > 500) { last = Date.now(); return; }
      last = 0;
      open();
    };
    onHold(row, (x, y) => {
      for (const r of list.children) r.classList.toggle('sel', r === row);
      contextMenu(x, y, [
        ['Open', kind, open],
        ...(it.dir ? [] : [['Save a copy on this device', 'folder-download', save]]),
        '-',
        ['Copy', 'text-x-generic', () => { clip = { parts: path, cut: false }; }],
        ['Cut', 'text-x-generic', () => { clip = { parts: path, cut: true }; }],
        ['Rename', 'text-x-generic', () => row.rename()],
        '-',
        ['Move to Recycle Bin', 'user-trash', async () => { try { await toTrash(dir, it.name, cwd); } catch (e) { note('In use by a running program'); } changed(); }],
      ]);
    });
    return row;
  }
  // the Recycle Bin: what was deleted, where from; Restore puts it back
  async function showTrash() {
    const items = (await readTrash()).sort((a, b) => b.at - a.at);
    list.replaceChildren(...items.map((t) => {
      const row = document.createElement('div');
      row.className = 'frow';
      row.innerHTML = '<span class="n"></span><span class="z"></span>';
      labelled(row.querySelector('.n'), t.dir ? 'folder' : 'text-x-generic', t.name);
      row.querySelector('.z').textContent = showPath(t.from) + ' · ' + new Date(t.at).toLocaleDateString();
      row.onclick = () => { for (const r of list.children) r.classList.toggle('sel', r === row); };
      onHold(row, (x, y) => contextMenu(x, y, [
        ['Restore', 'folder', async () => { await restoreTrash(t); changed(); }],
        ['Delete permanently', 'user-trash', async () => { await deleteTrash([t]); show(); }],
      ]));
      return row;
    }));
    if (!items.length) list.innerHTML = '<div class="empty">The Recycle Bin is empty.</div>';
  }
  p.refresh = show;
  go(start ? pathParts([], start) : []);
}

/* ---------- Settings ---------- */
function openSettings() {
  const old = progs.find((q) => q.tool === 'settings');
  if (old) { setHidden(old, false); raise(old); old.refresh(); return; }
  const p = toolWindow('Settings', 'win', 'preferences-system');
  p.tool = 'settings';
  p.con.classList.add('sets');
  const list = document.createElement('div');
  list.className = 'flist';
  p.con.querySelector('pre').replaceWith(list);
  // one row: a label (with an optional line under it) and a control
  const row = (label, hint, ctl) => {
    const r = document.createElement('div');
    r.className = 'row';
    const s = document.createElement('span');
    s.textContent = label;
    if (hint) { const h = document.createElement('small'); h.textContent = hint; s.append(h); }
    r.append(s, ctl);
    list.append(r);
    return r;
  };
  const head = (t) => { const h = document.createElement('h4'); h.textContent = t; list.append(h); };
  const check = (key) => { const c = document.createElement('input'); c.type = 'checkbox'; c.checked = !!prefs[key]; c.onchange = () => setPref(key, c.checked); return c; };
  const choose = (key, options) => {
    const s = document.createElement('select');
    for (const [v, t] of options) { const o = document.createElement('option'); o.value = v; o.textContent = t; s.append(o); }
    s.value = String(prefs[key]);
    s.onchange = () => setPref(key, typeof PREF_DEFAULTS[key] === 'number' ? +s.value : s.value);
    return s;
  };
  const button = (t, act) => { const b = document.createElement('button'); b.textContent = t; b.onclick = act; return b; };
  // a newer build (each drive listing asks for one) stops an older one still waiting
  let gen = 0;
  async function build() {
    const mine = ++gen;
    list.replaceChildren();
    head('Appearance');
    const walls = document.createElement('div');
    wallSwatches(walls);
    row('Wallpaper', 'A background, or the last tile for a photo of yours', document.createElement('i'));
    list.append(walls);
    row('Theme', 'Menus, bar and windows', choose('theme', [['auto', 'Like this device'], ['light', 'Light'], ['dark', 'Dark']]));
    row('Desktop icons', null, check('icons'));
    row('Icon size', null, choose('iconSize', [['small', 'Small'], ['medium', 'Medium'], ['large', 'Large']]));
    row('Program screens', 'Smooth when scaled, or sharp pixels (old games)', choose('smooth', [['true', 'Smooth'], ['false', 'Sharp pixels']])).querySelector('select').onchange = (e) => setPref('smooth', e.target.value === 'true');
    head('Taskbar');
    row('Clock', null, check('clock'));
    row('24-hour clock', null, check('clock24'));
    row('Show seconds', null, check('seconds'));
    head('Sound');
    row('Program sound', null, check('sound'));
    head('Game controller');
    const pad = document.createElement('input');
    pad.type = 'checkbox';
    pad.checked = !$('pad').hidden;
    pad.onchange = () => { $('padon').checked = pad.checked; $('padon').onchange(); };
    row('On-screen controller', 'Two sticks, D-pad, A B X Y, triggers', pad);
    const op = document.createElement('input');
    op.type = 'range'; op.min = '0.3'; op.max = '1'; op.step = '0.05'; op.value = prefs.padOpacity;
    op.oninput = () => setPref('padOpacity', +op.value);
    row('Controller opacity', null, op);
    row('Controller layout', 'Back to where the buttons start', button('Reset', () => { try { localStorage.removeItem('inksoc-pad'); } catch (e) { /* private */ } location.reload(); }));
    head('Advanced');
    const most = self.crossOriginIsolated ? Math.min(navigator.hardwareConcurrency || 4, 8) : 1;
    row('Cores', self.crossOriginIsolated ? 'For each program' : 'Several cores need hub.inkdos.workers.dev', choose('cores', [['0', 'Automatic'], ...Array.from({ length: most }, (_, i) => [String(i + 1), String(i + 1)])]));
    row('JIT compiler', 'Off only to find engine bugs (much slower)', check('jit'));
    // fixed real RAM: the recommended amount is always shown next to it, with the calibration that sets it
    const rec = recommendedMb(), cal = limits.get();
    const calLim = cal.minLimit || (cal.calib && cal.calib.mb) || 0;
    const sizes = [128, 256, 384, 512, 768, 1024, 1536, 2048, 3072];
    if (prefs.ramMb && !sizes.includes(prefs.ramMb)) sizes.push(prefs.ramMb);
    row('Real RAM for each program', 'Recommended: ' + rec + ' MB' + (calLim ? ' (half of the ' + fmt(calLim) + ' MB the tab held in calibration)' : ' (not calibrated yet: a safe amount)') + '. More than recommended can make the tab crash; the rest of a program\'s memory goes to swap on drive C:.',
      choose('ramMb', [['0', 'Recommended (' + rec + ' MB)'], ...sizes.sort((a, b) => a - b).map((v) => [String(v), v + ' MB' + (v > rec ? ' (above recommended)' : '')])]));
    row('Memory calibration', calLim ? 'Last result: the tab held ' + fmt(calLim) + ' MB' : 'Not done yet. The tab may reload while it measures', button(calLim ? 'Calibrate again' : 'Calibrate', () => { tool('diag', 'tests').then(() => { const b = document.getElementById('dg-calib'); if (b) b.click(); }); }));
    row('Last program', null, Object.assign(document.createElement('b'), { textContent: lastSpeed || '–' }));
    head('Storage');
    let persisted = false;
    try { persisted = await navigator.storage.persisted(); } catch (e) { /* not offered */ }
    if (mine !== gen) return;
    row('Drive C:', persisted ? 'Kept by the browser' : 'The browser may clear it when space runs short', Object.assign(document.createElement('b'), { textContent: driveBytes ? fmt(driveBytes / 1048576) + ' MB' : 'empty' }));
    const trash = await readTrash();
    if (mine !== gen) return;
    row('Recycle Bin', trash.length + ' item(s)', button('Empty', async () => { await deleteTrash(await readTrash()); build(); }));
    // erase: a second tap within 4 seconds confirms
    let ask = 0;
    const erase = button('Erase drive C:', async () => {
      if (Date.now() - ask > 4000) { ask = Date.now(); erase.textContent = 'Tap again to erase'; setTimeout(() => { erase.textContent = 'Erase drive C:'; }, 4000); return; }
      try { await (await navigator.storage.getDirectory()).removeEntry('inksoc-c', { recursive: true }); } catch (e) { /* already empty */ }
      try { await (await navigator.storage.getDirectory()).removeEntry('inksoc-trash.json'); } catch (e) { /* none */ }
      await listDrive();
      build();
    });
    row('Erase everything', 'Programs, files and settings of Windows programs', erase);
    head('More');
    row('Settings', 'Back to how they start', button('Reset', () => { resetPrefs(); build(); }));
    row('Tests and benchmarks', 'Memory calibration, speed, disk, graphics', button('System Monitor', () => tool('diag', 'tests')));
  }
  p.refresh = build;
  // the drive's size: counted once, then shown
  listDrive().then(build);
}


/* ---------- System Monitor: programs, resources and tests ---------- */
// Results of the memory tests stay in localStorage 'inksoc-limits' (poolMb() in desktop.js reads them).
const LIMITS = 'inksoc-limits';
const limits = {
  get() { try { return JSON.parse(localStorage.getItem(LIMITS)) || {}; } catch (e) { return {}; } },
  set(v) { try { localStorage.setItem(LIMITS, JSON.stringify(v)); } catch (e) { /* private window */ } },
};
// one engine worker for a test: it answers through onmsg until the test ends it
function testWorker(msg, onmsg, transfer) {
  const w = new Worker('worker.js?v=76');
  w.onmessage = (ev) => onmsg(ev.data, w);
  w.onerror = (e) => onmsg({ type: 'done', ok: false, error: e.message || 'worker error' }, w);
  w.postMessage(msg, transfer || []);
  return w;
}
// a Windows program run for a test: log lines go to out, the promise ends with the 'done' message
function testRun(files, exe, args, out, cores) {
  return new Promise((res) => {
    let stats = null;
    testWorker({ type: 'exec', files, exe, cmd: args ? exe + ' ' + args : exe, poolMb: poolMb(), targetMb: 8192, jit: prefs.jit, cores: cores || 1, strong: true, opt: true },
      (d, w) => {
        if (d.type === 'log') out(d.text.replace(/\r/g, ''));
        else if (d.type === 'stats') stats = d;
        else if (d.type === 'done') { w.terminate(); res(Object.assign(d, { stats })); }
      }, files ? files.map((f) => f.data) : []);
  });
}
const mips = (s) => s ? (s.insns / s.ms / 1000).toFixed(0) + ' MIPS' + (s.jit ? '' : ' (no JIT)') : '';
// the 7-Zip rating on its last "Tot:" line (MIPS of the compress and decompress test together)
const zipRating = (text) => { const m = text.match(/Tot:\s+\d+\s+\d+\s+(\d+)/g); return m ? +m[m.length - 1].split(/\s+/).pop() : 0; };
let zipExe = null;
const get7z = async () => (zipExe = zipExe || await fetch('bin/7za.exe?v=1').then((r) => r.arrayBuffer()));

// System Monitor: Processes (the running programs), Resources, Tests. Only the open tab works,
// and only while the window shows: a hidden monitor costs nothing.
function openDiag(tab) {
  const old = progs.find((q) => q.tool === 'diag');
  if (old) { setHidden(old, false); raise(old); if (tab) old.show(tab); return; }
  const p = toolWindow('System Monitor', 'win', 'utilities-system-monitor');
  p.tool = 'diag';
  p.con.classList.add('sets', 'diag');
  const tabs = document.createElement('div');
  tabs.className = 'mtabs';
  const pages = {};
  const body = document.createElement('div');
  body.className = 'flist';
  p.con.querySelector('pre').replaceWith(tabs, body);
  let shown = '';
  const show = p.show = (name) => {
    shown = name;
    for (const b of tabs.children) b.classList.toggle('on', b.dataset.tab === name);
    for (const k in pages) pages[k].el.hidden = k !== name;
    pages[name].enter();
  };
  for (const [name, label] of [['proc', 'Processes'], ['res', 'Resources'], ['log', 'Log'], ['tests', 'Tests']]) {
    const b = document.createElement('button');
    b.dataset.tab = name;
    b.textContent = label;
    b.onclick = () => show(name);
    tabs.append(b);
    const el = document.createElement('div');
    body.append(el);
    pages[name] = { el, enter: () => {} };
  }
  // refreshes the open tab once a second while the window shows
  const timer = setInterval(() => {
    if (!progs.includes(p)) { clearInterval(timer); return; }
    if (!p.hidden && pages[shown].tick) pages[shown].tick();
  }, 1000);

  /* ---- Processes: like a task manager ---- */
  {
    const pg = pages.proc;
    pg.el.innerHTML = '<table class="ptable"><thead><tr><th>Program</th><th>CPU</th><th>Threads</th><th>RAM</th><th>Swap</th><th></th></tr></thead><tbody></tbody></table><p class="hint"></p>';
    const tb = pg.el.querySelector('tbody'), note = pg.el.querySelector('.hint');
    pg.tick = () => {
      const running = progs.filter((q) => q.w);
      tb.replaceChildren();
      for (const q of running) {
        const s = q.stats || {};
        const tr = document.createElement('tr');
        const name = document.createElement('td');
        name.append(icon(q.icon || 'application-x-executable'), document.createTextNode(q.label || 'Program'));
        const cells = [q.mips ? fmt(q.mips) + ' MIPS' : '…', (s.threads || 1) + (s.cores > 1 ? ' · ' + s.cores + ' cores' : ''),
          s.frames ? fmt(s.resident * 4 / 1024) + ' / ' + fmt(s.frames * 4 / 1024) + ' MB' : '…', s.slots ? fmt(s.slots * 4 / 1024) + ' MB' : '0 MB'];
        tr.append(name, ...cells.map((t) => { const td = document.createElement('td'); td.textContent = t; return td; }));
        const end = document.createElement('td');
        const b = document.createElement('button');
        b.className = 'endt';
        b.title = 'End task';
        b.innerHTML = sym('process-stop');
        b.onclick = () => { close(q); pg.tick(); };
        end.append(b);
        tr.append(end);
        tb.append(tr);
      }
      note.textContent = running.length ? running.length + ' program(s) running. CPU is how many million x86 instructions a second the engine runs for it.' : 'No programs running. Open one from Start or File Explorer.';
    };
    pg.enter = pg.tick;
  }

  /* ---- Resources: what this device gives the programs ---- */
  {
    const pg = pages.res;
    const bar = (label, used, total, text) => {
      const r = document.createElement('div');
      r.className = 'meter';
      r.innerHTML = '<div class="ml"><span></span><small></small></div><div class="mb"><i></i></div>';
      r.querySelector('span').textContent = label;
      r.querySelector('small').textContent = text;
      r.querySelector('i').style.width = Math.min(100, total ? used / total * 100 : 0).toFixed(1) + '%';
      return r;
    };
    let quota = null;
    pg.tick = () => {
      const running = progs.filter((q) => q.w);
      const ram = running.reduce((a, q) => a + ((q.stats && q.stats.resident) || 0) * 4 / 1024, 0);
      const cpu = running.reduce((a, q) => a + (q.mips || 0), 0);
      const n = coresFor(), dev = navigator.hardwareConcurrency || 1;
      pg.el.replaceChildren(
        bar('Processor', n, dev, dev + ' cores in this device · programs get ' + n + (cpu ? ' · now ' + fmt(cpu) + ' MIPS' : '') + ' · JIT ' + (prefs.jit ? 'on' : 'off')),
        bar('Memory', ram, poolMb() * Math.max(1, running.length), fmt(ram) + ' MB in use · each program gets ' + poolMb() + ' MB of real RAM' + (limits.get().minLimit ? '' : ' (not calibrated: see Tests)')),
        bar('Drive C:', driveBytes, quota ? quota.quota : 0, fmt(driveBytes / 1048576) + ' MB' + (quota ? ' · ' + fmt((quota.quota - quota.usage) / 1048576) + ' MB free in this browser' : '')),
        bar('Graphics', 0, 0, 'WebGL2 ' + (document.createElement('canvas').getContext('webgl2') ? 'yes' : 'no') + ' · WebGPU ' + (navigator.gpu ? 'yes' : 'no') + (limits.get().tex ? ' · ' + fmt(limits.get().tex.mb) + ' MB of textures measured' : '')),
        bar('Network', 0, 0, 'programs reach the internet through the tunnel on hub.inkdos.workers.dev'),
      );
    };
    pg.enter = () => {
      pg.tick();
      if (navigator.storage && navigator.storage.estimate) navigator.storage.estimate().then((e) => { quota = e; pg.tick(); }, () => {});
    };
  }

  /* ---- Log: program errors, page errors and crashes of the page (errlog in desktop.js, kept across crashes) ---- */
  {
    const pg = pages.log;
    pg.el.innerHTML = '<p class="hint">Errors of the programs and of the page. If the page crashes, the next start adds what was running. Kept in this browser (last ' + LOG_MAX + ' lines).</p><div class="logbar"><button class="again">Copy</button> <button class="again">Clear</button></div><pre class="log"></pre>';
    const [copy, clear] = pg.el.querySelectorAll('button'), out = pg.el.querySelector('pre');
    const text = () => errlog.map((e) => new Date(e.at).toLocaleString() + '  [' + e.src + ']  ' + e.text).join('\n');
    let shownCount = -1;
    pg.tick = () => {
      if (shownCount === errlog.length && errlog.length < LOG_MAX) return;
      shownCount = errlog.length;
      out.replaceChildren();
      if (!errlog.length) { out.textContent = 'No errors.'; return; }
      for (const e of errlog) {
        const l = document.createElement('div');
        if (e.src === 'crash') l.className = 'bad';
        l.textContent = new Date(e.at).toLocaleTimeString() + '  [' + e.src + ']  ' + e.text;
        out.append(l);
      }
      out.scrollTop = out.scrollHeight;
    };
    pg.enter = () => { shownCount = -1; pg.tick(); };
    copy.onclick = async () => { try { await navigator.clipboard.writeText(text() || 'No errors.'); copy.lastChild.textContent = 'Copied'; } catch (e) { copy.lastChild.textContent = 'Copy failed'; } setTimeout(() => { copy.lastChild.textContent = 'Copy'; }, 2000); };
    clear.onclick = () => { clearLog(); pg.enter(); };
  }

  /* ---- Tests, most useful first ---- */
  const pg = pages.tests;
  let list = pg.el;
  const head = (t, hint) => {
    const h = document.createElement('h4'); h.textContent = t; list.append(h);
    if (hint) { const s = document.createElement('p'); s.className = 'hint'; s.textContent = hint; list.append(s); }
  };
  // a test: name, what it does, a Run button and a box for its results (id dg-<key>, used by tests/arm/bench.js)
  const test = (key, label, hint, act) => {
    const r = document.createElement('div');
    r.className = 'row';
    const s = document.createElement('span');
    s.textContent = label;
    if (hint) { const h = document.createElement('small'); h.textContent = hint; s.append(h); }
    const b = document.createElement('button');
    b.id = 'dg-' + key;
    b.innerHTML = sym('media-playback-start') + 'Run';
    const out = document.createElement('pre');
    out.className = 'res';
    out.id = 'dg-' + key + '-out';
    out.hidden = true;
    const say = (t) => { out.hidden = false; out.textContent += t; out.scrollTop = out.scrollHeight; };
    b.onclick = async () => {
      b.disabled = true;
      out.textContent = '';
      try { await act(say, out); } catch (e) { say('\nError: ' + (e.message || e)); }
      b.disabled = false;
    };
    r.append(s, b);
    list.append(r, out);
  };
  const state = (ok) => sym(ok === null ? 'dialog-warning' : ok ? 'object-select' : 'dialog-error', ok === null ? 'warn' : ok ? 'good' : 'bad');

  // 1. does the shell work here
  head('Check', 'What this browser gives the shell.');
  const checks = document.createElement('div');
  list.append(checks);
  const row = (name) => {
    const r = document.createElement('div');
    r.className = 'chk';
    r.innerHTML = '<b>' + sym('view-refresh', 'wait') + '</b><div><span></span><small></small></div>';
    r.querySelector('span').textContent = name;
    checks.append(r);
    return (ok, text) => { r.querySelector('b').innerHTML = state(ok); r.querySelector('small').textContent = text; };
  };
  let checked = false;
  async function check() {
    checked = true;
    checks.replaceChildren();
    const store = row('Drive C: storage');
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
      if (!at) kept(null, 'first check: reload the page and check again');
      else if (page === loadedAt) kept(null, 'reload the page and check again to know');
      else kept(true, 'a mark from ' + new Date(at).toLocaleString() + ' was still there');
      if (!at || page !== loadedAt) { const w = await h.createWritable(); await w.write(Date.now() + ' ' + loadedAt); await w.close(); }
    } catch (e) { kept(false, e.message || String(e)); }
    const mem = row('Memory for programs');
    const lim = limits.get().minLimit || 0;
    mem(lim ? true : null, lim ? 'tab limit ' + fmt(lim) + ' MB, programs get ' + poolMb() + ' MB of real RAM' : 'not calibrated: programs get a safe ' + poolMb() + ' MB. Run "Calibrate memory" below');
    const cores = row('Several cores');
    cores(self.crossOriginIsolated, self.crossOriginIsolated ? (navigator.hardwareConcurrency || '?') + ' cores in this device; programs get ' + coresFor() : 'this address gives 1 core; hub.inkdos.workers.dev gives several');
    const engine = row('Engine runs a Windows program');
    const t0 = performance.now();
    const done = await testRun(null, 'stage1.exe', '', () => {}, 1);
    engine(!!done.ok, done.ok ? 'stage1.exe finished in ' + Math.round(performance.now() - t0) + ' ms · ' + mips(done.stats) : 'stopped: ' + (done.error || 'exit code ' + done.exitCode));
    const room = row('Space for drive C:');
    try {
      const est = await navigator.storage.estimate();
      const persisted = navigator.storage.persisted ? await navigator.storage.persisted() : false;
      room(est.quota - est.usage > 1 << 30, size(est.usage) + ' used of ' + size(est.quota) + (persisted ? ', kept by the browser' : ', the browser may clear it when space runs short'));
    } catch (e) { room(null, 'the browser does not say'); }
    const net = row('Internet for programs');
    const url = /\.workers\.dev$/.test(location.hostname) ? `wss://${location.host}/net/wisp/` : 'wss://hub.inkdos.workers.dev/net/wisp/';
    await new Promise((res) => {
      let ws;
      const end = (ok, text) => { clearTimeout(t); try { ws.close(); } catch (e) { /* closed */ } net(ok, text); res(); };
      const t = setTimeout(() => end(false, 'no answer in 8 s'), 8000);
      try { ws = new WebSocket(url); ws.onopen = () => end(true, 'tunnel open'); ws.onerror = () => end(false, 'the tunnel did not open'); } catch (e) { end(false, e.message); }
    });
    const gfx = row('Graphics');
    const gl2 = !!document.createElement('canvas').getContext('webgl2');
    gfx(gl2 || !!navigator.gpu, '2D windows ok · WebGL2 ' + (gl2 ? 'yes' : 'no') + ' · WebGPU ' + (navigator.gpu ? 'yes' : 'no'));
    const snd = row('Sound');
    soundOn();
    snd(sound.ctx ? sound.ctx.state === 'running' : false, sound.ctx ? 'audio ' + sound.ctx.state + (sound.ctx.state === 'running' ? '' : ': tap the page once') : 'no Web Audio in this browser');
    const input = row('Touch, mouse and keyboard');
    input(seen.taps > 0 || seen.keys > 0, (matchMedia('(pointer: coarse)').matches ? 'touch screen' : 'mouse') + ' · ' + seen.taps + ' tap(s) · ' + seen.keys + ' key press(es)');
    const simd = WebAssembly.validate(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 123, 3, 2, 1, 0, 10, 10, 1, 8, 0, 65, 0, 253, 15, 253, 98, 11]));
    row('Wasm SIMD')(simd, simd ? 'yes' : 'no: programs run slower');
  }
  const again = document.createElement('button');
  again.className = 'again';
  again.innerHTML = sym('view-refresh') + 'Check again';
  again.onclick = check;
  list.append(again);
  pg.enter = () => { if (!checked) check(); };

  // 2. memory: sets how much real RAM every program gets
  head('Memory', 'Sets how much real RAM programs get; the rest of their memory goes to swap on drive C:. The tab may reload: that is the measurement.');
  const memOut = document.createElement('p');
  memOut.className = 'hint';
  const showLimits = () => {
    const s = limits.get(), lines = [];
    if (s.calib && s.calib.mb && (s.calib.running || s.calib.note)) {
      const m = Math.min(s.minLimit || Infinity, s.calib.mb);
      if (m !== s.minLimit) { s.minLimit = m; limits.set(s); }
    }
    const desc = (v, label) => {
      if (!v) return;
      if (v.running) lines.push(label + ': the tab reloaded after ' + fmt(v.mb) + ' MB, so that is the limit.');
      else lines.push(label + ': reached ' + fmt(v.mb) + ' MB without reloading' + (v.note ? ' (' + v.note + ')' : '') + '.');
    };
    desc(s.calib, 'Program memory');
    desc(s.tex, 'GPU textures');
    lines.push('Programs get ' + poolMb() + ' MB of real RAM' + (s.minLimit ? ' (half of the lowest limit seen).' : ' (safe default until calibrated).'));
    memOut.textContent = lines.join(' ');
  };
  test('calib', 'Calibrate memory', 'Grows memory in 128 MB steps until the browser says no', (say) => new Promise((res) => {
    limits.set(Object.assign(limits.get(), { calib: { mb: 0, running: true } }));
    testWorker({ type: 'calibrate' }, (d, w) => {
      if (d.type !== 'calib') { say('Error: ' + d.error); w.terminate(); return res(); }
      const s = limits.get();
      s.calib = { mb: d.mb, running: !(d.refused || d.max), note: d.refused ? 'the browser refused to grow further' : d.max ? '4 GB, the wasm maximum' : '' };
      limits.set(s);
      if (d.refused || d.max) { w.terminate(); showLimits(); say('Done: ' + fmt(d.mb) + ' MB.'); res(); } else setTimeout(() => w.postMessage({ type: 'calib-next' }), 50);
    });
  }));
  list.append(memOut);
  showLimits();

  // 3. disk: programs install and swap there
  head('Disk', 'Drive C: and swap live in this browser\'s storage.');
  test('disk', 'Disk speed', 'Writes and reads a 128 MB file, then 4 KB pages at random places (what swap does)', async (say) => {
    const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle('inksoc-diag', { create: true });
    const h = await dir.getFileHandle('speed', { create: true });
    const chunk = new Uint8Array(4 << 20);
    crypto.getRandomValues(chunk.subarray(0, 65536));
    let t = performance.now();
    const w = await h.createWritable();
    for (let i = 0; i < 32; i++) await w.write(chunk);
    await w.close();
    const wr = 128 / ((performance.now() - t) / 1000);
    t = performance.now();
    const back = await (await h.getFile()).arrayBuffer();
    const rd = (back.byteLength >> 20) / ((performance.now() - t) / 1000);
    say('Write ' + fmt(wr) + ' MB/s · read ' + fmt(rd) + ' MB/s\n');
    await dir.removeEntry('speed');
    await new Promise((res) => testWorker({ type: 'opfs' }, (d, wk) => {
      say(d.type === 'opfs' ? '4 KB pages: read ' + d.readUs.toFixed(1) + ' µs, write ' + d.writeUs.toFixed(1) + ' µs (' + fmt(1e6 / d.readUs) + ' pages/s)' : 'Error: ' + d.error);
      wk.terminate(); res();
    }));
  });

  // 4. speed of the processor through the engine
  head('Performance', 'Cores and JIT come from Settings › Performance.');
  test('7z', '7-Zip benchmark', 'Compress and decompress in memory, the classic CPU test (rating in MIPS)', async (say) => {
    const n = coresFor();
    say('7-Zip 23.01, ' + n + ' core(s)…\n');
    let text = '';
    const d = await testRun([{ name: '7za.exe', data: (await get7z()).slice(0) }], '7za.exe', 'b -md18 -mmt' + n, (t) => { text += t; say(t); }, n);
    say(d.ok ? '\nFinished. Rating: ' + fmt(zipRating(text)) + ' MIPS · engine ' + mips(d.stats) : '\nStopped: ' + (d.error || 'exit code ' + d.exitCode));
  });
  test('3d', '3D', 'Time per frame for 500 to 8000 separate draws, what a 3D game asks for', async (say) => {
    if (navigator.gpu) { try { await drawWebGPU(say); return; } catch (e) { say('WebGPU error: ' + e.message + '\n'); } }
    drawWebGL(say);
  });

  // 5. long runs: does the device slow down when hot
  head('Stress', 'Long runs: a falling number means the device slows down when hot.');
  test('stress', 'Processor (5 min)', 'The 7-Zip benchmark again and again', async (say) => {
    const n = coresFor(), t0 = performance.now(), got = [];
    while (performance.now() - t0 < 300000) {
      let text = '';
      const d = await testRun([{ name: '7za.exe', data: (await get7z()).slice(0) }], '7za.exe', 'b 1 -md18 -mmt' + n, (t) => { text += t; }, n);
      if (!d.ok) { say('Stopped: ' + (d.error || 'exit code ' + d.exitCode)); return; }
      got.push(zipRating(text));
      say(Math.round((performance.now() - t0) / 1000) + ' s · ' + fmt(got[got.length - 1]) + ' MIPS\n');
    }
    const first = got[0] || 1, last = got[got.length - 1];
    say('\nStart ' + fmt(first) + ', end ' + fmt(last) + ' MIPS: ' + (last >= first * 0.9 ? 'steady' : 'dropped ' + Math.round(100 - last / first * 100) + '% (heat)'));
  });
  test('gpu', 'Graphics (30 s)', 'Streams far more textures than the GPU holds, from the disk, at 60 frames per second', (say, out) => streamTest(say, out, limits));

  // 6. for engine work
  const adv = document.createElement('details');
  adv.innerHTML = '<summary>Advanced</summary>';
  list.append(adv);
  list = adv;
  test('engine', 'Engine self-test', 'Integers, floating point, DLLs, msvcrt and threads, each checked against the right answer', async (say) => {
    for (const exe of ['stage1.exe', 'stage2.exe', 'stage4.exe']) {
      const t0 = performance.now();
      const d = await testRun(null, exe, '', () => {}, exe === 'stage4.exe' ? coresFor() : 1);
      say((d.ok ? 'passed  ' : 'FAILED  ') + exe + ' · ' + Math.round(performance.now() - t0) + ' ms · ' + mips(d.stats) + (d.ok ? '' : ' · ' + (d.error || 'exit code ' + d.exitCode)) + '\n');
    }
  });
  test('swap', 'Swap', 'A program uses 512 MB with only 128 MB of real RAM: the rest goes to drive C: and back, every page checked', (say) => new Promise((res) => {
    let s = null;
    testWorker({ type: 'proof', poolMb: 128, targetMb: 8192, testMb: 512 }, (d, w) => {
      if (d.type === 'log') say(d.text);
      else if (d.type === 'stats') s = d;
      else if (d.type === 'done') {
        say((d.ok ? '\nPassed' : '\nFailed: ' + (d.error || 'exit code ' + d.exitCode)) + (s ? ' · ' + fmt(s.swapOut) + ' pages out, ' + fmt(s.swapIn) + ' in, ' + fmt(s.swapMs) + ' ms of ' + fmt(s.ms) + ' ms in swap' : ''));
        w.terminate(); res();
      }
    });
  }));
  test('tex', 'GPU memory', 'Fills the GPU with textures in 128 MB steps (the tab may reload)', async (say) => {
    const gl = document.createElement('canvas').getContext('webgl2');
    if (!gl) { say('No WebGL2.'); return; }
    const sz = 2048, data = new Uint8Array(sz * sz * 4);
    crypto.getRandomValues(data.subarray(0, 65536));
    for (let i = 65536; i < data.length; i += 65536) data.copyWithin(i, 0, 65536);
    const keep = [];
    let mb = 0;
    limits.set(Object.assign(limits.get(), { tex: { mb: 0, running: true } }));
    while (mb < 6144) {
      for (let k = 0; k < 8; k++) { const t = gl.createTexture(); gl.bindTexture(gl.TEXTURE_2D, t); gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, sz, sz, 0, gl.RGBA, gl.UNSIGNED_BYTE, data); keep.push(t); }
      gl.finish();
      const err = gl.getError();
      if (err || gl.isContextLost()) { const s = limits.get(); s.tex = { mb, running: false, note: err ? 'out of memory error' : 'context lost' }; limits.set(s); break; }
      mb += 128;
      const s = limits.get(); s.tex = { mb, running: true }; limits.set(s);
      if (!(mb % 1024)) say(fmt(mb) + ' MB… ');
      await new Promise((r) => setTimeout(r, 50));
    }
    const s = limits.get(); if (s.tex.running) { s.tex = { mb, running: false, note: 'test maximum' }; limits.set(s); }
    keep.forEach((t) => gl.deleteTexture(t));
    showLimits();
    say('\nDone: ' + fmt(mb) + ' MB.');
  });
  const clear = document.createElement('button');
  clear.className = 'again';
  clear.textContent = 'Forget memory results';
  clear.onclick = () => { limits.set({}); showLimits(); };
  adv.append(clear);
  const feat = document.createElement('pre');
  feat.className = 'res';
  feat.textContent = navigator.userAgent;
  adv.append(feat);

  p.refresh = () => {};
  show(tab || 'proc');
}

// draws per frame, WebGPU
async function drawWebGPU(say) {
  const adapter = await navigator.gpu.requestAdapter();
  const dev = await adapter.requestDevice();
  const ctx = new OffscreenCanvas(256, 256).getContext('webgpu');
  const format = navigator.gpu.getPreferredCanvasFormat();
  ctx.configure({ device: dev, format });
  const mod = dev.createShaderModule({ code: `
    struct U { v: vec4f };
    @group(0) @binding(0) var<uniform> u: U;
    @vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
      let p = array(vec2f(0, .1), vec2f(-.1, -.1), vec2f(.1, -.1));
      return vec4f(p[i] + u.v.xy, 0, 1);
    }
    @fragment fn fs() -> @location(0) vec4f { return vec4f(1, .5, 0, 1); }` });
  const bgl = dev.createBindGroupLayout({ entries: [{ binding: 0, visibility: GPUShaderStage.VERTEX, buffer: { type: 'uniform', hasDynamicOffset: true } }] });
  const pipe = dev.createRenderPipeline({ layout: dev.createPipelineLayout({ bindGroupLayouts: [bgl] }), vertex: { module: mod, entryPoint: 'vs' }, fragment: { module: mod, entryPoint: 'fs', targets: [{ format }] } });
  const N = 8000, ubuf = dev.createBuffer({ size: N * 256, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
  const data = new Float32Array(N * 64);
  for (let i = 0; i < N; i++) { data[i * 64] = Math.random() * 1.8 - .9; data[i * 64 + 1] = Math.random() * 1.8 - .9; }
  dev.queue.writeBuffer(ubuf, 0, data);
  const bg = dev.createBindGroup({ layout: bgl, entries: [{ binding: 0, resource: { buffer: ubuf, size: 16 } }] });
  const frame = (n) => {
    const enc = dev.createCommandEncoder();
    const pass = enc.beginRenderPass({ colorAttachments: [{ view: ctx.getCurrentTexture().createView(), loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 1] }] });
    pass.setPipeline(pipe);
    for (let i = 0; i < n; i++) { pass.setBindGroup(0, bg, [i * 256]); pass.draw(3); }
    pass.end();
    dev.queue.submit([enc.finish()]);
    return dev.queue.onSubmittedWorkDone();
  };
  say('WebGPU\n');
  for (const n of [500, 1000, 2000, 4000, 8000]) {
    await frame(n);
    const t = performance.now();
    for (let k = 0; k < 20; k++) await frame(n);
    say(n + ' draws: ' + ((performance.now() - t) / 20).toFixed(2) + ' ms per frame\n');
  }
  dev.destroy();
}
// draws per frame, WebGL2
function drawWebGL(say) {
  const cv = document.createElement('canvas'); cv.width = cv.height = 256;
  const gl = cv.getContext('webgl2');
  if (!gl) { say('No WebGL2.'); return; }
  const sh = (t, s) => { const o = gl.createShader(t); gl.shaderSource(o, s); gl.compileShader(o); return o; };
  const pr = gl.createProgram();
  gl.attachShader(pr, sh(gl.VERTEX_SHADER, '#version 300 es\nuniform vec2 o; void main(){ vec2 v[3]=vec2[](vec2(0,.1),vec2(-.1,-.1),vec2(.1,-.1)); gl_Position=vec4(v[gl_VertexID]+o,0,1);}'));
  gl.attachShader(pr, sh(gl.FRAGMENT_SHADER, '#version 300 es\nprecision mediump float; out vec4 c; void main(){ c=vec4(1,.5,0,1);}'));
  gl.linkProgram(pr); gl.useProgram(pr);
  const loc = gl.getUniformLocation(pr, 'o'), px = new Uint8Array(4);
  say('WebGL2\n');
  for (const n of [500, 1000, 2000, 4000, 8000]) {
    const t = performance.now();
    for (let k = 0; k < 20; k++) {
      gl.clear(gl.COLOR_BUFFER_BIT);
      for (let i = 0; i < n; i++) { gl.uniform2f(loc, Math.random() * 1.8 - .9, Math.random() * 1.8 - .9); gl.drawArrays(gl.TRIANGLES, 0, 3); }
      gl.readPixels(0, 0, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
    }
    say(n + ' draws: ' + ((performance.now() - t) / 20).toFixed(2) + ' ms per frame\n');
  }
}

// GPU stress: a fixed pool of GPU textures, 2 GB of textures in a disk file, a "camera" that keeps
// needing new ones: the least used leave the GPU and come back from the disk (as a game's would)
async function streamTest(say, out, limits) {
  const lim = limits.get().minLimit || 1024;
  if (!navigator.gpu) { say('This test needs WebGPU.'); return; }
  const total = Math.floor(lim * 0.7), gpu = Math.max(64, Math.floor(total * 0.35 / 4) * 4);
  const texMb = 2048, SIZE = 1024, BYTES = SIZE * SIZE * 4, count = texMb / 4, SHOW = 48;
  say('GPU pool ' + gpu + ' MB (' + gpu / 4 + ' textures), ' + texMb + ' MB of textures on the disk\n');
  const io = new Worker('worker.js?v=76');
  await new Promise((r) => { io.onmessage = r; io.postMessage({ type: 'tex-init', count, size: BYTES }); });
  const dev = await (await navigator.gpu.requestAdapter()).requestDevice();
  let gpuErr = '';
  dev.onuncapturederror = (e) => { gpuErr = e.error.message; };
  const cv = document.createElement('canvas');
  cv.width = 512; cv.height = 288;
  cv.className = 'gpuview';
  out.after(cv);
  const off = new OffscreenCanvas(cv.width, cv.height);
  const ctx = off.getContext('webgpu'), show = cv.getContext('bitmaprenderer');
  const format = navigator.gpu.getPreferredCanvasFormat();
  ctx.configure({ device: dev, format });
  const mod = dev.createShaderModule({ code: `
    @group(0) @binding(0) var t: texture_2d<f32>;
    @group(0) @binding(1) var s: sampler;
    struct O { @builtin(position) p: vec4f, @location(0) uv: vec2f };
    @vertex fn vs(@builtin(vertex_index) v: u32, @builtin(instance_index) i: u32) -> O {
      let q = array(vec2f(0, 0), vec2f(1, 0), vec2f(0, 1), vec2f(0, 1), vec2f(1, 0), vec2f(1, 1));
      let uv = q[v]; let c = f32(i % 8u); let r = f32(i / 8u);
      var o: O; o.p = vec4f((c + uv.x) / 4.0 - 1.0, 1.0 - (r + uv.y) / 3.0, 0, 1); o.uv = uv; return o;
    }
    @fragment fn fs(o: O) -> @location(0) vec4f { return textureSample(t, s, o.uv); }` });
  const pipe = dev.createRenderPipeline({ layout: 'auto', vertex: { module: mod, entryPoint: 'vs' }, fragment: { module: mod, entryPoint: 'fs', targets: [{ format }] } });
  const sampler = dev.createSampler({ magFilter: 'linear', minFilter: 'linear' });
  const bgFor = (tx) => dev.createBindGroup({ layout: pipe.getBindGroupLayout(0), entries: [{ binding: 0, resource: tx.createView() }, { binding: 1, resource: sampler }] });
  const ph = dev.createTexture({ size: [1, 1], format: 'rgba8unorm', usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST });
  dev.queue.writeTexture({ texture: ph }, new Uint8Array([40, 40, 40, 255]), { bytesPerRow: 4 }, [1, 1]);
  const phBG = bgFor(ph);
  const nSlots = gpu / 4, slots = [];
  for (let i = 0; i < nSlots; i++) {
    const tx = dev.createTexture({ size: [SIZE, SIZE], format: 'rgba8unorm', usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST });
    slots.push({ tx, bg: bgFor(tx), id: -1, used: -1 });
  }
  const where = new Map(), pending = new Set();
  let frame = 0, loads = 0, waits = 0, pos = 0;
  io.onmessage = (e) => {
    const d = e.data; if (d.type !== 'tex') return;
    pending.delete(d.id);
    let best = -1;
    for (let i = 0; i < nSlots; i++) {
      const sl = slots[i];
      if (sl.used >= frame) continue;
      if (best < 0 || sl.used < slots[best].used) best = i;
      if (sl.id < 0) { best = i; break; }
    }
    if (best < 0) return;
    const sl = slots[best];
    if (sl.id >= 0) where.delete(sl.id);
    dev.queue.writeTexture({ texture: sl.tx }, d.data, { bytesPerRow: SIZE * 4 }, [SIZE, SIZE]);
    loads++;
    sl.id = d.id; sl.used = frame; where.set(d.id, best);
  };
  const t0 = performance.now();
  let last = t0, worst = 0, mark = 0;
  await new Promise((done) => {
    const tick = (now) => {
      const dt = now - last; last = now; if (frame > 5) worst = Math.max(worst, dt);
      frame++;
      pos += 0.5;
      const base = Math.floor(pos);
      const enc = dev.createCommandEncoder();
      const pass = enc.beginRenderPass({ colorAttachments: [{ view: ctx.getCurrentTexture().createView(), loadOp: 'clear', storeOp: 'store', clearValue: [0, 0, 0, 1] }] });
      pass.setPipeline(pipe);
      for (let i = 0; i < SHOW + 8; i++) {
        const id = (base + i) % count, k = where.get(id);
        if (i < SHOW) {
          if (k !== undefined) { slots[k].used = frame; pass.setBindGroup(0, slots[k].bg); } else { waits++; pass.setBindGroup(0, phBG); }
          pass.draw(6, 1, 0, i);
        }
        if (k === undefined && !pending.has(id) && pending.size < 6) { pending.add(id); io.postMessage({ type: 'tex-get', id }); }
      }
      pass.end();
      dev.queue.submit([enc.finish()]);
      show.transferFromImageBitmap(off.transferToImageBitmap());
      const sec = (now - t0) / 1000;
      if (sec - mark >= 5) { mark = sec; say(Math.floor(sec) + ' s · ' + (frame / sec).toFixed(0) + ' fps\n'); }
      if (sec < 30) requestAnimationFrame(tick); else done();
    };
    requestAnimationFrame(tick);
  });
  const secs = (performance.now() - t0) / 1000;
  say('\n' + (frame / secs).toFixed(0) + ' fps average, worst frame ' + worst.toFixed(0) + ' ms\n');
  say('Streamed ' + fmt(loads * 4) + ' MB from the disk (' + (loads * 4 / secs).toFixed(0) + ' MB/s); ' + (waits / (frame * SHOW) * 100).toFixed(1) + '% of draws waited for a texture' + (gpuErr ? '\nGPU error: ' + gpuErr : ''));
  io.postMessage({ type: 'tex-done' });
  setTimeout(() => io.terminate(), 500);
  slots.forEach((sl) => sl.tx.destroy());
  dev.destroy();
  cv.remove();
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
