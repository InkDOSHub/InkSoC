// Runs Wine's conformance tests (<dll>_test.exe, built from the Wine release the engine
// ships) in the engine, one test unit per page load, and writes out/<dll>/result.json:
// per unit the tests executed, failures, todo, skipped, or why it did not finish.
//   node tests/winetest/run.mjs <dir with *_test.exe> <dll> [unit timeout s]
import { createRequire } from 'node:module';
import { writeFileSync, mkdirSync, existsSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { serve } from '../compat/serve.mjs';

const require = createRequire(import.meta.url);
const { chromium } = require('playwright');
const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../..');
const [dir, dll, limit = '150'] = process.argv.slice(2);
const exe = `${dll}_test.exe`;
const file = resolve(dir, exe);
if (!existsSync(file)) { console.error('missing ' + file); process.exit(2); }
const out = join(here, 'out', dll);
mkdirSync(out, { recursive: true });

const server = await serve(root, { [exe]: file });
const base = `http://127.0.0.1:${server.address().port}`;
// a profile on disk: the test program stays on drive C: between page loads
const browser = await chromium.launchPersistentContext(mkdtempSync(join(tmpdir(), 'inksoc-wt-')), { viewport: { width: 1100, height: 800 } });
const page = browser.pages()[0] || (await browser.newPage());
const con = async () => (await page.textContent('#con').catch(() => '')) || '';
const t0 = Date.now();
const log = (s) => console.log(`[${((Date.now() - t0) / 1000).toFixed(0)}s] ${s}`);
const END = /Finished \(exit code (-?\d+)\)|\nExit code (-?\d+)|out of memory|Stopped:/;

// waits until the program ends; the console text and how it ended
async function finish(ms) {
  const end = Date.now() + ms;
  for (;;) {
    const t = await con();
    const m = t.match(END);
    if (m) return { text: t, end: (m[1] ?? m[2]) !== undefined ? 'exit ' + (m[1] ?? m[2]) : 'crash' };
    if (Date.now() > end) return { text: t, end: 'timeout' };
    await page.waitForTimeout(1000);
  }
}

// 1. the program goes to C:\InkSoC\Downloads and runs without arguments: Wine's test
//    programs then list their units ("Valid test names:")
await page.goto(base + '/?file=1');
await page.evaluate(async (n) => {
  const b = await (await fetch('/extra/' + n)).blob();
  window.dispatchEvent(new CustomEvent('inkos:file', { detail: new File([b], n), cancelable: true }));
}, exe);
const first = await finish(300000);
const list = (first.text.split(/Valid test names:/)[1] || '').split(/Finished \(|\nExit code|\[engine\]|\n\s*\n/)[0];
const units = list.split(/\s+/).filter((w) => /^[a-z0-9_]+$/i.test(w));
log(`${units.length} units: ${units.join(' ')}`);
writeFileSync(join(out, 'list.txt'), first.text);

// 2. each unit on its own page load, with a time limit
const results = [];
const SUM = /(\d+) tests? executed \((\d+) marked as todo, (?:\d+ as flaky, )?(\d+) failures?\), (\d+) skipped/;
for (const u of units) {
  const s = Date.now();
  await page.goto(base + '/?run=' + encodeURIComponent('C:\\InkSoC\\Downloads\\' + exe) + '&args=' + encodeURIComponent(u));
  const r = await finish(+limit * 1000);
  const m = r.text.match(SUM);
  const res = { unit: u, end: r.end, seconds: Math.round((Date.now() - s) / 1000) };
  if (m) Object.assign(res, { executed: +m[1], todo: +m[2], failures: +m[3], skipped: +m[4] });
  // the first failing checks, to group failures by cause
  res.failed = [...r.text.matchAll(/^.*Test failed:.*$/gm)].slice(0, 15).map((x) => x[0].slice(0, 300));
  res.unimplemented = [...new Set([...r.text.matchAll(/(?:unimplemented|not implemented|stub)[^\n]{0,120}/gi)].map((x) => x[0]))].slice(0, 15);
  results.push(res);
  writeFileSync(join(out, u + '.txt'), r.text);
  log(`${u}: ${r.end}${m ? `, ${res.executed} tests, ${res.failures} failures` : ''}`);
}
const sum = (k) => results.reduce((a, r) => a + (r[k] || 0), 0);
const result = {
  dll, units: units.length,
  clean: results.filter((r) => r.end === 'exit 0' && r.failures === 0).length,
  executed: sum('executed'), failures: sum('failures'), todo: sum('todo'), skipped: sum('skipped'),
  crashed: results.filter((r) => r.end === 'crash').length, timedOut: results.filter((r) => r.end === 'timeout').length,
  seconds: Math.round((Date.now() - t0) / 1000), results,
};
writeFileSync(join(out, 'result.json'), JSON.stringify(result, null, 2));
log(`${dll}: ${result.clean}/${result.units} units clean, ${result.failures} failures of ${result.executed}`);
await browser.close();
server.close();
