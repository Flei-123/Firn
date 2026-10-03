// SPDX-License-Identifier: MPL-2.0
// lib/web/dom.js -- THE HOST OF lib/web/dom.fi. Loads a Firn module built
// with --target=wasm32-browser and lends it the page's DOM through handles.
// No program logic lives here: every line is plumbing between the module
// and the browser (the six runtime imports, a node table, event delivery).
//
//   <script src="dom.js" data-wasm="app.wasm"></script>
//
// The module's `main` runs once the page is parsed; when it returns, the
// module stays alive and its handlers (dom.on) are called on events.
// `window.firnDom` = { ready: Promise, exit: code or null, instance } for tests.
'use strict';

(() => {
    const tag = document.currentScript;
    const wasmUrl = new URLSearchParams(location.search).get('wasm') || tag.dataset.wasm || 'app.wasm';
    const enc = new TextEncoder();
    const dec = new TextDecoder();
    let mem = null, x = null;
    const bytes = (p, n) => new Uint8Array(mem.buffer, p >>> 0, n >>> 0);
    const str = (p, n) => (n ? dec.decode(bytes(p, n).slice()) : '');
    class Exit { constructor(code) { this.code = code; } }

    // ---- the runtime's six imports (as tools/wasm/run.mjs, firn.js)
    const lines = { 1: '', 2: '' };
    const sdec = { 1: new TextDecoder(), 2: new TextDecoder() };
    const firn = {
        write(fd, p, n) {
            if (fd !== 1 && fd !== 2) return -9;
            lines[fd] += sdec[fd].decode(bytes(p, n), { stream: true });
            let i;
            while ((i = lines[fd].indexOf('\n')) >= 0) {
                (fd === 1 ? console.log : console.error)(lines[fd].slice(0, i));
                lines[fd] = lines[fd].slice(i + 1);
            }
            return n;
        },
        read() { return 0; },
        exit(code) { throw new Exit(code); },
        clock_ns(clk) { return BigInt(Math.round((clk === 0 ? Date.now() : performance.now()) * 1e6)); },
        random(p, n) {
            for (let k = 0; k < n; k += 65536) crypto.getRandomValues(bytes(p + k, Math.min(65536, n - k)));
            return n;
        },
        sleep_ns(ns) {
            const until = performance.now() + Number(ns) / 1e6;
            while (performance.now() < until) { /* a page cannot sleep */ }
            return 0;
        },
    };

    // ---- the node table: handle -> node; 0 is "none"
    const nodes = [null];
    const free = [];
    const keep = (n) => {
        if (!n) return 0;
        const h = free.length ? free.pop() : nodes.length;
        nodes[h] = n;
        return h;
    };
    const node = (h) => nodes[h] || null;
    const drop = (h) => { if (h > 0 && nodes[h]) { nodes[h] = null; free.push(h); } };

    let held = new Uint8Array(0); // the last text asked for (firn_dom_get / firn_js_eval)
    const hold = (s) => {
        if (s === null || s === undefined) { held = new Uint8Array(0); return -1; }
        held = enc.encode(String(s));
        return held.length;
    };
    let ev = null; // the event being delivered

    const env = {
        firn_dom_query(sp, sn) { return keep(document.querySelector(str(sp, sn))); },
        firn_dom_query_in(h, sp, sn) { const n = node(h); return n ? keep(n.querySelector(str(sp, sn))) : 0; },
        firn_dom_create(tp, tn) { return keep(document.createElement(str(tp, tn))); },
        firn_dom_append(p, c) { const a = node(p), b = node(c); if (a && b) a.appendChild(b); },
        firn_dom_remove(h) { const n = node(h); if (n) n.remove(); drop(h); },
        firn_dom_clear(h) { const n = node(h); if (n) n.replaceChildren(); },
        firn_dom_release(h) { drop(h); },
        firn_dom_set_text(h, p, n) { const e = node(h); if (e) e.textContent = str(p, n); },
        firn_dom_set_value(h, p, n) { const e = node(h); if (e) e.value = str(p, n); },
        firn_dom_set_attr(h, kp, kn, vp, vn) { const e = node(h); if (e) e.setAttribute(str(kp, kn), str(vp, vn)); },
        firn_dom_set_style(h, kp, kn, vp, vn) {
            const e = node(h); if (!e) return;
            const k = str(kp, kn), v = str(vp, vn);
            if (v) e.style.setProperty(k, v); else e.style.removeProperty(k);
        },
        firn_dom_class(h, np, nn, on) { const e = node(h); if (e) e.classList.toggle(str(np, nn), !!on); },
        firn_dom_get(h, what, kp, kn) {
            switch (what) {
                case 0: { const e = node(h); return hold(e ? e.textContent : null); }
                case 1: { const e = node(h); return hold(e ? e.value : null); }
                case 2: { const e = node(h); return hold(e ? e.getAttribute(str(kp, kn)) : null); }
                case 3: return hold(ev && ev.key !== undefined ? ev.key : null);
                case 4: return hold(ev && ev.target && ev.target.value !== undefined ? ev.target.value : null);
            }
            return hold(null);
        },
        firn_dom_take(dst, cap) {
            const n = Math.min(cap, held.length);
            bytes(dst, n).set(held.subarray(0, n));
            return n;
        },
        firn_dom_listen(h, ep, en, id, prevent) {
            const e = node(h); if (!e) return;
            e.addEventListener(str(ep, en), (event) => {
                if (prevent) event.preventDefault();
                ev = event;
                try { x.firn_dom_event(id, h); }
                catch (err) { if (err instanceof Exit) window.firnDom.exit = err.code; else console.error(err); }
                finally { ev = null; }
            });
        },
        firn_js_eval(p, n) {
            // The program's own code (see dom.eval); indirect eval = global scope.
            try { return hold((0, eval)(str(p, n))); } catch (e) { console.error(e); return hold(null); }
        },
    };

    const state = { exit: null, instance: null, ready: null };
    window.firnDom = state;
    state.ready = (async () => {
        if (document.readyState === 'loading') await new Promise((r) => document.addEventListener('DOMContentLoaded', r, { once: true }));
        const resp = await fetch(wasmUrl);
        if (!resp.ok) throw new Error(`${wasmUrl}: ${resp.status}`);
        const { instance } = await WebAssembly.instantiate(await resp.arrayBuffer(), { firn, env });
        state.instance = instance;
        x = instance.exports;
        mem = x.memory;
        try {
            x._start();
            state.exit = 0;
        } catch (e) {
            if (!(e instanceof Exit)) throw e;
            state.exit = e.code;
        }
        if (state.exit) console.error(`firn: main returned ${state.exit}`);
        return state.exit;
    })();
})();
