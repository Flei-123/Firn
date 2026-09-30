// SPDX-License-Identifier: MPL-2.0
// tools/phone_remote/check.cjs -- examples/phone_remote end to end: a real
// Chromium with a phone's touch screen (Playwright, Pixel 7 emulation)
// opens the printed URL, and every gesture has to arrive at lib/input.
//
//   node tools/phone_remote/check.cjs <shadow build of the example> <port>
//
// The binary is the example built with tools/input/record_backend.fi as its
// input backend (tools/phone_remote/run.sh), so what reaches lib/input is
// read from its stdout. Counter-checks: a browser without the token gets
// 403 and no WebSocket; a wrong token gets 403.
'use strict';
const { spawn } = require('child_process');
const { chromium, devices } = require(process.env.PLAYWRIGHT || 'playwright');

const bin = process.argv[2];
let ok = 0, total = 0, refusals = 0;
function check(name, cond, detail) {
  total++;
  if (cond) ok++; else console.log('  FAIL', name, detail === undefined ? '' : JSON.stringify(detail).slice(0, 300));
}
const sleep = ms => new Promise(r => setTimeout(r, ms));

(async () => {
  const srv = spawn(bin, [], { stdio: ['ignore', 'pipe', 'pipe'] });
  let out = '';
  srv.stdout.on('data', d => { out += d.toString(); });
  while (!out.includes('\n')) await sleep(20);
  const first = out.split('\n')[0];
  check('prints "Open on the phone: http://<ip>:<port>/?t=<token>"',
    /^Open on the phone: http:\/\/\d+\.\d+\.\d+\.\d+:\d+\/\?t=[0-9a-f]{32}$/.test(first), first);
  const url = first.replace(/^Open on the phone: http:\/\/[^:]+/, 'http://127.0.0.1');
  const lines = () => out.split('\n').slice(1).filter(Boolean);

  const browser = await chromium.launch();
  // --- counter-checks first: no token, wrong token
  const stranger = await browser.newContext({ ...devices['Pixel 7'] });
  const sp = await stranger.newPage();
  const r0 = await sp.goto(url.replace(/\?t=.*$/, ''));
  check('REFUSED no token -> 403', r0.status() === 403, r0.status()); refusals++;
  const r1 = await sp.goto(url.replace(/t=[0-9a-f]+/, 't=' + '0'.repeat(32)));
  check('REFUSED wrong token -> 403', r1.status() === 403, r1.status()); refusals++;
  const wsOk = await sp.evaluate(u => new Promise(res => {
    const w = new WebSocket(u); w.onopen = () => res(true); w.onerror = () => res(false);
  }), url.replace(/^http/, 'ws').replace(/\/\?t=.*$/, '/ws'));
  check('REFUSED WebSocket without the pairing cookie', wsOk === false, wsOk); refusals++;
  await stranger.close();
  check('nothing reached lib/input from the stranger', lines().length === 0, lines());

  // --- the phone
  const ctx = await browser.newContext({ ...devices['Pixel 7'] });
  const page = await ctx.newPage();
  const resp = await page.goto(url);
  check('the paired URL ends on the page, token gone from the address bar',
    resp.status() === 200 && !page.url().includes('t='), [resp.status(), page.url()]);
  await page.waitForFunction(() => document.getElementById('st').textContent === 'connected', null, { timeout: 5000 });
  check('the page says connected', true);
  const cdp = await ctx.newCDPSession(page);
  const box = await page.locator('#pad').boundingBox();
  const cx = box.x + box.width / 2, cy = box.y + box.height / 2;
  const touch = (type, pts) => cdp.send('Input.dispatchTouchEvent', {
    type, touchPoints: pts.map(([x, y], id) => ({ x, y, id })) });

  // 1. a drag: 60 px right, 20 px up in 10 steps -> moves summing to +120/-40 (factor 2)
  let n0 = lines().length;
  await touch('touchStart', [[cx, cy]]);
  for (let i = 1; i <= 10; i++) { await touch('touchMove', [[cx + 6 * i, cy - 2 * i]]); await sleep(15); }
  await touch('touchEnd', []);
  await sleep(300);
  let got = lines().slice(n0);
  let sx = 0, sy = 0, clicks = 0;
  for (const l of got) {
    const p = l.split(' ');
    if (p[0] === 'rel') { sx += +p[1]; sy += +p[2]; }
    if (p[0] === 'btn') clicks++;
  }
  check('drag -> mouse_move adds up to (+120, -40)', sx === 120 && sy === -40, [sx, sy, got]);
  check('drag -> no click', clicks === 0, got);

  // 2. a tap -> left click
  n0 = lines().length;
  await touch('touchStart', [[cx, cy]]);
  await sleep(40);
  await touch('touchEnd', []);
  await sleep(300);
  got = lines().slice(n0);
  check('tap -> left click (272 down, up)', JSON.stringify(got) === JSON.stringify(['btn 272 1', 'btn 272 0']), got);

  // 3. a two-finger tap -> right click
  n0 = lines().length;
  await touch('touchStart', [[cx, cy], [cx + 40, cy]]);
  await sleep(40);
  await touch('touchEnd', []);
  await sleep(300);
  got = lines().slice(n0);
  check('two-finger tap -> right click (273)', JSON.stringify(got) === JSON.stringify(['btn 273 1', 'btn 273 0']), got);

  // 4. two fingers dragged 100 px down -> scroll -4 notches in total
  n0 = lines().length;
  await touch('touchStart', [[cx, cy], [cx + 40, cy]]);
  for (let i = 1; i <= 10; i++) { await touch('touchMove', [[cx, cy + 10 * i], [cx + 40, cy + 10 * i]]); await sleep(15); }
  await touch('touchEnd', []);
  await sleep(300);
  got = lines().slice(n0);
  let wheel = 0;
  for (const l of got) { const p = l.split(' '); if (p[0] === 'rel') wheel += +p[3]; }
  check('two-finger drag down -> scroll -4', wheel === -4 && got.every(l => l.startsWith('rel 0 0')), [wheel, got]);

  // 5. the buttons: volume, mute, play/pause, scroll, clicks
  const expect = {
    'vol+': ['key 115 1', 'key 115 0'], 'vol-': ['key 114 1', 'key 114 0'], 'mute': ['key 113 1', 'key 113 0'],
    'play': ['key 164 1', 'key 164 0'], 'scroll,3': ['rel 0 0 3 0'], 'click': ['btn 272 1', 'btn 272 0'],
  };
  for (const [c, want] of Object.entries(expect)) {
    n0 = lines().length;
    await page.locator(`button[data-c="${c}"]`).tap();
    await sleep(250);
    got = lines().slice(n0);
    check(`button ${c}`, JSON.stringify(got) === JSON.stringify(want), got);
  }

  // 6. the page reconnects after its socket dropped, and is still paired
  await page.evaluate(() => ws.close());
  await page.waitForFunction(() => document.getElementById('st').textContent === 'connected', null, { timeout: 5000 });
  n0 = lines().length;
  await page.locator('button[data-c="mute"]').tap();
  await sleep(250);
  check('reconnects after the socket dropped, still paired', JSON.stringify(lines().slice(n0)) === '["key 113 1","key 113 0"]', lines().slice(n0));

  await browser.close();
  srv.kill();
  console.log(`   ${ok} / ${total} phone remote cases, of them ${refusals} refusals`);
  console.log(ok === total ? `PHONE REMOTE OK: ${ok} / ${total}, refusals ${refusals}` : `PHONE REMOTE FAILED: ${ok} / ${total}`);
  process.exit(ok === total ? 0 : 1);
})().catch(e => { console.log('  FAIL exception', e.message); process.exit(1); });
