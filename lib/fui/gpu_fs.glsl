#version 300 es
// SPDX-License-Identifier: MPL-2.0
// lib/fui/gpu_fs.glsl -- the fragment shader of lib/fui/gpu.fi: every kind of quad, per pixel
precision highp float;
precision highp int;
precision highp sampler2D;
uniform vec2 u_size;
uniform vec2 u_srcsize;
uniform sampler2D u_atlas;
uniform sampler2D u_stream;
uniform sampler2D u_src;
uniform vec4 u_spot[8];
uniform int u_nspot;
flat in vec4 v_c0;
flat in vec4 v_c1;
flat in vec4 v_g;
flat in vec4 v_p;
flat in vec4 v_t;
flat in vec4 v_clip;
flat in vec4 v_k;
out vec4 o;

float ovl(float a0, float a1, float b0, float b1) { return max(0.0, min(a1, b1) - max(a0, b0)); }

// canvas.blend_px: a straight colour (0..255) over, coverage f
vec4 blendf(vec4 c, float f) {
    float a = c.a / 255.0 * min(f, 1.0);
    if (a <= 0.0) discard;
    return vec4(c.rgb / 255.0 * a, a);
}

// effect.over: the alpha rounded to a whole octet first
vec4 over_int(vec4 c, float cov) {
    float a = floor(c.a * cov + 0.5);
    if (a <= 0.0) discard;
    if (a >= 255.0) return vec4(c.rgb / 255.0, 1.0);
    float k = a / 255.0;
    return vec4(c.rgb / 255.0 * k, k);
}

float edge_cov(float d, float a, float b) {
    float t = -d;
    float lo = (a + b) / 2.0;
    if (t <= -lo) return 0.0;
    if (t >= lo) return 1.0;
    float k = (b - a) / 2.0;
    if (a < 0.000001) return 0.5 + t / b;
    if (t < -k) { float s = t + lo; return s * s / (2.0 * a * b); }
    if (t > k) { float s = lo - t; return 1.0 - s * s / (2.0 * a * b); }
    return 0.5 + t / b;
}

float rbox_cov(vec2 p, vec2 c, vec2 hh, float r) {
    vec2 q = abs(p - c) - hh;
    float cov;
    if (q.x > 0.0 && q.y > 0.0) {
        float len = sqrt(q.x * q.x + q.y * q.y);
        float a = q.x / len;
        float b = q.y / len;
        if (a > b) { float t = a; a = b; b = t; }
        cov = edge_cov(len - r, a, b);
    } else {
        cov = 0.5 - (max(q.x, q.y) - r);
    }
    return clamp(cov, 0.0, 1.0);
}

float rbox_q(vec2 p, vec2 c, vec2 hh, float r) {
    vec2 q = abs(p - c) - hh;
    vec2 oq = max(q, vec2(0.0));
    float inside = min(max(q.x, q.y), 0.0);
    float big = max(oq.x, oq.y);
    float small = min(oq.x, oq.y);
    return big + 0.414 * small + inside - r;
}

ivec4 oct(vec4 v) { return ivec4(floor(v * 255.0 + 0.5)); }
vec4 unoct(ivec4 v) { return vec4(v) / 255.0; }

// a picture pixel of the sampled target (rows counted from the top)
ivec4 src_at(ivec2 p) {
    return oct(texelFetch(u_src, ivec2(p.x, int(u_srcsize.y) - 1 - p.y), 0));
}

ivec4 lerp4(ivec4 a, ivec4 b, int t) { return (a * (256 - t) + b * t + 128) >> 8; }

