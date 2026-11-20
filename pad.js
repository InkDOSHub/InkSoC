/* InkSoC game controller: an on-screen gamepad laid out like a modern console controller
   (two sticks, D-pad, A B X Y, L1 R1 L2 R2, Select, Start, Menu), plus any real controller
   the browser sees (Gamepad API: Bluetooth or USB).
   Games get it two ways at once: as an Xbox controller (XInput, engine winapi/xinput.rs)
   and, for games that only read the keyboard, as keys (arrows, Z X A S, Q W E R, Enter, Esc).
   "Edit" moves and resizes the controls; the layout stays in this browser. */
'use strict';

// XInput button bits
const XI = { up: 0x1, down: 0x2, left: 0x4, right: 0x8, start: 0x10, back: 0x20, l3: 0x40, r3: 0x80, l1: 0x100, r1: 0x200, a: 0x1000, b: 0x2000, x: 0x4000, y: 0x8000 };
// the keys a keyboard-only game gets for each control
const KEY = { up: 38, down: 40, left: 37, right: 39, a: 90, b: 88, x: 65, y: 83, l1: 81, r1: 87, l2: 69, r2: 82, start: 13, back: 27 };
// default layout: side (l or r edge), x and y from that edge and the bottom, size in px
const CONTROLS = [
  { id: 'l2', kind: 'trig', label: 'L2', side: 'l', x: 12, y: 300, w: 78, h: 30 },
  { id: 'l1', kind: 'trig', label: 'L1', side: 'l', x: 12, y: 262, w: 78, h: 30 },
  { id: 'r2', kind: 'trig', label: 'R2', side: 'r', x: 12, y: 300, w: 78, h: 30 },
  { id: 'r1', kind: 'trig', label: 'R1', side: 'r', x: 12, y: 262, w: 78, h: 30 },
  { id: 'ls', kind: 'stick', label: '', side: 'l', x: 20, y: 120, w: 120, h: 120 },
  { id: 'dpad', kind: 'dpad', label: '', side: 'l', x: 130, y: 8, w: 112, h: 112 },
  { id: 'y', kind: 'btn', label: 'Y', side: 'r', x: 70, y: 146, w: 54, h: 54, color: '#facc15' },
  { id: 'x', kind: 'btn', label: 'X', side: 'r', x: 124, y: 96, w: 54, h: 54, color: '#60a5fa' },
  { id: 'b', kind: 'btn', label: 'B', side: 'r', x: 16, y: 96, w: 54, h: 54, color: '#f87171' },
  { id: 'a', kind: 'btn', label: 'A', side: 'r', x: 70, y: 46, w: 54, h: 54, color: '#4ade80' },
  { id: 'rs', kind: 'stick', label: '', side: 'r', x: 190, y: 8, w: 96, h: 96 },
  { id: 'back', kind: 'small', label: 'Select', side: 'l', x: 160, y: 300, w: 60, h: 26 },
  { id: 'start', kind: 'small', label: 'Start', side: 'r', x: 160, y: 300, w: 60, h: 26 },
  { id: 'menu', kind: 'small', label: '', sym: 'open-menu', side: 'r', x: 100, y: 300, w: 44, h: 26 },
];

const padEl = $('pad');
const state = { buttons: 0, lt: 0, rt: 0, lx: 0, ly: 0, rx: 0, ry: 0 };
const keysDown = new Set(); // keyboard keys the on-screen controls hold
let layout = {};
try { layout = JSON.parse(localStorage.getItem('inksoc-pad')) || {}; } catch (e) { layout = {}; }
let editing = false, picked = null;

