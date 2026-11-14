// Runs the 7-Zip benchmark (4 cores) on an InkSoC page in headless Chromium and prints the result lines.
// usage: [BROWSER=webkit] node bench.js <page url>
// Old commits have the test page (tests.html, #exec7z); newer ones run it from the shell's Diagnostics window.
const pw = require('playwright');
const engine = pw[process.env.BROWSER || 'chromium'];
(async () => {
  const b = await engine.launch(); const p = await b.newPage();
  p.on('pageerror', (e) => console.log('ERR', e.message));
  await p.goto(process.argv[2]);
  let out = '#exec-out';
  if (await p.$('#exec7z')) {
    await p.waitForSelector('#exec7z:not([disabled])', { timeout: 60000 });
    await p.selectOption('#cores', '4'); await p.click('#exec7z');
  } else {
    await p.evaluate(() => { setPref('cores', 4); tool('diag'); });
    await p.waitForSelector('#dg-7z', { timeout: 60000 });
    await p.click('#dg-7z');
    out = '#dg-7z-out';
  }
  await p.waitForFunction((sel) => /Finished|Stopped|rror/.test(document.querySelector(sel).textContent), out, { timeout: 600000 });
  const t = await p.textContent(out);
  console.log((t.match(/18:.*|Tot:.*|Rating.*/g) || [t.slice(-400)]).join(' / '));
  await b.close();
})();
