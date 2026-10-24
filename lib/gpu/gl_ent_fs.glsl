#version 330 core
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/gl_ent_fs.glsl -- entity box shading = mc_raster shading with ao = 3 (fq = 256): channel * face_k >> 8, then fog.
uniform vec4 u_fog;     // x = 1 fog on, y = start, z = end
uniform vec3 u_fogc;    // fog colour 0..255
flat in vec3 v_col;
flat in int v_fk;
flat in float v_em;
in float v_depth;
out vec4 o_color;
void main() {
    ivec3 c = (ivec3(v_col + 0.5) * v_fk) >> 8;
    if (u_fog.x > 0.5 && v_em < 0.5) {
        float tt = clamp((v_depth - u_fog.y) / (u_fog.z - u_fog.y), 0.0, 1.0);
        int f = int(floor(tt * 256.0));
        c = (c * (256 - f) + ivec3(u_fogc) * f) >> 8;
    }
    o_color = vec4(vec3(c) / 255.0, 1.0);
}
