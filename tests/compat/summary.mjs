// Markdown table of out/*/result.json (for the workflow's job summary).
import { readdirSync, readFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const dir = process.argv[2] || join(dirname(fileURLToPath(import.meta.url)), 'out');
const rows = existsSync(dir) ? readdirSync(dir).map((d) => join(dir, d, 'result.json')).filter(existsSync).map((f) => JSON.parse(readFileSync(f, 'utf8'))) : [];
console.log('| Case | Result | Time | Wasm memory | Problems |\n|---|---|---|---|---|');
for (const r of rows.sort((a, b) => a.case.localeCompare(b.case))) {
  console.log(`| ${r.case} | ${r.ok ? '✅' : '❌'} | ${r.seconds} s | ${r.wasmMemoryMb ?? '?'} MB | ${r.problems.map((p) => p.replace(/\|/g, '\\|').slice(0, 300)).join('<br>')} |`);
}
