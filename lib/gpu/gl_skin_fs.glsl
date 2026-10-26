#version 330 core
// SPDX-License-Identifier: MPL-2.0
uniform sampler2D u_skin;
uniform vec4 u_fog;
uniform vec3 u_fogc;
in vec2 v_uv;
flat in int v_layer;
flat in vec3 v_tint;
flat in int v_fk;
in float v_depth;
out vec4 o_color;
void main() {
    ivec2 tc = ivec2(clamp(int(floor(v_uv.x)), 0, 63), v_layer * 64 + clamp(int(floor(v_uv.y)), 0, 63));
    vec4 t = texelFetch(u_skin, tc, 0);
    if (t.a < 0.5) discard;
    ivec3 tx = ivec3(t.rgb * 255.0 + 0.5);
    ivec3 tq = ivec3(clamp(v_tint, 0.0, 255.0)) + 1;
    ivec3 c = (tx * tq) >> 8;
    c = (c * v_fk) >> 8;
    if (u_fog.x > 0.5) {
        float tt = clamp((v_depth - u_fog.y) / (u_fog.z - u_fog.y), 0.0, 1.0);
        int f = int(floor(tt * 256.0));
        c = (c * (256 - f) + ivec3(u_fogc) * f) >> 8;
    }
    o_color = vec4(vec3(c) / 255.0, 1.0);
}
