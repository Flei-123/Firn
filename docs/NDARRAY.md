# NDARRAY — Entwurf eines n-dimensionalen Arrays für Firn (r318, NUR ENTWURF)

Stand 08.10.2026. **Nichts davon ist gebaut.** Alle Zahlen und Zeilenangaben sind in den genannten Dateien nachgelesen
(Zeilen beziehen sich auf den Stand von heute); was nicht belegt ist, steht als **unbekannt** da.

## 1. Kurzfassung

- Der konkrete Bedarf ist **kleiner und anders**, als der Auftrag vermuten ließ:
  - **LogicLab** hat den einzigen echten Matrix-Kern (dichte LU, n×n, f64) — aber in **TypeScript**, nicht in Firn. Das FEM-Backend ist
    **Python/scipy, dünn besetzt** (NdArray deckt das nicht ab).
  - **OpenPlan-Thermal** braucht **kein** NdArray: skalare Intervallhalbierung über 1-D-Tabellen (126 Werte) und ≤ 100 Schichten.
  - **Certus** selbst hat keinen Media-Code. Die Medien-Dekoder liegen in der **Firn-Lib** (`lib/ton/mp3.fi`, `lib/jpeg`, `lib/webp`,
    `lib/audio`, `lib/paint`) und arbeiten mit **rohen `u64`-Adressen** — dort fehlt ein typisierter 1-D/2-D-**View**, keine Matrix-Algebra.
- Empfehlung: **klein anfangen** — Typ + Views + Elementzugriff + elementweise Ops + Reduktionen + `matmul` + dichte LU (alles skalar, f32/f64/i32/u8/i16).
  SIMD erst danach und zuerst für **f32** (die Intrinsics gibt es), f64 braucht neue Compiler-Intrinsics.
- Größtes Risiko: Firn hat **keine Operator-Überladung** (SPEC.md Z. 84, 486) → kein `a[i, j]`, kein `a + b`; alles sind Funktionsaufrufe.

## 2. Bedarf aus den Projekten (belegt)

### 2.1 LogicLab-Solver (`/root/projects/logiclab`, Branch main)

Sprache: **TypeScript** (Browser/Worker) + **Python** (FEM-Backend). Nichts davon ist Firn; der Bedarf ist ein *Portierungs-/Vergleichsziel*.

| Stelle | Datenform | Operationen | Größe |
|---|---|---|---|
| `src/simulators/electronic/mna.ts:357-386` `LU.factor` | 2-D dicht, **zeilenweise** `a[i*n+j]`, `Float64Array`, `Int32Array perm` | LU mit Teilpivotierung, **O(n³)**, Zeilentausch (Z. 372-374), Rang-1-Update (Z. 378-383) | n = Knoten + Zweige; **typische Größe unbekannt** (kein Benchmark/Test mit Zahl gefunden) |
| `mna.ts:388-401` `LU.solve` | 1-D `Float64Array` | Vorwärts-/Rückwärtseinsetzen, O(n²) | wie oben |
| `mna.ts:881-917` `stampG/stampVcvs/…`, `buildBase` Z. 1033-1039 | `A`: n×n f64, `A[k*n+k] += GMIN` | Streu-Addition („stamp“) in einzelne Elemente, `fill(0)`, `set(base)` | n² Elemente |
| `mna.ts:1455-1492` `solvePoint` | `A`, `rhs`, `x`, `xNew` (1-D f64) | **LU in jeder Newton-Iteration** (Z. 1481), bis `MAX_NR = 150` (Z. 289), Konvergenz-Schleife (Z. 1487-1491), `x.every(isFinite)` | 1 Faktorisierung je Iteration → O(150·n³) Worst-Case |
| `mna.ts:2195-2250` `ComplexLU` | dicht, Real- und Imaginärteil in **getrennten** Arrays | dieselbe LU komplex (AC-Analyse) | wie oben |
| `src/simulators/electronic/eigen.ts` | n×n f64, 1-basiert als Array von Zeilen | Balancing, Hessenberg, QR (EISPACK `balanc/elmhes/hqr`), O(n³) | Kommentar Z. 1-4: „bis zu einigen hundert Unbekannten“ |
| `src/simulators/electronic/fourier.ts` | 1-D Abtastwerte | Harmonische per Schleife (Z. 87-113), **keine FFT** | unbekannt |
| `backend/app/fem/solver.py:19-20,143` | **dünn besetzt** (`coo_matrix`, `splu`), `np.ndarray` 1-D/2-D | FEM-Netz, `splu`, `bincount`, `meshgrid` | `(nx+1)(ny+1) ≤ 160 000` (`spec.py:71`), `nx,ny ≤ 2000` (`spec.py:82-83`) |

