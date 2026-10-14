#version 450
// Pack / unpack functions (GLSL.std.450 54..64), test 2938.
layout(location = 0) in vec4 f4;
layout(location = 1) flat in uvec2 u2;
layout(location = 0) out uint p0; // packUnorm4x8
layout(location = 1) out uint p1; // packSnorm4x8
layout(location = 2) out uint p2; // packUnorm2x16
layout(location = 3) out uint p3; // packSnorm2x16
layout(location = 4) out uint p4; // packHalf2x16
layout(location = 5) out vec4 q0; // unpackUnorm4x8(u2.x)
layout(location = 6) out vec4 q1; // unpackSnorm4x8(u2.x)
layout(location = 7) out vec2 q2; // unpackUnorm2x16(u2.y)
layout(location = 8) out vec2 q3; // unpackSnorm2x16(u2.y)
layout(location = 9) out vec2 q4; // unpackHalf2x16(u2.y)
void main() {
    p0 = packUnorm4x8(f4);
    p1 = packSnorm4x8(f4);
    p2 = packUnorm2x16(f4.xy);
    p3 = packSnorm2x16(f4.zw);
    p4 = packHalf2x16(f4.yz);
    q0 = unpackUnorm4x8(u2.x);
    q1 = unpackSnorm4x8(u2.x);
    q2 = unpackUnorm2x16(u2.y);
    q3 = unpackSnorm2x16(u2.y);
    q4 = unpackHalf2x16(u2.y);
}
