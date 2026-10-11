# mc_texture -- Atlas + Block->Tile-Tabellen (eigenes Standardpaket, keine Mojang-Dateien)

`lib/mc/mc_texture.fi`, Test `tests/2751_mc_texture.fi`, Cross-Check `bash tools/mc/texture/run.sh`
(`RESULT mc-texture: ok`).

## Atlas
* `atlas_new(tile, cols, rows)`, Standard `atlas_default()` = 16x16 Kacheln zu 16 Texeln = 256x256 RGBA8
  (Bytes R G B A; u32 little endian = 0xAABBGGRR). Tile-ID t >= 1 liegt bei Spalte (t-1)%cols, Zeile (t-1)/cols; 0 = keine Flaeche.
* `atlas_tile_uv(a,t,&[f64;4])` = (u0,v0,u1,v1), v waechst nach unten (wie Pixelzeilen).
* Sampling **nearest** mit Kachel-lokalen Koordinaten: `atlas_texel(a,t,tx,ty)` (wrap per Maske) und
  `atlas_sample(a,t,u,v)` (u,v in Kachel-Einheiten; w > 1 wiederholt, so wie Greedy-Quads u/v aus mc_mesh).
  Wrap passiert IN der Kachel -> kein Bluten, kein Padding.
* Mipmaps: nicht gebaut. `atlas_avg(a,t)` = Durchschnittsfarbe je Kachel (fuer ferne Flaechen).
* Eigene Texturen: `atlas_set_tile(a,t,rgba_ptr)` (z. B. nach `paint.png.decode_png`; PNG-Decoder existiert in `lib/paint/png.fi`).

## Standardpaket (prozedural, deterministisch, ganzzahlig)
IDs 1 stone, 2 dirt, 3 grass_top, 4 grass_side, 5 sand, 6 gravel, 7 log_side, 8 log_top, 9 planks, 10 leaves (Alpha-0-Loecher),
11 water (Alpha 190), 12 cobble, 13 bedrock, 14 snow, 15 ice, 16 glass (Alpha 0 innen), 17 sandstone, 18 ore, 19 lava, 20 bricks,
21 netherrack, 22 clay, 23 deepslate, 24 terracotta, 25..40 die 16 Farbstoff-Farben, 41..56 16 Hash-Farben,
57 unknown (magenta/schwarz), 58 test (rot/gruen/blau/weiss, schwarzer Rand, Marker oben links - Ausrichtungstest).
Alpha-Konvention: 0 = Cutout (verwerfen), 190 = durchscheinend (Wasser), 255 = deckend. Der Software-Renderer v1 verwirft Alpha < 128.

## Block-Zustand -> Kachel (`tex_build_tables(&registry, &MeshTables)`)
Fuellt `opaque` (solid und nicht transparent) und `tiles` (u16[32768*6], Reihenfolge -x +x -y +y -z +z) fuer jede Version, deren
Registry < 32768 Zustaende hat. Regeln in dieser Reihenfolge: unsichtbar (air, cave_air, void_air, barrier, light, structure_void) = 0;
exakte Namen (grass_block oben/Seite/unten, stone, dirt, sand, ...); Endungen `_planks`, `_log/_wood/_stem/_hyphae` (Seiten Rinde,
oben/unten Ringe), `_leaves`, `_ore`, `_glass`, `_terracotta`; Farb-Praefix (white_, light_blue_, ...) bei soliden Bloecken;
jeder andere SOLIDE Block -> eine der 16 Hash-Kacheln aus dem Namen (FNV-1a); nicht-solide (Pflanzen, Fackeln, Schienen) = 0, bis es
Kreuz-/Teilmodelle gibt. Treppen/Stufen/Zaeune sind vorerst volle Wuerfel ihres Materials; Achse/Ausrichtung wird ignoriert (Logs immer senkrecht).

## Gemessen / geprueft
Atlas bauen ~2,5 ms, Tabellen fuer 24135 Zustaende ~3 ms. Unabhaengige Python-Neuableitung aus minecraft-data (`blocks.json`,
`minStateId/maxStateId`, eigener Regel-Nachbau): alle 24135 Zustaende x 6 Flaechen + Opaque-Flag identisch (19429 Zustaende mit sichtbarer
Flaeche); einfache Rausch-, Farb-, unknown- und Test-Kacheln pixelgenau; gemusterte Kacheln (grass_side, log, planks, leaves, ore, cobble,
bricks, glass, sandstone) nur strukturell (Alpha, Rand, Farbverlauf) -- ihr Muster ist NICHT pixelweise unabhaengig geprueft. 3 Build-Stufen
byte-gleich, 3 Gegenproben.