Folgerung: dichte **f64-Matrix + LU/solve** (Zeilen-major, kein Stride nötig) deckt MNA/Eigen ab. Dünn besetzte Löser (FEM) sind **außerhalb** dieses Entwurfs.
Eine FFT braucht LogicLab nach dem Code **nicht**.

### 2.2 OpenPlan-Thermal (`/root/projects/openplan/src/core/cabinet/thermal.fi`, 837 Z.; `boardheat.fi` 64 Z.)

- Datenform: nur **1-D `Vec[f64]`**. Drei Fluss-Tabellen `qv/qt/qb` mit je 126 Werten (`solve_profile` Z. 475-598, Füllen Z. 491-497; `interp` Z. 444-452),
  `s_layer`, `m_if`, `tl` mit `layers` Einträgen (Standard 10, erlaubt 2..100: Z. 220-222).
- Operationen: **skalare** Intervallhalbierung — äußere Schleife 60× (Z. 556; `th_solve` Z. 690 ebenfalls 60×), je Schicht innere Schleife 50× (Z. 570), lineare Interpolation (`interp`),
  Prefix-Summen (Z. 519-535). Kein Matmul, keine Gleichungssystem-Lösung, kein 2-D-Gitter. `boardheat.fi` enthält nur Schleifen über Bauteillisten (Z. 14-50).
- Grenzen der Rechenmenge: höchstens 60 · 100 · 50 = 300 000 `layer_loss`-Aufrufe je Lösung (aus den Schleifengrenzen); gemessene Laufzeit: **unbekannt**.
- Schmerz heute: `(*vec_elem_ptr[f64](&v, i as usize))` statt `v[i]` (Z. 121, 609 und im ganzen `solve_profile`) — ein typisierter Elementzugriff würde Lesbarkeit
  bringen, **kein Tempo**.
- **Fazit: NdArray ist hier nicht begründet.** Ein `Vec[f64]`-Hilfsmodul (`vec_at` gibt es nur für `T: Scalar`, und `Scalar` schließt Gleitkomma aus — siehe 3.4) wäre der kleinere Schritt.

### 2.3 Medien („Certus-Media“)

`/root/certus` (Browser-Repo, `lib/{browser,css,dom,font,html,js,layout,net,paint}`) enthält **keinen** MP3-/Audio-/Video-Code (grep nach `mp3|imdct|polyphase|fft`: nur `docs/ROUND71.md`,
`lib/dom/ua_data.fi`). Die Medien-Kerne liegen in **Firn** (`/root/firn/lib`):

| Stelle | Datenform / Speicher | Operationen | Größe / Messung |
|---|---|---|---|
| `lib/ton/mp3.fi:387-431` `Mp3Dec`, `Mp3Scratch` | feste Arrays `[[f32;288];2]`, `[f32;960]`, `grbuf [[f32;576];2]`, `syn [[f32;64];33]`, `ist_pos [[u8;39];2]` | Granule 576 Werte je Kanal | Größen belegt (576 / 64×33 / 288) |
| `mp3.fi:103-127` `adr4/lf/sf/adr2/s16/lb/sb` | **rohe `u64`-Adressen**, Index mit `+%`/`*%` (Z. 100-105 begründet: ohne Sprung/Phi, damit der Compiler die Adresse in den Befehl faltet) | typfreie Element-Zugriffe f32 / i16 / u8 | — |
| `mp3.fi:1697-1838` `synth` | 1-D f32, Fenstertabelle `win`, Ring-Puffer `lins`, 4 Summen als Einzelvariablen | Polyphasen-Filter: **Faltung mit Fenster**; „hier verbringt der Dekoder etwa zwei Drittel seiner Zeit“ (Kommentar Z. 1716ff, `docs/TON1.md`) | 15 Durchläufe × 8 Fensterhälften × 4 Stimmen (Kommentar) |
| `mp3.fi:1397-1630` `dct_ii_4`, `dct_ii` | f32, 4 Spalten gleichzeitig, `__v128_*` (48 Loads, 36 `addf32`, 32 `mulf32`, 24 `subf32`, 10 `shuffle32` in der Datei) | DCT-II (32 Punkte) als Folge von Vektor-Add/Mul | — |
| `mp3.fi:1134-1204` `l3_imdct36`, `1090` `l3_dct3_9` | f32, 18 Werte je Band | IMDCT 36 (matmul-artig, fest ausgerollt) | — |
| `mp3.fi:760-902` `l3_huffman` | Bit-Strom, u8/i32 | Huffman-Dekodierung — **kein** Array-Problem | — |
| `lib/jpeg/jpeg.fi:583-666` `idct` | 8×8-Block `i16` (`blk: u64`), Hilfsfeld `[i32;64]`, Ausgabe Zeilen mit `stride` | 2-D-IDCT, Spalten- dann Zeilendurchgang; `upsample` (Z. 996), `ycc` (Z. 1129) je Pixel | Block 8×8 (Code); Bildgrößen: unbekannt |
| `/root/certus/lib/paint/canvas.fi:63`, `png.fi:294` | `px: *mut u8`, **w·h·4 RGBA** (vormultipliziert bzw. gerade), `w,h,cap` | Blend, Füllen, Kopieren von Zeilen | Größe je Seite; unbekannt |
| `lib/audio/pcm.fi:33-50` `pcm_scale` | `i16` LE, Byte für Byte aus `u64`-Adresse gelesen | elementweises Skalieren mit Sättigung | — |

