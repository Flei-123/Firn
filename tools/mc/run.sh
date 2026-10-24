#!/usr/bin/env bash
# tools/mc/run.sh -- every Minecraft library (lib/mc/mc_*.fi) with its tests.
#
#  1. tests/*_mc_*.fi in three build stages (tools/mc/testmc.sh)
#  2. tools/mc/<lib>/run.sh: the cross-check of each library against an
#     independent reference (python / node / java tools, never Mojang code).
#     Each prints a line `RESULT <name>: ok` and exits 0 on success.
#
# Usage: bash tools/mc/run.sh [lib]      (lib = a directory name under tools/mc/, or wine | lodplay | ents | vkplay | vkview | tpplay | ajplay | hashplay | contplay | chatplay | healthplay | combatplay | swimplay | sessplay | edge | modelplay | vanillaplay | lodseam | biomezoom)
set -uo pipefail
cd "$(dirname "$0")/../.."
ERR=0
ONLY=${1:-}
export MC_VANILLA=${MC_VANILLA:-0}
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
# r220: a firnc older than r167 makes every GL stage (mcplay MCPLAY_GL=1 with mesher threads) segfault in the first frame
case " $ONLY " in
    "  "|" chatplay "|" healthplay "|" combatplay "|" swimplay "|" wine "|" lodplay "|" ents "|" vkplay "|" vkview "|" tpplay "|" ajplay "|" hashplay "|" contplay "|" sessplay "|" modelplay "|" vanillaplay "|" lodseam "|" biomezoom "|" play ")
        bash tools/mc/check_firnc.sh "$FIRNC" || { echo "RESULT mc: FAIL (stale firnc)"; exit 1; } ;;
