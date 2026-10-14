#version 450
// lib/gpu/spv/test/arith.frag -- float / int / vector / matrix arithmetic for tests/2921.
layout(location = 0) in vec4 a;
layout(location = 1) in vec4 b;
layout(location = 2) flat in ivec4 ia;
layout(location = 3) flat in ivec4 ib;
layout(location = 4) flat in uvec2 ua;
layout(location = 0) out vec4 o0;
layout(location = 1) out vec4 o1;
layout(location = 2) out ivec4 o2;
layout(location = 3) out ivec4 o3;
layout(location = 4) out vec4 o4;
layout(location = 5) out uvec2 o5;
void main() {
    o0 = a * b + a - b / vec4(2.0);
    o1 = vec4(dot(a.xyz, b.xyz), length(a.xyz), sqrt(a.x), a.x / b.y);
    o2 = ia * ib + ia / ib - ib % ia;
    o3 = ivec4(ia.x << 2, ib.y >> 1, ia.z & ib.z, ia.w ^ ib.w);
    mat2 m = mat2(a.x, a.y, b.x, b.y);
    vec2 mv = m * vec2(a.z, b.z);
    vec2 vm = vec2(a.w, b.w) * m;
    o4 = vec4(mv, vm);
    o5 = uvec2(ua.x + ua.y, ua.x * ua.y) + uvec2(ua.yx / uvec2(3u)) + uvec2(ua.x >> 1u, ua.y | 1u);
}
