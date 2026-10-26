#!/usr/bin/env bash
run_trader_stage() {
  local nm=Player21 cnt line i xy tr
  case "$MCV" in 1.20.*|1.19.*|1.18.*) cnt="Count";; *) cnt="count";; esac
  case "$cnt" in Count) tr="{buy:{id:\"minecraft:emerald\",Count:10b},sell:{id:\"minecraft:iron_pickaxe\",Count:1b},maxUses:99,xp:5,priceMultiplier:0.05f}";; *) tr="{buy:{id:\"minecraft:emerald\",count:10},sell:{id:\"minecraft:iron_pickaxe\",count:1},maxUses:99,xp:5,priceMultiplier:0.05f}";; esac
  env "${CJENV[@]}" MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_CONT_UI=6 MCPLAY_PROGRESS=1 MCPLAY_CONT_LOG=1 MCPLAY_RUN_S=60 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
      timeout 170 "$W/mcplay" 127.0.0.1 "$PORT" "$nm" > "$W/$nm.out" 2>&1 &
  PP=$!
  for i in $(seq 1 90); do grep -q "$nm joined the game" "$W/server.log" 2>/dev/null && break; sleep 1; done
  for i in $(seq 1 60); do grep -q 'mcplay: progress .* chunks=[1-9][0-9]' "$W/$nm.out" 2>/dev/null && break; sleep 1; done
  sleep 3
  echo "execute at $nm run fill ~-3 ~-1 ~-3 ~3 ~-1 ~3 minecraft:stone" >&3
  echo "execute at $nm run fill ~-3 ~ ~-3 ~3 ~3 ~3 minecraft:air" >&3
  echo "kill @e[type=minecraft:villager]" >&3
  echo "kill @e[type=minecraft:item]" >&3
  echo "gamemode survival $nm" >&3
  echo "effect give $nm minecraft:hero_of_the_village 600 0 true" >&3
  echo "give $nm emerald 64" >&3
  echo "execute at $nm run summon minecraft:villager ~ ~ ~2 {NoAI:1b,Invulnerable:1b,VillagerData:{profession:\"minecraft:toolsmith\",level:2,type:\"minecraft:plains\"},Offers:{Recipes:[$tr]}}" >&3
  for i in $(seq 1 45); do grep -q 'contui cell trade2 ' "$W/$nm.out" 2>/dev/null && break; sleep 1; done
  line=$(grep 'contui open' "$W/$nm.out" | head -1)
  case "$line" in
    *"kind=8 "*)
      sleep 1.5
      for i in $(seq 1 10); do grep -q 'mcplay: trades ' "$W/$nm.out" && break; sleep 0.5; done
      sleep 1
      xy=$(grep 'contui cell trade0 ' "$W/$nm.out" | head -1 | sed -n 's/.* x=\([0-9]*\) y=\([0-9]*\).*/\1 \2/p')
      xdotool mousemove $xy; sleep 0.5; xdotool mousedown 1; sleep 0.25; xdotool mouseup 1; sleep 1.5
      if [ -n "${TRADER_SHOT:-}" ]; then
        local wid; wid=$(xdotool search --name mcplay 2>/dev/null | head -1)
        xwd -id "$wid" -silent -out "$W/trader.xwd" && python3 -I tools/mc/menu/xwd_to_ppm.py "$W/trader.xwd" "$TRADER_SHOT"
      fi
      echo "data get entity $nm Inventory" >&3; sleep 2
      xy=$(grep 'contui cell ws=2 ' "$W/$nm.out" | head -1 | sed -n 's/.* x=\([0-9]*\) y=\([0-9]*\).*/\1 \2/p')
      xdotool mousemove $xy; sleep 0.5; xdotool keydown shift; sleep 0.2; xdotool mousedown 1; sleep 0.25; xdotool mouseup 1; sleep 0.3; xdotool keyup shift; sleep 1.5
      xdotool key Escape; sleep 1.5
      echo "data get entity $nm Inventory" >&3; sleep 2;;
    *) echo "  FAIL  trader ui: villager window did not open: ${line:-no contui line} $(grep 'contui FAIL' "$W/$nm.out" | head -1)"; ERR=1; xdotool key Escape;;
  esac
  wait $PP
  local tl; tl=$(grep 'mcplay: trades ' "$W/$nm.out" | head -1)
  echo "  $tl"; echo "  $(grep 'mcplay: trade click' "$W/$nm.out" | head -1)"
  python3 -I tools/mc/play/trader_check.py "$tl" "$(grep "$nm has the following entity data" "$W/server.log" | tail -1)" 64 || ERR=1
}