Messungen (Quelle `docs/TEMPO15.md`, `release-fast`, 8 s Ton, Callgrind, ausgabe **bit-gleich** zu minimp3):
gesamt **91,64 M Instruktionen** (vorher 107,61 M), `minimp3` in C mit `gcc -O2`: **74,8 M**; `synth+scale_pcm4` 25,97 M (C 22,7 M); `l3_imdct36+l3_dct3_9`
16,25 M (C 12,9 M); `dct_ii_4+dct_ii` 10,82 M (C 11,1 M); `l3_huffman` 15,28 M (C 12,1 M).

Folgerung: Der Bedarf heißt **typisierte, bounds-lose 1-D/2-D-Views über vorhandenen Speicher** (`*mut u8`, `Buf`, feste Arrays) — der Dekoder darf durch
den View **nicht langsamer** werden (TEMPO-Runden haben gerade Adress-Arithmetik gefaltet, `mp3.fi:100-105`). FFT: in keinem der Medienpfade belegt.

## 3. Entwurf

### 3.1 Typ

```firn
struct NdArray[T] {
    ptr: u64,            // address of element [0,0,...] (offset already applied)
    rank: u32,           // 1..4
    shape: [usize; 4],   // unused dims = 1
    stride: [isize; 4],  // in ELEMENTS, may be negative (reversed view), unused dims = 0
    cap: usize,          // elements owned by this array; 0 = a view (never frees)
}
```

- **Rang ≤ 4**: 1-D (PCM, Vektoren), 2-D (Matrizen, Plane, RGBA als `[h][w*4]`), 3-D (`[h][w][c]`), 4-D Reserve (Batch). Mehr ist **unbekannt** und nicht belegt.
- **Strides in Elementen, vorzeichenbehaftet**: `row(i)`, `col(j)`, `transpose`, `slice`, `reverse` sind Kopie-freie **Views** (nur `ptr/shape/stride` ändern sich).
  Besitz wie bei `Vec[T]`: wer `nd_new` aufruft, ruft `nd_free`; Views geben nie frei (`cap == 0`). **Keine Lebensdauer-Prüfung** (wie `Vec`/`Buf` heute).
- **Speicher**: `rt.heap_alloc` (Seiten-ausgerichtet, null-initialisiert, mit Cache; `lib/std/rt.fi:155`) — also automatisch 16-Byte-ausgerichtet. **Nicht `Gc[T]`**:
  GC-Heaps sind thread-lokal (SPEC.md §7) und die Medien-Pfade sind `#[no_gc]`. Zusätzlich `nd_from_ptr(ptr, shape, stride)` als View auf fremden Speicher
  (z. B. `Canvas.px`, `Mp3Scratch.grbuf`, `Buf`).
