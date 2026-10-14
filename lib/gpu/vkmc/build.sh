#!/usr/bin/env bash
# Rebuilds the SPIR-V of the Vulkan voxel back end (needs glslangValidator + spirv-val). The .spv files are committed.
set -e
cd "$(dirname "$0")"
for s in mc.vert mc.frag; do glslangValidator -V --target-env vulkan1.0 -o "$s.spv" "$s"; spirv-val --target-env vulkan1.0 "$s.spv"; done
ls -l *.spv
