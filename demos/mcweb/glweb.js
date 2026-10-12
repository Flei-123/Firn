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
        const run = { text: '', exit_code: -1, gl: null, error: null };

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
        try {
            const { instance } = await WebAssembly.instantiate(bytes, { firn, env });
            memory = instance.exports.memory;
            instance.exports._start();
            run.exit_code = 0;
        } catch (e) {
            if (e instanceof Exit) run.exit_code = e.code;
            else { run.exit_code = 134; run.error = (e && e.stack) ? e.stack : String(e); }
        }
        return run;
    }

    root.startMcWeb = startMcWeb;
})(typeof window !== 'undefined' ? window : globalThis);
