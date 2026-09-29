// SPDX-License-Identifier: MPL-2.0
// examples/codehub/check_web.cjs -- the CodeHub page in headless Chromium
// (Playwright): pictures, load time, frame time, hover, scroll, and the
// browser's pixels against the native PNG of the same frame.
//
//   node examples/codehub/check_web.cjs <outdir> [native-1996x1211.png]
//
// PLAYWRIGHT=<path to the playwright package> if it is not on NODE_PATH.
'use strict';
const http = require('http');
const fs = require('fs');
const path = require('path');
const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');

const root = path.join(__dirname, 'site');
const out = process.argv[2] || '/tmp/codehub-web';
const native = process.argv[3];
fs.mkdirSync(out, { recursive: true });
const types = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.ttf': 'font/ttf' };
const server = http.createServer((req, res) => {
    const u = new URL(req.url, 'http://x');
    const file = path.join(root, u.pathname === '/' ? 'index.html' : u.pathname);
    fs.readFile(file, (err, b) => {
        if (err) { res.writeHead(404); res.end(); return; }
        res.writeHead(200, { 'Content-Type': types[path.extname(file)] || 'application/octet-stream' });
        res.end(b);
    });
});

let failed = 0, passed = 0;
const check = (name, ok, got) => {
    if (ok) { passed++; console.log('OK    ' + name + (got !== undefined ? '  (' + got + ')' : '')); }
    else { failed++; console.log('FAIL  ' + name + '  got: ' + JSON.stringify(got)); }
};

// differing pixels of two PNG screenshots of the same size (via the page's
// own canvas decoding, so no PNG library is needed here)
async function diff(p, a, b) {
    return p.evaluate(async ([a, b]) => {
        const load = (src) => new Promise((r) => { const i = new Image(); i.onload = () => r(i); i.src = src; });
        const [ia, ib] = await Promise.all([load(a), load(b)]);
        if (ia.width !== ib.width || ia.height !== ib.height) return -1;
        const c = document.createElement('canvas'); c.width = ia.width; c.height = ia.height;
        const g = c.getContext('2d');
        g.drawImage(ia, 0, 0); const da = g.getImageData(0, 0, c.width, c.height).data;
        g.drawImage(ib, 0, 0); const db = g.getImageData(0, 0, c.width, c.height).data;
        let n = 0;
        for (let i = 0; i < da.length; i += 4)
            if (Math.abs(da[i] - db[i]) > 2 || Math.abs(da[i + 1] - db[i + 1]) > 2 || Math.abs(da[i + 2] - db[i + 2]) > 2) n++;
        return n;
    }, [a, b]);
}
const dataUrl = (f) => 'data:image/png;base64,' + fs.readFileSync(f).toString('base64');

async function open(browser, port, w, h, dpr) {
    const ctx = await browser.newContext({ viewport: { width: w, height: h }, deviceScaleFactor: dpr });
    const p = await ctx.newPage();
    p.on('pageerror', (e) => console.log('page error: ' + e.message));
    p.on('console', (m) => { if (m.type() === 'error') console.log('console: ' + m.text()); });
    // the moment the first picture is presented, relative to navigation start
    await p.addInitScript(() => {
        let f = 0;
        Object.defineProperty(window, 'firnFrames', {
            get() { return f; },
            set(v) { if (!f && v) window.firnFirst = performance.now(); f = v; },
        });
    });
    await p.goto(`http://127.0.0.1:${port}/?w=${w}&h=${h}&theme=dark`);
    await p.waitForFunction(() => (window.firnFrames || 0) > 0, null, { timeout: 30000 });
    return { ctx, p };
}

(async () => {
    await new Promise((r) => server.listen(0, '127.0.0.1', r));
    const port = server.address().port;
    const browser = await chromium.launch({ args: ['--disable-gpu'] });
    const wasmBytes = fs.statSync(path.join(root, 'codehub.wasm')).size;
    console.log(`codehub.wasm: ${wasmBytes} octets`);
    const result = { wasm: wasmBytes };

    for (const [name, w, h, dpr] of [['desktop', 1996, 1211, 1], ['phone', 360, 800, 1], ['phone-dpr3', 360, 800, 3]]) {
        const { ctx, p } = await open(browser, port, w, h, dpr);
        const first = await p.evaluate(() => window.firnFirst);
        const nav = await p.evaluate(() => { const e = performance.getEntriesByType('navigation')[0]; return e ? e.domContentLoadedEventEnd : 0; });
        check(`${name}: first picture ${first.toFixed(0)} ms after navigation (DOM ready ${nav.toFixed(0)} ms)`, first > 0 && first < 5000, first.toFixed(0));
        result[name + '_load_ms'] = Math.round(first);
        // the intro and the typing are over after ~3.5 s
        await p.waitForTimeout(4200);
        const shot = path.join(out, `web-${name}.png`);
        await p.screenshot({ path: shot });
        // frame time in the browser: 60 whole frames through the page's stopwatch
        const ms = await p.evaluate(() => {
            const x = window.firnExports; x.codehub_bench(5);
            const t = performance.now(); x.codehub_bench(60); return (performance.now() - t) / 60;
        });
        check(`${name}: frame ${ms.toFixed(2)} ms (goal <= 16)`, ms <= 16, ms.toFixed(2));
        result[name + '_frame_ms'] = +ms.toFixed(2);
        await p.waitForTimeout(100);
        if (name === 'desktop') {
            if (native && fs.existsSync(native)) {
                const n = await diff(p, dataUrl(native), dataUrl(shot));
                check(`desktop: browser vs native PNG, differing pixels ${n} of ${w * h}`, n >= 0 && n < w * h * 0.001, n);
                result.native_diff_px = n;
            }
            // hover the red button: the spring settles, the picture changes
            const bx = await p.evaluate(() => window.firnExports.codehub_hot_x(3));
            const by = await p.evaluate(() => window.firnExports.codehub_hot_y(3));
            await p.mouse.move(bx, by);
            await p.waitForTimeout(800);
            const hov = path.join(out, `web-${name}-hover.png`);
            await p.screenshot({ path: hov });
            const hn = await diff(p, dataUrl(shot), dataUrl(hov));
            check(`desktop: hover on the red button changes ${hn} pixels`, hn > 500, hn);
            const cur = await p.evaluate(() => getComputedStyle(document.getElementById('firn')).cursor);
            check(`desktop: pointer cursor over the button`, cur === 'pointer', cur);
            await p.mouse.move(5, 300);
            await p.waitForTimeout(800);
            const back = path.join(out, `web-${name}-back.png`);
            await p.screenshot({ path: back });
            const bn = await diff(p, dataUrl(shot), dataUrl(back));
            check(`desktop: pointer gone, the button is back (${bn} px differ)`, bn < 50, bn);
        }
        if (name === 'phone') {
            await p.mouse.move(180, 400);
            await p.mouse.wheel(0, 2000);
            await p.waitForTimeout(400);
            const sc = path.join(out, `web-${name}-scrolled.png`);
            await p.screenshot({ path: sc });
            const sn = await diff(p, dataUrl(shot), dataUrl(sc));
            check(`phone: the wheel scrolls the page (${sn} px differ)`, sn > 10000, sn);
        }
        await ctx.close();
    }
    await browser.close();
    server.close();
    fs.writeFileSync(path.join(out, 'result.json'), JSON.stringify(result, null, 1));
    console.log(JSON.stringify(result));
    console.log(failed ? `CODEHUB WEB: ${failed} FAILED, ${passed} passed` : `CODEHUB WEB PASSED (${passed} checks)`);
    process.exit(failed ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(1); });