/* ---------- drawing the controls ---------- */
function place(el, c) {
  const l = Object.assign({}, c, layout[c.id] || {});
  const s = l.scale || 1;
  el.style.width = l.w * s + 'px';
  el.style.height = l.h * s + 'px';
  el.style.left = l.side === 'l' ? l.x + 'px' : '';
  el.style.right = l.side === 'r' ? l.x + 'px' : '';
  el.style.bottom = l.y + 'px';
}
function build() {
  padEl.replaceChildren();
  for (const c of CONTROLS) {
    const el = document.createElement('div');
    el.className = 'gp ' + c.kind;
    el.dataset.id = c.id;
    if (c.kind === 'stick') el.innerHTML = '<i></i>';
    else if (c.kind === 'dpad') el.innerHTML = '<svg viewBox="0 0 120 120"><path d="M42 4h36v38h38v36H78v38H42V78H4V42h38z"/><path class="ar" d="M60 14l9 13H51zM60 106l9-13H51zM14 60l13-9v18zM106 60l-13-9v18z"/></svg>';
    else if (c.sym) el.innerHTML = sym(c.sym);
    else el.textContent = c.label;
    if (c.color) el.style.color = c.color;
    place(el, c);
    control(el, c);
    padEl.append(el);
  }
  const bar = document.createElement('div');
  bar.className = 'gpbar';
  bar.innerHTML = '<button data-a="edit">Edit</button><span hidden><input type="range" min="0.6" max="1.8" step="0.05" title="Size of the picked control"> <button data-a="reset">Reset</button></span>';
  padEl.append(bar);
  const [edit, reset] = bar.querySelectorAll('button');
  const size = bar.querySelector('input');
  edit.onclick = () => {
    editing = !editing;
    edit.textContent = editing ? 'Done' : 'Edit';
    bar.querySelector('span').hidden = !editing;
    padEl.classList.toggle('editing', editing);
    if (!editing) { picked = null; save(); }
  };
  size.oninput = () => {
    if (!picked) return;
    layout[picked.id] = Object.assign(layout[picked.id] || {}, { scale: +size.value });
    place(padEl.querySelector('[data-id="' + picked.id + '"]'), picked);
  };
  reset.onclick = () => { layout = {}; save(); build(); editing = false; };
  padEl.pick = (c) => { picked = c; size.value = (layout[c.id] && layout[c.id].scale) || 1; for (const el of padEl.querySelectorAll('.gp')) el.classList.toggle('picked', el.dataset.id === c.id); };
}
function save() { try { localStorage.setItem('inksoc-pad', JSON.stringify(layout)); } catch (e) { /* private window */ } }

/* ---------- touch: each control follows its own finger ---------- */
function control(el, c) {
  el.addEventListener('contextmenu', (e) => e.preventDefault());
  el.addEventListener('click', (e) => e.stopPropagation());
  el.addEventListener('pointerdown', (e) => {
    e.preventDefault();
    e.stopPropagation();
    soundOn();
    el.setPointerCapture(e.pointerId);
    if (editing) return moveControl(el, c, e);
    const r = el.getBoundingClientRect();
    const at = (m) => touch(c, el, (m.clientX - r.left) / r.width * 2 - 1, (m.clientY - r.top) / r.height * 2 - 1, true);
    at(e);
    el.onpointermove = at;
    el.onpointerup = el.onpointercancel = () => { el.onpointermove = null; touch(c, el, 0, 0, false); };
  });
}
// x, y: where the finger is, from -1 to 1 across the control
function touch(c, el, x, y, down) {
  if (c.kind === 'stick') {
    const len = Math.hypot(x, y), k = len > 1 ? 1 / len : 1;
    x *= k; y *= k;
    if (!down) x = y = 0;
    el.firstChild.style.transform = `translate(${x * 30}%, ${y * 30}%)`;
    el.classList.toggle('down', down);
    const ax = Math.round(x * 32767), ay = Math.round(-y * 32767);
    if (c.id === 'ls') {
      state.lx = ax; state.ly = ay;
      // keyboard games: the left stick also presses the arrows
      setKey('left', x < -0.5); setKey('right', x > 0.5); setKey('up', y < -0.5); setKey('down', y > 0.5);
    } else { state.rx = ax; state.ry = ay; }
  } else if (c.kind === 'dpad') {
    const dirs = { up: down && y < -0.33, down: down && y > 0.33, left: down && x < -0.33, right: down && x > 0.33 };
    for (const d in dirs) { setBit(d, dirs[d]); setKey(d, dirs[d]); }
    el.dataset.dir = Object.keys(dirs).filter((d) => dirs[d]).join(' ');
  } else if (c.id === 'menu') {
    if (down) { padEl.hidden = true; sendPad(); } // the menu control hides the pad (the controller button brings it back)
    return;
  } else {
    el.classList.toggle('down', down);
    if (c.id === 'l2') state.lt = down ? 255 : 0;
    else if (c.id === 'r2') state.rt = down ? 255 : 0;
    else setBit(c.id, down);
    setKey(c.id, down);
  }
  sendPad();
}
function setBit(id, on) { if (XI[id]) state.buttons = on ? state.buttons | XI[id] : state.buttons & ~XI[id]; }
// a keyboard key held by a control, sent to the program in front (key down / key up)
function setKey(id, on) {
  const vk = KEY[id];
  if (!vk || on === keysDown.has(id)) return;
  const p = padTarget();
  if (on) keysDown.add(id); else keysDown.delete(id);
  const ch = /^[A-Z]$/.test(String.fromCharCode(vk)) ? String.fromCharCode(vk).toLowerCase().codePointAt(0) : vk === 13 || vk === 27 ? vk : 0;
  if (p) send(p, on ? 4 : 5, 0, 0, vk | (on ? ch << 16 : 0));
}
// editing: drag a control to a new place (kept as distance from the nearer edge and the bottom)
function moveControl(el, c, e) {
  padEl.pick(c);
  const r0 = el.getBoundingClientRect(), dx = e.clientX - r0.left, dy = e.clientY - r0.top;
  el.onpointermove = (m) => {
    const pr = padEl.getBoundingClientRect(), left = m.clientX - dx, bottom = pr.bottom - (m.clientY - dy) - el.offsetHeight;
    const side = left + el.offsetWidth / 2 < innerWidth / 2 ? 'l' : 'r';
    const x = side === 'l' ? left : innerWidth - left - el.offsetWidth;
    layout[c.id] = Object.assign(layout[c.id] || {}, { side, x: Math.max(0, Math.round(x)), y: Math.max(0, Math.round(bottom)) });
    place(el, c);
  };
  el.onpointerup = el.onpointercancel = () => { el.onpointermove = null; };
}

