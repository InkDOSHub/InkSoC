/* InkSoC wallpaper: a few light backgrounds drawn by CSS (no image to load or keep in memory),
   or a photo of the person's own, shrunk once to the screen size and kept in the browser's
   storage (OPFS "inksoc-wall.jpg"). The choice stays in this browser. */
'use strict';

const WALLS = {
  blue: '#3a6ea5',
  night: 'linear-gradient(160deg,#0f2027,#203a43 55%,#2c5364)',
  aurora: 'radial-gradient(circle at 25% 30%,rgba(233,84,32,.75),transparent 55%),linear-gradient(135deg,#2c001e,#77216f)',
  ocean: 'linear-gradient(135deg,#0b3d6e,#1565c0 55%,#26a69a)',
  forest: 'linear-gradient(150deg,#1b3326,#2e6b47 60%,#7cb342)',
  plasma: 'linear-gradient(135deg,#1d99f3,#3daee9 30%,#8e44ad 75%,#2c3e50)',
  dusk: 'linear-gradient(170deg,#283c86,#7b4397 55%,#dc2430)',
  graphite: 'linear-gradient(160deg,#232526,#414345)',
};
const WALL_KEY = 'inksoc-wall';
let wallUrl = null;

async function showWall(id) {
  if (wallUrl) { URL.revokeObjectURL(wallUrl); wallUrl = null; }
  if (id === 'photo') {
    try {
      const f = await (await (await navigator.storage.getDirectory()).getFileHandle('inksoc-wall.jpg')).getFile();
      wallUrl = URL.createObjectURL(f);
      desk.style.background = `#222 url(${wallUrl}) center / cover no-repeat`;
    } catch (e) { id = 'blue'; }
  }
  if (id !== 'photo') desk.style.background = WALLS[id] || WALLS.blue;
  for (const s of document.querySelectorAll('#walls [data-wall]')) s.classList.toggle('on', s.dataset.wall === id);
}
function chooseWall(id) {
  try { localStorage.setItem(WALL_KEY, id); } catch (e) { /* private window */ }
  showWall(id);
}
// a photo: shrunk to the screen's size (at most 1920 px) and saved as JPEG
async function wallPhoto(file) {
  const img = await createImageBitmap(file);
  const max = Math.min(1920, Math.round(Math.max(screen.width, screen.height) * Math.min(devicePixelRatio || 1, 2)));
  const k = Math.min(1, max / Math.max(img.width, img.height));
  const c = document.createElement('canvas');
  c.width = Math.round(img.width * k);
  c.height = Math.round(img.height * k);
  c.getContext('2d').drawImage(img, 0, 0, c.width, c.height);
  img.close();
  const blob = await new Promise((ok) => c.toBlob(ok, 'image/jpeg', 0.85));
  const w = await (await (await navigator.storage.getDirectory()).getFileHandle('inksoc-wall.jpg', { create: true })).createWritable();
  await w.write(blob);
  await w.close();
  chooseWall('photo');
}

// the swatches in Settings (the window builds them when it opens)
function wallSwatches(walls) {
  walls.id = 'walls';
  for (const [id, bg] of Object.entries(WALLS)) {
    const b = document.createElement('button');
    b.dataset.wall = id;
    b.title = id[0].toUpperCase() + id.slice(1);
    b.style.background = bg;
    b.onclick = () => chooseWall(id);
    walls.append(b);
  }
  const photo = document.createElement('label');
  photo.dataset.wall = 'photo';
  photo.title = 'A photo of yours';
  photo.textContent = '＋';
  photo.innerHTML += '<input type="file" accept="image/*" hidden>';
  photo.querySelector('input').onchange = (e) => { const f = e.target.files[0]; e.target.value = ''; if (f) wallPhoto(f).catch(() => {}); };
  walls.append(photo);
  let now = 'blue';
  try { now = localStorage.getItem(WALL_KEY) || 'blue'; } catch (e) { /* private window */ }
  for (const s of walls.querySelectorAll('[data-wall]')) s.classList.toggle('on', s.dataset.wall === now);
}
let saved = 'blue';
try { saved = localStorage.getItem(WALL_KEY) || 'blue'; } catch (e) { /* private window */ }
showWall(saved);
