#version 330 core
// SPDX-License-Identifier: MPL-2.0
layout(location = 0) in vec3 a_pos;
layout(location = 1) in float a_face;
layout(location = 2) in vec4 i0;
layout(location = 3) in vec4 i1;
layout(location = 4) in vec4 i2;
layout(location = 5) in vec4 i3;
layout(location = 6) in vec4 i4;
layout(location = 7) in vec4 i5;
layout(location = 8) in vec4 i6;
uniform mat4 u_rv;
out vec2 v_uv;
flat out int v_layer;
flat out vec3 v_tint;
flat out int v_fk;
out float v_depth;
float clampf(float v, float lo, float hi) { return min(max(v, lo), hi); }
void main() {
    vec3 size = vec3(i0.w, i1.x, i1.y);
    vec3 r0 = vec3(i1.z, i1.w, i2.x);
    vec3 r1 = vec3(i2.y, i2.z, i2.w);
    vec3 r2 = vec3(i3.x, i3.y, i3.z);
    float u0 = i3.w;
    float v0 = i4.x;
    float tw = i4.y;
    float th = i4.z;
    float td = i4.w;
    bool mirror = i5.y > 0.5;
    int f = int(a_face + 0.5);
    int axis = f / 2;
    float sgn = float(f % 2) * 2.0 - 1.0;
    vec3 q = a_pos * size;
    vec3 p = vec3(dot(r0, q), dot(r1, q), dot(r2, q)) + i0.xyz;
    gl_Position = u_rv * vec4(p, 1.0);
    v_depth = gl_Position.w;
    vec3 n = vec3(0.0);
    n[axis] = sgn;
    vec3 nw = vec3(dot(r0, n), dot(r1, n), dot(r2, n));
    vec3 an = abs(nw);
    if (an.x >= an.y && an.x >= an.z) v_fk = 204;
    else if (an.y >= an.z) v_fk = nw.y < 0.0 ? 128 : 256;
    else v_fk = 153;
    vec3 t = a_pos + 0.5;
    if (mirror) t.x = 1.0 - t.x;
    int sf = f;
    if (mirror && f < 2) sf = 1 - f;
    float ulo = u0;
    float uhi = u0 + td;
    float ut = 0.0;
    if (sf == 0) { ut = 1.0 - t.z; }
    else if (sf == 1) { ulo = u0 + td + tw; uhi = ulo + td; ut = t.z; }
    else if (sf == 2 || sf == 4) { ulo = u0 + td; uhi = ulo + tw; ut = t.x; }
    else if (sf == 3) { ulo = u0 + td + tw; uhi = ulo + tw; ut = t.x; }
    else { ulo = u0 + td + tw + td; uhi = ulo + tw; ut = 1.0 - t.x; }
    float vlo = v0;
    float vhi = v0 + td;
    float vt = 0.0;
    if (sf == 2) { vt = 1.0 - t.z; }
    else if (sf == 3) { vt = t.z; }
    else { vlo = v0 + td; vhi = vlo + th; vt = t.y; }
    v_uv = vec2(clampf(ulo + (uhi - ulo) * ut, ulo + 0.02, uhi - 0.02), clampf(vlo + (vhi - vlo) * vt, vlo + 0.02, vhi - 0.02));
    v_layer = int(i5.x + 0.5);
    v_tint = vec3(i5.zw, i6.x);
}
