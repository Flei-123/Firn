#version 450
// Image / sampler test shader (tests 2936, 2937): one output per feature, compared against Firn f32 reference code.
layout(location = 0) in vec2 uv;
layout(location = 1) in float lodv;
layout(location = 0) out vec4 c0;  // texture(s0, uv): implicit LOD (quad) or LOD 0
layout(location = 1) out vec4 c1;  // textureLod(s0, uv, lodv)
layout(location = 2) out vec4 c2;  // texelFetch(s0, ivec2(uv * 8.0), 1)
layout(location = 3) out vec4 c3;  // textureSize(s0, 0), textureSize(s0, 1), textureQueryLevels
layout(location = 4) out vec4 c4;  // separate texture + sampler
layout(location = 5) out vec4 c5;  // textureOffset(s0, uv, ivec2(1, -1)) + textureLodOffset
layout(location = 6) out vec4 c6;  // textureGrad
layout(location = 7) out vec4 c7;  // dFdx(uv), dFdy(uv)
layout(location = 8) out vec4 c8;  // dFdxCoarse(uv), dFdyCoarse(uv)
layout(location = 9) out vec4 c9;  // fwidth(uv), fwidthCoarse(lodv), dFdxFine(lodv)
layout(location = 10) out vec4 c10; // textureProj, texture with bias
layout(location = 11) out vec2 c11; // textureQueryLod
layout(set = 0, binding = 0) uniform sampler2D s0;
layout(set = 0, binding = 1) uniform texture2D t1;
layout(set = 0, binding = 2) uniform sampler smp;
void main() {
    c0 = texture(s0, uv);
    c1 = textureLod(s0, uv, lodv);
    c2 = texelFetch(s0, ivec2(uv * 8.0), 1);
    ivec2 a = textureSize(s0, 0);
    ivec2 b = textureSize(s0, 1);
    c3 = vec4(float(a.x), float(a.y), float(b.x), float(textureQueryLevels(s0)));
    c4 = texture(sampler2D(t1, smp), uv);
    c5 = textureOffset(s0, uv, ivec2(1, -1)) + textureLodOffset(s0, uv, 0.0, ivec2(-2, 3));
    c6 = textureGrad(s0, uv, vec2(0.25, 0.0), vec2(0.0, 0.125));
    c7 = vec4(dFdx(uv), dFdy(uv));
    c8 = vec4(dFdxCoarse(uv), dFdyCoarse(uv));
    c9 = vec4(fwidth(uv), fwidthCoarse(lodv), dFdxFine(lodv));
    c10 = textureProj(s0, vec3(uv * 2.0, 2.0)) + texture(s0, uv, 0.5);
    c11 = textureQueryLod(s0, uv);
}