- **Elementtyp** über Generics `NdArray[T]`, Monomorphisierung (SPEC §6.1) — kein Laufzeit-dtype. Getestet (alter `firnc`, `/tmp`-Prototyp, gelöscht): ein
  `struct Nd[T]{…}` mit `shape: [usize;4]` und `fn at2[T](a:*mut Nd[T],i,j)->T` **übersetzt und läuft für `f64`**, auch `fn add[T](a:T,b:T)->T { a + b }`.
  **Aber:** die Schranke `T: Scalar` **lehnt `f64` ab** („allowed are integers, bool and pointers“) — also **ohne Schranke** schreiben und per `comptime assert` absichern
  (Fehler erst bei Instanziierung, SPEC §6.1). Unterstützte Elementtypen: `f32 f64 i32 u8 i16`; `bool`/Zeiger/Struct: unbekannt/nicht Ziel.
- **Kein `a[i, j]`, kein `a + b`** (keine Operator-Überladung): `nd_at2(&a, i, j)`, `nd_set2(&a, i, j, v)`, `nd_add(&out, &a, &b)` — Elementzugriff muss **inline** und
  ohne Aufrufkosten sein; Messpunkt in 5.

### 3.2 Minimale Operationen (Stufe 1, alle skalar)

| Gruppe | Funktionen | Aufwand |
|---|---|:-:|
| Anlegen/Frei | `nd_new[T](shape)`, `nd_zeros`, `nd_free`, `nd_from_ptr`, `nd_len`, `nd_is_contiguous` | S |
| Zugriff | `nd_at1/2/3`, `nd_set1/2/3` (ohne Prüfung; `_chk`-Variante mit Position) | S |
| Views | `nd_slice(a, axis, lo, hi, step)`, `nd_row`, `nd_col`, `nd_transpose`, `nd_reshape` (nur zusammenhängend), `nd_reverse` | S–M |
| Elementweise | `nd_fill`, `nd_copy`, `nd_add/sub/mul/div`, `nd_scale`, `nd_axpy`, `nd_map(fn)` | S |
| Reduktionen | `nd_sum`, `nd_min`, `nd_max`, `nd_dot`, `nd_argmax` | S |
| Matrix | `nd_matmul(out, a, b)` (zeilenweise Schleife, B^T-Trick/Blockung), `nd_matvec` | M |
| Lineare Algebra | `nd_lu(a, perm)`, `nd_lu_solve` (dicht, Teilpivot, f64) — Port von `mna.ts:359-401` | M |

Eigenes Modul `lib/num/nd.fi` (Kern), `lib/num/nd_linalg.fi` (LU/solve), damit der Kern klein bleibt.

### 3.3 SIMD-Anbindung

Vorhanden (`compiler/src/simd.rs:276ff`, `SPEC.md` §8.6, `docs/TEMPO*.md`): `v128` ohne Operatoren; **f32x4**: `__v128_addf32/subf32/mulf32`, `cmplt/cmple/cmpnlt_f32`,
`trunc_f32_i32`, `cvt_i32_f32`; i32/u8: `add8/add32/add64/sub32`, `and/or/xor/andnot`, `shuffle32/8`, `unpack*`, `load` (**unaligned**, `movdqu`, `simd.rs:1116`) und `store`.
Backends: x86_64 SSE2 (`simd.rs`), aarch64 NEON (`simd_a64.rs:375ff`), wasm SIMD (`codegen_wasm.rs:2837ff`).

**Fehlt** (für NdArray nötig; im Quelltext nicht gefunden): `f64x2` (add/mul), Division, `sqrt`, `min/max`, Broadcast (`mp3.fi:1377` baut `bcast` selbst per Load+Shuffle),
horizontale Summe, FMA. 256-Bit ist **bewusst nicht** vorgesehen (SPEC §8.6, „Deliberately not“). Der Optimierer vektorisiert nicht automatisch
(`docs/BENCHMARKS.md`: „No auto-vectorisation“; `bench/RESULTS.md`: `matmul` ist der größte Rest).

Plan: (a) Stufe 2a **f32**: `nd_dot/axpy/add/mul/matmul_f32` mit den vorhandenen Intrinsics, Rest-Schleife skalar, Pfad per `comptime if T == f32`. (b) Stufe 2b **f64**: neue
Intrinsics `__v128_addf64/mulf64/subf64` (3 Backends, je M) — erst wenn 2a zeigt, dass es sich lohnt. (c) AVX/`--cpu=avx` nur Dreioperanden-Form, keine 256 Bit.

### 3.4 Was NICHT enthalten ist

Dünn besetzt (FEM/`splu`), komplexer Elementtyp (getrennte Re/Im-Arrays wie `ComplexLU`), Broadcasting, Fancy-Indexing, einsum, **FFT** (kein Bedarf belegt),
Rang > 4, Laufzeit-dtype, GPU, Autodiff, String-Elemente, Operator-Syntax, Lebensdauer-Prüfung, Thread-Parallelität (unbekannt, ob `#[shareable]` auf solche Structs passt).

