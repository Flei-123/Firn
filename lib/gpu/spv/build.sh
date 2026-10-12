#!/usr/bin/env bash
# Rebuilds the SPIR-V binaries (needs glslangValidator). The .spv files are committed: the tests do not need the tool.
set -e
cd "$(dirname "$0")"
for s in tri.vert tri.frag; do glslangValidator -V --target-env vulkan1.0 -o "$s.spv" "$s"; spirv-val --target-env vulkan1.0 "$s.spv"; done
ls -l *.spv
