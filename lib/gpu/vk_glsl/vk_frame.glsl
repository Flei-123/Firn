// SPDX-License-Identifier: MPL-2.0
// lib/gpu/vk_glsl/vk_frame.glsl -- shared interface of ALL Vulkan shaders of the MC renderer (docs/VK-RENDERER.md (Shaders)).
// Included by every stage (GL_GOOGLE_include_directive). Not compiled on its own.
//
// set 0 is the ONLY descriptor set. One descriptor set layout / one pipeline layout fits every pipeline; a stage simply
// does not touch the bindings it does not need.
//   binding 0  UBO  Frame        vertex + fragment   all former GL uniforms except the per-draw ones (std140, 320 octets)
//   binding 1  sampler2DArray u_atlas    fragment    atlas page 0 (the whole atlas when it fits into maxImageArrayLayers)
//   binding 2  sampler2DArray u_atlas1   fragment    atlas pages 1..3 (unused pages: bind page 0 again)
//   binding 3  sampler2DArray u_atlas2
//   binding 4  sampler2DArray u_atlas3
//   binding 5  usamplerBuffer u_gran     vertex      pool mode: slot number per 64 vertices (R32_UINT texel buffer)
//   binding 6  samplerBuffer  u_org      vertex      pool mode: section origin per slot (RGBA32F texel buffer)
// push constants (vertex stage, 16 octets): see mc_vs.glsl (u_origin, u_pool).
//
// std140 offsets (the host struct in lib/gpu/vk_shaders.fi uses the same numbers):
//   0 u_rv(mat4)  64 u_fog  80 u_fade  96 u_light  112 u_cam  128 u_sun  144 u_fogc  160 u_r  176 u_u  192 u_f
//   208 u_zen  224 u_hor  240 u_glc  256 u_tint  272 u_tiles(vec2)  280 u_size(vec2)  288 u_pagel  292 u_trans
//   296 u_filter  300 u_discs  304 u_invert  308 u_h  (312..319 padding)       vec3 members occupy 16 octets (float pad).
layout(set = 0, binding = 0, std140) uniform Frame {
    mat4 u_rv;          // projection * view rotation (no translation), column-major, OpenGL clip space (z in -w..w); the
                        // vertex shaders remap z to Vulkan's 0..w, so the CPU keeps ONE matrix for GL and Vulkan
    vec4 u_fog;         // x = 1 fog on, y = start, z = end
    vec4 u_fade;        // x = fade a (>= 64 off), y = 1 inverted, z = frame height (LOD cross-fade, screen door), w = frame width (vertex snap r261)
    vec4 u_light;       // x = day (sky light multiplier 0..256), yzw = block light tint per channel 0..256
    vec4 u_cam;         // sky: half width, half height, focal length x, focal length y (pixels)
    vec4 u_sun;         // sky: xyz = unit direction to the sun, w = glow strength 0..256
    vec3 u_fogc;        // fog colour 0..255
    float pad0;
    vec3 u_r;           // sky: camera right / up / forward (world)
    float pad1;
    vec3 u_u;
    float pad2;
    vec3 u_f;
    float pad3;
    vec3 u_zen;         // sky: zenith colour 0..255
    float pad4;
    vec3 u_hor;         // sky: horizon colour
    float pad5;
    vec3 u_glc;         // sky: sunset glow colour
    float pad6;
    vec3 u_tint;        // tint pass: light tint (multiplied into the framebuffer by blending)
    float pad7;
    vec2 u_tiles;       // x = tile edge in texels (power of two), y = unused
    vec2 u_size;        // HUD: frame size in pixels
    int u_pagel;        // layers per atlas page; >= 1048576 = single array (page 0 only)
    int u_trans;        // 1 = translucent pass
    int u_filter;       // 0 = exact texelFetch like mc_raster, 1 = filtered (mipmaps / anisotropy)
    int u_discs;        // sky: 1 = draw sun / moon discs
    int u_invert;       // HUD: 1 = white for the invert blend
    float u_h;          // frame height in pixels
    int pad8;
    int pad9;
};
