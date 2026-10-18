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
out vec2 v_light;
out vec3 v_uvao;
flat out int v_tile;
flat out int v_fk;
flat out int v_tint;
out float v_depth;
void main() {
    gl_Position = u_rv * vec4(a_pos + u_origin, 1.0);
    v_depth = gl_Position.w;
    v_uvao = vec3(a_uvt.xy, a_lt.w);
    v_tile = int(a_uvt.z + 0.5);
    v_fk = int(a_uvt.w + 0.5);
    v_light = a_lt.xy;
    v_tint = int(a_lt.z); // exact: tint <= 0xFFFFFF fits a float, +0.5 would not
}
