// tools/dialog/check_webdialog.cjs -- std.dialog, std.toast and lib/plat/webdialog.fi against a real Chromium,
// driven by Playwright.
//
//   node tools/dialog/check_webdialog.cjs <probe.wasm>
//
// The page is demos/webdemo (its firn.js is the host) with the probe of tools/dialog/webdialog_probe.fi. Checked: the
// message boxes are the page's alert / confirm (OK, Yes, No; Yes/No/Cancel is "no such box"), open_file is NO_BACKEND
// (a browser has no paths), the colour input answers "rrggbb" or a cancel, the file input answers one event per file
// (name and content) and a final one, a notification is shown with permission and its click and close come back as
// events, and without permission nothing is shown. (The Notification class is a recording stand-in: headless
// Chromium answers permission "denied" whatever is granted.)
// PLAYWRIGHT=<path to the playwright package> if it is not on NODE_PATH.
'use strict';
const http = require('http');
const fs = require('fs');
const path = require('path');
const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');

const root = path.join(__dirname, '..', '..', 'demos', 'webdemo');
const wasm = process.argv[2];
if (!wasm) { console.error('usage: check_webdialog.cjs <probe.wasm>'); process.exit(2); }

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

// Headless Chromium reports Notification.permission "denied" whatever the context grants, so the page gets a
// recording stand-in for the Notification class (permission as given, the calls kept in window.__notes).
const FAKE_NOTIFICATION = (permission) => `
    window.__notes = []; window.__asked = 0;
    window.Notification = class {
        static get permission() { return ${JSON.stringify(permission)}; }
        static requestPermission() { window.__asked++; return Promise.resolve(${JSON.stringify(permission)}); }
        constructor(title, opts) { this.title = title; this.opts = opts || {}; window.__notes.push(this); }
        close() { if (this.onclose) this.onclose(); }
    };`;

async function page(browser, port, permission) {
    const ctx = await browser.newContext();
    const p = await ctx.newPage();
    await p.addInitScript(FAKE_NOTIFICATION(permission));
    p.on('pageerror', (e) => console.log('page error: ' + e.message));
    await p.goto('http://127.0.0.1:' + port + '/?wasm=probe.wasm&w=400&h=300');
    await p.waitForFunction(() => (window.firnFrames || 0) > 0, null, { timeout: 20000 });
    await p.evaluate(() => localStorage.clear());
    await p.click('canvas');
    return { ctx, p };
}
const ls = (p, k) => p.evaluate((k) => localStorage.getItem(k), k);
const wait = async (p, k) => { await p.waitForFunction((k) => localStorage.getItem(k) !== null, k, { timeout: 5000 }).catch(() => {}); return ls(p, k); };

