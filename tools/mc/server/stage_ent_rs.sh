#!/usr/bin/env bash
# sourced by run.sh: stage K entities (ent_run.sh) and stage L redstone (rs_run.sh stage1 + stage2), each with its own vanilla + Firn server pair
run_stage() {
    local label=$1 skip=$2
    shift 2
    if [ -n "${!skip:-}" ]; then return; fi
    echo "stage ${label}:"
    if bash "$@" > "$W/${label}_stage.out" 2>&1; then
        cat "$W/${label}_stage.out"
    else
        cat "$W/${label}_stage.out"
        ERR=1
    fi
}
run_stage ent MCS_NO_ENT "$TOOLS/ent_run.sh"
run_stage rs MCS_NO_RS "$TOOLS/rs_run.sh" stage1
run_stage rs2 MCS_NO_RS "$TOOLS/rs_run.sh" stage2
