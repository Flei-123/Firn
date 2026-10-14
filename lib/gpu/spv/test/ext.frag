#version 450
// lib/gpu/spv/test/ext.frag -- GLSL.std.450 functions for tests/2923.
layout(location = 0) in vec3 a;
layout(location = 1) in vec3 b;
layout(location = 2) in float t;
layout(location = 0) out vec4 r0;
layout(location = 1) out vec4 r1;
layout(location = 2) out vec4 r2;
layout(location = 3) out vec4 r3;
layout(location = 4) out vec3 r4;
layout(location = 5) out vec3 r5;
layout(location = 6) out vec4 r6;
void main() {
    r0 = vec4(floor(t), ceil(t), fract(t), abs(t));
    r1 = vec4(min(a.x, b.x), max(a.y, b.y), clamp(t, -0.5, 0.5), sign(t));
    r2 = vec4(mix(a.x, b.x, 0.25), step(0.5, t), smoothstep(0.0, 1.0, t), 0.0);
    r3 = vec4(normalize(a), length(a));
    r4 = cross(a, b);
    r5 = reflect(a, normalize(b));
    r6 = vec4(pow(abs(t), 2.5), exp(t), log(abs(t) + 1.0), sin(t) * cos(t));
}
