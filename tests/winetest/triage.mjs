// Short triage report of the winetest results under <dir> (<dll>/result.json and
// <dll>/<unit>.txt): units that stop are grouped by how they stop, stubs and
// "not yet" calls are counted, and each group is ranked by how many units it
// touches. It is meant to be read instead of the raw logs.
import { readdirSync, readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const dir = process.argv[2];
// calls that show up around every crash (debug output, locks) and say nothing
const NOISE = /!(TlsGetValue|TlsSetValue|ReleaseMutex|WaitForSingleObject|EnterCriticalSection|LeaveCriticalSection|__wine_dbg_\w+|WriteConsole\w*|WriteFile|VerifyConsoleIoHandle|GetLastError|SetLastError|GetStdHandle|GetConsoleMode|FlushFileBuffers)$/;

const groups = new Map(); // class -> { units: [], example }
const add = (map, key, unit, example) => {
  if (!map.has(key)) map.set(key, { units: [], example });
  map.get(key).units.push(unit);
};
const stubs = new Map();
const messages = new Map(); // failure text without numbers -> units

for (const d of existsSync(dir) ? readdirSync(dir) : []) {
  const f = join(dir, d, 'result.json');
  if (!existsSync(f)) continue;
  const res = JSON.parse(readFileSync(f, 'utf8'));
  for (const r of res.results || []) {
    const unit = `${res.dll}:${r.unit}`;
    for (const s of r.unimplemented || []) add(stubs, s.replace(/^stub: /, ''), unit);
    const txtf = join(dir, d, `${r.unit}.txt`);
    const txt = existsSync(txtf) ? readFileSync(txtf, 'utf8') : '';
    for (const m of txt.matchAll(/\[\w+\] (\w+) \(not yet\)/g)) add(stubs, `${m[1]} (not yet)`, unit);
    // "exit N" is a unit that ran to the end with N failures; only crashes, hangs and
    // 255 (the program itself gave up) count as stopping
    for (const l of r.failed || []) add(messages, l.replace(/^[\w.]+:\d+: Test failed: /, '').replace(/0x[0-9a-f]+|\b\d+\b/gi, 'N').slice(0, 120), unit);
    if (!/^(crash|timeout|exit 255)$/.test(r.end || '')) continue;
    let key;
    let example = '';
    const stop = txt.match(/^Stopped: (.*)$/m);
    if (stop) {
      const where = (stop[1].match(/\[in ([^\]+]+)/) || [])[1] || '?';
      const kind = stop[1].replace(/\s*\[in .*\]/, '').replace(/0x[0-9a-f]+/g, 'X');
      const calls = [...txt.slice(stop.index).matchAll(/^\s+(\S+!\S+) from (\S+)/gm)];
      const last = calls.find((c) => !NOISE.test(c[1]));
      key = `${kind} in ${where}${last ? `, after ${last[1]}` : ''}`;
      example = `${stop[1]}${last ? ` (last call ${last[1]} from ${last[2]})` : ''}`;
    } else {
      key = r.end === 'timeout' ? 'timeout' : `ends with ${r.end}`;
    }
    const lastFail = (r.failed || []).slice(-1)[0];
    add(groups, key, unit, example + (lastFail ? `; last failure: ${lastFail}` : ''));
  }
}

const rank = (map) => [...map.entries()].sort((a, b) => b[1].units.length - a[1].units.length);
const list = (u) => (u.length > 6 ? `${u.slice(0, 6).join(', ')} +${u.length - 6}` : u.join(', '));

console.log('## Triage: units that stop, by class\n');
console.log('| units | class | which | example |');
console.log('|---|---|---|---|');
for (const [k, v] of rank(groups)) console.log(`| ${v.units.length} | ${k} | ${list(v.units)} | ${(v.example || '').slice(0, 220)} |`);
console.log('\n## Triage: stubs and "not yet" calls\n');
console.log('| units | call | which |');
console.log('|---|---|---|');
for (const [k, v] of rank(stubs).slice(0, 30)) console.log(`| ${new Set(v.units).size} | ${k} | ${list([...new Set(v.units)])} |`);
console.log('\n## Triage: failure messages seen in several units\n');
console.log('| units | message | which |');
console.log('|---|---|---|');
for (const [k, v] of rank(messages).map(([k, v]) => [k, { units: [...new Set(v.units)] }]).filter(([, v]) => v.units.length > 1).sort((a, b) => b[1].units.length - a[1].units.length).slice(0, 20)) console.log(`| ${v.units.length} | ${k} | ${list(v.units)} |`);
