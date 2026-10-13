#version 300 es
precision highp float;
precision highp int;
// ES 3.00 twin of gl_tint_fs.glsl for WebGL2 (docs/WEBGL2.md): body identical except where noted; keep in sync.
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/gl_tint_fs.glsl -- light tint of mc_sky.sky_tint_frame: multiplies (blend DST_COLOR, ZERO) every pixel that has
// geometry (depth < 1; the strip is drawn at depth 1.0 with GL_GREATER) by tint / 256.
uniform vec3 u_tint;
out vec4 o_color;
void main() {
    o_color = vec4(u_tint, 1.0);
}
