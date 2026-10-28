#!/usr/bin/env bash
player_shot() {
  local nm=$1 name=$2 x=$3 y=$4 wid=$5
  xdotool mousemove --window "$wid" "$x" "$y"; sleep 1
  xwd -id "$wid" -silent -out "$W/$nm-$name.xwd" && python3 -I tools/mc/menu/xwd_to_ppm.py "$W/$nm-$name.xwd" "$W/$nm-$name.ppm"
}

run_player_stage() {
  local nm=Player40 wid i
  env "${CJENV[@]}" MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_INV_PLAYER_FREEZE=1 MCPLAY_RUN_S=50 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
      timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" "$nm" > "$W/$nm.out" 2>&1 &
  PP=$!
  for i in $(seq 1 90); do grep -q "$nm joined the game" "$W/server.log" 2>/dev/null && break; sleep 1; done
  sleep 2
  echo "gamemode survival $nm" >&3
  sleep 2
  wid=$(xdotool search --name mcplay 2>/dev/null | head -1)
  xdotool key e; sleep 2
  if player_shot "$nm" left 10 150 "$wid" && player_shot "$nm" right 630 150 "$wid" && player_shot "$nm" up 10 5 "$wid"; then
    python3 -I tools/mc/play/hud_check.py invplayer "$W/$nm-left.ppm" "$CJAR" "$W/$nm-right.ppm" "$W/$nm-up.ppm" || ERR=1
  else
    echo "  FAIL  inv player: screenshot"; ERR=1
  fi
  xdotool key Escape; sleep 1
  wait $PP
}
