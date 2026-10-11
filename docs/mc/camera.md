# mc_camera -- Kamera: View, Projektion, Frustum, Pick-Ray

`lib/mc/mc_camera.fi`, Tests `tests/2750_mc_camera.fi`, Cross-Check `bash tools/mc/camera/run.sh`
(`RESULT mc-camera: ok`). Baut auf `mc_math` (Mat4, Frustum, `grid_raycast`).

## Festlegungen
* **Alles f64** (wie mc_math). GPU-Upload: `mc_math.mat4_to_f32`; weit weg vom Ursprung vorher
  `camera_rel` (Weltpunkt - Kameraposition, in f64) abziehen und die kleinen Werte als f32 hochladen.
* Welt rechtshaendig: +x Ost, +y oben, +z Sued. **yaw 0 = Sued (+z)**, 90 = West (-x), -90 = Ost (+x);
  **pitch > 0 = nach unten**, geklemmt auf [-90, 90]; yaw wird auf [-180, 180) gewickelt.
  forward = (-sin yaw cos pitch, -sin pitch, cos yaw cos pitch); right = (-cos yaw, 0, -sin yaw) (kein
  NaN bei pitch = +-90); up = right x forward.
* Pixel: (0,0) = obere linke Ecke des ersten Pixels, Mitte von Pixel (i,j) = (i+0.5, j+0.5), y waechst nach unten.
* Matrizen spaltenweise (mc_math). Standard GL-Clip-Tiefe [-1,1]; `zero_to_one = true` (vor `camera_update`
  setzen) fuer Vulkan/D3D.

## API (Zustand in `Camera`, nach jedem Setter `camera_update`)
| Funktion | Zweck |
|---|---|
| `camera_new(w,h)` | fov 70, near 0.05, far 512, Position 0, Blick Sued |
| `camera_set_pos / set_angles / set_lens(fov,near,far) / resize / look_at` | Setter (Werte werden geklemmt) |
| `camera_update` | Basis, `view`, `proj`, `vp`, `inv_vp`, `frustum` |
| `camera_to_view(c,x,y,z,&v)` | Skalar-Hotpath: Weltpunkt -> (rechts, oben, Tiefe entlang Blickrichtung) |
| `camera_project(c,x,y,z,&out)` | Weltpunkt -> Pixel (x,y) + Tiefe; `false` wenn hinter near |
| `camera_pick_ray(c,px,py,&o,&d)` | Strahl durch Pixelposition, d normiert |
| `camera_pick_block(c,&grid,px,py,reach,&hit)` | Pick-Ray + `mc_math.grid_raycast` |
| `camera_box_visible / camera_section_visible(cx,sy,cz,min_y)` | 0 aussen, 1 schneidet (konservativ), 2 innen |

## Gemessen (Server, tools/mc/camera/bench.fi, Last ~10)
camera_update 0,44 M/s, Sektions-Frustumtest 43 M/s (16 Chunks Radius x 24 Sektionen = 19 K Tests pro Frame
~0,45 ms), project + pick_ray 17 M Paare/s.

## Geprueft (unabhaengig, numpy)
Basis aus Rotationsmatrizen, View/Proj, 8 Frustumecken -> 6 Ebenen, `inv_vp` bildet den NDC-Wuerfel auf die
Ecken ab, Pick-Ray per Unprojektion, Pixel per `proj @ view`, Boxen + Sektionen per **exaktem
Separating-Axis-Test** gegen das Frustum: ein exakter Schnitt wird nie als "aussen" gemeldet, "innen" heisst
alle 8 Ecken innen. 3 Seeds x 40 Kameras (Positionen bis 3e6), 3 Build-Stufen byte-gleich, 6 Gegenproben.
Konservative Fehlalarme (1 obwohl exakt aussen): ca. 0,1 % der Boxen.
