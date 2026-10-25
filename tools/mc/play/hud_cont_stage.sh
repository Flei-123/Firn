#!/usr/bin/env bash
launch_cont() {
  local nm=$1 mode=$2
  printf 'name=%s\nprotocol=%s\nview=3\n' "$nm" "$PROTO" > "$W/$nm.cfg"
  if [ "$WINE_RUN" = 1 ]; then
    MC_VANILLA_JAR="Z:$MC_VANILLA_JAR" MCPLAY_CONFIG="Z:$W/$nm.cfg" MCPLAY_PROTOCOL=$PROTO MCPLAY_CONT_UI=$mode MCPLAY_CONT_LOG=1 MCPLAY_RUN_S=70 wine "$W/mcplay.exe" 127.0.0.1 $PORT "$nm" > "$W/$nm.out" 2>&1 &
  else
    MCPLAY_CONFIG="$W/$nm.cfg" MCPLAY_PROTOCOL=$PROTO MCPLAY_CONT_UI=$mode MCPLAY_CONT_LOG=1 MCPLAY_RUN_S=70 "$W/mcplay" 127.0.0.1 $PORT "$nm" > "$W/$nm.out" 2>&1 &
  fi
  CONT_PID=$!
}

stop_cont() {
  kill "$CONT_PID" 2>/dev/null; wait "$CONT_PID" 2>/dev/null
  [ "$WINE_RUN" = 1 ] && wineserver -k 2>/dev/null
  sleep 1
}

cont_case() {
  local nm=$1 mode=$2 kind=$3 rows=$4 want_n=$5
  shift 5
  launch_cont "$nm" "$mode"
  if ! wait_log "$nm joined the game" 120; then echo "  FAIL  $nm: no join"; ERR=1; stop_cont; return; fi
  local cmd
  for cmd in "$@"; do echo "execute at $nm run $cmd" >&3; done
  sleep 1
  echo "give $nm stone 1" >&3; echo "give $nm dirt 1" >&3
  local i
  for i in $(seq 1 60); do grep -q 'contui open' "$W/$nm.out" 2>/dev/null && break; sleep 1; done
  local line; line=$(grep 'contui open' "$W/$nm.out" | head -1)
  case "$line" in
    *" n=$want_n "*) ;;
    *) echo "  FAIL  $nm: container did not open as expected (want n=$want_n): ${line:-no contui line}"; ERR=1; grep -E "contui|window" "$W/$nm.out" | head -5; stop_cont; return;;
  esac
  local wp; wp=$(win) || { echo "  FAIL  $nm: no window"; ERR=1; stop_cont; return; }
  xdotool windowfocus "$wp" 2>/dev/null
  sleep 2
  xdotool mousemove --window "$wp" 5 5; sleep 0.8
  if shot "$wp" "$nm"; then
    python3 -I "$CHECK" cont "$W/$nm.ppm" "$MC_VANILLA_JAR" "$kind" "$rows" || { ERR=1; python3 -I -c "from PIL import Image; Image.open('$W/$nm.ppm').save('$W/$nm.png')"; }
  else
    echo "  FAIL  screenshot $nm"; ERR=1
  fi
  stop_cont
}

run_cont_stage() {
  cont_case HudCa 3 generic 3 27 "setblock ~1 ~ ~ minecraft:chest"
  cont_case HudCb 3 generic 6 54 "setblock ~1 ~ ~ minecraft:chest[type=left,facing=east]" "setblock ~1 ~ ~1 minecraft:chest[type=right,facing=east]"
  cont_case HudCf 4 furnace 3 3 "setblock ~1 ~ ~ minecraft:furnace"
  cont_case HudCw 5 craft 3 10 "setblock ~1 ~ ~ minecraft:crafting_table"
}
