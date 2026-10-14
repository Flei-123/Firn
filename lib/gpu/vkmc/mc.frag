#version 450
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/vkmc/mc.frag -- Vulkan twin of lib/gpu/mc_fs.glsl (u_filter == 0, one atlas array): the INTEGER shading of lib/mc/mc_raster.fi.
layout(set = 0, binding = 0) uniform U {
    mat4 rv;
    vec4 fog;
    vec4 fogc;
    vec4 tiles;
} u;
layout(set = 0, binding = 1) uniform sampler2DArray u_atlas;
layout(push_constant) uniform P {
    vec4 org;
    vec4 fade;
    vec4 light;
} pc;
layout(location = 0) in vec2 v_light;
layout(location = 1) in vec3 v_uvao;
layout(location = 2) flat in int v_tile;
layout(location = 3) flat in int v_fk;
layout(location = 4) in float v_depth;
layout(location = 0) out vec4 o_color;
void main() {
    int tsz = int(u.tiles.x);
    int tl = v_tile - 1;
    int tx = int(floor(v_uvao.x * float(tsz))) & (tsz - 1);
    int ty = int(floor(v_uvao.y * float(tsz))) & (tsz - 1);
    ivec4 t = ivec4(texelFetch(u_atlas, ivec3(tx, ty, tl), 0) * 255.0 + 0.5);
    bool trans = pc.org.w > 0.5;
    if (trans ? t.a < 1 : t.a < 128) discard;
    if (pc.fade.x < 63.5) {
        int px = int(gl_FragCoord.x);
        int py = int(gl_FragCoord.y);
        int xr = (px ^ py) & 7;
        int yy = py & 7;
        int bay = ((xr & 1) << 5) | ((yy & 1) << 4) | ((xr & 2) << 2) | ((yy & 2) << 1) | ((xr & 4) >> 1) | ((yy & 4) >> 2);
        int fa = int(pc.fade.x + 0.5);
        if (pc.fade.y > 0.5 ? bay < fa : bay >= fa) discard;
    }
    int fq = clamp(int(floor(100.0 + 52.0 * v_uvao.z)), 0, 256);
    int total = (fq * v_fk) >> 8;
    int s256 = clamp(int(floor(v_light.x * pc.light.x / 15.0 + 0.5)), 0, 256);
    int b256 = clamp(int(floor(v_light.y * 256.0 / 15.0 + 0.5)), 0, 256);
    ivec3 lum = max(ivec3(s256), (b256 * ivec3(pc.light.yzw)) >> 8);
    ivec3 lf = 24 + ((lum * 232) >> 8);
    ivec3 tot = (total * lf) >> 8;
    ivec3 c = (t.rgb * tot) >> 8;
    if (u.fog.x > 0.5) {
        float tt = clamp((v_depth - u.fog.y) / (u.fog.z - u.fog.y), 0.0, 1.0);
        int f = int(floor(tt * 256.0));
        c = (c * (256 - f) + ivec3(u.fogc.xyz) * f) >> 8;
    }
    o_color = vec4(vec3(c) / 255.0, trans ? float(t.a) / 255.0 : 1.0);
}
