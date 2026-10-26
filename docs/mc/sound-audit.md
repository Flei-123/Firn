# Sound audit (vanilla sounds.json 1.20.2 vs client)

Vanilla rule: the server excludes the acting player for block place, door/button/lever/trapdoor/gate, bucket, hurt, eat/drink, splash/swim. Everything else (mobs, other players, explosions, chests, attack hits, bow) is broadcast and arrives as a Sound Effect packet that mc_audio plays.

Gate: `bash tools/mc/play/run_snd.sh` (MCPLAY_AUDIO_LOG=1 prints `audiolog ... ev=<id> ... t_ms=<monotonic ms>`).

Pre-warm: mc_snd_use.su_prewarm decodes the player events and every material step/break/place/hit variant at start (lazy_ensure).

Mob hurt/death/ambient: no client prediction (server broadcasts them; Entity Status 2/3 would double them).

| event | status |
|---|---|
| ambient.underwater.enter | client (mc_snd_use, eyes-in-water toggle) |
| ambient.underwater.exit | client (mc_snd_use) |
| ambient.underwater.loop | OPEN |
| ambient.underwater.loop.additions | OPEN |
| ambient.underwater.loop.additions.rare | OPEN |
| ambient.underwater.loop.additions.ultra_rare | OPEN |
| block.fence_gate.close | client prediction (mc_snd_use interact) |
| block.fence_gate.open | client prediction (mc_snd_use interact) |
| block.fire.ambient | OPEN: client random per fire block |
| block.fire.extinguish | server packet |
| block.iron_trapdoor.close | client prediction (mc_snd_use interact) |
| block.iron_trapdoor.open | client prediction (mc_snd_use interact) |
| block.lava.ambient | client random (mc_snd_use lava scan 9x9x5, approximate rate) |
| block.lava.pop | client random (mc_snd_use lava scan) |
| block.lever.click | client prediction (mc_snd_use interact) |
| block.stone_button.click_on | client prediction (mc_snd_use interact) |
| block.wooden_button.click_on | client prediction (mc_snd_use interact) |
| block.wooden_door.close | client prediction (mc_snd_use interact) |
| block.wooden_door.open | client prediction (mc_snd_use interact) |
| block.wooden_trapdoor.close | client prediction (mc_snd_use interact) |
| block.wooden_trapdoor.open | client prediction (mc_snd_use interact) |
| entity.generic.drink | client prediction (mc_snd_use) |
| entity.generic.eat | client prediction (mc_snd_use, 200 ms cadence, only while hungry) |
| entity.lightning_bolt.impact | server packet + client weather |
| entity.lightning_bolt.thunder | server packet + client weather |
| entity.player.attack.crit | server packet |
| entity.player.attack.knockback | server packet |
| entity.player.attack.nodamage | server packet |
| entity.player.attack.strong | server packet |
| entity.player.attack.sweep | server packet |
| entity.player.attack.weak | server packet |
| entity.player.big_fall | client prediction (mc_audio walker) |
| entity.player.breath | server packet |
| entity.player.burp | client prediction (mc_snd_use, after 1.6 s eating) |
| entity.player.death | client prediction (health dead) |
| entity.player.hurt | client prediction (mc_snd_events, health packet) |
| entity.player.hurt_drown | client prediction (damage type, protocol 763 only) |
| entity.player.hurt_freeze | client prediction (damage type, protocol 763 only) |
| entity.player.hurt_on_fire | client prediction (damage type, protocol 763 only) |
| entity.player.hurt_sweet_berry_bush | client prediction (damage type, protocol 763 only) |
| entity.player.levelup | server packet |
| entity.player.small_fall | client prediction (mc_audio walker) |
| entity.player.splash | client prediction (mc_snd_events water tick) |
| entity.player.splash.high_speed | client prediction (mc_snd_events) |
| entity.player.swim | client prediction (mc_snd_events, every 1.67 blocks) |
| item.bucket.empty | client prediction (mc_snd_use, played at the player) |
| item.bucket.empty_lava | client prediction (mc_snd_use) |
| item.bucket.fill | OPEN: needs a fluid raycast (fluids are not pickable) |
| item.bucket.fill_lava | OPEN: needs a fluid raycast |
| weather.rain | client (mc_weather_snd, run_weather.sh gate) |
| weather.rain.above | client (mc_weather_snd) |

Not listed: other `block.*` (material step/break/place/hit resolved by mca_block_sound_name, rest server packets) and mob `entity.<mob>.*` (server broadcast).
OPEN: bucket fill, fire ambient, underwater loop, chest/barrel/bow/attack verified only as server broadcast (not gated).
sounds.json events total: 1485.
