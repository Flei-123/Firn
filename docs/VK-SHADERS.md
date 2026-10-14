# Vulkan shaders of the MC renderer (round VK2, r178)

Status 2026-10-14. The GL shaders `lib/gpu/{mc,gl_sky,gl_ent,gl_hud,gl_tint}_*.glsl` ported 1:1 to Vulkan GLSL 450.
Merge this file into `docs/VK-RENDERER.md` (VK1) as the "Shaders" section when both are on main.
**No real GPU was available**: the SPIR-V is validated (`spirv-val`, Vulkan 1.0), interface-checked (test 3090) and accepted by Mesa
lavapipe (`vkCreateShaderModule`, test 3091). Pixel output through a pipeline is NOT measured yet (needs VK1's descriptor sets, r177).

## Files
| File | Content |
|---|---|
| `lib/gpu/vk_glsl/*.glsl` | sources: `mc_vs/mc_fs`, `sky_vs/sky_fs`, `ent_vs/ent_fs`, `hud_vs/hud_fs`, `tint_fs`, shared `vk_frame.glsl` (UBO + bindings, `#include`) |
| `lib/gpu/spv/vk_{mc,sky,ent,hud}.{vert,frag}.spv`, `vk_tint.frag.spv` | committed SPIR-V 1.0 (the tint pass uses `vk_sky.vert.spv`) |
| `lib/gpu/vk_shaders.fi` | `import gpu.vk_shaders`: embedded octets (`MC_VS ...`, `shader_code(i)/shader_size(i)`) + every number below as a constant |
| `tools/gpu/mkspv.sh` | no argument = rebuild; `--check` byte-equal with the committed files; `--parity` body diff against the GL shaders must equal `tools/gpu/vk_parity.expected` (`--parity --update` after a deliberate change) |
| `tests/3090_mc_vk_shader_spv.fi` | binary parse: stage, entry point, binding / location masks, std140 offsets == `vk_shaders` constants |
| `tests/3091_mc_vk_shader_modules.fi` | all modules on a Vulkan device (lavapipe), SKIP without one |
| `lib/gpu/vk.fi` | only change: `shader_make` exported, new `shader_free` |
`tools/gpu/vk/run.sh` runs `--check` and `--parity` as well (SKIP without glslang).

## Descriptor set 0 (the ONLY set; one layout for every pipeline)
| binding | type | stage | content |
|---|---|---|---|
| 0 | UNIFORM_BUFFER (dynamic offset ok) | vertex + fragment | `Frame`, 320 octets std140 (below) |
| 1..4 | COMBINED_IMAGE_SAMPLER, view 2D_ARRAY | fragment | atlas page 0..3 (`u_atlas`, `u_atlas1..3`; unused pages: bind page 0 again) |
| 5 | UNIFORM_TEXEL_BUFFER `R32_UINT` | vertex | `u_gran`: slot number per 64 vertices (pool mode) |
| 6 | UNIFORM_TEXEL_BUFFER `R32G32B32A32_SFLOAT` | vertex | `u_org`: section origin per slot (pool mode) |
A stage only declares what it uses (bindings used: mc.vert 0,5,6; mc.frag 0..4; sky.vert none; every other stage 0).
Push constants (vertex stage, 16 octets, `mc.vert` only): `vec3 u_origin` at 0 (section origin minus camera, f64 on the CPU), `int u_pool` at 12.

## `Frame` UBO (std140), offsets = `vk_shaders.U_*`
`u_rv` mat4 0 | `u_fog` vec4 64 | `u_fade` 80 | `u_light` 96 | `u_cam` 112 | `u_sun` 128 | `u_fogc` vec3 144 | `u_r` 160 | `u_u` 176 | `u_f` 192 |
`u_zen` 208 | `u_hor` 224 | `u_glc` 240 | `u_tint` 256 | `u_tiles` vec2 272 | `u_size` vec2 280 | `u_pagel` int 288 | `u_trans` 292 | `u_filter` 296 |
`u_discs` 300 | `u_invert` 304 | `u_h` float 308. Each vec3 takes 16 octets (4 octets padding). Same names, same meaning as the GL uniforms
(comments in `vk_frame.glsl`). Per-pass values (`u_trans`, `u_filter`, `u_fade`) = one UBO slice per pass (dynamic offset) or one write per pass.

## Vertex inputs (locations identical to GL)
| shader | binding 0 | binding 1 |
|---|---|---|
| mc.vert | per vertex, stride 8: loc 0 `R32G32_UINT` (the mc_mesh vertex) | - |
| ent.vert | per vertex, stride 16: loc 0 `R32G32B32_SFLOAT` pos (+0), loc 1 `R32_SFLOAT` face (+12); 24 vertices, 36 indices | per INSTANCE, stride 48: loc 2/3/4 `R32G32B32A32_SFLOAT` at +0/+16/+32 |
| hud.vert | per INSTANCE, stride 20: loc 0 `R32G32B32A32_SINT` rect (+0), loc 1 `R32_UINT` 0xAABBGGRR (+16); `vkCmdDraw(4, n, 0, 0)` strip | - |
| sky.vert | none; `vkCmdDraw(4, 1, 0, 0)` TRIANGLE_STRIP (sky and tint) | - |
Varyings: mc `0 v_light, 1 v_uvao, 2 v_tile(flat), 3 v_fk(flat), 4 v_depth`; ent `0 v_col(flat), 1 v_fk(flat), 2 v_depth`; hud `0 v_col(flat)`. Fragment output location 0.

## The three GL -> Vulkan differences (everything else is line for line the same; `vk_parity.expected` lists all 14 differing lines)
1. **Y flip**: draw with the FLIPPED viewport (`x=0, y=height, width, height=-height`, core in Vulkan 1.1, else `VK_KHR_maintenance1`). Then the GL clip
   coordinates give an upright image, front faces stay counter-clockwise (`frontFace = COUNTER_CLOCKWISE`; voxel pass culls back faces like `glbe_set_cull`, sky / tint / HUD / entity boxes draw without culling as in `gl_scene.fi`), the HUD formula
   `1 - y/h*2` and the sky strip are unchanged, and `gl_FragCoord.y` is already counted from the top (= row + 0.5). Hence in `mc_fs` the screen-door row is
   `py = int(gl_FragCoord.y)` (GL: `u_fade.z - 1 - int(gl_FragCoord.y)`) and in `sky_fs` `ey = (u_cam.y - gl_FragCoord.y) / u_cam.w` (GL: `u_cam.y - (u_h - y)`).
   Without the flip the image is upside down AND these two shaders are wrong. Read back is top-down without a flip of rows (as `vk.read_pixels`).
2. **Depth 0..1**: `u_rv` stays the OpenGL matrix (z in -w..w), so the CPU keeps ONE matrix; `mc_vs` / `ent_vs` remap `z = (z + w) * 0.5` AFTER `v_depth = w`.
   `sky_vs` writes z = w = 1 -> depth exactly 1.0 as in GL (tint: depth test GREATER finds covered pixels). `hud_vs` z = 0.5 (= GL's window depth for z = 0).
3. **Interface**: `gl_VertexID` -> `gl_VertexIndex` (= index + vertexOffset, the same value `glDrawElementsBaseVertex` gives; the pool mode relies on it), loose
   uniforms -> `Frame` UBO / push constants / texel buffers, explicit `location =` on every in / out.

## Pipeline state per pass (what GL does, to be copied by `vk_mc`; NOT yet run)
| pass | shaders | depth | blend |
|---|---|---|---|
| sky | sky.vert / sky.frag | off | off |
| opaque world | mc.vert / mc.frag, `u_trans=0` | test LESS (or LESS_OR_EQUAL as GL), write on, clear 1.0 | off |
| translucent world | mc, `u_trans=1` | test, write off | SRC_ALPHA, ONE_MINUS_SRC_ALPHA |
| entities | ent.vert / ent.frag | as opaque | off |
| tint | sky.vert / tint.frag | test GREATER, write off | DST_COLOR, ZERO (alpha: any) |
| HUD | hud.vert / hud.frag, `u_invert=0` | off | SRC_ALPHA, ONE_MINUS_SRC_ALPHA |
| HUD invert | hud.vert / hud.frag, `u_invert=1` | off | ONE_MINUS_DST_COLOR, ZERO |
(Authoritative are the GL calls in `lib/gpu/gl_mc.fi` / `gl_scene.fi`; this table is from the shader comments.)

## Honest limits
- No GPU, no validation layer: lavapipe only accepts the modules; behaviour of a full pipeline (descriptor layout, flipped viewport, texel buffers,
  `textureGrad` in the filtered path, translucent order) is untested until VK1's backend draws the same scenes as `tools/gpu/dump.fi` (gate: PSNR vs `mc_raster`).
- Texel buffer limits (`maxTexelBufferElements`, >= 65536 guaranteed) and `maxImageArrayLayers` (>= 256 guaranteed) decide pool size / atlas paging as in GL.
- One shader variant per stage with runtime branches (`u_trans`, `u_filter`, fog, fade) exactly like GL. Specialization constants for the hot variants are an
  optimisation for later, not needed for correctness.
- Float rounding in `sky_fs` is not bit-equal to the CPU reference (same as GL).

## Rebuild
`bash tools/gpu/mkspv.sh` (glslangValidator `-V --target-env vulkan1.0`, `spirv-val`), then `bash tools/gpu/mkspv.sh --check`, `--parity`, and
`bash tools/mc/testmc.sh vk_shader`.
