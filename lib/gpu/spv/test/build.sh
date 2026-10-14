#!/usr/bin/env bash
# Rebuilds the SPIR-V test shaders (needs glslangValidator + spirv-val). The .spv files are committed: the tests do not need the tools.
set -e
cd "$(dirname "$0")"
for s in arith flow ext tex pack; do
    glslangValidator -V --target-env vulkan1.0 -o "$s.frag.spv" "$s.frag"
    spirv-val --target-env vulkan1.0 "$s.frag.spv"
done
ls -l *.spv
