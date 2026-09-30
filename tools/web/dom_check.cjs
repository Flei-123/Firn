// SPDX-License-Identifier: MPL-2.0
// tools/web/dom_check.cjs -- drives examples/dom (lib/web/dom.fi + dom.js)
// in a headless Chromium with real mouse and keyboard input and checks
// what the page's DOM holds afterwards.  node dom_check.cjs <url>
const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');
(async () => {
    const url = process.argv[2];
    const browser = await chromium.launch({ executablePath: process.env.CHROMIUM || '/usr/bin/chromium', args: ['--no-sandbox'] });
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', (e) => errors.push(String(e)));
    page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
    let pass = 0, fail = 0;
    const check = (ok, what) => { console.log(`${ok ? 'PASS' : 'FAIL'} ${what}`); ok ? pass++ : fail++; };
    await page.goto(url);
    const exit = await page.evaluate(() => window.firnDom.ready);
    check(exit === 0, `main ran and returned 0 (got ${exit})`);
    check(await page.textContent('h1') === 'Firn -> DOM', 'h1 created from Firn');
    check(await page.textContent('#count') === 'clicked 0 times', 'button with its text');
    for (let i = 0; i < 3; i++) await page.click('#count');
    check(await page.textContent('#count') === 'clicked 3 times', 'three real clicks reach the Firn handler (f"..." text)');
    await page.click('#todo');
    await page.keyboard.type('milk');
    await page.keyboard.press('Enter');
    await page.fill('#todo', 'bread äöü €');
    await page.click('#add');
    await page.fill('#todo', '<b>no html</b>');
    await page.keyboard.press('Enter');
    await page.click('#add'); // empty input: nothing added
    const items = await page.$$eval('#list li', (l) => l.map((e) => e.textContent));
    check(JSON.stringify(items) === JSON.stringify(['milk', 'bread äöü €', '<b>no html</b>']), `list = ${JSON.stringify(items)}`);
    check(await page.$$eval('#list b', (l) => l.length) === 0, 'text is never parsed as HTML');
    check(await page.inputValue('#todo') === '', 'input cleared by Firn (set_value)');
    check(await page.$eval('#list', (e) => e.style.fontFamily) === 'sans-serif', 'set_style');
    const ua = await page.getAttribute('body', 'data-ua');
    check(typeof ua === 'string' && ua.startsWith('Mozilla'), `dom.eval result back in Firn (${ua})`);
    for (let i = 0; i < 200; i++) await page.click('#count');
    check(await page.textContent('#count') === 'clicked 203 times', '200 more clicks, no drift');
    // 5000 more items through the handlers (synthetic events): every one
    // reads the input into a GC string -- the collector runs between events.
    const n = await page.evaluate(() => {
        const inp = document.getElementById('todo');
        for (let i = 0; i < 5000; i++) {
            inp.value = 'item ' + i + ' ' + 'x'.repeat(i % 300);
            inp.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
        }
        const li = document.querySelectorAll('#list li');
        return [li.length, li[li.length - 1].textContent.slice(0, 9), inp.value];
    });
    check(n[0] === 5003 && n[1] === 'item 4999' && n[2] === '', `5000 items through the collector (${JSON.stringify(n)})`);
    check(errors.length === 0, `no page errors ${errors.length ? JSON.stringify(errors) : ''}`);
    await browser.close();
    console.log(`dom: ${pass}/${pass + fail}`);
    process.exit(fail ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(2); });