esac
# every .fi file of the tree must be in the canonical shape of firnfmt (test.sh section 23
# fails otherwise): format with `build/firnfmt -w <file>`; this builds it into a temp dir.
if [ -z "$ONLY" ]; then
    FT=$(mktemp -d)
    trap 'rm -rf "$FT"' EXIT
    if FIRNLIB="$(pwd)/lib" "$FIRNC" -o "$FT/firnfmt" tools/fmt/firnfmt.fi 2>"$FT/err"; then
        files=$(ls lib/mc/*.fi lib/mc/v*/*.fi tests/*_mc_*.fi demos/mcclient/*.fi demos/mcview/*.fi demos/mcplay/*.fi 2>/dev/null; find tools/mc -name '*.fi')
        if bad=$("$FT/firnfmt" -c $files 2>&1) ; then echo "  ok    all mc sources are in canonical shape (firnfmt -c)"
        else echo "  FAIL  not formatted (run firnfmt -w):"; echo "$bad" | head -10 | sed 's/^/        /'; ERR=$((ERR+1)); fi
    else echo "  FAIL  cannot build tools/fmt/firnfmt.fi"; head -5 "$FT/err"; ERR=$((ERR+1)); fi
fi
if [ -z "$ONLY" ]; then
    bash tools/mc/testmc.sh || ERR=$((ERR+1))
fi
for s in tools/mc/*/run.sh; do
    [ -f "$s" ] || continue
    lib=$(basename "$(dirname "$s")")
    [ -n "$ONLY" ] && [ "$ONLY" != "$lib" ] && continue
    echo "== mc/$lib =="
    bash "$s" || ERR=$((ERR+1))
done
# Windows build under Wine (cross-compile, login, window, raw mouse, F11, GUI exe + bank + log, recv-timeout seam): ~4 min, needs wine + Xvfb + java.
if [ -z "$ONLY" ] || [ "$ONLY" = wine ]; then
    echo "== mc/wine =="
    bash tools/mc/play/run_wine.sh || ERR=$((ERR+1))
fi
# mcplay with LOD rings (MCPLAY_LOD, mc_stream) against the real server: ~2 min, needs Xvfb + xdotool + java.
if [ -z "$ONLY" ] || [ "$ONLY" = lodplay ]; then
    echo "== mc/lodplay =="
    bash tools/mc/play/run_lod.sh || ERR=$((ERR+1))
fi
# r254: models + LOD together: seam full ring / LOD ring (pixel-exact outside the stairs), ring change LOD -> full (remesh with models), ~8 min; LODSEAM_PERF=1 adds the frame-time series.
if [ -z "$ONLY" ] || [ "$ONLY" = lodseam ]; then
    echo "== mc/lodseam =="
    bash tools/mc/play/run_lodseam.sh || ERR=$((ERR+1))
fi
if [ -z "$ONLY" ] || [ "$ONLY" = biomezoom ]; then
    echo "== mc/biomezoom =="
    bash tools/mc/play/run_biomezoom.sh || ERR=$((ERR+1))
fi
# mcplay with pack models (MCPLAY_MODELS=1, mc_model_mesh: stairs from baked quads, own fixture pack, T reload) against the real server, flat world: ~3 min, needs Xvfb + xdotool + java.
if [ -z "$ONLY" ] || [ "$ONLY" = modelplay ]; then
    echo "== mc/modelplay =="
    bash tools/mc/play/run_models.sh || ERR=$((ERR+1))
fi
# r245: the same pack models on the GL (MCPLAY_GL=1) and Vulkan (MCPLAY_GFX=vk) back ends vs the CPU picture (PSNR >= 34 dB, llvmpipe / lavapipe, no real GPU): ~6 min.
if [ -z "$ONLY" ] || [ "$ONLY" = modelplay ] || [ "$ONLY" = modelgl ]; then
    echo "== mc/modelgl =="
    bash tools/mc/play/run_models_gl.sh || ERR=$((ERR+1))
fi
# vanilla resource pack default: jar -> run time cache -> mcplay (models on), MC_VANILLA=0 and no-jar fallback, cache reuse: ~4 min, needs Xvfb + xdotool + java + network (client jar, cached).
if [ "$ONLY" = vanillaplay ]; then
    echo "== mc/vanillaplay =="
    bash tools/mc/play/run_vanilla.sh || ERR=$((ERR+1))
fi
# entities in the mcplay play path against the real server (live packets + CPU/GL pictures with / without boxes): ~4 min, needs Xvfb + java; plus the rider entity scenario of tools/mcclient/run.sh.
if [ -z "$ONLY" ] || [ "$ONLY" = ents ]; then
    echo "== mc/ents =="
    bash tools/mc/play/run_ents.sh || ERR=$((ERR+1))
    # entity scenario of the byte-compare harness WITH the rider part (MC_ENT_RIDER=1: mount a minecart, two teleports with a rider, dismount): Firn vs the reference
    # client against the real 1.20.1 server, A == A2, B == A, entities EQUAL: ~5 min (45 s settle), needs node + java.
    echo "== mc/ents rider =="
    RC=$(mktemp -d)
    if FIRNLIB="$(pwd)/lib" "$FIRNC" -o "$RC/mcclient" demos/mcclient/main.fi 2>"$RC/err"; then
        MC_ENT_RIDER=1 MC_VERSION=1.20.1 bash tools/mcclient/run.sh "$RC/mcclient" || ERR=$((ERR+1))
    else
        echo "  FAIL  demos/mcclient does not compile"; head -5 "$RC/err"; ERR=$((ERR+1))
    fi
    rm -rf "$RC"
fi
# mcplay with the Vulkan back end (MCPLAY_GFX=vk; lavapipe = CPU Vulkan, NO real GPU) against the real server: picture vs CPU raster PSNR >= 34, frame counters,
# simulated device loss + recovery: ~4 min, needs Xvfb + java; SKIP without a Vulkan device.
if [ -z "$ONLY" ] || [ "$ONLY" = vkplay ]; then
    echo "== mc/vkplay =="
    bash tools/mc/play/run_vk.sh || ERR=$((ERR+1))
fi
if [ -z "$ONLY" ] || [ "$ONLY" = vkplay ] || [ "$ONLY" = vkview ]; then
    echo "== mc/vkview =="
    bash tools/mc/play/run_vkview.sh || ERR=$((ERR+1))
fi
# mcplay WITHOUT fly: /tp into the air, fall, land, walk; no rubber band / "moved too quickly" after the Teleport Confirm (r197): ~2 min, needs Xvfb + xdotool + java.
if [ -z "$ONLY" ] || [ "$ONLY" = tpplay ]; then
    echo "== mc/tpplay =="
    bash tools/mc/play/run_tp.sh || ERR=$((ERR+1))
fi
# mcplay Auto-Jump (1 block step, option off = stays, MCPLAY_AUTOJUMP=1 = climbs) + relative position flags (`tp @s ~2 ~6 ~`, client == server): ~3 min.
if [ -z "$ONLY" ] || [ "$ONLY" = ajplay ]; then
    echo "== mc/ajplay =="
    bash tools/mc/play/run_aj.sh || ERR=$((ERR+1))
    MC_VERSION=1.21.4 bash tools/mc/play/run_aj.sh || ERR=$((ERR+1))   # u32 flags + delta (protocol 769)
fi
# FleiLauncher hand-off (r39): MCPLAY_SESSION_FILE=<file>|- (host, port, name, ...), file deleted after reading, command line overrides: ~1.5 min.
if [ -z "$ONLY" ] || [ "$ONLY" = sessplay ]; then
    echo "== mc/sessplay =="
    bash tools/mc/play/run_session.sh || ERR=$((ERR+1))
fi
# hashed item stacks (1.21.5+): a stack with six components is picked up / put back, the server must not re-send the slot; control run without hashes: ~4 min each version.
if [ -z "$ONLY" ] || [ "$ONLY" = hashplay ]; then
    echo "== mc/hashplay =="
    bash tools/mc/play/run_hash.sh 1.21.5 || ERR=$((ERR+1))
    bash tools/mc/play/run_hash.sh 1.21.8 || ERR=$((ERR+1))
fi
# container windows: title, hopper / dispenser layout, sneak + right click places the block (1.20.1 Entity Action, 1.21.4 Player Input)
if [ -z "$ONLY" ] || [ "$ONLY" = contplay ]; then
    echo "== mc/contplay =="
    bash tools/mc/play/run_cont.sh 1.20.1 || ERR=$((ERR+1))
    bash tools/mc/play/run_cont.sh 1.21.4 || ERR=$((ERR+1))
fi
# chat: console say / tellraw lines arrive, MCPLAY_CHAT_TEST sends a message and a command (1.20.1 JSON components, 1.21.4 NBT components)
if [ -z "$ONLY" ] || [ "$ONLY" = chatplay ]; then
    echo "== mc/chatplay =="
    bash tools/mc/play/run_chat.sh 1.20.1 || ERR=$((ERR+1))
    bash tools/mc/play/run_chat.sh 1.21.4 || ERR=$((ERR+1))
fi
# health: UPDATE_HEALTH hearts / food, death screen from DEATH_COMBAT_EVENT, respawn key -> client_command respawn (1.20.1 + 1.21.4)
if [ -z "$ONLY" ] || [ "$ONLY" = healthplay ]; then
    echo "== mc/healthplay =="
    bash tools/mc/play/run_health.sh 1.20.1 || ERR=$((ERR+1))
    bash tools/mc/play/run_health.sh 1.21.4 || ERR=$((ERR+1))
fi
# combat: entity pick under the crosshair -> attack (cow loses 2 hp), interact, use item (snowball consumed), release (1.20.1 + 1.21.4)
if [ -z "$ONLY" ] || [ "$ONLY" = combatplay ]; then
    echo "== mc/combatplay =="
    bash tools/mc/play/run_combat.sh 1.20.1 || ERR=$((ERR+1))
    bash tools/mc/play/run_combat.sh 1.21.4 || ERR=$((ERR+1))
fi
# sneak edge (maybeBackOffFromEdge): sneaking W / W+D on a platform never walks off, the control run without sneak falls; client == server: ~3 min per version.
if [ -z "$ONLY" ] || [ "$ONLY" = edge ]; then
    echo "== mc/edge =="
    bash tools/mc/play/run_edge.sh || ERR=$((ERR+1))
    MC_VERSION=1.21.4 bash tools/mc/play/run_edge.sh || ERR=$((ERR+1))
    MC_VERSION=1.21.8 bash tools/mc/play/run_edge.sh || ERR=$((ERR+1))
fi
if [ -z "$ONLY" ] || [ "$ONLY" = swimplay ]; then
    echo "== mc/swimplay =="
    bash tools/mc/play/run_swim.sh 1.20.1 || ERR=$((ERR+1))
    bash tools/mc/play/run_swim.sh 1.21.4 || ERR=$((ERR+1))
fi
if [ $ERR -eq 0 ]; then echo "RESULT mc: ok"; else echo "RESULT mc: $ERR failing parts"; fi
exit $ERR
