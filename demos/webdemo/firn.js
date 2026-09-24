// SPDX-License-Identifier: MPL-2.0
// demos/webdemo/firn.js -- THE LOADER. The one file of this tree that is
// not Firn, and the reason is not a choice: a browser starts WebAssembly
// only from JavaScript. So this file does exactly what nothing else can do
// and nothing more -- it instantiates the module, answers its imports
// (lib/plat/web.fi and the runtime), puts the pixels the module hands over
// onto the <canvas>, passes the DOM's events in, and lends the module the
// browser's network (fetch, server-sent events), its address bar, its
// local storage and -- through one invisible text field -- its keyboard.
// Every decision is made in Firn (lib/plat/web.fi, lib/fui/*, the page).
//
// URL parameters, for tests: ?w=1240&h=720 fixes the size (CSS pixels),
// ?theme=light|dark overrides the system preference, ?wasm= and ?font=
// load other files (defaults: data-wasm / data-font of the <script> tag).
// `window.firnFrames` counts the pictures presented.
'use strict';

const firnTag = document.currentScript;
(async () => {
    const q = new URLSearchParams(location.search);
    const canvas = document.getElementById('firn');
    const g = canvas.getContext('2d', { alpha: false });
    const utf8 = new TextEncoder();
    const text = new TextDecoder();
    let mem = null, x = null; // the memory and the exports
    const bytes = (p, n) => new Uint8Array(mem.buffer, p >>> 0, n >>> 0);
    const str = (p, n) => text.decode(bytes(p, n).slice());
    const give = (v) => { // octets into the module: [address, length]
        const b = typeof v === 'string' ? utf8.encode(v) : v;
        if (!b.length) return [0, 0];
        const p = x.firn_web_alloc(b.length);
        bytes(p, b.length).set(b);
        return [p, b.length];
    };
    const put = (v, p, cap) => { // octets into a buffer of the module
        const b = utf8.encode(v || '').subarray(0, cap);
        bytes(p, b.length).set(b);
        return b.length;
    };
    class Exit { constructor(code) { this.code = code; } }

    // write(fd, buf, len): standard output and error go to the console.
    const lines = { 1: '', 2: '' };
    const dec = { 1: new TextDecoder(), 2: new TextDecoder() };
    const firn = {
        write(fd, p, n) {
            if (fd !== 1 && fd !== 2) return -9;
            lines[fd] += dec[fd].decode(bytes(p, n), { stream: true });
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
            return BigInt(Math.round((clk === 0 ? Date.now() : performance.now()) * 1e6));
        },
        random(p, n) {
            for (let k = 0; k < n; k += 65536) crypto.getRandomValues(bytes(p + k, Math.min(65536, n - k)));
            return n;
        },
        sleep_ns(ns) { // there is no sleeping on a page: wait it out
            const until = performance.now() + Number(ns) / 1e6;
            while (performance.now() < until) { /* spin */ }
            return 0;
        },
    };
    // THE FRAME CLOCK (lib/plat/web.fi, ONLY WHAT CHANGED): no loop of
    // our own. A frame is asked for when something came in and afterwards
    // only while the module answers FRAME_AGAIN (bit 1) or set a timer --
    // a page that sits still costs nothing, not even a call per frame. A
    // module without firn_web_clock gets the old loop.
    let raf = 0, wakeT = 0, wakeAt = 0, onDemand = false;
    const tick = (t) => {
        raf = 0;
        const r = x.firn_web_frame(t);
        if (!onDemand || (r & 2)) frame();
    };
    const frame = () => { if (!raf) raf = requestAnimationFrame(tick); };
    const ev = (r) => { frame(); return r; };
    let frames = 0, es = null;
    const ta = document.createElement('textarea'); // the keyboard's door
    // THE PICTURE, wrapped once: an ImageData over the module's own memory
    // (no copy), made again only when the memory grew (its buffer is then a
    // new one) or the canvas got another size.
    let img = null;
    const image = (p, w, h) => {
        if (!img || img.data.buffer !== mem.buffer || img.firnP !== p || img.width !== w || img.height !== h) {
            img = new ImageData(new Uint8ClampedArray(mem.buffer, p >>> 0, w * h * 4), w, h);
            img.firnP = p;
        }
        return img;
    };
    const env = {
        firn_web_present(p, w, h) {
            if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
            g.putImageData(image(p, w, h), 0, 0);
            window.firnFrames = ++frames;
        },
        // Only the rectangle that changed goes over (the dirty rectangle of
        // putImageData); a canvas that got a new size takes the whole picture.
        firn_web_present_rect(p, w, h, rx, ry, rw, rh) {
            if (canvas.width !== w || canvas.height !== h) return env.firn_web_present(p, w, h);
            g.putImageData(image(p, w, h), 0, 0, rx, ry, rw, rh);
            window.firnFrames = ++frames;
        },
        // A timer of the page: a frame in `ms` milliseconds (the earliest wins).
        firn_web_wake(ms) {
            const at = performance.now() + ms;
            if (wakeT && at >= wakeAt) return;
            if (wakeT) clearTimeout(wakeT);
            wakeAt = at;
            wakeT = setTimeout(() => { wakeT = 0; frame(); }, Math.max(0, ms));
        },
        firn_web_fetch(id, mp, mn, up, un, bp, bn) {
            const o = { method: str(mp, mn) || 'GET', credentials: 'same-origin', cache: 'no-store',
                headers: { Accept: 'application/json' } };
            if (bn) { o.body = bytes(bp, bn).slice(); o.headers['Content-Type'] = 'application/json'; }
            fetch(str(up, un), o).then(async (r) => [r.status, new Uint8Array(await r.arrayBuffer())])
                .catch(() => [0, new Uint8Array(0)])
                .then(([st, b]) => ev(x.firn_web_fetch_done(id, st, ...give(b))));
        },
        firn_web_stream_open(up, un) {
            if (es) es.close();
            const s = es = new EventSource(str(up, un));
            const on = (name) => s.addEventListener(name, (e) => { if (s === es) ev(x.firn_web_stream_event(...give(name), ...give(e.data || ''))); });
            ['ready', 'msg', 'ping'].forEach(on);
            s.onopen = () => { if (s === es) ev(x.firn_web_stream_state(1)); };
            s.onerror = () => { if (s !== es) return; s.close(); es = null; ev(x.firn_web_stream_state(0)); };
        },
        firn_web_stream_close() { if (es) { es.close(); es = null; } },
        firn_web_navigate(up, un) { location.assign(str(up, un)); },
        firn_web_keyboard(show, cx, cy, cw, ch) {
            Object.assign(ta.style, { left: cx + 'px', top: cy + 'px', width: cw + 'px', height: ch + 'px' });
            if (show) { if (document.activeElement !== ta) ta.focus({ preventScroll: true }); } else if (document.activeElement === ta) canvas.focus({ preventScroll: true });
        },
        firn_web_location(p, cap) { return put(location.pathname + location.search, p, cap); },
        firn_web_store_get(kp, kn, p, cap) { try { return put(localStorage.getItem(str(kp, kn)), p, cap); } catch (e) { return 0; } },
        firn_web_store_set(kp, kn, p, n) { try { localStorage.setItem(str(kp, kn), str(p, n)); } catch (e) { /* private mode */ } },
        firn_web_cursor(k) { canvas.style.cursor = ['default', 'pointer', 'text'][k] || 'default'; },
    };

    const wasm = q.get('wasm') || firnTag.dataset.wasm || 'gallery9.wasm';
    const { instance } = await WebAssembly.instantiate(await (await fetch(wasm)).arrayBuffer(), { firn, env });
    x = instance.exports;
    mem = x.memory;
    onDemand = typeof x.firn_web_clock === 'function' && x.firn_web_clock() >= 2;
    try {
        x._start();
    } catch (e) {
        if (!(e instanceof Exit)) throw e;
        if (e.code !== 0) console.error(`firn: main ended with exit code ${e.code}`);
    }

    // The font. A page has no files, so the host fetches it and hands the
    // octets over; the module keeps them.
    const font = new Uint8Array(await (await fetch(q.get('font') || firnTag.dataset.font || 'DejaVuSans.ttf')).arrayBuffer());
    const fp = x.firn_web_alloc(font.length);
    bytes(fp, font.length).set(font);
    if (!ev(x.firn_web_font(fp, font.length))) console.error('firn: the font was refused');

    // The size: CSS pixels, the device ratio in per mille, bit 0 = dark.
    const fixed = q.has('w') && q.has('h');
    const resize = () => {
        // The VISUAL viewport where there is one: on a phone it is what
        // is left above the on-screen keyboard.
        const vv = window.visualViewport;
        const w = fixed ? +q.get('w') : Math.round(vv ? vv.width : window.innerWidth);
        const h = fixed ? +q.get('h') : Math.round(vv ? vv.height : window.innerHeight);
        canvas.style.width = w + 'px';
        canvas.style.height = h + 'px';
        const dark = q.has('theme') ? q.get('theme') === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches;
        ev(x.firn_web_resize(w, h, Math.round(devicePixelRatio * 1000), dark ? 1 : 0));
    };
    resize();
    addEventListener('resize', resize);
    if (window.visualViewport) visualViewport.addEventListener('resize', resize);
    // In the background or back: an event of its own on the stream path
    // (name "vis", data "0" hidden / "1" visible), sent at once when the
    // page starts hidden. A page that does not know the name ignores it.
    const vis = () => ev(x.firn_web_stream_event(...give('vis'), ...give(document.hidden ? '0' : '1')));
    document.addEventListener('visibilitychange', vis);
    if (document.hidden) vis();
    // The window's focus (name "focus", "0" lost / "1" back -- a caret
    // stops blinking) and the system's wish for less motion (name "motion",
    // "1" = prefers-reduced-motion), the same way; each sent at once when
    // it is not the ordinary state.
    // (Each asks for a frame: a caret that may blink again needs one.)
    const said = (n, v) => ev(x.firn_web_stream_event(...give(n), ...give(v)));
    addEventListener('blur', () => said('focus', '0'));
    addEventListener('focus', () => said('focus', '1'));
    if (!document.hasFocus()) said('focus', '0');
    const rm = matchMedia('(prefers-reduced-motion: reduce)');
    const motion = () => said('motion', rm.matches ? '1' : '0');
    if (rm.addEventListener) rm.addEventListener('change', motion);
    if (rm.matches) motion();

    // The events, as they come. Bit 8 of the buttons: a finger or a pen.
    const at = (e) => [e.offsetX, e.offsetY, e.buttons | (e.pointerType === 'mouse' ? 0 : 256)];
    canvas.addEventListener('pointermove', (e) => ev(x.firn_web_pointer(0, ...at(e))));
    canvas.addEventListener('pointerdown', (e) => {
        canvas.setPointerCapture(e.pointerId);
        if (document.activeElement !== ta) canvas.focus({ preventScroll: true });
        ev(x.firn_web_pointer(1, ...at(e)));
    });
    canvas.addEventListener('pointerup', (e) => ev(x.firn_web_pointer(2, ...at(e))));
    canvas.addEventListener('pointercancel', (e) => ev(x.firn_web_pointer(2, ...at(e))));
    canvas.addEventListener('pointerleave', (e) => ev(x.firn_web_pointer(3, ...at(e))));
    canvas.addEventListener('wheel', (e) => { e.preventDefault(); ev(x.firn_web_wheel(e.deltaX, e.deltaY, e.deltaMode)); }, { passive: false });
    const mods = (e) => (e.shiftKey ? 1 : 0) | (e.ctrlKey ? 2 : 0) | (e.altKey ? 4 : 0) | (e.metaKey ? 8 : 0);
    const key = (down) => (e) => {
        if (e.isComposing || e.key === 'Process' || e.key === 'Unidentified') return;
        // Printable keys in the text field arrive as text (input below).
        if (e.target === ta && [...e.key].length === 1 && !e.ctrlKey && !e.metaKey) return;
        const k = utf8.encode(e.key).subarray(0, 255);
        const p = x.firn_web_scratch();
        bytes(p, k.length).set(k);
        if (ev(x.firn_web_key(down, p, k.length, mods(e)))) e.preventDefault();
    };
    for (const t of [canvas, ta]) { t.addEventListener('keydown', key(1)); t.addEventListener('keyup', key(0)); }

    // THE TEXT FIELD. Invisible, but real: a phone shows its keyboard for
    // it, an input method composes in it, paste lands in it. It holds one
    // marker character; what stands behind the marker is new text, a
    // marker that is gone was a Backspace.
    const MARK = '​';
    // Off the screen until the page places it (firn_web_keyboard puts it over
    // the field, so the tap that follows lands on it and keeps the focus).
    // Unplaced, the invisible box used to sit over the top left corner of the
    // canvas and swallowed the taps there -- FirnChat's back arrow on a phone
    // (24.09.2026).
    Object.assign(ta.style, { position: 'fixed', opacity: 0, border: 0, padding: 0, resize: 'none',
        fontSize: '16px', background: 'transparent', color: 'transparent', caretColor: 'transparent',
        left: '-10000px', top: '0px', width: '1px', height: '1px' });
    ta.setAttribute('autocapitalize', 'sentences'); ta.value = MARK;
    document.body.appendChild(ta);
    const flush = () => {
        const v = ta.value;
        if (!v.startsWith(MARK)) env.firn_web_key_name('Backspace');
        const t = v.split(MARK).join('');
        if (t) ev(x.firn_web_text(...give(t)));
        ta.value = MARK;
        ta.setSelectionRange(1, 1);
    };
    env.firn_web_key_name = (name) => {
        const k = utf8.encode(name), p = x.firn_web_scratch();
        bytes(p, k.length).set(k);
        ev(x.firn_web_key(1, p, k.length, 0));
    };
    ta.addEventListener('input', (e) => { if (!e.isComposing) flush(); });
    ta.addEventListener('compositionend', flush);

    // The first frame; every later one is asked for (THE FRAME CLOCK above).
    frame();
})().catch((e) => {
    console.error(e);
    document.body.dataset.error = String(e);
});
