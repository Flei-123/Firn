#!/usr/bin/env bash
# sourced by run.sh: stage K entities (ent_run.sh) and stage L redstone (rs_run.sh), each with its own vanilla + Firn server pair
for stage in ent rs; do
    skip="MCS_NO_${stage^^}"
    if [ -n "${!skip:-}" ]; then continue; fi
    echo "stage ${stage}:"
    if bash "$TOOLS/${stage}_run.sh" > "$W/${stage}_stage.out" 2>&1; then
        cat "$W/${stage}_stage.out"
    else
        cat "$W/${stage}_stage.out"
        ERR=1
    fi
done