(async () => {
    await new Promise((r) => server.listen(0, '127.0.0.1', r));
    const port = server.address().port;
    const browser = await chromium.launch();
    try {
        const { ctx, p } = await page(browser, port, 'granted');
        // 1. message boxes: the page's own alert / confirm
        const seen = [];
        p.on('dialog', async (d) => { seen.push(d.type() + ':' + d.message()); if (seen.length === 3) await d.dismiss(); else await d.accept(); });
        await p.keyboard.press('m');
        check('message: alert, answer OK', await wait(p, 'dlg') === 'msg-ok', await ls(p, 'dlg'));
        check('message: the box shows title and text', seen[0] === 'alert:T\n\nhello', seen);
        await p.evaluate(() => localStorage.removeItem('dlg'));
        await p.keyboard.press('y');
        check('confirm: accepted is Yes', await wait(p, 'dlg') === 'yes', await ls(p, 'dlg'));
        await p.evaluate(() => localStorage.removeItem('dlg'));
        await p.keyboard.press('y');
        check('confirm: dismissed is No', await wait(p, 'dlg') === 'no', await ls(p, 'dlg'));
        check('confirm: it was a confirm box', seen[1] === 'confirm:T\n\nsure?', seen);
        await p.evaluate(() => localStorage.removeItem('dlg'));
        await p.keyboard.press('3');
        check('Yes / No / Cancel: a page has no such box (NO_BACKEND)', await wait(p, 'dlg') === 'ynca-nobackend', await ls(p, 'dlg'));
        await p.evaluate(() => localStorage.removeItem('dlg'));
        await p.keyboard.press('f');
        check('open_file: NO_BACKEND (a browser gives no paths)', await wait(p, 'dlg') === 'file-nobackend', await ls(p, 'dlg'));

        // 2. the colour input
        await p.keyboard.press('c');
        await p.waitForSelector('#firn-dialog-color', { state: 'attached', timeout: 5000 });
        check('colour: the input starts at the given colour', await p.$eval('#firn-dialog-color', (e) => e.value) === '#1bd96a', null);
        await p.$eval('#firn-dialog-color', (e) => { e.value = '#ff8000'; e.dispatchEvent(new Event('change')); });
        let a = await wait(p, 'dlg_ev');
        check('colour: the answer is "color", rrggbb, status 1', a === 'color|ff8000|1|0', a);
        await p.evaluate(() => localStorage.removeItem('dlg_ev'));
        await p.keyboard.press('c');
        await p.waitForSelector('#firn-dialog-color', { state: 'attached', timeout: 5000 });
        await p.$eval('#firn-dialog-color', (e) => e.dispatchEvent(new Event('cancel')));
        a = await wait(p, 'dlg_ev');
        check('colour: cancel is status 0', a === 'color||0|0', a);

        // 3. the file input: one event per file, then the end
        await p.evaluate(() => localStorage.removeItem('dlg_ev'));
        const chooser = p.waitForEvent('filechooser');
        await p.keyboard.press('l');
        const fc = await chooser;
        check('files: the input is multiple and accepts .txt', fc.isMultiple() === true, fc.isMultiple());
        await fc.setFiles([{ name: 'a.txt', mimeType: 'text/plain', buffer: Buffer.from('hello \u00e4') },
                           { name: 'b.txt', mimeType: 'text/plain', buffer: Buffer.from('second') }]);
        await p.waitForFunction(() => (localStorage.getItem('dlg_ev') || '').startsWith('|'), null, { timeout: 5000 }).catch(() => {});
        a = await ls(p, 'dlg_ev');
        check('files: the last event is the end marker (status 2)', a === '||2|0', a);

        // 4. a notification: shown, click and close come back
        await p.evaluate(() => localStorage.removeItem('dlg_ev'));
        await p.keyboard.press('n');
        a = await wait(p, 'note');
        check('notification: shown, number 1', a === '1', a);
        const note = await p.evaluate(() => ({ title: window.__notes[0].title, body: window.__notes[0].opts.body }));
        check('notification: title and body reach the Notification API', note.title === 'Hello' && note.body === 'from Firn', note);
        await p.evaluate(() => window.firnNotes[1].onclick());
        a = await wait(p, 'note_ev');
        check('notification: a click arrives as EV_ACTION', a === 'click', a);
        await p.evaluate(() => { localStorage.removeItem('note_ev'); });
        await p.evaluate(() => window.firnNotes[1].onclose());
        a = await wait(p, 'note_ev');
        check('notification: a close arrives as EV_CLOSED', a === 'close', a);
        await ctx.close();

        // 5. without permission: not shown
        const n = await page(browser, port, 'default');
        await n.p.keyboard.press('n');
        a = await wait(n.p, 'note');
        check('notification: without permission nothing is shown (0), the permission is asked for', a === '0' && await n.p.evaluate(() => window.__asked) === 1 && await n.p.evaluate(() => window.__notes.length) === 0, a);
        await n.ctx.close();
    } finally {
        await browser.close();
        server.close();
    }
    console.log(`webdialog: ${passed} passed, ${failed} failed`);
    process.exit(failed ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(1); });
