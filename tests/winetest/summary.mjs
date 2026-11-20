// Markdown table of the winetest results under <dir> (each <dll>/result.json).
import { readdirSync, readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const dir = process.argv[2];
const rows = [];
for (const d of readdirSync(dir)) {
  const f = join(dir, d, 'result.json');
  if (existsSync(f)) rows.push(JSON.parse(readFileSync(f, 'utf8')));
}
rows.sort((a, b) => a.dll.localeCompare(b.dll));
const pct = (a, b) => (b ? ((100 * a) / b).toFixed(1) + '%' : '-');
console.log('## Wine conformance tests in the engine\n');
console.log('| DLL | units clean | checks passed | failures | crashed | timed out |');
console.log('|---|---|---|---|---|---|');
const t = { u: 0, c: 0, e: 0, f: 0 };
for (const r of rows) {
  console.log(`| ${r.dll} | ${r.clean}/${r.units} (${pct(r.clean, r.units)}) | ${pct(r.executed - r.failures, r.executed)} of ${r.executed} | ${r.failures} | ${r.crashed} | ${r.timedOut} |`);
  t.u += r.units; t.c += r.clean; t.e += r.executed; t.f += r.failures;
}
console.log(`| **all** | ${t.c}/${t.u} (${pct(t.c, t.u)}) | ${pct(t.e - t.f, t.e)} of ${t.e} | ${t.f} | | |`);
