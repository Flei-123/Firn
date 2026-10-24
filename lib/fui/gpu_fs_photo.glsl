#version 300 es
// SPDX-License-Identifier: MPL-2.0
// lib/fui/gpu_fs_photo.glsl -- the fragment shader of a PHOTO (gpu_draw_photo): the vertex shader
// of lib/fui/gpu.fi, and nothing else in the fragment stage but one filtered fetch. The big
// shader of gpu_fs.glsl (kind 16 there does the same) carries every other kind of quad and
// is slow to run on a full-screen photo where the card is weak; this one is as small as a
// shader can be.
precision highp float;
precision highp int;
precision highp sampler2D;
uniform vec2 u_size;
uniform vec2 u_srcsize;
uniform sampler2D u_src;
flat in vec4 v_c0;
flat in vec4 v_g;
flat in vec4 v_t;
flat in vec4 v_clip;
out vec4 o;

float ovl(float a0, float a1, float b0, float b1) { return max(0.0, min(a1, b1) - max(a0, b0)); }

vec4 box_average(vec2 t0, vec2 t1) {
    vec2 a0 = max(t0, vec2(0.0));
    vec2 a1 = min(t1, u_srcsize);
    if (a1.x <= a0.x || a1.y <= a0.y) return vec4(0.0);
    ivec2 c0 = ivec2(floor(a0));
    ivec2 c1 = ivec2(floor(a1 - vec2(0.0000001)));
    ivec2 lim = ivec2(u_srcsize) - ivec2(1);
    c1 = min(c1, lim);
    c0 = min(c0, c1);
    vec4 sum = vec4(0.0);
    float sw = 0.0;
    if ((c1.x - c0.x + 1) * (c1.y - c0.y + 1) <= 64) {
        for (int y = c0.y; y <= c1.y; y++) {
            float wy = ovl(float(y), float(y) + 1.0, a0.y, a1.y);
            for (int x = c0.x; x <= c1.x; x++) {
                float w = wy * ovl(float(x), float(x) + 1.0, a0.x, a1.x);
                sum += w * texelFetch(u_src, ivec2(x, y), 0);
                sw += w;
            }
        }
    } else {
        vec2 cell = (a1 - a0) / 8.0;
        for (int y = 0; y < 8; y++) {
            for (int x = 0; x < 8; x++) {
                sum += texture(u_src, (a0 + (vec2(float(x), float(y)) + 0.5) * cell) / u_srcsize);
                sw += 1.0;
            }
        }
    }
    return sum / sw;
}

void main() {
    vec2 p = vec2(gl_FragCoord.x, u_size.y - gl_FragCoord.y);
    vec2 i = floor(p);
    vec2 dd = v_g.zw - v_g.xy;
    if (dd.x <= 0.0 || dd.y <= 0.0) discard;
    float clipf = ovl(i.x, i.x + 1.0, v_clip.x, v_clip.z) * ovl(i.y, i.y + 1.0, v_clip.y, v_clip.w);
    float cov = ovl(i.x, i.x + 1.0, v_g.x, v_g.z) * ovl(i.y, i.y + 1.0, v_g.y, v_g.w) * clipf;
    if (cov <= 0.0) discard;
    vec2 uv = clamp((vec2(i.x + 0.5, i.y + 0.5) - v_g.xy) / dd, 0.0, 1.0);
    vec2 scale = (v_t.zw - v_t.xy) / dd;
    vec4 s;
    if (v_c0.b > 0.5 && (scale.x > 1.0 || scale.y > 1.0)) {
        vec2 t0 = mix(v_t.xy, v_t.zw, (i - v_g.xy) / dd);
        vec2 t1 = mix(v_t.xy, v_t.zw, (i + vec2(1.0) - v_g.xy) / dd);
        s = box_average(t0, t1);
    } else {
        s = texture(u_src, mix(v_t.xy, v_t.zw, uv) / u_srcsize);
    }
    o = s * (v_c0.a / 255.0 * min(cov, 1.0));
}
