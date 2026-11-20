// Runs one case of corpus.json in headless Chromium and writes out/<case>/: the console
// output (console.txt), screenshots, and result.json. Exits 1 when the case fails.
//   node tests/compat/run.mjs <case> [downloads dir]
import { createRequire } from 'node:module';
import { readFileSync, writeFileSync, mkdirSync, existsSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { serve } from './serve.mjs';

const require = createRequire(import.meta.url);
const { chromium } = require('playwright');
const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../..');
const name = process.argv[2];
const dl = resolve(process.argv[3] || join(here, 'downloads'));
const c = JSON.parse(readFileSync(join(here, 'corpus.json'), 'utf8')).cases[name];
if (!c) { console.error('no case ' + name); process.exit(2); }
const out = join(here, 'out', name);
mkdirSync(out, { recursive: true });

const file = c.local ? join(root, c.local) : c.file ? join(dl, c.file) : null;
if (file && !existsSync(file)) { console.error('missing ' + file + ' (download it first)'); process.exit(2); }
const fname = c.local ? c.local.split('/').pop() : c.file;
const server = await serve(root, file ? { [fname]: file } : {});
const base = `http://127.0.0.1:${server.address().port}`;
// a profile on disk, as a user's browser has: an incognito context keeps its storage in
// memory with a small quota, too small for the spill file of a big installer
const profile = mkdtempSync(join(tmpdir(), 'inksoc-profile-'));
const browser = await chromium.launchPersistentContext(profile, { viewport: { width: 1100, height: 800 } });
const page = browser.pages()[0] || (await browser.newPage());
page.on('console', (m) => { if (/^\[swap\]/.test(m.text())) console.log(m.text()); });
const errors = [];
page.on('pageerror', (e) => errors.push(e.message));

const t0 = Date.now();
const log = (s) => console.log(`[${((Date.now() - t0) / 1000).toFixed(0)}s] ${s}`);
let earlier = ''; // console output of pages left by a "run" step
const con = async () => earlier + ((await page.textContent('#con').catch(() => '')) || '');
const shots = [];
const shot = async (n) => { const p = `${shots.length}-${n}.png`; await page.screenshot({ path: join(out, p) }); shots.push(p); };

let failed = null;
try {
  await page.goto(base + '/' + (c.query || ''));
  if (file) {
    await page.evaluate(async (n) => {
      const b = await (await fetch('/extra/' + n)).blob();
      window.dispatchEvent(new CustomEvent('inkos:file', { detail: new File([b], n), cancelable: true }));
    }, fname);
  }
  for (const s of c.steps) {
    log(JSON.stringify(s));
    const [op, a, b] = s;
    if (op === 'wait') await page.waitForTimeout(a);
    else if (op === 'key') await page.keyboard.press(a);
    else if (op === 'click') await page.mouse.click(a, b);
    else if (op === 'type') { await page.fill('.cline', a); await page.press('.cline', 'Enter'); }
    else if (op === 'shot') await shot(a);
    else if (op === 'run') {
      earlier = await con();
      await page.goto(base + '/?run=' + encodeURIComponent(a) + (b ? '&args=' + encodeURIComponent(b) : '') + (c.query.includes('winedebug') ? '&winedebug=2' : ''));
    } else if (op === 'until') {
      const re = new RegExp(a), end = Date.now() + b;
      while (!re.test(await con())) {
        if (Date.now() > end) throw new Error(`timed out waiting for /${a}/`);
        if (/out of memory|Stopped:/.test(await con())) throw new Error(`stopped while waiting for /${a}/`);
        await page.waitForTimeout(5000);
      }
    }
  }
} catch (e) {
  failed = e.message;
  await shot('failure').catch(() => {});
}
const text = await con();
writeFileSync(join(out, 'console.txt'), text);
const problems = [];
if (failed) problems.push(failed);
for (const r of c.expect || []) if (!new RegExp(r).test(text)) problems.push('missing /' + r + '/');
for (const r of c.reject || []) { const m = text.match(new RegExp(r)); if (m) problems.push('found /' + r + '/: ' + text.slice(Math.max(0, m.index - 200), m.index + 200).replace(/\s+/g, ' ')); }
for (const e of errors) problems.push('page error: ' + e);
const mem = [...text.matchAll(/wasm memory (\d+) MB/g)].map((m) => +m[1]);
const result = { case: name, ok: problems.length === 0, seconds: Math.round((Date.now() - t0) / 1000), wasmMemoryMb: mem.length ? Math.max(...mem) : null, problems, shots };
writeFileSync(join(out, 'result.json'), JSON.stringify(result, null, 2));
log((result.ok ? 'PASS ' : 'FAIL ') + name + (problems.length ? '\n  ' + problems.join('\n  ') : ''));
await browser.close();
server.close();
process.exit(result.ok ? 0 : 1);
