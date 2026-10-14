#version 300 es
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/campreview_fs.glsl -- the fragment shader of the camera preview: YUV_420_888 -> RGB (full range
// BT.601, as the camera delivers it), the sensor rotation and the aspect crop in ONE fetch chain.
//   u_crop  (x0, y0, w, h): the visible window inside the ROTATED image (0..1, y down); a negative w is a
//           horizontal mirror (front camera)
//   u_m, u_t: rotated image -> sensor image: s = (m.x*r.x + m.y*r.y + t.x, m.z*r.x + m.w*r.y + t.y)
//   u_mode  x: 1 = planar (u_uv = U, u_v = V, both R8), 0 = semi-planar (u_uv = RG8, interleaved)
//           y: 1 = the first byte of a pair is V (NV21), 0 = U (NV12)
precision highp float;
precision highp sampler2D;
uniform sampler2D u_y;
uniform sampler2D u_uv;
uniform sampler2D u_v;
uniform vec4 u_crop;
uniform vec4 u_m;
uniform vec2 u_t;
uniform vec2 u_mode;
in vec2 v_d;
out vec4 o;

void main() {
    vec2 r = vec2(u_crop.x + v_d.x * u_crop.z, u_crop.y + v_d.y * u_crop.w);
    vec2 s = vec2(u_m.x * r.x + u_m.y * r.y + u_t.x, u_m.z * r.x + u_m.w * r.y + u_t.y);
    float y = texture(u_y, s).r;
    vec2 c;
    if (u_mode.x > 0.5) {
        c = vec2(texture(u_uv, s).r, texture(u_v, s).r);
    } else {
        c = texture(u_uv, s).rg;
        if (u_mode.y > 0.5) { c = c.yx; }
    }
    c = c - 0.5;
    vec3 rgb = vec3(y + 1.402 * c.y, y - 0.344136 * c.x - 0.714136 * c.y, y + 1.772 * c.x);
    o = vec4(clamp(rgb, 0.0, 1.0), 1.0);
}
