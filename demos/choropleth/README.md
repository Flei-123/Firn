# demos/choropleth -- a map, drawn by Firn itself

GeoJSON in, PNG out, in one process, with nothing but this repository: no
image library, no font library, no cartography library, no C.

```
                lib/std/io.fi        the file
                lib/std/json.fi      3 MB of GeoJSON -> nodes
                main.fi              lon/lat -> pixels
                lib/font/raster.fi   rings -> coverage per pixel
                lib/paint/stroke.fi  borders -> one outline per ring
                lib/paint/canvas.fi  coverage -> pixels
                lib/font/ttf.fi      the labels, from a real TrueType file
                lib/paint/png.fi     pixels -> PNG
                lib/std/deflate.fi   and its compression
```

## Run it

The map data is **Natural Earth** `ne_50m_admin_0_countries` (public
domain, https://www.naturalearthdata.com/), as GeoJSON, at
`/tmp/eu.geojson`; the labels use DejaVu Sans Bold from
`/usr/share/fonts/truetype/dejavu/`. Both paths stand at the top of
`main()`.

```sh
export FIRNLIB=$(pwd)/lib
compiler/target/release/firnc -o build/choropleth demos/choropleth/main.fi
./build/choropleth      # writes /tmp/firn_map.png
```

Measured on this machine (AMD EPYC 7571, one core): **2.9 s**, 245 rings,
1540 x 1665 pixels, 412 KB of PNG. About two thirds of that is the JSON
parser and `deflate`; the drawing itself is under a second.

## What the file is for

It is the load test for three things `lib/` could not do before the round
FIRN-LUECKEN, and it says so at every place where it used to work around
them:

* `const` could not hold a floating point number, so the map window was
  four **functions** returning constants. It is four `const f64` now.
* `lib/paint` could fill but not stroke, so a border was a **quadrilateral
  per segment**, built in this file. It is `paint.stroke` now -- with
  joins, caps, and a stroke that can sit inside its outline, which is what
  finally gives Malta a border without eating its colour.
* `raster_begin` cleared the whole window, so every polygon opened a
  **window of its own** computed from its bounding box. The rasteriser
  clears what it wrote now, so there is one window: the picture.

The file lost 55 lines that way (588 -> 533) and got a border on every
island it draws.
