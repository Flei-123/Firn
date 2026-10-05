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

void main() {
    vec2 p = vec2(gl_FragCoord.x, u_size.y - gl_FragCoord.y);
    vec2 i = floor(p);
    vec2 dd = v_g.zw - v_g.xy;
    if (dd.x <= 0.0 || dd.y <= 0.0) discard;
    float clipf = ovl(i.x, i.x + 1.0, v_clip.x, v_clip.z) * ovl(i.y, i.y + 1.0, v_clip.y, v_clip.w);
    float cov = ovl(i.x, i.x + 1.0, v_g.x, v_g.z) * ovl(i.y, i.y + 1.0, v_g.y, v_g.w) * clipf;
    if (cov <= 0.0) discard;
    vec2 uv = clamp((vec2(i.x + 0.5, i.y + 0.5) - v_g.xy) / dd, 0.0, 1.0);
    vec4 s = texture(u_src, mix(v_t.xy, v_t.zw, uv) / u_srcsize);
    o = s * (v_c0.a / 255.0 * min(cov, 1.0));
}
