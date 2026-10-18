#version 330 core
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/mc_model_vs.glsl -- vertex shader of the MODEL quad pass of the voxel back end (lib/gpu/gl_mc.fi, MESH_MODEL / MESH_MODEL_T).
// Float vertices (12 per vertex): pos xyz (section relative, block units) | u v tile fk | sky block tint ao. The default voxel program
// (mc_vs.glsl / mc_fs.glsl) is untouched; this pair only differs by the float input and the tint.
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec4 a_uvt;   // u, v in tile units, tile id, face brightness 0..256
layout(location = 2) in vec4 a_lt;    // sky, block light 0..15, tint (R | G << 8 | B << 16, -1 none), ao 0..3
uniform mat4 u_rv;
uniform vec3 u_origin;
uniform vec4 u_fade;     // x = fade a, y = inverted, z = frame height, w = frame width (vertex snap, r261)
out vec2 v_light;
out vec3 v_uvao;
flat out int v_tile;
flat out int v_fk;
flat out int v_tint;
out float v_depth;
void main() {
    gl_Position = u_rv * vec4(a_pos + u_origin, 1.0);
    v_depth = gl_Position.w;
    // r261: snap the vertex to the 1/16 pixel grid of lib/mc/mc_raster.fi (snap()), so llvmpipe / lavapipe / a GPU (1/256 or 1/4096 px) see the
    // same triangle corners as the CPU rasterizer. u_fade.w = frame width in pixels (0 = off); u_fade.z = frame height.
    if (u_fade.w > 0.5 && gl_Position.w > 0.0001) {
        vec2 hs = vec2(u_fade.w, u_fade.z) * 0.5;
        vec2 ndc = gl_Position.xy / gl_Position.w;
        vec2 sc = floor(vec2(hs.x + ndc.x * hs.x, hs.y - ndc.y * hs.y) * 16.0 + 0.5) * 0.0625;
        // r265 tie bias: llvmpipe (GL, y-up FBO) resolves exact ties (pixel centre on an edge) the other way than the CPU raster / lavapipe (top-left). All
        // vertices move up by 1/256 px (llvmpipe sub-pixel grid): ties then fall the CPU way. Measured lodseam S (settled shot): GL vs exact CPU 27.4 -> 36.5 dB.
        // Only the vertices at a pixel centre line (16 * y mod 16 == 8) give 31.1 dB, so diagonal ties matter too.
        sc.y -= 0.00390625;
        gl_Position.xy = vec2((sc.x - hs.x) / hs.x, (hs.y - sc.y) / hs.y) * gl_Position.w;
    }
    v_uvao = vec3(a_uvt.xy, a_lt.w);
    v_tile = int(a_uvt.z + 0.5);
    v_fk = int(a_uvt.w + 0.5);
    v_light = a_lt.xy;
    v_tint = int(a_lt.z); // exact: tint <= 0xFFFFFF fits a float, +0.5 would not
}
