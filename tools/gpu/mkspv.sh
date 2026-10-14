#!/usr/bin/env bash
# tools/gpu/mkspv.sh -- builds the Vulkan SPIR-V of the MC renderer from lib/gpu/vk_glsl/*.glsl (docs/VK-RENDERER.md (Shaders)).
#   bash tools/gpu/mkspv.sh            regenerate lib/gpu/spv/vk_*.spv (glslangValidator -V, Vulkan 1.0) + spirv-val
#   bash tools/gpu/mkspv.sh --check    build into a temp dir and compare BYTE-EQUAL with the committed .spv (needs the tools)
#   bash tools/gpu/mkspv.sh --parity   compare the shader bodies with the OpenGL shaders (lib/gpu/*.glsl): the diff must be
#                                      exactly tools/gpu/vk_parity.expected (= the documented GL -> Vulkan differences)
#   bash tools/gpu/mkspv.sh --parity --update   rewrite tools/gpu/vk_parity.expected after a deliberate change
# The .spv files are committed; the tests (tests/3090..) do not need glslang. Output is deterministic.
set -eu
cd "$(dirname "$0")/../.."
SRC=lib/gpu/vk_glsl
OUT=lib/gpu/spv
# name:stage:glsl source  (the tint pass reuses vk_sky.vert)
LIST="vk_mc.vert:vert:mc_vs vk_mc.frag:frag:mc_fs vk_sky.vert:vert:sky_vs vk_sky.frag:frag:sky_fs vk_ent.vert:vert:ent_vs vk_ent.frag:frag:ent_fs vk_hud.vert:vert:hud_vs vk_hud.frag:frag:hud_fs vk_tint.frag:frag:tint_fs"
# GL twin of every Vulkan source (for --parity)
PAIRS="mc_vs:mc_vs mc_fs:mc_fs sky_vs:gl_sky_vs sky_fs:gl_sky_fs ent_vs:gl_ent_vs ent_fs:gl_ent_fs hud_vs:gl_hud_vs hud_fs:gl_hud_fs tint_fs:gl_tint_fs"

build_to() { # $1 = output dir
    local item name stage src
    for item in $LIST; do
        name=${item%%:*}; stage=${item#*:}; stage=${stage%%:*}; src=${item##*:}
        glslangValidator -V --target-env vulkan1.0 -S "$stage" -I"$SRC" -o "$1/$name.spv" "$SRC/$src.glsl" >"$1/$name.log" 2>&1 \
            || { echo "FAIL glslang $src"; cat "$1/$name.log"; return 1; }
        spirv-val --target-env vulkan1.0 "$1/$name.spv" || { echo "FAIL spirv-val $name"; return 1; }
    done
}

body_of() { # strip comments / blank lines and everything before the first function or constant of the shader
    sed -e 's://.*$::' -e 's/[[:space:]]*$//' "$1" | grep -v '^$' | awk '/^(const int FK|ivec3 lerp3|vec4 fetch_page|void main)/ {p=1} p'
}

parity() {
    local pair v g tmp
    tmp=$(mktemp -d); trap 'rm -rf "$tmp"' RETURN
    for pair in $PAIRS; do
        v=${pair%%:*}; g=${pair##*:}
        echo "### $v vs $g"
        diff <(body_of "lib/gpu/$g.glsl") <(body_of "$SRC/$v.glsl") || true
    done >"$tmp/actual"
    if [ "${1:-}" = "--update" ]; then cp "$tmp/actual" tools/gpu/vk_parity.expected; echo "updated tools/gpu/vk_parity.expected"; return 0; fi
    if diff -u tools/gpu/vk_parity.expected "$tmp/actual"; then echo "parity ok: $(grep -c '^[<>]' "$tmp/actual") differing lines (all expected)"; else echo "FAIL parity"; return 1; fi
}

case "${1:-}" in
    --parity) parity "${2:-}";;
    --check)
        T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
        build_to "$T"
        rc=0
        for item in $LIST; do
            name=${item%%:*}
            if cmp -s "$T/$name.spv" "$OUT/$name.spv"; then echo "ok    $name.spv"; else echo "DIFF  $name.spv"; rc=1; fi
        done
        exit $rc;;
    "")
        build_to "$OUT"; rm -f "$OUT"/vk_*.log
        ls -l "$OUT"/vk_*.spv;;
    *) echo "usage: $0 [--check | --parity [--update]]"; exit 2;;
esac
