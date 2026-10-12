#version 330 core
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/mc_vs.glsl -- vertex shader of the voxel back end (lib/gpu/gl_mc.fi).
// Input: the 8 byte mc_mesh vertex as two uints (lib/mc/mc_mesh.fi):
//   word0 = x | y<<8 | z<<16 | face<<24 | ao<<27      word1 = tile | u<<16 | v<<24
// Section origin minus camera position (computed in f64 on the CPU, so the f32 values stay small):
//   u_pool == 0 : u_origin (one draw call per section)
//   u_pool == 1 : the section is found from gl_VertexID (= index + base vertex = vertex number in the pool page):
//                 u_gran[vertex / 64] = slot number, u_org[slot] = origin (docs/mc/gpu-world.md). This is what lets ONE
//                 glMultiDrawElementsBaseVertex / Indirect call draw many sections without gl_DrawID (GL 4.6).
// u_rv = projection * view rotation (no translation), column-major.
layout(location = 0) in uvec2 a_w;
uniform mat4 u_rv;
uniform vec3 u_origin;
uniform int u_pool;
uniform usamplerBuffer u_gran;
uniform samplerBuffer u_org;
out vec3 v_uvao;       // tile repeat u, v (0..w, 0..h of the greedy quad) and ambient occlusion 0..3
flat out int v_tile;
flat out int v_fk;      // face brightness out of 256
out float v_depth;      // view depth (distance along the view direction), for fog
const int FK[8] = int[8](204, 204, 128, 256, 153, 153, 256, 256);
void main() {
    uint w0 = a_w.x;
    uint w1 = a_w.y;
    vec3 org = u_origin;
    if (u_pool != 0) {
        int slot = int(texelFetch(u_gran, gl_VertexID >> 6).r);
        org = texelFetch(u_org, slot).xyz;
    }
    vec3 p = vec3(float(w0 & 255u), float((w0 >> 8) & 255u), float((w0 >> 16) & 255u)) + org;
    gl_Position = u_rv * vec4(p, 1.0);
    v_depth = gl_Position.w;
    v_uvao = vec3(float((w1 >> 16) & 255u), float((w1 >> 24) & 255u), float((w0 >> 27) & 3u));
    v_tile = int(w1 & 65535u);
    v_fk = FK[int((w0 >> 24) & 7u)];
}