void main() {
    vec2 p = vec2(gl_FragCoord.x, u_size.y - gl_FragCoord.y);
    vec2 i = floor(p);
    int kind = int(v_k.x + 0.5);
    float clipf = ovl(i.x, i.x + 1.0, v_clip.x, v_clip.z) * ovl(i.y, i.y + 1.0, v_clip.y, v_clip.w);
    if (kind == 0) {
        vec4 r = vec4(max(v_g.x, v_clip.x), max(v_g.y, v_clip.y), min(v_g.z, v_clip.z), min(v_g.w, v_clip.w));
        float f = ovl(i.x, i.x + 1.0, r.x, r.z) * ovl(i.y, i.y + 1.0, r.y, r.w);
        if (f <= 0.0) discard;
        o = blendf(v_c0, f);
        return;
    }
    if (clipf <= 0.0) discard;
    if (kind == 1) {
        ivec2 t = ivec2(v_t.xy) + ivec2(i) - ivec2(v_t.zw);
        float cov = v_k.y > 0.5 ? texelFetch(u_stream, t, 0).r : texelFetch(u_atlas, t, 0).r;
        if (cov <= 0.0) discard;
        vec4 c = v_c0;
        if (v_k.z > 0.5) {
            float tt = clamp((i.y - v_p.x) / max(v_p.y - v_p.x, 0.0001), 0.0, 1.0);
            c = vec4(floor(v_c0.rgb + (v_c1.rgb - v_c0.rgb) * tt + 0.5), 255.0);
        }
        o = blendf(c, cov * clipf);
        return;
    }
    if (kind == 2) {
        float x = v_g.x; float y = v_g.y; float w = v_g.z; float h = v_g.w; float r = v_p.x;
        float pyc = i.y + 0.5;
        vec4 c = v_c0;
        if (v_c0 != v_c1) {
            float tt = clamp((pyc - y) / h, 0.0, 1.0);
            int k = clamp(int(floor(tt * 256.0 + 0.5)), 0, 256);
            ivec4 a = ivec4(v_c0); ivec4 b = ivec4(v_c1);
            c = vec4(a + (((b - a) * k) >> 8));
        }
        float fy = ovl(i.y, i.y + 1.0, y, y + h);
        bool corner = i.y < ceil(y + r) || i.y >= floor(y + h - r);
        if (corner && (i.x < ceil(x + r) || i.x >= floor(x + w - r))) {
            o = over_int(c, rbox_cov(vec2(i.x + 0.5, pyc), vec2(x + w / 2.0, y + h / 2.0), vec2(w / 2.0 - r, h / 2.0 - r), r));
            return;
        }
        if (i.x >= ceil(x) && i.x < floor(x + w) && fy >= 1.0 && c.a >= 255.0) {
            o = vec4(c.rgb / 255.0, 1.0);
            return;
        }
        o = over_int(c, ovl(i.x, i.x + 1.0, x, x + w) * fy);
        return;
    }
    if (kind == 3) {
        float x = v_g.x; float y = v_g.y; float w = v_g.z; float h = v_g.w;
        float r = v_p.x; float lift = v_p.y; float blur = v_p.z;
        float sy = y + lift;
        vec2 hh = vec2(w / 2.0 - r, h / 2.0 - r);
        float pyc = i.y + 0.5;
        bool deep = pyc > y + 1.0 && pyc < y + h - 1.0;
        if (deep && i.x >= ceil(x + r + 1.0) && i.x < floor(x + w - r - 1.0)) discard;
        vec2 pc = vec2(i.x + 0.5, pyc);
        if (rbox_q(pc, vec2(x + w / 2.0, y + h / 2.0), hh, r) < -1.0) discard;
        float d = rbox_q(pc, vec2(x + w / 2.0, sy + h / 2.0), hh, r);
        float t = (d + blur * 0.5) / (blur * 1.5);
        if (t >= 1.0) discard;
        t = max(t, 0.0);
        o = over_int(v_c0, 1.0 - t * t * (3.0 - 2.0 * t));
        return;
    }
    if (kind == 4) {
        float x = v_g.x; float y = v_g.y; float w = v_g.z; float h = v_g.w;
        int dir = int(v_p.x + 0.5);
        float u;
        if (dir == 0) u = ((i.y + 0.5) - y) / h;
        else if (dir == 1) u = (y + h - i.y - 0.5) / h;
        else if (dir == 2) u = ((i.x + 0.5) - x) / w;
        else u = (x + w - i.x - 0.5) / w;
        u = clamp(u, 0.0, 1.0);
        float k = 1.0 - u;
        float a = floor(v_c0.a * k * k + 0.5);
        if (a <= 0.0) discard;
        o = over_int(vec4(v_c0.rgb, 255.0), a / 255.0);
        return;
    }
    if (kind == 5) {
        float a = v_c0.a / 255.0;
        if (a <= 0.0) discard;
        o = vec4(v_c0.rgb / 255.0 * a, a);
        return;
    }
    if (kind == 6) {
        float y = v_g.y; float h = max(v_g.w, 1.0);
        float fy = i.y + 0.5; float fx = i.x + 0.5;
        float t = clamp((fy - y) / h, 0.0, 1.0);
        vec3 c = v_c0.rgb + (v_c1.rgb - v_c0.rgb) * t;
        for (int k = 0; k < 4; k++) {
            if (k >= u_nspot) break;
            vec4 s = u_spot[k * 2];
            vec4 s2 = u_spot[k * 2 + 1];
            float dx = fx - s.x; float dy = fy - s.y;
            float d2 = dx * dx * s.z + dy * dy * s.w;
            if (d2 < 1.0) {
                float uu = 1.0 - d2;
                float wg = uu * uu * s2.x;
                c = c + (s2.yzw - c) * wg;
            }
        }
        int bx = int(i.x) & 3; int by = int(i.y) & 3;
        int ord[16] = int[16](0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5);
        float d = (float(ord[by * 4 + bx]) + 0.5) / 16.0 - 0.5;
        o = vec4(clamp(floor(c + d + 0.5), 0.0, 255.0) / 255.0, 1.0);
        return;
    }
    if (kind == 7) {
        ivec2 t = ivec2(v_t.xy) + ivec2(i) - ivec2(v_t.zw);
        vec4 s;
        if (v_k.y > 0.5) s = texelFetch(u_src, ivec2(t.x, int(u_srcsize.y) - 1 - t.y), 0);
        else s = texelFetch(u_src, t, 0);
        float a = v_c0.a / 255.0;
        o = s * a;
        if (o.a <= 0.0 && o.r <= 0.0 && o.g <= 0.0 && o.b <= 0.0) discard;
        return;
    }
    if (kind == 8) {
        float x = v_g.x; float y = v_g.y; float w = v_g.z; float h = v_g.w; float r = v_p.x; float st = v_p.y;
        vec2 pc = vec2(i.x + 0.5, i.y + 0.5);
        float rr = min(r, min(w, h) / 2.0);
        float cov = rbox_cov(pc, vec2(x + w / 2.0, y + h / 2.0), vec2(w / 2.0 - rr, h / 2.0 - rr), rr);
        if (st > 0.0) {
            float iw = w - 2.0 * st; float ih = h - 2.0 * st;
            if (iw > 0.0 && ih > 0.0) {
                float ir = min(max(r - st, 0.0), min(iw, ih) / 2.0);
                cov = max(cov - rbox_cov(pc, vec2(x + w / 2.0, y + h / 2.0), vec2(iw / 2.0 - ir, ih / 2.0 - ir), ir), 0.0);
            }
        }
        if (cov <= 0.0) discard;
        vec4 c = v_c0;
        if (v_k.y > 0.5) {
            float dx = v_p.z; float dy = v_p.w;
            float span = w * abs(dx) + h * abs(dy);
            if (span <= 0.0) span = 1.0;
            float ox = dx < 0.0 ? x + w : x;
            float oy = dy < 0.0 ? y + h : y;
            float tt = clamp(((pc.x - ox) * dx + (pc.y - oy) * dy) / span, 0.0, 1.0);
            c = clamp(floor(v_c0 + (v_c1 - v_c0) * tt + 0.5), 0.0, 255.0);
        }
        o = blendf(c, cov * clipf);
        return;
    }
    if (kind == 9) {
        // frost_rect, step 5: the small band stretched back, integer lerp
        int f = int(v_p.x + 0.5); int sw = int(v_p.y + 0.5); int sh = int(v_p.z + 0.5);
        int xx = int(i.x - v_g.x); int yy = int(i.y - v_g.y);
        int fx = max(((xx * 2 + 1) * 128) / f - 128, 0);
        int c0 = fx >> 8; int wx = fx & 255;
        if (c0 >= sw - 1) { c0 = sw - 1; wx = 0; }
        int fy = max(((yy * 2 + 1) * 128) / f - 128, 0);
        int r0 = fy >> 8; int wy = fy & 255;
        if (r0 >= sh - 1) { r0 = sh - 1; wy = 0; }
        int r1 = min(r0 + 1, sh - 1);
        ivec4 a0 = lerp4(src_at(ivec2(c0, r0)), src_at(ivec2(c0, r1)), wy);
        if (wx == 0) { o = unoct(a0); return; }
        ivec4 a1 = lerp4(src_at(ivec2(c0 + 1, r0)), src_at(ivec2(c0 + 1, r1)), wy);
        o = unoct(lerp4(a0, a1, wx));
        return;
    }
    if (kind == 10) {
        // frost_rect, step 1: the mean of 2 x 2 pixels in each f x f block
        int f = int(v_p.x + 0.5); int iw = int(v_p.y + 0.5); int ih = int(v_p.z + 0.5);
        int off = f / 2 - f / 4;
        int sx = int(i.x); int sy = int(i.y);
        int ya = min(sy * f + off, ih - 1);
        int yb = f < 2 ? ya : min(ya + f / 2, ih - 1);
        int xa = min(sx * f + off, iw - 1);
        int xb = f < 2 ? xa : min(xa + f / 2, iw - 1);
        ivec2 b0 = ivec2(v_g.xy);
        ivec4 s = src_at(b0 + ivec2(xa, ya)) + src_at(b0 + ivec2(xb, ya)) + src_at(b0 + ivec2(xa, yb)) + src_at(b0 + ivec2(xb, yb));
        o = unoct((s + 2) >> 2);
        return;
    }
    if (kind >= 11 && kind <= 14) {
        // the box passes: blur_buffer ((sum + n/2) / n) and box2 (sum * inv >> 16)
        int r = int(v_p.x + 0.5); int w = int(v_p.y + 0.5); int h = int(v_p.z + 0.5);
        int n = 2 * r + 1;
        ivec2 c = ivec2(i);
        ivec4 sum = ivec4(0);
        for (int j = -64; j <= 64; j++) {
            if (j < -r) continue;
            if (j > r) break;
            ivec2 q = c;
            if (kind == 11 || kind == 13) q.x = clamp(c.x + j, 0, w - 1); else q.y = clamp(c.y + j, 0, h - 1);
            sum += src_at(q);
        }
        if (kind <= 12) { o = unoct((sum + n / 2) / n); return; }
        int inv = (65536 + n / 2) / n;
        o = unoct((sum * inv + 32768) >> 16);
        return;
    }
    if (kind == 15) {
        // frost_rect, step 3: the tint over the small band
        ivec4 v = src_at(ivec2(i));
        int ta = int(v_c0.a + 0.5);
        ivec4 tk = ivec4(ivec3(v_c0.rgb + 0.5) * ta, 255 * ta);
        ivec4 x = v * (255 - ta) + tk + 128;
        o = unoct((x + (x >> 8)) >> 8);
        return;
    }
    discard;
}
