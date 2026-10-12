#version 450
// lib/gpu/spv/tri.frag -- colour from the vertex stage, opaque.
layout(location = 0) in vec3 vcol;
layout(location = 0) out vec4 outc;
void main() {
    outc = vec4(vcol, 1.0);
}
