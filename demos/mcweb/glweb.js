// SPDX-License-Identifier: MPL-2.0
// demos/mcweb/glweb.js -- THE HOST OF lib/gpu/glweb.fi (docs/WEBGL2.md).
//
// A WebAssembly module cannot reach the GPU by itself. lib/gpu/glweb.fi declares `firn_glw_*` imports (module `env`);
// this file answers each of them with the WebGL2 call of the same name, 1:1 and WITHOUT program logic -- the shading,
// the meshes, the camera and the Backend table all live in Firn. What is here is plumbing a browser forces on a module:
// the table of WebGL objects (the Firn side holds small integers), memory views that are made anew after every
// memory.grow, the shader log copied back into the module's memory, and the six `firn.*` imports every
// wasm32-browser program has (tools/wasm/run.mjs is the same set for node).
//
//     const run = await startMcWeb({ canvas, wasm: 'mcscene.wasm', onPresent, onText });
//     // run.exit_code, run.text (everything the program wrote), run.gl (renderer / version strings)
(function (root) {
    'use strict';

    class Exit { constructor(code) { this.code = code; } }

    async function startMcWeb(opts) {
        const canvas = opts.canvas;
        const dec = new TextDecoder();
        let memory = null;
        let gl = null;
        const O = [null];                       // WebGL objects; Firn holds the index, 0 = none / default framebuffer
        const obj = (o) => { O.push(o); return O.length - 1; };
        const get = (id) => (id ? O[id] : null);
        const del = (id) => { if (id) O[id] = null; };
        const u8 = (p, n) => new Uint8Array(memory.buffer, p >>> 0, n >>> 0);
        const str = (p, n) => dec.decode(u8(p, n));
        const t0 = performance.now();
        const run = { text: '', exit_code: -1, gl: null, error: null, loop: false, frames: 0, ctx_lost_count: 0, ctx_restored_count: 0, stop: null };
        let ctxLost = false;                    // between webglcontextlost and webglcontextrestored
        let ctxGen = 0;                         // +1 per restore; the Firn side recreates its objects when it changes
        let instance = null;

        const firn = {
            write(fd, p, n) {
                if (fd !== 1 && fd !== 2) return -9;
                const s = str(p, n);
                run.text += s;
                if (opts.onText) opts.onText(s, fd);
                return n;
            },
            read() { return 0; },
            exit(code) { throw new Exit(code); },
            clock_ns(clk) {
                if (clk === 0) return BigInt(Date.now()) * 1000000n;
                return BigInt(Math.round((performance.now() - t0) * 1e6)) + 1n;
            },
            random(p, n) { crypto.getRandomValues(u8(p, n)); return n; },
            sleep_ns() { return 0; },
        };

        const glw = {
            firn_glw_init(w, h, flags) {
                canvas.width = w; canvas.height = h;
                gl = canvas.getContext('webgl2', {
                    alpha: false, antialias: false, depth: false, stencil: false,
                    preserveDrawingBuffer: true, powerPreference: 'high-performance',
                });
                if (!gl) return 0;
                canvas.addEventListener('webglcontextlost', (e) => {
                    e.preventDefault();         // allows the browser to restore the context later
                    ctxLost = true; run.ctx_lost_count++;
                    if (opts.onContext) opts.onContext('lost');
                });
                canvas.addEventListener('webglcontextrestored', () => {
                    O.length = 1;               // every id from before is dead
                    ctxLost = false; ctxGen++; run.ctx_restored_count++;
                    if (opts.onContext) opts.onContext('restored');
                });
                const info = gl.getExtension('WEBGL_debug_renderer_info');
                run.gl = {
                    version: gl.getParameter(gl.VERSION),
                    renderer: info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER),
                    glsl: gl.getParameter(gl.SHADING_LANGUAGE_VERSION),
                };
                return 1;
            },
            firn_glw_size(w, h) { canvas.width = w; canvas.height = h; },
            firn_glw_program(vp, vn, fp, fnn, lp, lcap) {
                const mk = (type, src) => {
                    const s = gl.createShader(type);
                    gl.shaderSource(s, src); gl.compileShader(s);
                    return s;
                };
                const vs = mk(gl.VERTEX_SHADER, str(vp, vn));
                const fs = mk(gl.FRAGMENT_SHADER, str(fp, fnn));
                const p = gl.createProgram();
                gl.attachShader(p, vs); gl.attachShader(p, fs); gl.linkProgram(p);
                const ok = gl.getProgramParameter(p, gl.LINK_STATUS);
                if (!ok) {
                    const log = 'vs: ' + gl.getShaderInfoLog(vs) + ' fs: ' + gl.getShaderInfoLog(fs) + ' link: ' + gl.getProgramInfoLog(p);
                    run.error = log;
                    if (lp && lcap > 1) {
                        const b = new TextEncoder().encode(log).subarray(0, lcap - 1);
                        const m = u8(lp, lcap); m.set(b); m[b.length] = 0;
                    }
                    return 0;
                }
                gl.deleteShader(vs); gl.deleteShader(fs);
                return obj(p);
            },
            firn_glw_delete_program(p) { gl.deleteProgram(get(p)); del(p); },
            firn_glw_use_program(p) { gl.useProgram(get(p)); },
            firn_glw_uniform_location(p, np, nn) {
                const l = gl.getUniformLocation(get(p), str(np, nn));
                return l ? obj(l) : -1;
            },
            firn_glw_ctx_state() { return ctxLost || (gl && gl.isContextLost()) ? 1 : 0; },
            firn_glw_ctx_gen() { return ctxGen; },
            firn_glw_uniform1f(l, a) { if (l >= 0) gl.uniform1f(O[l], a); },
            firn_glw_attrib_pointer(i, size, ty, norm, stride, off) { gl.vertexAttribPointer(i, size, ty, !!norm, stride, off); },
            firn_glw_attrib_divisor(i, d) { gl.vertexAttribDivisor(i, d); },
            firn_glw_blend_func(s, d) { gl.blendFunc(s, d); },
            firn_glw_draw_arrays(mode, first, count) { gl.drawArrays(mode, first, count); },
            firn_glw_draw_arrays_instanced(mode, first, count, n) { gl.drawArraysInstanced(mode, first, count, n); },
            firn_glw_draw_elements_instanced(mode, count, ty, off, n) { gl.drawElementsInstanced(mode, count, ty, off, n); },
            firn_glw_uniform1i(l, v) { if (l >= 0) gl.uniform1i(O[l], v); },
            firn_glw_uniform2f(l, a, b) { if (l >= 0) gl.uniform2f(O[l], a, b); },
            firn_glw_uniform3f(l, a, b, c) { if (l >= 0) gl.uniform3f(O[l], a, b, c); },
            firn_glw_uniform4f(l, a, b, c, d) { if (l >= 0) gl.uniform4f(O[l], a, b, c, d); },
            firn_glw_uniform_matrix4fv(l, p) {
                // slice: the module's f32 array need not be 4-aligned for a Float32Array view
                if (l >= 0) gl.uniformMatrix4fv(O[l], false, new Float32Array(memory.buffer.slice(p >>> 0, (p >>> 0) + 64)));
            },
            firn_glw_create_buffer() { return obj(gl.createBuffer()); },
            firn_glw_bind_buffer(t, b) { gl.bindBuffer(t, get(b)); },
            firn_glw_buffer_data(t, p, n, usage) { gl.bufferData(t, p ? u8(p, n) : n, usage); },
            firn_glw_delete_buffer(b) { gl.deleteBuffer(get(b)); del(b); },
            firn_glw_create_vao() { return obj(gl.createVertexArray()); },
            firn_glw_bind_vao(v) { gl.bindVertexArray(get(v)); },
            firn_glw_delete_vao(v) { gl.deleteVertexArray(get(v)); del(v); },
            firn_glw_enable_attrib(i) { gl.enableVertexAttribArray(i); },
            firn_glw_attrib_ipointer(i, size, ty, stride, off) { gl.vertexAttribIPointer(i, size, ty, stride, off); },
            firn_glw_create_texture() { return obj(gl.createTexture()); },
            firn_glw_bind_texture(t, x) { gl.bindTexture(t, get(x)); },
            firn_glw_active_texture(u) { gl.activeTexture(u); },
            firn_glw_tex_image_2d(t, l, ifmt, w, h, fmt, ty, p) {
                gl.texImage2D(t, l, ifmt, w, h, 0, fmt, ty, p ? u8(p, w * h * 4) : null);
            },
            firn_glw_tex_image_3d(t, l, ifmt, w, h, d, fmt, ty, p) {
                gl.texImage3D(t, l, ifmt, w, h, d, 0, fmt, ty, p ? u8(p, w * h * d * 4) : null);
            },
            firn_glw_tex_parameteri(t, pn, v) { gl.texParameteri(t, pn, v); },
            firn_glw_pixel_storei(pn, v) { gl.pixelStorei(pn, v); },
            firn_glw_delete_texture(x) { gl.deleteTexture(get(x)); del(x); },
            firn_glw_create_framebuffer() { return obj(gl.createFramebuffer()); },
            firn_glw_bind_framebuffer(t, f) { gl.bindFramebuffer(t, get(f)); },
            firn_glw_framebuffer_texture_2d(t, at, tt, x, l) { gl.framebufferTexture2D(t, at, tt, get(x), l); },
            firn_glw_framebuffer_renderbuffer(t, at, rbt, rb) { gl.framebufferRenderbuffer(t, at, rbt, get(rb)); },
            firn_glw_check_framebuffer_status(t) { return gl.checkFramebufferStatus(t); },
            firn_glw_delete_framebuffer(f) { gl.deleteFramebuffer(get(f)); del(f); },
            firn_glw_create_renderbuffer() { return obj(gl.createRenderbuffer()); },
            firn_glw_bind_renderbuffer(t, rb) { gl.bindRenderbuffer(t, get(rb)); },
            firn_glw_renderbuffer_storage(t, fmt, w, h) { gl.renderbufferStorage(t, fmt, w, h); },
            firn_glw_delete_renderbuffer(rb) { gl.deleteRenderbuffer(get(rb)); del(rb); },
            firn_glw_blit_framebuffer(a, b, c, d, e, f, g, h, m, fl) { gl.blitFramebuffer(a, b, c, d, e, f, g, h, m, fl); },
            firn_glw_viewport(x, y, w, h) { gl.viewport(x, y, w, h); },
            firn_glw_enable(c) { gl.enable(c); },
            firn_glw_disable(c) { gl.disable(c); },
            firn_glw_depth_func(f) { gl.depthFunc(f); },
            firn_glw_depth_mask(b) { gl.depthMask(!!b); },
            firn_glw_color_mask(r, g, b, a) { gl.colorMask(!!r, !!g, !!b, !!a); },
            firn_glw_cull_face(m) { gl.cullFace(m); },
            firn_glw_front_face(m) { gl.frontFace(m); },
            firn_glw_clear_color(r, g, b, a) { gl.clearColor(r, g, b, a); },
            firn_glw_clear(m) { gl.clear(m); },
            firn_glw_draw_elements(mode, count, ty, off) { gl.drawElements(mode, count, ty, off); },
            firn_glw_read_pixels(x, y, w, h, p) { gl.readPixels(x, y, w, h, gl.RGBA, gl.UNSIGNED_BYTE, u8(p, w * h * 4)); },
            firn_glw_flush() { gl.flush(); },
            firn_glw_finish() { gl.finish(); },
            firn_glw_get_error() { return gl ? gl.getError() : 0; },
            // the frame is on the canvas now (glweb_show): a test takes its snapshot here
            firn_glw_present(w, h) { if (opts.onPresent) opts.onPresent(canvas, w, h); },
        };

        const bytes = opts.bytes || await (await fetch(opts.wasm)).arrayBuffer();
        const env = new Proxy(glw, {
            get(t, name) {
                if (name in t) return t[name];
                return () => { throw new Error('the program imports env.' + String(name) + ', which this host does not provide'); };
            },
        });
        // ---- input: Pointer Lock + keyboard + mouse -> exports of lib/input/webraw.fi (same events as lib/input/raw) ----------
        glw.firn_in_lock = (on) => {
            if (on) { if (canvas.requestPointerLock) canvas.requestPointerLock(); }
            else if (document.exitPointerLock) document.exitPointerLock();
        };
        try {
            const r = await WebAssembly.instantiate(bytes, { firn, env });
            instance = r.instance;
            memory = instance.exports.memory;
            instance.exports._start();
            run.exit_code = 0;
        } catch (e) {
            if (e instanceof Exit) run.exit_code = e.code;
            else { run.exit_code = 134; run.error = (e && e.stack) ? e.stack : String(e); }
            // _start ends in exit(main's result): a clean exit(0) of a program that exports firn_mc_frame keeps the instance alive for the frame loop
            if (!(run.exit_code === 0 && instance && instance.exports && typeof instance.exports.firn_mc_frame === 'function')) return run;
        }
        const x = instance.exports;
        if (typeof x.firn_mc_frame !== 'function') return run;   // one-shot program (gate 131 scenes)

        // ---- frame loop: requestAnimationFrame -> firn_mc_frame(ms) -> 0 = stop, else keep going ----
        run.loop = true;
        // opts.timerLoop (tests): setTimeout instead of requestAnimationFrame -- headless Chromium with --virtual-time-budget starves rAF (4 callbacks in 30 s)
        const sched = opts.timerLoop ? (f) => setTimeout(() => f(performance.now()), 0) : (f) => requestAnimationFrame(f);
        const unsched = opts.timerLoop ? (id) => clearTimeout(id) : (id) => cancelAnimationFrame(id);
        let raf = 0, stopped = false;
        const guard = (f) => (...a) => {
            if (stopped) return;
            try { f(...a); } catch (e) {
                if (e instanceof Exit) run.exit_code = e.code; else { run.exit_code = 134; run.error = (e && e.stack) ? e.stack : String(e); }
                stop();
            }
        };
        // ---- gamepads (lib/input/webpad.fi): navigator.getGamepads() is polled once per frame, standard mapping only ----
        const padOn = [false, false, false, false];
        const pollPads = () => {
            if (typeof x.firn_pad_state !== 'function' || !navigator.getGamepads) return;
            const list = navigator.getGamepads();
            for (let i = 0; i < 4; i++) {
                const g = list && list[i];
                if (g && g.connected && g.mapping === 'standard') {
                    let m = 0;
                    for (let b = 0; b < 17 && b < g.buttons.length; b++) if (g.buttons[b].pressed) m |= (1 << b);
                    const tv = (b) => (g.buttons[b] ? Math.round(g.buttons[b].value * 1000) : 0);
                    const ax = (a) => Math.round(Math.max(-1, Math.min(1, g.axes[a] || 0)) * 1000);
                    x.firn_pad_state(i, m >>> 0, tv(6), tv(7), ax(0), ax(1), ax(2), ax(3));
                    padOn[i] = true;
                } else if (padOn[i]) {
                    x.firn_pad_gone(i); padOn[i] = false;
                }
            }
        };
        const tick = guard((ms) => {
            raf = 0;
            pollPads();
            const r = x.firn_mc_frame(ms);
            run.frames++;
            if (r !== 0 && !stopped) raf = sched(tick);
            else stop();
        });
        const dprMilli = () => Math.round((window.devicePixelRatio || 1) * 1000);
        const sendResize = guard(() => {
            if (typeof x.firn_mc_resize !== 'function') return;
            const r = canvas.getBoundingClientRect();
            x.firn_mc_resize(Math.max(1, Math.round(r.width)), Math.max(1, Math.round(r.height)), dprMilli());
        });
        const cleanup = [];
        const on = (t, ev, f, o) => { t.addEventListener(ev, f, o); cleanup.push(() => t.removeEventListener(ev, f, o)); };
        function stop() {
            if (stopped) return;
            stopped = true;
            if (raf) unsched(raf);
            for (const c of cleanup) c();
            if (opts.onStop) opts.onStop(run);
        }
        run.stop = stop;
        run.pump = () => { if (!raf && !stopped) raf = sched(tick); };   // restart after a paused loop

        if (typeof ResizeObserver !== 'undefined') {
            const ro = new ResizeObserver(sendResize); ro.observe(canvas); cleanup.push(() => ro.disconnect());
        }
        if (window.matchMedia) {                // devicePixelRatio changes (zoom, monitor switch)
            const mq = window.matchMedia('(resolution: ' + (window.devicePixelRatio || 1) + 'dppx)');
            const h = () => sendResize();
            if (mq.addEventListener) { mq.addEventListener('change', h); cleanup.push(() => mq.removeEventListener('change', h)); }
        }
        if (typeof x.firn_in_key === 'function' && opts.input !== false) {
            const dpr = () => (window.devicePixelRatio || 1);
            const KEYS = run.keymap = SCANCODES;
            const sym = (e) => (e.key && e.key.length === 1) ? e.key.codePointAt(0) : 0;
            on(window, 'keydown', guard((e) => {
                const sc = KEYS[e.code]; if (sc === undefined || e.repeat) { if (sc !== undefined && opts.captureKeys) e.preventDefault(); return; }
                x.firn_in_key(1, sc, sym(e), e.timeStamp >>> 0);
                if (opts.captureKeys) e.preventDefault();
            }));
            on(window, 'keyup', guard((e) => {
                const sc = KEYS[e.code]; if (sc === undefined) return;
                x.firn_in_key(0, sc, sym(e), e.timeStamp >>> 0);
                if (opts.captureKeys) e.preventDefault();
            }));
            const pos = (e) => { const r = canvas.getBoundingClientRect(); return [Math.round((e.clientX - r.left) * dpr()), Math.round((e.clientY - r.top) * dpr())]; };
            on(canvas, 'mousedown', guard((e) => { const p = pos(e); x.firn_in_button(1, e.button + 1, p[0], p[1]); if (opts.lockOnClick && !document.pointerLockElement && canvas.requestPointerLock) canvas.requestPointerLock(); }));
            on(window, 'mouseup', guard((e) => { const p = pos(e); x.firn_in_button(0, e.button + 1, p[0], p[1]); }));
            on(window, 'mousemove', guard((e) => { const p = pos(e); x.firn_in_motion(p[0], p[1], Math.round(e.movementX * dpr()), Math.round(e.movementY * dpr())); }));
            on(canvas, 'wheel', guard((e) => { x.firn_in_wheel(e.deltaY < 0 ? 1 : (e.deltaY > 0 ? -1 : 0), e.deltaX < 0 ? -1 : (e.deltaX > 0 ? 1 : 0)); e.preventDefault(); }), { passive: false });
            on(window, 'blur', guard(() => x.firn_in_focus(0)));
            on(window, 'focus', guard(() => x.firn_in_focus(1)));
            on(document, 'pointerlockchange', guard(() => x.firn_in_lock_state(document.pointerLockElement === canvas ? 1 : 0)));
            on(canvas, 'contextmenu', (e) => { if (document.pointerLockElement === canvas) e.preventDefault(); });
            // for tests: events as plain calls (no DOM event needed)
            run.input = {
                key: (down, code, key) => x.firn_in_key(down, KEYS[code], key ? key.codePointAt(0) : 0, (performance.now() | 0) >>> 0),
                button: (down, b, px, py) => x.firn_in_button(down, b, px, py),
                motion: (px, py, dx, dy) => x.firn_in_motion(px, py, dx, dy),
                wheel: (dy, dx) => x.firn_in_wheel(dy, dx || 0),
                lock_state: (on_) => x.firn_in_lock_state(on_ ? 1 : 0),
            };
        }
        sendResize();
        raf = sched(tick);
        return run;
    }

    // KeyboardEvent.code -> Linux scancode (linux/input-event-codes.h), the numbers lib/input/raw uses on every system.
    const SCANCODES = {
        Escape: 1, Digit1: 2, Digit2: 3, Digit3: 4, Digit4: 5, Digit5: 6, Digit6: 7, Digit7: 8, Digit8: 9, Digit9: 10, Digit0: 11,
        Minus: 12, Equal: 13, Backspace: 14, Tab: 15, KeyQ: 16, KeyW: 17, KeyE: 18, KeyR: 19, KeyT: 20, KeyY: 21, KeyU: 22, KeyI: 23,
        KeyO: 24, KeyP: 25, BracketLeft: 26, BracketRight: 27, Enter: 28, ControlLeft: 29, KeyA: 30, KeyS: 31, KeyD: 32, KeyF: 33,
        KeyG: 34, KeyH: 35, KeyJ: 36, KeyK: 37, KeyL: 38, Semicolon: 39, Quote: 40, Backquote: 41, ShiftLeft: 42, Backslash: 43,
        KeyZ: 44, KeyX: 45, KeyC: 46, KeyV: 47, KeyB: 48, KeyN: 49, KeyM: 50, Comma: 51, Period: 52, Slash: 53, ShiftRight: 54,
        NumpadMultiply: 55, AltLeft: 56, Space: 57, CapsLock: 58, F1: 59, F2: 60, F3: 61, F4: 62, F5: 63, F6: 64, F7: 65, F8: 66,
        F9: 67, F10: 68, F11: 87, F12: 88, ControlRight: 97, AltRight: 100, Home: 102, ArrowUp: 103, PageUp: 104, ArrowLeft: 105,
        ArrowRight: 106, End: 107, ArrowDown: 108, PageDown: 109, Insert: 110, Delete: 111, MetaLeft: 125, MetaRight: 126,
    };

    root.startMcWeb = startMcWeb;
})(typeof window !== 'undefined' ? window : globalThis);
