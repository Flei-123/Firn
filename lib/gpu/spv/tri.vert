#version 450
// lib/gpu/spv/tri.vert -- test shader of gpu.vk: 2D position in NDC + RGB colour per vertex.
layout(location = 0) in vec2 pos;
layout(location = 1) in vec3 col;
layout(location = 0) out vec3 vcol;
void main() {
    gl_Position = vec4(pos, 0.0, 1.0);
    vcol = col;
}
