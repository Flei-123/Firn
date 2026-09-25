// tools/libmvp/check_webclip.cjs -- lib/plat/webclip.fi (OpenPlan LIB-006, the
// browser part) against Chromium's real clipboard, driven by Playwright.
//
//   node tools/libmvp/check_webclip.cjs <probe.wasm>
//
// The page is demos/webdemo (its firn.js is the host) with the probe of
// tools/libmvp/webclip_probe.fi. Checked: a copy from Firn is what the
// browser's clipboard holds; text put there from outside (UTF-8) comes
// back to Firn; a program's own type goes through and back; a type that is
// not there answers status 0; and without the read permission the page
// still gets its own copy back (the host's keep).
// PLAYWRIGHT=<path to the playwright package> if it is not on NODE_PATH.
'use strict';
const http = require('http');
const fs = require('fs');
const path = require('path');
const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');

const root = path.join(__dirname, '..', '..', 'demos', 'webdemo');
const wasm = process.argv[2];
if (!wasm) { console.error('usage: check_webclip.cjs <probe.wasm>'); process.exit(2); }

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

async function page(browser, grant, port) {
    const ctx = await browser.newContext();
    if (grant) await ctx.grantPermissions(['clipboard-read', 'clipboard-write'], { origin: 'http://127.0.0.1:' + port });
    const p = await ctx.newPage();
    p.on('pageerror', (e) => console.log('page error: ' + e.message));
    await p.goto('http://127.0.0.1:' + port + '/?wasm=probe.wasm&w=400&h=300');
    await p.waitForFunction(() => (window.firnFrames || 0) > 0, null, { timeout: 20000 });
    await p.evaluate(() => { localStorage.removeItem('webclip'); localStorage.removeItem('webclip_w'); });
    await p.click('canvas');
    return { ctx, p };
}

const answer = async (p, key) => {
    await p.evaluate(() => localStorage.removeItem('webclip'));
    await p.keyboard.press(key);
    await p.waitForFunction(() => localStorage.getItem('webclip') !== null, null, { timeout: 5000 }).catch(() => {});
    return p.evaluate(() => localStorage.getItem('webclip'));
};

(async () => {
    await new Promise((r) => server.listen(0, '127.0.0.1', r));
    const port = server.address().port;
    const browser = await chromium.launch();
    try {
        const { ctx, p } = await page(browser, true, port);
        const TEXT = 'text/plain;charset=utf-8';
        // 1. Firn copies, the browser's clipboard holds it
        await p.keyboard.press('c');
        await p.waitForTimeout(200);
        check('copy: web_clip_write says yes', await p.evaluate(() => localStorage.getItem('webclip_w')) === '1', null);
        const held = await p.evaluate(() => navigator.clipboard.readText());
        check('copy: the clipboard holds the UTF-8 text', held === 'Firn → Zwischenablage ✓', held);
        // 2. text from outside comes into Firn
        await p.evaluate(() => navigator.clipboard.writeText('von außen – 日本 😀'));
        let a = await answer(p, 'v');
        check('paste: text from outside arrives (status 1, type, UTF-8)', a === '1|' + TEXT + '|von außen – 日本 😀', a);
        // 3. a program's own type, through and back
        await p.keyboard.press('j');
        await p.waitForTimeout(300);
        check('own type: web_clip_write says yes', await p.evaluate(() => localStorage.getItem('webclip_w')) === '1', null);
        a = await answer(p, 'k');
        check('own type: comes back as application/x-openplan+json', a === '1|application/x-openplan+json|{"devices":["-K1","-Q2"]}', a);
        const asText = await p.evaluate(() => navigator.clipboard.readText());
        check('own type: other programs get it as text', asText === '{"devices":["-K1","-Q2"]}', asText);
        // 4. a type that is not there
        a = await answer(p, 'p');
        check('missing type: image/png answers status 0', a === '0|image/png|', a);
        await ctx.close();
        // 5. no permission to read: the page still gets its own copy
        const n = await page(browser, false, port);
        await n.p.keyboard.press('c');
        a = await answer(n.p, 'v');
        check('no read permission: the own copy comes back', a === '1|' + TEXT + '|Firn → Zwischenablage ✓', a);
        await n.ctx.close();
    } finally {
        await browser.close();
        server.close();
    }
    console.log(`webclip: ${passed} passed, ${failed} failed`);
    process.exit(failed ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(1); });
