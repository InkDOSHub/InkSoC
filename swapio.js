/* InkSoC swap I/O worker (several cores): the only holder of the OPFS swap file.
   Any core asks it for one 4 KB page through a shared mailbox and waits:
   ctl[0] state (0 idle, 1 request, 2 done, 3 quit), ctl[1] op (1 write, 2 read),
   ctl[2] slot, ctl[3] address in the engine's memory, ctl[4] mailbox lock.
   It also holds the drive C: files open for reading: op 3 reads ctl[5] bytes of file ctl[2]
   at ctl[6] + ctl[7] * 2^32 into ctl[3], and leaves the count read in ctl[5]; op 4 writes them.
   File SPILL is the spill file: closed program files too big to keep in memory. */
'use strict';
const PAGE = 4096, SPILL = 0x7FFFFFF0;
onmessage = async (ev) => {
  const { memory, ctl: sab, name, drive = [], spill: spillName } = ev.data;
  const ctl = new Int32Array(sab);
  let swap = null;
  const ram = new Map();
  try {
    const root = await navigator.storage.getDirectory();
    const fh = await root.getFileHandle(name, { create: true });
    swap = await fh.createSyncAccessHandle();
    swap.truncate(0);
  } catch (e) { swap = null; }
  let spill = null, files = [];
  const opened = []; // every drive handle opened so far, so a failure can close them all
  // from here on, a failure is reported as {failed} and ready is never sent
  try {
    if (spillName) {
      // a spill handle that opened but could not be prepared is closed, and the run goes on without spill
      let h = null;
      try { h = await (await (await navigator.storage.getDirectory()).getFileHandle(spillName, { create: true })).createSyncAccessHandle(); h.truncate(0); spill = h; }
      catch (e) { spill = null; if (h) { try { h.close(); } catch (_) { /* already closed */ } } }
    }
    // drive files come as OPFS paths (Safari cannot send file handles between workers)
    const dirRoot = await navigator.storage.getDirectory();
    const handleAt = async (at) => { let d = dirRoot; for (const n of at.slice(0, -1)) d = await d.getDirectoryHandle(n); return d.getFileHandle(at[at.length - 1]); };
    const track = (x) => { opened.push(x); return x; };
    // all files open in parallel; the results keep the order of drive, with null for a file that will not open
    files = await Promise.all(drive.map(async (at) => {
      let h;
      try { h = await handleAt(at); } catch (e) { return null; }
      try { return track(await h.createSyncAccessHandle({ mode: 'read-only' })); }
      catch (e) { try { return track(await h.createSyncAccessHandle()); } catch (e2) { return null; } }
    }));
    postMessage({ ready: true, spill: !!spill, opfs: !!swap, drive: files.map((f) => !!f) });
  } catch (e) {
    const why = (e && e.message) || String(e);
    // close what this worker opened; a failure while closing must not replace the cause
    for (const h of new Set([swap, spill, ...opened])) { if (!h) continue; try { h.close(); } catch (_) { /* already closed */ } }
    postMessage({ failed: why });
    return;
  }
  const buf = new Uint8Array(PAGE);
  let writeErr = false;
  for (;;) {
    const s = Atomics.load(ctl, 0);
    if (s === 3) break;
    if (s !== 1) { Atomics.wait(ctl, 0, s); continue; }
    const op = ctl[1], slot = ctl[2];
    if (op === 3 || op === 4) {
      const f = slot === SPILL ? spill : files[slot], n = ctl[5], at = (ctl[6] >>> 0) + ctl[7] * 4294967296;
      if (op === 4) {
        // a full browser storage (QuotaExceededError) is a failed write, never a dead worker
        try { ctl[5] = f ? f.write(new Uint8Array(memory.buffer, ctl[3] >>> 0, n).slice(), { at }) : 0; }
        catch (e) { ctl[5] = 0; if (!writeErr) { writeErr = true; console.warn(`[swap] write of ${n} bytes at ${at} failed: ${e && e.name} ${e && e.message}`); } }
      }
      else {
        const b = new Uint8Array(n), got = f ? f.read(b, { at }) : 0;
        new Uint8Array(memory.buffer, ctl[3] >>> 0, got).set(b.subarray(0, got));
        ctl[5] = got;
      }
      Atomics.store(ctl, 0, 2);
      Atomics.notify(ctl, 0);
      continue;
    }
    const page = new Uint8Array(memory.buffer, ctl[3] >>> 0, PAGE);
    if (op === 1) { buf.set(page); if (swap) swap.write(buf, { at: slot * PAGE }); else ram.set(slot, buf.slice()); }
    else { if (swap) swap.read(buf, { at: slot * PAGE }); else buf.set(ram.get(slot) || new Uint8Array(PAGE)); page.set(buf); }
    Atomics.store(ctl, 0, 2);
    Atomics.notify(ctl, 0);
  }
  if (swap) { swap.flush(); swap.truncate(0); swap.close(); }
  for (const f of files) if (f) f.close();
  if (spill) { spill.flush(); spill.close(); }
  postMessage({ closed: true });
};

