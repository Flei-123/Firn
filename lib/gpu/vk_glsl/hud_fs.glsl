#version 450
#extension GL_GOOGLE_include_directive : require
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/vk_glsl/hud_fs.glsl -- Vulkan port of lib/gpu/gl_hud_fs.glsl: u_invert == 0: colour with its alpha as blend factor
// (SRC_ALPHA, ONE_MINUS_SRC_ALPHA); u_invert == 1: white with blend (ONE_MINUS_DST_COLOR, ZERO) = 255 - dst.
#include "vk_frame.glsl"
layout(location = 0) flat in vec4 v_col;
layout(location = 0) out vec4 o_color;
void main() {
    if (u_invert != 0) {
        o_color = vec4(1.0);
    } else {
        o_color = v_col;
    }
}
