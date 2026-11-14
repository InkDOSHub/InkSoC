/* InkSoC preferences: what the Settings window changes, kept in this browser (localStorage).
   Loaded first, so the theme, clock and desktop look right from the first frame. */
'use strict';

const PREF_KEY = 'inksoc-prefs';
const PREF_DEFAULTS = {
  theme: 'auto', // auto, light, dark
  icons: true, // desktop icons shown
  iconSize: 'medium', // small, medium, large
  clock: true,
  clock24: false,
  seconds: false,
  sound: true,
  padOpacity: 1,
  cores: 0, // 0 = automatic
  jit: true,
  smooth: true, // program screens scaled smoothly (off: sharp pixels)
};
const prefs = Object.assign({}, PREF_DEFAULTS);
try { Object.assign(prefs, JSON.parse(localStorage.getItem(PREF_KEY)) || {}); } catch (e) { /* private window */ }

function setPref(key, value) {
  prefs[key] = value;
  try { localStorage.setItem(PREF_KEY, JSON.stringify(prefs)); } catch (e) { /* private window */ }
  applyPrefs();
}
function resetPrefs() {
  Object.assign(prefs, PREF_DEFAULTS);
  try { localStorage.removeItem(PREF_KEY); } catch (e) { /* private window */ }
  applyPrefs();
}
// the cores a program gets: the chosen number, or up to 4 where the page is isolated (shared memory)
function coresFor() {
  if (!self.crossOriginIsolated) return 1;
  return prefs.cores || Math.max(1, Math.min(navigator.hardwareConcurrency || 2, 4));
}
// what the page shows for the preferences (called again by each change)
function applyPrefs() {
  const root = document.documentElement;
  if (prefs.theme === 'auto') root.removeAttribute('data-theme');
  else root.dataset.theme = prefs.theme;
  root.dataset.icons = prefs.icons ? prefs.iconSize : 'none';
  root.dataset.smooth = prefs.smooth ? '1' : '0';
  const pad = document.getElementById('pad');
  if (pad) pad.style.opacity = prefs.padOpacity;
  const clock = document.getElementById('clock');
  if (clock) clock.hidden = !prefs.clock;
  if (typeof tick === 'function') tick();
}
applyPrefs();
