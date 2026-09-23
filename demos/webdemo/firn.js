// SPDX-License-Identifier: MPL-2.0
// demos/webdemo/firn.js -- THE LOADER. The one file of this tree that is
// not Firn, and the reason is not a choice: a browser starts WebAssembly
// only from JavaScript. So this file does exactly what nothing else can do
// and nothing more -- it instantiates the module, answers its imports
// (the six calls lib/plat/web.fi and the runtime need from a host), puts
// the pixels the module hands over onto the <canvas>, and passes the DOM's
// events in. Every decision -- what a key means, what the wheel scrolls,
// when to paint -- is made in Firn (lib/plat/web.fi, lib/fui/*).
//
// URL parameters, for tests: ?w=1240&h=720 fixes the size (CSS pixels)
// instead of following the window, ?theme=light|dark overrides the
// system preference. `window.firnFrames` counts the pictures presented.
'use strict';

(async () => {
    const q = new URLSearchParams(location.search);
    const canvas = document.getElementById('firn');
    const g = canvas.getContext('2d', { alpha: false });
    const utf8 = new TextEncoder();
    const text = new TextDecoder();
    let mem = null;
    let x = null; // the exports
    const bytes = (p, n) => new Uint8Array(mem.buffer, p >>> 0, n >>> 0);
    class Exit { constructor(code) { this.code = code; } }

    // write(fd, buf, len): standard output and error go to the console,
    // one line per console call.
    const lines = { 1: '', 2: '' };
    const firn = {
        write(fd, p, n) {
            if (fd !== 1 && fd !== 2) return -9;
            lines[fd] += text.decode(bytes(p, n));
            let i;
            while ((i = lines[fd].indexOf('\n')) >= 0) {
                (fd === 1 ? console.log : console.error)(lines[fd].slice(0, i));
                lines[fd] = lines[fd].slice(i + 1);
            }
            return n;
        },
        read() { return 0; }, // a page has no standard input: end of file
        exit(code) { throw new Exit(code); },
        clock_ns(clk) {
            const ms = clk === 0 ? Date.now() : performance.now();
            return BigInt(Math.round(ms * 1e6));
        },
        random(p, n) {
            for (let k = 0; k < n; k += 65536) {
                crypto.getRandomValues(bytes(p + k, Math.min(65536, n - k)));
            }
            return n;
        },
        sleep_ns(ns) { // there is no sleeping on a page: wait it out
            const until = performance.now() + Number(ns) / 1e6;
            while (performance.now() < until) { /* spin */ }
            return 0;
        },
    };
    let frames = 0;
    const env = {
        firn_web_present(p, w, h) {
            if (canvas.width !== w || canvas.height !== h) {
                canvas.width = w;
                canvas.height = h;
            }
            const px = new Uint8ClampedArray(mem.buffer, p >>> 0, w * h * 4);
            g.putImageData(new ImageData(px, w, h), 0, 0);
            window.firnFrames = ++frames;
        },
    };

    const wasm = q.get('wasm') || 'gallery9.wasm';
    const { instance } = await WebAssembly.instantiate(
        await (await fetch(wasm)).arrayBuffer(), { firn, env });
    x = instance.exports;
    mem = x.memory;
    try {
        x._start();
    } catch (e) {
        if (!(e instanceof Exit)) throw e;
        if (e.code !== 0) console.error(`firn: main ended with exit code ${e.code}`);
    }

    // The font. A page has no files, so the host fetches it and hands the
    // octets over; the module keeps them.
    const font = new Uint8Array(await (await fetch(q.get('font') || 'DejaVuSans.ttf')).arrayBuffer());
    const fp = x.firn_web_alloc(font.length);
    bytes(fp, font.length).set(font);
    if (!x.firn_web_font(fp, font.length)) console.error('firn: the font was refused');

    // The size: CSS pixels, the device ratio in per mille, bit 0 = dark.
    const fixed = q.has('w') && q.has('h');
    const resize = () => {
        const w = fixed ? +q.get('w') : window.innerWidth;
        const h = fixed ? +q.get('h') : window.innerHeight;
        canvas.style.width = w + 'px';
        canvas.style.height = h + 'px';
        const dark = q.has('theme') ? q.get('theme') === 'dark'
            : matchMedia('(prefers-color-scheme: dark)').matches;
        x.firn_web_resize(w, h, Math.round(devicePixelRatio * 1000), dark ? 1 : 0);
    };
    resize();
    addEventListener('resize', resize);

    // The events, as they come.
    const at = (e) => [e.offsetX, e.offsetY];
    canvas.addEventListener('pointermove', (e) => x.firn_web_pointer(0, ...at(e), e.buttons));
    canvas.addEventListener('pointerdown', (e) => {
        canvas.setPointerCapture(e.pointerId);
        canvas.focus();
        x.firn_web_pointer(1, ...at(e), e.buttons);
    });
    canvas.addEventListener('pointerup', (e) => x.firn_web_pointer(2, ...at(e), e.buttons));
    canvas.addEventListener('pointerleave', (e) => x.firn_web_pointer(3, ...at(e), e.buttons));
    canvas.addEventListener('wheel', (e) => {
        e.preventDefault();
        x.firn_web_wheel(e.deltaX, e.deltaY, e.deltaMode);
    }, { passive: false });
    const key = (down) => (e) => {
        const k = utf8.encode(e.key).subarray(0, 255);
        const p = x.firn_web_scratch();
        bytes(p, k.length).set(k);
        if (x.firn_web_key(down, p, k.length, e.shiftKey ? 1 : 0)) e.preventDefault();
    };
    canvas.addEventListener('keydown', key(1));
    canvas.addEventListener('keyup', key(0));

    // The frame loop: the module decides whether there is anything to paint.
    const tick = (t) => {
        x.firn_web_frame(t);
        requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
})().catch((e) => {
    console.error(e);
    document.body.dataset.error = String(e);
});
