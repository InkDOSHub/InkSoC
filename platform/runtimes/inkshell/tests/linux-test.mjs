// Linux test for the Alpine terminal's engine (node 18+, no network needed).
// usage: node tests/linux-test.mjs [engine.wasm]   (run from apps/alpine)
// Boots Alpine like the page does, creates a user, and checks sudo, files, fork/exec, apk,
// /proc, and the swap (small real RAM, so pages of several processes go out and come back).
import fs from 'node:fs';
import zlib from 'node:zlib';
const wasmPath = process.argv[2] || 'engine.wasm';
const POOL = 24; // MB of real RAM: small on purpose
const dec = new TextDecoder(), enc = new TextEncoder();
let mem, x, out = '';
const swap = new Map();
const page = (p) => new Uint8Array(mem.buffer, p, 4096);
const env = {
  host_console: (p, n) => { out += dec.decode(new Uint8Array(mem.buffer, p, n).slice(), { stream: true }); },
  host_now_ms: () => performance.now(), host_epoch_ms: () => Date.now(),
  host_swap_write: (s, p) => { swap.set(s, page(p).slice()); },
  host_swap_read: (s, p) => { page(p).set(swap.get(s) || new Uint8Array(4096)); },
  wine_call: () => 0,
    host_random: (p, n) => { const b = new Uint8Array(n); crypto.getRandomValues(b); new Uint8Array(mem.buffer, p, n).set(b); },
    host_audio: () => {}, host_gl: () => 0, host_drive_read: () => 0, host_drive_write: () => 0,
  host_jit: (p, n) => {
    try {
      const m = new WebAssembly.Module(new Uint8Array(mem.buffer, p, n).slice());
      const i = new WebAssembly.Instance(m, { env: { memory: mem, rd: x.jit_rd, wr: x.jit_wr, step: x.jit_step, cond: x.jit_cond } });
      const t = x.__indirect_function_table, k = t.grow(1); t.set(k, i.exports.f); return k;
    } catch (e) { return -1; }
  },
};
x = (await WebAssembly.instantiate(fs.readFileSync(wasmPath), { env })).instance.exports;
mem = x.memory;
const put = (b) => { const p = x.buffer(b.length); new Uint8Array(mem.buffer, p, b.length).set(b); };
const add = (name, data) => { const nm = enc.encode(name), d = new Uint8Array(data), b = new Uint8Array(nm.length + 1 + d.length); b.set(nm); b.set(d, nm.length + 1); put(b); x.add_file(); };
x.init(POOL, 8192);
add('busybox', fs.readFileSync('busybox'));
add('rootfs.tar', zlib.gunzipSync(fs.readFileSync('alpine-rootfs.tar.gz')));
add('zoverlay.tar', fs.readFileSync('overlay.tar'));
add('stage1', fs.readFileSync(new URL('./stage1', import.meta.url))); // tests/src/stage1.c, built static
put(enc.encode('busybox\0sh /etc/inksoc/start'));
if (x.start() !== 0) { console.log('FAIL start:', dec.decode(new Uint8Array(mem.buffer, x.msg_ptr(), x.msg_len()))); process.exit(1); }
x.linux_winsize(30, 100);
const send = (t) => { const b = enc.encode(t); put(b); x.linux_stdin(b.length); };
// run until the program waits for the terminal again (or ends)
async function settle(maxMs = 120000) {
  const t0 = Date.now();
  for (let idle = 0; Date.now() - t0 < maxMs;) {
    const st = x.run(2000000);
    if (st === 1 || st === 2) return st;
    // waiting: a timer due soon (sleep, a timeout) is not idle; only waiting for the terminal is
    if (st === 3) { const ms = x.stat(17); if (ms >= 30 && !(x.linux_timer_ms?.() >= 0) && ++idle > 20) return 3; await new Promise((r) => setTimeout(r, Math.min(ms, 30) || 2)); } else idle = 0;
  }
  return 0;
}
const results = [];
async function step(input, expect, name) {
  const before = out.length;
  send(input);
  const st = await settle();
  const got = out.slice(before);
  const ok = expect.test(got);
  results.push(ok);
  console.log((ok ? 'PASS ' : 'FAIL ') + name + (ok ? '' : '\n---- output ----\n' + got.slice(-600) + '\n----------------'));
  if (st === 2) { console.log('engine stopped:', dec.decode(new Uint8Array(mem.buffer, x.msg_ptr(), x.msg_len()))); process.exit(1); }
}
const t0 = Date.now();
await settle();
results.push(/New UNIX username/.test(out)); console.log((results[0] ? 'PASS' : 'FAIL') + ' first start asks for a user');
await step('tester\n', /New password/, 'user name accepted');
await step('pw1\n', /Retype/, 'password asked twice');
await step('pw1\n', /tester@inksoc/, 'shell as the new user');
await step('id -un; echo $HOME\n', /tester\s+\/home\/tester/, 'id and home');
await step('sudo id -u\n', /Password/, 'sudo asks the password');
await step('pw1\n', /^\s*0\s*$/m, 'sudo runs as root');
await step('echo hello > f.txt && cat f.txt\n', /hello/, 'files');
await step('sh -c "for i in 1 2 3 4 5; do /bin/true; done; echo forked-ok"\n', /forked-ok/, 'fork and exec');
await step('apk info | wc -l\n', /^\s*1[0-9]\s*$/m, 'apk (dynamic program, shared libraries)');
await step('free -m | grep Mem\n', /Mem:\s+8192/, '/proc and sysinfo: programs see 8 GB');
await step('awk \'BEGIN{for(i=0;i<400000;i++)a[i]="item number " i; print length(a)}\'\n', /400000/, 'a program using more than the real RAM');
results.push(swap.size > 0); console.log((swap.size > 0 ? 'PASS' : 'FAIL') + ' pages went to swap (' + swap.size + ')');
await step('sh -c \'awk "BEGIN{for(i=0;i<200000;i++)a[i]=i; print 1}" & awk "BEGIN{for(i=0;i<200000;i++)b[i]=i; print 2}"; wait\' | sort | tr "\\n" " "; echo\n', /1 2/, 'two processes swapping at once');
await step('/root/stage1 threads\n', /threads 200000/, 'threads, mutex and condition variable (futex)');
await step('/root/stage1 signals\n', /signals 10 7 15/, 'signal handlers, pause, kill');
await step('/root/stage1 sockets\n', /sockets ping pong 0 tcp-ok/, 'socketpair, unix server, TCP on 127.0.0.1');
await step('/root/stage1 epoll\n', /epoll 0 1 22 1 11 5 select 0 1/, 'epoll, eventfd, select');
await step('/root/stage1 timers\n', /timers 14 1 fd 10 14/, 'alarm, setitimer, timerfd, timer_create; SIGALRM ends a process');
await step('/root/stage1 pty\n', /pty size tty line \^C sig 2/, 'pseudo-terminal: size, tty name, line mode, Ctrl+C');
await step('trap "echo caught-usr1" USR1; kill -USR1 $$\n', /caught-usr1/, 'shell trap');
await step('(nc -l -p 8098 > /tmp/nc.txt &) ; sleep 0.3; echo over-nc | nc 127.0.0.1 8098; sleep 0.3; cat /tmp/nc.txt\n', /^over-nc/m, 'nc server and client on this machine');
{ // Ctrl+C stops a running program and the shell comes back
  const before = out.length;
  send('sleep 30; echo after-sleep\n'); await settle(1500);
  x.linux_interrupt(); const t1 = Date.now(); await settle(10000);
  send('echo back-$((1+1))\n'); await settle(10000);
  const got = out.slice(before), ok = /back-2/.test(got) && Date.now() - t1 < 10000;
  results.push(ok); console.log((ok ? 'PASS' : 'FAIL') + ' Ctrl+C stops sleep' + (ok ? '' : '\n' + got.slice(-400)));
}
const pass = results.filter(Boolean).length;
console.log(`\n${pass}/${results.length} passed in ${((Date.now() - t0) / 1000).toFixed(1)} s · swap slots used: ${swap.size}`);
process.exit(pass === results.length ? 0 : 1);
