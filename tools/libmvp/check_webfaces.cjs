'use strict';
const http = require('http');
const fs = require('fs');
const path = require('path');
const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');

const root = path.join(__dirname, '..', '..', 'demos', 'webdemo');
const [lazyWasm, embeddedWasm, fixtures, expectPdf, expectDocx] = process.argv.slice(2);
if (!expectDocx) { console.error('usage: check_webfaces.cjs <lazy.wasm> <embedded.wasm> <fixture dir> <pdf line> <docx line>'); process.exit(2); }

const types = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.ttf': 'font/ttf' };
const files = {
    '/lazy.wasm': lazyWasm,
    '/embedded.wasm': embeddedWasm,
    '/a.pdf': path.join(fixtures, 'a.pdf'),
    '/a.docx': path.join(fixtures, 'a.docx'),
};
let log = [];
const page = (wasm, attrs) => `<!doctype html><html><head><meta charset="utf-8"></head><body><canvas id="firn" tabindex="0"></canvas>` +
    `<script src="/firn.js" data-wasm="${wasm}" data-assets="/a.pdf /a.docx" ${attrs}></script></body></html>`;
const server = http.createServer((req, res) => {
    const u = new URL(req.url, 'http://x');
    log.push(u.pathname);
    if (u.pathname.startsWith('/t-')) {
        const [, wasm, ...rest] = u.pathname.slice(1).split('-');
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(page('/' + wasm + '.wasm', decodeURIComponent(rest.join('-'))));
        return;
    }
    const file = files[u.pathname] || path.join(root, u.pathname);
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
const packHits = () => log.filter((l) => l.endsWith('.pack')).length;

async function session(browser, port, tag, wasm, attrs) {
    return Promise.race([sessionRun(browser, port, tag, wasm, attrs), sleep(60000).then(() => ({ tag, errors: ['timeout'], atStart: -1, afterPdf: -1, afterDocx: -1, pdf: 'timeout', docx: 'timeout', stats: null }))]);
}

async function sessionRun(browser, port, tag, wasm, attrs) {
    log = [];
    const ctx = await browser.newContext();
    const p = await ctx.newPage();
    const errors = [];
    p.on('pageerror', (e) => errors.push(e.message));
    await p.goto('http://127.0.0.1:' + port + '/t-' + wasm + '-' + encodeURIComponent(attrs) + '?w=600&h=400');
    await p.waitForFunction(() => (window.firnFrames || 0) > 0, null, { timeout: 20000 });
    const atStart = packHits();
    await p.click('canvas');
    const run = async (key, store) => {
        await p.keyboard.press(key);
        await p.waitForFunction((k) => localStorage.getItem(k) !== null, store, { timeout: 30000 });
        return p.evaluate((k) => localStorage.getItem(k), store);
    };
    const pdf = await run('p', 'webfaces-pdf');
    const afterPdf = packHits();
    const docx = await run('d', 'webfaces-docx');
    const afterDocx = packHits();
    const stats = await p.evaluate(() => window.firnFaceStats || null);
    await p.keyboard.press('t');
    await p.waitForFunction(() => localStorage.getItem('webfaces-clock') !== null, null, { timeout: 30000 });
    const clock = await p.evaluate(() => localStorage.getItem('webfaces-clock'));
    await ctx.close();
    return { tag, atStart, afterPdf, afterDocx, pdf, docx, stats, errors, clock };
}

(async () => {
    await new Promise((r) => server.listen(0, '127.0.0.1', r));
    const port = server.address().port;
    const browser = await chromium.launch();

    const lazy = await session(browser, port, 'lazy', 'lazy', '');
    check('lazy: no page error', lazy.errors.length === 0, lazy.errors);
    check('lazy: font pack is not requested at startup', lazy.atStart === 0, lazy.atStart);
    check('lazy: font pack fetched on the first PDF', lazy.afterPdf === 1, lazy.afterPdf);
    check('lazy: font pack fetched once for PDF and DOCX', lazy.afterDocx === 1, lazy.afterDocx);
    check('lazy: PDF page identical to native', lazy.pdf === expectPdf, { got: lazy.pdf, want: expectPdf });
    check('lazy: DOCX page identical to native', lazy.docx === expectDocx, { got: lazy.docx, want: expectDocx });
    check('lazy: host loaded 11 faces into the module', lazy.stats && lazy.stats.reads === 11, lazy.stats);

    const [c0, cd] = (lazy.clock || '0|0').split('|').map(Number);
    check('clock: wasm micros() advances in step with the wall clock (' + cd + ' us for a 30000 us wait)', c0 > 0 && cd >= 30000 && cd < 300000, lazy.clock);

    const emb = await session(browser, port, 'embedded', 'embedded', '');
    check('embedded: no font pack request', emb.afterDocx === 0, emb.afterDocx);
    check('embedded: PDF page identical to native', emb.pdf === expectPdf, { got: emb.pdf, want: expectPdf });
    check('embedded: DOCX page identical to native', emb.docx === expectDocx, { got: emb.docx, want: expectDocx });

    const pre = await session(browser, port, 'prefetch', 'lazy', 'data-faces-prefetch="yes"');
    check('prefetch: pack requested at startup', pre.atStart === 1, pre.atStart);
    check('prefetch: PDF identical', pre.pdf === expectPdf, { got: pre.pdf });
    check('prefetch: DOCX identical', pre.docx === expectDocx, { got: pre.docx });

    const miss = await session(browser, port, 'missing', 'lazy', 'data-faces="/nothing.pack"');
    check('missing pack: no page error, no hang', miss.errors.length === 0 && miss.pdf !== 'timeout' && miss.docx !== 'timeout', miss);
    check('missing pack: DOCX is refused cleanly, PDF opens without text', miss.docx === 'open-failed' && miss.pdf !== 'open-failed', miss);
    check('missing pack: output differs (no text drawn)', miss.pdf !== expectPdf, miss.pdf);

    await browser.close();
    server.close();
    console.log(passed + ' passed, ' + failed + ' failed');
    process.exit(failed ? 1 : 0);
})();
