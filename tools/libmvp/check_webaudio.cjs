'use strict';
const http = require('http');
const fs = require('fs');
const path = require('path');
const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');

const root = path.join(__dirname, '..', '..', 'demos', 'webdemo');
const wasm = process.argv[2];
if (!wasm) { console.error('usage: check_webaudio.cjs <probe.wasm>'); process.exit(2); }

const types = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.ttf': 'font/ttf' };
const server = http.createServer((req, res) => {
    const u = new URL(req.url, 'http://x');
    const file = u.pathname === '/probe.wasm' ? wasm : path.join(root, u.pathname === '/' ? 'index.html' : u.pathname);
    fs.readFile(file, (err, b) => {
        if (err) { res.writeHead(404); res.end(); return; }
        res.writeHead(200, { 'Content-Type': types[path.extname(file)] || 'application/octet-stream' });
        res.end(b);
    });
});

let failed = 0, passed = 0;
const check = (name, ok, got) => {
    if (ok) { passed++; console.log('OK    ' + name); } else { failed++; console.log('FAIL  ' + name + '  got: ' + JSON.stringify(got)); }
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const status = async (p) => {
    const v = await p.evaluate(() => localStorage.getItem('webaudio'));
    if (!v) return null;
    const [pos, state, running] = v.split('|').map(Number);
    return { pos, state, running };
};

const level = (p) => p.evaluate(() => {
    const s = window.firnAudioStreams && window.firnAudioStreams.get(1);
    if (!s || !s.analyser) return -1;
    const a = new Float32Array(s.analyser.fftSize);
    s.analyser.getFloatTimeDomainData(a);
    let m = 0;
    for (const v of a) m = Math.max(m, Math.abs(v));
    return m;
});

const info = (p) => p.evaluate(() => {
    const s = window.firnAudioStreams && window.firnAudioStreams.get(1);
    return s ? { ctx: s.ctx.state, mode: s.mode, pushed: s.pushed, consumed: s.consumed, played: s.played } : null;
});

async function run(browser, port, mode, gesture) {
    const tag = mode + (gesture ? '/gesture' : '/no-gesture');
    const ctx = await browser.newContext();
    const p = await ctx.newPage();
    if (!gesture) {
        await p.addInitScript(() => {
            const Base = window.AudioContext;
            window.AudioContext = class extends Base {
                constructor(o) { super(o); super.suspend(); }
                resume() { return window.__unlock ? super.resume() : Promise.resolve(); }
            };
        });
    }
    p.on('pageerror', (e) => console.log('page error: ' + e.message));
    await p.goto('http://127.0.0.1:' + port + '/?wasm=probe.wasm&w=400&h=300' + (mode === 'script' ? '&audio=script' : ''));
    await p.waitForFunction(() => (window.firnFrames || 0) > 0, null, { timeout: 20000 });
    await p.evaluate(() => localStorage.removeItem('webaudio'));
    if (gesture) await p.click('canvas');
    if (gesture) await p.keyboard.press('p');
    else await p.evaluate(() => document.getElementById('firn').dispatchEvent(new KeyboardEvent('keydown', { key: 'p', code: 'KeyP' })));
    await p.waitForFunction(() => localStorage.getItem('webaudio') !== null, null, { timeout: 5000 }).catch(() => {});
    await sleep(600);
    const a = await status(p);
    check(tag + ': playback started', a && a.running === 1, a);
    const t0 = Date.now();
    await sleep(1500);
    const b = await status(p);
    const wall = Date.now() - t0;
    const adv = b && a ? b.pos - a.pos : -1;
    const i = await info(p);
    if (gesture) {
        check(tag + ': context running', i && i.ctx === 'running', i);
        check(tag + ': audio mode ' + mode, i && i.mode === (mode === 'script' ? 2 : 1), i);
        check(tag + ': clock follows audio (' + adv + ' ms in ' + wall + ' ms wall)', adv > wall * 0.7 && adv < wall * 1.3, { adv, wall });
        check(tag + ': samples consumed', i && i.consumed > 20000, i);
        let peak = 0;
        for (let k = 0; k < 10; k++) { peak = Math.max(peak, await level(p)); await sleep(30); }
        check(tag + ': analyser sees the 440 Hz tone (peak ' + peak.toFixed(3) + ')', peak > 0.3 && peak < 0.6, peak);
        await p.keyboard.press('s');
        await sleep(300);
        const c = await status(p);
        const ic = await info(p);
        await sleep(700);
        const d = await status(p);
        const id = await info(p);
        check(tag + ': pause stops clock and consumption', d.pos - c.pos < 60 && id.consumed - ic.consumed < 1500, { c, d, ic, id });
        await p.keyboard.press('r');
        await sleep(800);
        const e = await status(p);
        check(tag + ': resume continues', e.pos - d.pos > 400, { d, e });
        await p.keyboard.press('k');
        await sleep(500);
        const f = await status(p);
        check(tag + ': seek to 2500 ms lands near it (' + f.pos + ')', f.pos >= 2450 && f.pos < 3300, f);
        await sleep(2500);
        const g = await status(p);
        check(tag + ': ends at the duration with state ENDED', g.state === 3 && g.pos >= 3900, g);
    } else {
        check(tag + ': context waits for a gesture (autoplay block simulated: headless Chromium ignores the policy)', i && i.ctx === 'suspended', i);
        check(tag + ': clock stays at 0 without audio (not wall time)', b.pos < 100, b);
        check(tag + ': nothing consumed', i.consumed === 0, i);
        await p.evaluate(() => { window.__unlock = true; });
        await p.mouse.click(100, 100);
        await sleep(1200);
        const c = await status(p);
        const ic = await info(p);
        check(tag + ': a click unlocks the context and playback runs', ic.ctx === 'running' && c.pos > 400, { c, ic });
    }
    await ctx.close();
}

(async () => {
    await new Promise((r) => server.listen(0, '127.0.0.1', r));
    const port = server.address().port;
    const browser = await chromium.launch({ args: ['--autoplay-policy=document-user-activation-required'] });
    for (const mode of ['worklet', 'script']) {
        await run(browser, port, mode, true);
        await run(browser, port, mode, false);
    }
    await browser.close();
    server.close();
    console.log(passed + ' passed, ' + failed + ' failed');
    process.exit(failed ? 1 : 0);
})();