## 4. Aufwand und Reihenfolge

| # | Baustein | Aufwand | Abhängig von |
|---|---|:-:|---|
| 1 | `NdArray[T]` + Anlegen/Frei/View-Ableitung + `at/set` (1-3-D) + Tests (4 Stufen) | M | — |
| 2 | Elementweise Ops, Reduktionen, `fill/copy` | S | 1 |
| 3 | `nd_from_ptr` an `mp3.fi` (`grbuf`, `syn`), `jpeg.idct` (Plane, stride) und `Canvas.px` als **Pilot**; Messung ohne Verlust | M | 1 |
| 4 | `nd_matmul` skalar (+ Messplan 5) | M | 1, 2 |
| 5 | `nd_lu`/`nd_lu_solve` f64 (Port von `mna.ts`) + Vergleich zu Node/TS | M | 1 |
| 6 | SIMD f32 (`dot/axpy/matmul`) | M | 4 |
| 7 | f64x2-Intrinsics in 3 Backends | L | 6 zeigt Nutzen |
| 8 | Dünn besetzte Matrizen / FFT | L, **nicht geplant** | Bedarf erst belegen |

Reihenfolge: **1 → 2 → 3 → 4 → 5 → 6**, 7 nur nach Messung. Gesamtschätzung (Aufwand, nicht gemessen): 1-5 ≈ M+S+M+M+M, 6 ≈ M, 7 ≈ L.

## 5. Messplan

1. **Basis, schon vorhanden**: `bench/firn/matmul.fi` (240×240 `i32`, 3 Durchläufe, rohe Adressen) gegen `bench/rust/matmul.rs`: `release-fast` 0,042 s gegen `rustc -O` 0,024 s = **1,77×**
   (`bench/RESULTS.md` Z. 16, Rechner EPYC 7571). Das ist die Latte, die ein NdArray-`matmul` **nicht unterschreiten darf** (Ziel-Vorschlag: höchstens gleich langsam).
2. **View-Overhead** (Kennzahl A): dieselbe Schleife (a) mit rohen Adressen (wie `matmul.fi`), (b) mit `nd_at2`. Maß: Instruktionen (Callgrind, wie TEMPO) und Zeit; Ziel-Vorschlag **≤ 5 %** Mehraufwand.
3. **matmul f64/f32, n = 128, 256, 512** gegen (a) Rust-Schleife ohne Crate, (b) C `gcc -O2` und `-O3` (vektorisiert), (c) Python/numpy wenn vorhanden. Kennzahl B: **GFLOP/s = 2n³/t** und Verhältnis zu Rust.
4. **LU/solve f64**, n = 50, 100, 200, 400 (Größen aus `eigen.ts`-Kommentar; echte MNA-Größe unbekannt) gegen `mna.ts` unter Node (gleiche Matrizen). Kennzahl C: Zeit je Faktorisierung.
5. **Pilot-Nichtverschlechterung** (Kennzahl D): MP3 8 s, Callgrind-Gesamtinstruktionen vor/nach View-Umbau (jetzt 91,64 M); Ausgabe bit-gleich zu `ref60.pcm`.
6. Alles `release-fast` und `release-safe`, 9 Läufe Median (wie `bench/run.sh`), Maschine unter Last angeben.

## 6. Unbekannt / offene Fragen

- Typische MNA-Größe n in LogicLab (kein Benchmark gefunden) und ob LogicLab je nach Firn portiert wird (heute TS + Python).
- Laufzeit von `solve_profile` (nicht gemessen); ob OpenPlan überhaupt 2-D-Wärmefelder plant.
- Bildgrößen/Frame-Raten der Medienpfade; es gibt **keinen Video-Dekoder** in Certus oder Firn (grep) — „Video-Puffer“ aus dem Auftrag: nicht belegt.
- Ob `a + b` in einer Funktion mit `T`-Schranke außer `Scalar` (z. B. künftiges `Num`) vorgesehen ist; heute nur ohne Schranke.
- Verhalten von `NdArray` über Threads (`#[shareable]`), wasm/aarch64-Messwerte, Windows.
- Ob der Optimierer `nd_at2` (Stride-Multiplikation je Zugriff) so faltet wie `adr4` (`TEMPO14/15`, Strength Reduction): erst die Messung 2 zeigt es.