/* ---------- sending the state to the programs ---------- */
const padTarget = () => focus && focus.cv ? focus : progs.filter((p) => p.cv && !p.hidden).sort((a, b) => b.z - a.z)[0];
let real = null; // a real controller's state, read from the Gamepad API
let lastSent = '', lastAt = 0;
function sendPad() {
  const on = !padEl.hidden || !!real;
  const s = real ? merge(state, real) : state;
  const b = s.buttons | (s.lt << 16) | (s.rt << 24);
  const l = (s.lx & 0xFFFF) | ((s.ly & 0xFFFF) << 16), r = (s.rx & 0xFFFF) | ((s.ry & 0xFFFF) << 16);
  const key = [on, b, l, r].join();
  // a program that started later learns the state from the repeat every half second
  if (key === lastSent && performance.now() - lastAt < 500) return;
  lastSent = key;
  lastAt = performance.now();
  for (const p of progs) if (p.w) p.w.postMessage({ type: 'pad', on: on ? 1 : 0, b: b >>> 0, l: l >>> 0, r: r >>> 0 });
}
function merge(a, b) {
  const big = (u, v) => (Math.abs(u) > Math.abs(v) ? u : v);
  return { buttons: a.buttons | b.buttons, lt: Math.max(a.lt, b.lt), rt: Math.max(a.rt, b.rt), lx: big(a.lx, b.lx), ly: big(a.ly, b.ly), rx: big(a.rx, b.rx), ry: big(a.ry, b.ry) };
}
// a real controller (standard mapping): buttons 0-3 A B X Y, 4-5 L1 R1, 6-7 L2 R2, 8-9 Select Start,
// 10-11 stick clicks, 12-15 D-pad; axes 0-1 left stick, 2-3 right stick
const REAL = [XI.a, XI.b, XI.x, XI.y, XI.l1, XI.r1, 0, 0, XI.back, XI.start, XI.l3, XI.r3, XI.up, XI.down, XI.left, XI.right];
function readReal() {
  const g = navigator.getGamepads ? [...navigator.getGamepads()].find((x) => x && x.connected) : null;
  if (!g) { if (real) { real = null; sendPad(); } return; }
  let buttons = 0;
  g.buttons.forEach((bt, i) => { if (bt.pressed && REAL[i]) buttons |= REAL[i]; });
  const ax = (i) => Math.round((Math.abs(g.axes[i] || 0) < 0.12 ? 0 : g.axes[i]) * 32767);
  real = { buttons, lt: Math.round(((g.buttons[6] && g.buttons[6].value) || 0) * 255), rt: Math.round(((g.buttons[7] && g.buttons[7].value) || 0) * 255), lx: ax(0), ly: -ax(1), rx: ax(2), ry: -ax(3) };
  sendPad();
}
setInterval(() => { if (progs.some((p) => p.w)) { readReal(); sendPad(); } }, 50);

build();
