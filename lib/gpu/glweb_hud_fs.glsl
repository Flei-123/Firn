#version 300 es
precision highp float;
precision highp int;
// ES 3.00 twin of gl_hud_fs.glsl for WebGL2 (docs/WEBGL2.md): body identical except where noted; keep in sync.
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/gl_hud_fs.glsl -- u_invert == 0: colour with its alpha as blend factor (SRC_ALPHA, ONE_MINUS_SRC_ALPHA);
// u_invert == 1: white with blend (ONE_MINUS_DST_COLOR, ZERO) = 255 - dst, like mc_hud.hud_invert.
uniform int u_invert;
flat in vec4 v_col;
out vec4 o_color;
void main() {
    if (u_invert != 0) {
        o_color = vec4(1.0);
    } else {
        o_color = v_col;
    }
}
