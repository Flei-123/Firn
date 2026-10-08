# SCRIPT-REPLACEMENT — Kann Firn die Python-Skripte von OpenPlan ersetzen?

Stand 08.10.2026. Auftrag von Justin: **nur untersuchen und messen**, nichts
Großes umbauen. Geprüft: Firn `main` 6986c77b7 (Compiler `compiler/target/release/firnc`,
Stand 04.10.) gegen OpenPlan `/root/projects/openplan` (`tools/ci`, `tools/web`, `tools/interop`,
dazu `tools/model`, `tools/plc`, …). Python 3.11.2.

Alles in diesem Dokument ist gezählt, gelesen oder gemessen. Wo nicht: **unbekannt**.
Zahlen mit Zeit sind auf einer **ausgelasteten Maschine** gemessen (Load 12–16, andere
Läufe im `heavy`-Pool) — absolute Werte sind verrauscht, die Verhältnisse waren über je
3 Wiederholungen stabil. Messung von Hand (20–40 Läufe, Mittel), kein hyperfine.

## 0. Ergebnis in 8 Zeilen

| Frage | Antwort |
|---|---|
| Lohnt der Wechsel? | **Teilweise.** Nicht „alle 120 Skripte umschreiben“, sondern: Lücken schließen, neue Checks in Firn schreiben, das Skript mit den schweren pip-Abhängigkeiten (`webcheck.py`) zuerst. |
| Laufzeit | Ein Python-CI-Skript kostet **~50 ms** Start+Imports (`python3 -c pass` 23 ms; Standard-Importsatz 50 ms). Das Firn-Programm startet in **<1 ms** (`hello` 0,8 ms). Bei 103 Skripten sind das **~5 s** pro CI-Lauf; ein `tools/ci.sh`-Lauf lief bei der Messung schon >13 min. Gewinn **<1 %** — kein Grund. |
| Code-Menge | Prototyp: `check_lists.py` **42 Zeilen → 241 Zeilen Firn (5,7×)**; ohne den CSV-Teil (gehört in eine Lib) ~140 Zeilen (3,3×). |
| Start/Compile | Kein `firn run`. Ein Skript mit 5 Imports braucht **0,45 s Compile** (kalt) und ~16 ms Lauf. 103 Skripte kalt ≈ 46 s CPU. Mit Cache (Prototyp `firn-run`): +6 ms Overhead pro Aufruf. |
| Shebang | **Nein** — `#!` wird abgelehnt (`error: expected '[' after '#', found '!'`). Kleiner Fix oder Wrapper. |
| Libs | Da: Prozess, Datei, Hash, base64, gzip, JSON, Regex, HTTP, WS, Sockets, PNG/JPEG. **Fehlt:** CSV, glob, mkdtemp, copy_file/copytree, generischer XML-Parser, Statistik, Test-`assert`, JSON `sort_keys`. |
| numpy/PIL | `webcheck.py` ist der **einzige** numpy-Nutzer (8 Stellen). Ersatz klein (S). PNG-Decode: Firn ≈ gleich schnell wie `import numpy+PIL` + Decode (siehe 3). |
| Nicht ersetzen | `jsonschema` (12×), `lxml`/XSD, `pypdf`, `yaml`, `tkinter` — extern/Python lassen oder L-Aufwand. |

## 1. Importe in den OpenPlan-Skripten

120 `.py`-Dateien unter `tools/` (17 851 Zeilen): `ci` 103 (13 188 Z.), `model` 5 (2 628), `web` 2 (707),
`interop` 2 (204), `plc` 2 (273), `desktop` 1 (123), `bench` 2 (100), `library` 1 (338), `english*.py` 2.
Gezählt mit `ast` (Dateien, die das Modul importieren; „Aufrufe“ = Attribut-Zugriffe `modul.x`):

| Modul | Dateien | Aufrufe | Wofür |
|---|---:|---:|---|
| `sys` | 118 | 271 | `argv` (141×), `exit` (114×) |
| `os` / `os.path` | 117 | 1298 | `path.join` 619×, `dirname` 329×, `abspath` 125×, `exists` 76×; `environ` 26×, `makedirs` 22×, `listdir` 21×, `walk` 8× |
| `subprocess` | 114 | 235 | `run` 197× (`capture_output` 189×, `input=` 127×, `cwd=` 125×, `timeout=` 25×, `env=` 9×), `Popen` 11× (5 Dateien), `DEVNULL` 21× |
| `json` | 93 | 629 | `loads` 356×, `dumps` 175× (`sort_keys` 24×, `indent` 10×), `load` 70×, `dump` 28× |
| `tempfile` | 56 | 87 | `TemporaryDirectory` 48×, `mkdtemp` 39× |
| `csv` | 48 | 73 | `DictReader` 64×, `writer` 7×, `DictWriter` 2× |
| `io` | 48 | 65 | `StringIO` 64× (fast nur für `csv`) |
| `re` | 28 | 110 | `findall` 40×, `search` 22×, `compile` 14×; **2 Muster mit Lookaround**, keine Rückverweise |
| `shutil` | 22 | 81 | `copy` 30×, `rmtree` 25×, `copytree` 18×, `which` 7× |
| `time` | 12 | 101 | `sleep` 67×, `time` 19×, `perf_counter` 7× |
| `socket` | 6 | 12 | freien Port finden (`check_save`, `webcheck`), TCP-Client (`openplc_runtime_check`), AF_UNIX-Server (`model/opmodel.py`) |
| `glob` | 5 | 8 | `library/*/*.json`, `tests/golden/*/project.json`, `page*.svg` |
| `hashlib` | 5 | 10 | `sha256` 9×, `md5` 1× |
| `copy` | 4 | 13 | `deepcopy` |
| `jsonschema` | 4 | 12 | `Draft202012Validator` 8× (`parts_check`, `check_parts_*`, `model/opmodel.py`) |
| `xml.etree` | 3 | 7 | `fromstring`/`parse` (`check_tools`, `export_check`, `check_oneline`) |
| `PIL` | 3 | 12 | `Image.open(...).convert("RGB")` 11×, `ImageChops.difference().getbbox()` 1× |
| `struct` | 3 | 14 | `pack`/`unpack` |
| `urllib.request` | 2 | – | `webcheck` (CDP-Port abfragen), `openplc_upload` (Formular + Cookies) |
| `base64` | 2 | 9 | `b64decode` 7× (Screenshot aus CDP), `b64encode` 2× |
| `statistics` | 1 | 6 | `median` 6× (`webcheck`) |
| `gzip` | 1 | 1 | `compress` (Modulgröße gezippt, `webcheck`) |
| `threading` | 1 | 2 | Server im Hintergrund (`model/test_opmodel.py`) |
| `numpy` | 1 | 8 | nur `webcheck.py`: `asarray(...).astype(int32)`, `kron`, `array`, `ones`, Vergleich/`sum` |
| `websocket` | 1 | – | `webcheck.py`: `create_connection(url, suppress_origin=True)` (Chromium DevTools) |
| `http.server` | 1 | – | `check_print.py`: Fake-CUPS-Server (`BaseHTTPRequestHandler`) auf 127.0.0.1 |
| `tkinter` | 1 | – | `check_clip.py`: Zwischenablage lesen/setzen (`clipboard_get`) |
| `html.parser` | 1 | – | `check_dossier.py`: `HTMLParser`-Unterklasse prüft Tag-Balance |
| `lxml` | 1 | – | `plc_check.py`: `etree.XMLSchema` (PLCopen-XSD), optional (`skip`, wenn nicht da) |
| `pypdf` | 1 | – | `export_check.py`: PDF lesen (optional, sonst poppler) |
| `yaml` | 1 | 3 | `check_cables3.py`: `safe_load` |
| `filecmp`, `zlib`, `math`, `argparse`, `cookiejar` | je 1 | – | byteweiser Vergleich; `decompress`; `sin/cos`; 1 Parser; Cookies |

Typisches Skript (`check_lists.py`, `check_find.py`, …): `sys.argv` → `subprocess.run([OP, …], input=Datei, capture_output=True)`
→ Ausgabe als JSON/CSV/Text prüfen → `check(ok, what)` → `sys.exit(1 if failed else 0)`. Das trifft den Großteil der 103 `ci`-Skripte
(118 von 120 Dateien importieren `sys`/`os`, 114 `subprocess`).

## 2. Was hat Firn? (Belege: Pfad:Zeile in `main` 6986c77b7)

Firn-Libs sind **Low-Level**: Zeiger + `rt.Buf`, Fehler-Unions mit `catch`, kein `String`-Typ, keine Dicts mit
String-Schlüsseln. Alles geht, ist aber ausführlicher als Python.

| Python | Firn | Beleg | Status |
|---|---|---|---|
| `subprocess.run(..., input=, capture_output=, cwd=, env=, timeout=)` | `process.command/add_arg/set_cwd/set_env/run_io/run_capture` (poll-basiert, kein Deadlock, Timeout → `ProcError::Timeout`) | `lib/std/process.fi:245,267,276,340,1199,1228`; Test `tests/2062_std_process.fi` (3 MiB durch `cat`) | **da** — ~12 Zeilen pro Aufruf; Komfort-Funktion fehlt (S) |
| `Popen` + lesen + `kill` | `spawn/read_out_nb/wait_timeout/kill/kill_tree` | `process.fi:636,899,965,1015,1035` | **da** |
| `sys.argv` | `main(start: u64)` + `rt.arg_count/arg_ptr/c_length` | `lib/rt/rt.fi:502,511,518`; Beispiel `tools/fmt/firnfmt.fi:49` | **da**, roh |
| `sys.exit(n)` | `return n` aus `main` | alle Tools | **da** |
| `os.environ` | `env.get/has/list_into/find_executable` | `lib/std/env.fi:109,100,123,166` | **da** |
| `os.path.*` | `fs.path_join/path_parent/path_name`; `exists/is_dir/is_file/stat` | `lib/std/fs.fi:682,696,715,411,416,421,376` | **da** (`relpath`/`abspath`: nicht gefunden) |
| `os.makedirs/remove/rmtree/rename/chmod/symlink` | `fs.mkdir_all/remove/remove_tree/rename/chmod/symlink` | `fs.fi:184,280,310,238,639,257` | **da** |
| `os.listdir/walk` | `dir.open/next/close` (Iterator; rekursiv selbst) | `lib/std/dir.fi:73,99,143` | **da**, kein `walk` |
| `tempfile.mkdtemp/TemporaryDirectory` | nur `fs.temp_dir()` / `shell.temp_dir()`; Zufall in `lib/std/crypto/random.fi` | `fs.fi:677`, `shell.fi:230` | **fehlt** (S) |
| `shutil.copy/copytree` | — (`grep 'fn copy_file\|copy_tree\|copytree\|copy_dir' lib` leer) | – | **fehlt** (S) |
| `shutil.which` | `env.find_executable`, `shell.find_in_path` | `env.fi:166`, `shell.fi:187` | **da** |
| `glob.glob` | — (kein `glob`/`fnmatch` in Datei-APIs) | – | **fehlt** (S) |
| `open().read()/write()` | `fs.read_file/write_file/write_atomic/append_file` | `fs.fi:467,521,586,526` | **da** |
| `json.loads` | `json.json_parse` → Knoten-Array; `json_get/json_child/json_string/json_int/json_number/json_bool` | `lib/std/json.fi:789,907,932,950,943,968`; JSONTestSuite 318 Dateien | **da** (RFC 8259 strikt) |
| `json.dumps` (kompakt/`indent`) | `json_write/json_write_pretty` + Streaming `jw_*` | `json.fi:1214,1220,1249` | **da**, **`sort_keys` fehlt** (24 Nutzungen) (S) |
| `csv.DictReader/writer` | — (kein CSV in `lib/`) | – | **fehlt** (S) — Prototyp in `tools/script_port/check_lists.fi` (≈95 Zeilen, Quotes/CRLF) |
| `re` | `regex.regex_compile/regex_find/regex_replace_all` (RE2-Syntax, linear; Lookaround `(?=` `(?!` `(?<=` `(?<!` seit r320 da — polynomiell statt linear, siehe docs/SKRIPT-LIBS.md; **kein** Rückverweis → `Unsupported`) | `lib/regex/regex.fi` Kopf (Z. 1–40) | **da**; `findall` per Schleife; die 2 Lookaround-Muster laufen unverändert |
| `hashlib.sha256/md5` | `crypto.sha256(p,n,out)`, `md5_new`, `hashfile.hash_file_hex(path, algo)` | `lib/std/crypto/sha256.fi:238`, `lib/std/md5.fi:86`, `lib/std/hashfile.fi:274` | **da** |
| `base64` | `base64.encode/encode_url/decode` | `lib/std/base64.fi:74,78,85` | **da** |
| `gzip.compress` / `zlib.decompress` | `deflate.gzip_compress`, `gzip.gz_decompress`, `inflate_into` | `lib/std/deflate.fi:1738,663`, `lib/compress/gzip.fi:708` | **da** |
| `struct.pack/unpack` | `bytes.put_u16/u32/u64/f64…`, `get_*` | `lib/std/bytes.fi:340–375` | **da** |
| `time.sleep/time/perf_counter` | `time.sleep_ms/now_unix_ms/monotonic_ns` | `lib/std/time.fi:103,88,94` | **da** |
| `xml.etree` | `lib/svg/xml.fi` (Elemente, Attribute, Text, CDATA, Entities, fehlertolerant) und HTML-Tokenizer/DOM | `lib/svg/xml.fi` Kopf; `lib/html/`, `lib/dom/` | **teilweise** — für SVG gebaut, kein Namespace-/Pfad-API (M) |
| `html.parser` | `lib/html/tokenizer.fi` (html5lib-geprüft) | `lib/html/tokenizer.fi` | **da** |
| `socket` TCP-Client/Server | `net.connect_tcp/listen_tcp/accept/read/write/read_full/write_all/connect_tcp_timeout`; Port 0 + `listener_port` | `lib/std/net.fi:443,370,406,533,554,598,610,455,656` | **da** (nur IPv4 `u32`) |
| `socket.AF_UNIX` | `unix.unix_connect/unix_listen/unix_accept/fd_wait` | `lib/net/unix.fi:105,126,150,179` | **da** |
| `urllib.request` | `http.http_get/http_post` + Client (Redirect, Cache, gzip, Header) | `lib/net/http.fi:1868,1897,321` | **da**; Cookie-Jar: **unbekannt** (`openplc_upload.py` braucht Cookies) |
| `http.server` | `server.app/route/route_text/route_ws/route_sse` | `lib/http/server.fi:270,294,287,300,314` | **da** (Fake-CUPS/IPP-Parser: M) |
| `websocket-client` | `ws.connect/connect_tls/send_text/recv/close` | `lib/ws/ws.fi:392,403,430,467,444` | **da**; Lauf gegen Chromium-DevTools (`suppress_origin`): **unbekannt** |
| `threading` | `std.pool` (Worker + `submit`), `async.post` | `lib/std/pool.fi:129,211`, `docs/ASYNC.md` | **da** |
| `PIL.Image.open(png/jpg)` | `png.decode_png`, `jpeg.jpeg_decode`, `webp`, `gif`, Sammler `uiimagedec.image_from_bytes` | `lib/paint/png.fi:374`, `lib/jpeg/jpeg.fi`, `docs/IMAGE_DECODERS.md` | **da** |
| PNG schreiben | `png.write_png` | `png.fi:132` | **da** |
| `numpy` | — (nur skalare `std.math`; `lib/fui/transform.fi` 2D-Matrix; kein Array-/Statistik-Modul) | `lib/std/math.fi` | **fehlt** (S–M), siehe 3 |
| `statistics.median` | — (nur `vec_sort`, `vec_min/max`) | `lib/std/vec.fi:473,398,414` | **fehlt** (S) |
| `filecmp.cmp` | `hashfile.hash_file` oder `fs.read_file` + `mem_eq` | `hashfile.fi:230`, `rt.fi:224` | **da** |
| `copy.deepcopy` | nicht übertragbar (keine dynamischen Dicts) | – | **entfällt** meist |
| `jsonschema` | – | – | **fehlt** (L) |
| `lxml` XSD, `pypdf`, `yaml`, `tkinter` | – (`lib/pdf` ist nur **Writer**; Zwischenablage nur an Fenster gebunden, `lib/window/backend.fi:1782`) | – | **fehlt** (L / extern lassen) |
| Test-Rahmen | `#[test]` + `firnc --test` (Prozess pro Test, Timeout, JSON/TAP) | `compiler/src/testrun.rs`, `lib/test/runner.fi` | **da**, aber **kein `assert`/`expect_eq`**: das Beispiel in `testrun.rs` ruft `syscall(60, …)` zum Fehlschlagen |

## 3. numpy und PIL — was wird wirklich gebraucht?

**numpy** (`tools/web/webcheck.py`, einziger Nutzer, 8 Stellen):
- Z. 109/142: Screenshot als `int32`-Array `(H, W, 3)` (`np.asarray(img).astype(np.int32)`).
- Z. 119–122: `diff(a,b)` = `(a != b).any(axis=2).sum()` → **Anzahl verschiedener Pixel**.
- Z. 373: `np.kron(p1, np.ones((2,2,1)))` → 2×2-Pixelverdopplung (1×/2×-Vergleich).
- Z. 422–423: `(s[...,:3] == [30,41,59]).all(axis=-1).sum()` → Pixel einer Wunschfarbe zählen.
- `statistics.median` über Frame-Zeiten (Z. 654–668).

Das ist **kein** numpy im eigentlichen Sinn (keine Matrizen, kein BLAS), sondern vier Pixelschleifen und ein Median.
Was in Firn fehlt: Median/Perzentil/Mittel/Std (nur skalare `std.math`; Sortieren geht, `vec_sort`),
keine Array-Lib; **Matmul/Matrix** nur als Benchmark (`tools/bench`) und als 2D-Affin in `lib/fui/transform.fi`.
SIMD-Intrinsics gibt es (Tempo-Runden 4–7), aber **keine Lib** darauf.

| Baustein | Aufwand | Anmerkung |
|---|---|---|
| `std.stats`: `median`, `percentile`, `mean`, `stddev` über `Vec[f64]` | **S** (<150 Z.) | `vec_sort[T: Ord]` existiert; ob `f64` `Ord` ist: **unbekannt** (sonst `vec_sort_by`) |
| Pixel-API: `count_diff(a,b)`, `count_color(img, rgb)`, `upscale(img, k)` auf `png.Image` | **S** (~100 Z.) | `png.image_pixel` ist langsam → Zeilenzeiger nutzen |
| Allgemeine N-d-Arrays, Matmul, Broadcasting | **L** | nur falls jemand es wirklich braucht |

**PIL**: gebraucht werden `Image.open(file/bytes).convert("RGB")` (11×), `getpixel` (2× in `check_image`), `ImageChops.difference().getbbox()` (1×),
`img.save(png)` (1×, `webcheck`). Alle Eingaben sind **PNG** (CDP-Screenshot, `opview --shot`, `pdftoppm -png`). Firn hat PNG-Decoder
(alle Farbtypen, Adam7, 16 bit) und Encoder (`lib/paint/png.fi`), JPEG/WebP/GIF zusätzlich. **Passt.** Zuschneiden/Resize wird nirgends benutzt.

**Messung** (`tools/script_port/pngdiff.fi` gegen `PIL+numpy`, zwei 1280×720-PNG aus `OpenPlan/tests/editors/`; beide Programme liefern dieselben Zahlen: `740126` bzw. `0` bei gleichem Bild):

| Variante | Zeit/Lauf (Mittel aus 20–40) |
|---|---:|
| Python `import numpy, PIL.Image` allein | 197 ms |
| Python gesamt (Import + 2× Decode + Vergleich) | 317 / 327 ms |
| Firn, Standard-Optimierung | 317 / 345 ms |
| Firn, `--opt-level=release-fast` | **146 ms** |
| Firn nur Decode (Vergleichsschleife weggelassen) | 274 ms |
| Python Decode+`asarray` eines Bildes (im Prozess gemessen) | 51 ms |

Schluss: **gleichauf** (Standard) bis **2× schneller** (release-fast). Der Decoder allein ist in Firn eher langsamer als libpng+PIL
(Firn grob 70–135 ms/Bild gegen 51 ms; Maschine unter Last), der Python-Import (197 ms) frisst den Vorsprung auf.
Die Pixelschleife ruft pro Pixel `image_pixel` (mit Grenzprüfung). **Kein Showstopper.**

## 4. Run-Modus, Start, Test-Rahmen

Gemessen mit `firnc` (Stage 0, `compiler/target/release/firnc`), Maschine unter Last:

| | Messung |
|---|---|
| `firn run` / Skript-Modus | **existiert nicht.** `firnc --help`: nur `-o`, `--emit=…`, `--test`, `--package`. Programm wird mit `as`+`ld` zu einer ELF gebunden und separat gestartet. |
| Compile `check_lists.fi` (5 Imports, 241 Z.) | kalt **0,42–0,52 s** (3×); `--timings`: Optimizer 134 ms, as+ld 134 ms, Codegen 106 ms, Parser 24 ms, Sema 24 ms. `--opt-level=dev-fast` 0,45–0,49 s; `--no-opt` **0,60–0,65 s** (langsamer: größeres `.s`, `as+ld` 414 ms). |
| Compile `hello` (keine Imports) | <10 ms |
| Start des fertigen Programms | `hello` **0,8 ms** (`/bin/true` 1,1 ms) |
| `check_lists` Lauf (fertig übersetzt, inkl. 5× `op`) | **15,5 ms** (Python: **48–52 ms**; `op` allein 3,5–4,2 ms/Aufruf) |
| Python-Start | `python3 -c pass` 23,5 ms; `-I -S` 12,4 ms; Importsatz csv/json/subprocess/tempfile/shutil/re/glob/hashlib 49,7 ms |
| Shebang `#!/usr/bin/env firn-run` | Fehler `expected '[' after '#', found '!'` (`sheb.fi:1:2`). Workaround im Prototyp: Zeile 1 per `sed` leeren (Zeilennummern bleiben). Sauber: **S** im Compiler (Zeile 1 mit `#!` überspringen; Fundstelle nicht bestimmt, Lexer: `compiler/src/lexer.rs`). |
| Prototyp-Wrapper `tools/script_port/firn-run` | Cache `~/.cache/firn-run/<sha256(Quelle+Compiler)>`; **kalt 0,49 s**, **warm 22 ms** gegenüber direktem Binary 16 ms (= ~6 ms Overhead: `sha256sum` + `firnc --version`). Mit mtime-Schlüssel ließe sich das drücken (S). |
| Test-Rahmen | `#[test] fn` + `firnc --test x.fi [--format=json|tap] [--test-limit=s]`: Prozess pro Test (fork), `alarm`-Timeout, Position `datei:zeile:spalte` aus der Panic-Meldung. **Kein `assert`**, keine Vergleichs-Hilfen mit Meldung, keine Fixtures. Für Skripte reicht ohnehin `main` + Exit-Code (Stil `ci.sh`). |

CI-Folge: Ein frischer Checkout ohne Cache übersetzt alle Skripte neu: 103 × ~0,45 s ≈ **46 s CPU** (bei 8 Slots ~6 s). Das ist der einzige Punkt, an dem
Firn **langsamer** wäre — außer der Cache wird im CI gehalten oder die Skripte werden in **ein** Binary (Sub-Kommandos) gebaut.

## 5. Prototyp: `check_lists.py` → `tools/script_port/check_lists.fi`

Ausgewählt, weil typisch: `subprocess` + `csv` + `os.path` + Exit-Code. Läuft gegen den echten, fertig gebauten `op` (`/root/opw/b149/op`)
im OpenPlan-Wurzelordner. **Nichts in OpenPlan geändert.**

| | Python | Firn |
|---|---:|---:|
| Zeilen (gesamt / nicht leer) | 42 / 33 | 241 / ~210 (davon CSV ≈ 95, Hilfsfunktionen ≈ 40) |
| Ausgabe | 5× `ok`, `lists: all checks passed` | **identisch** (`diff` leer) |
| Fehlerfall (Fake-`op` mit leerer CSV) | 3 FAILED, Exit 1 | 3× FAIL, Exit 1 (dieselben 3 Prüfungen) |
| Lauf | 48–52 ms | 15,5 ms |
| Compile | – | 0,42–0,52 s |
| Binary | – | 351 KB |
| Format/Englisch | – | `firnfmt -c` grün; Kommentare/Bezeichner englisch |

Aufwand (ehrlich): ein Agent-Durchgang; der Compile klappte **beim ersten Versuch** (nach Lesen von `process.fi`, `fs.fi`, `rt.fi`, `vec.fi`, `text.fi`).
Der größte Teil der Zeilen ist der CSV-Leser. Der Rest ist ~3× so lang wie Python, weil: kein Text-Builder (`rt.Buf` + `push`), kein `dict`,
kein `subprocess.run`-Einzeiler, Fehler-Union-Behandlung (`catch 0`). Skalierung auf ~100 Skripte: **unbekannt**; Hochrechnung 3,3–5,7× Zeilen
(13 188 Z. `tools/ci` → grob 40–70 k Z.) ist aus **einem** Prototyp geschätzt.

Zweites Experiment: `tools/script_port/pngdiff.fi` (57 Z.) = PIL+numpy-Kern von `webcheck.diff()` (Messung in 3).

## 6. Lücken-Liste (→ Roadmap, Gruppe „Skript-Ersatz“)

| # | Lücke | Aufwand | Nutzen (OpenPlan-Zahlen) |
|---|---|:-:|---|
| 1 | **`std.script`-Kit**: `run(args…)→{code,out,err}` mit `input`/`cwd`/`env`/`timeout`; `check(ok, what)` + Zähler + `finish()`→Exit-Code; Text-Builder | S | 114 Dateien `subprocess`, 118 `sys`; spart ~40 Z. Boilerplate pro Skript |
| 2 | **`std.fsx`**: `mkdtemp`/`TempDir` (Cleanup per `defer`), `copy_file`, `copy_tree` (mit Ignore), `glob`, `walk`, `relpath/abspath` | S–M | tempfile 56, shutil 22 (copy 30×, rmtree 25×, copytree 18×), glob 5, os.walk/listdir |
| 3 | **`std.csv`**: Leser mit Kopf (`DictReader`-artig), Writer (RFC 4180, Quoting) | S | 48 Dateien / 73 Aufrufe — Vorlage in `check_lists.fi` |
| 4 | **JSON-Komfort**: `sort_keys`-Writer, Pfadzugriff (`get_path(d, "a.b[0]")`) | S | 93 Dateien / 629 Aufrufe, `sort_keys` 24× |
| 5 | **`#!`-Zeile im Compiler** + `firn run` (Cache nach Inhalt+Compiler, Argumente, Exit-Code) | S–M | Skripte starten wie Python; kalt 0,45 s, warm ~1–6 ms |
| 6 | **`std.stats` + Pixel-Helfer** (`median`, `percentile`, `mean`, `stddev`; `count_diff`, `count_color`, `upscale`) | S | `webcheck.py` (6× median, 4× numpy), `check_key_profile`, `check_image` |
| 7 | **Test-Kit**: `assert`/`expect_eq(a,b,msg)` mit Position; `#[test]` ohne `syscall(60,…)` | S | Test-Rahmen ohne Vergleichs-Hilfen; jedes Skript baut sein `check()` selbst (103×) |
| 8 | **WebSocket-Client gegen Chromium-DevTools prüfen** (`suppress_origin`, Fragmente, IPv4-localhost) + HTTP-Cookie-Jar | S (Prüfen) / M (Cookies) | `webcheck.py`, `openplc_upload.py` |
| 9 | **Generischer XML-Baum** (Namespaces, Attribut-Suche, einfache Pfade) auf Basis `lib/svg/xml.fi` | M | xml.etree 3 Dateien |
| 10 | **Regex-Lookaround**: nur die zwei Muster umschreiben (kein Engine-Umbau, RE2-Garantie bleibt) | S | 85 `re`-Aufrufe, 2 Muster mit Lookaround |
| 11 | JSON-Schema (Draft 2020-12) | **L** | 4 Dateien — besser in Python lassen |
| 12 | XSD (lxml), PDF-Leser (pypdf), YAML, Zwischenablage (tkinter → `xclip` per Prozess) | L / extern | je 1 Datei, alle **optional** |

Reihenfolge nach Nutzen/Aufwand: 7 → 1 → 3 → 2 → 4 → 5 → 6 → 8.

## 7. Empfehlung

**Teilweise** — Begründung in Zahlen:

- **Kein Tempo-Grund.** Python-Overhead ~50 ms × 103 Skripte ≈ 5 s gegen >13 min `ci.sh`: **<1 %**. Der Firn-Compile (46 s CPU kalt) wäre ohne Cache sogar ein Nachteil.
- **Kein Code-Grund.** 3,3–5,7× mehr Zeilen (Hochrechnung aus einem Prototyp, unsicher).
- **Ja-Gründe:** (a) eine Toolchain — `webcheck.py` verlangt pip-Pakete (numpy, PIL, websocket-client), die ein frisches CI-Image nicht hat; (b) „eigenes Futter essen“:
  die fehlenden Libs (CSV, Tempdir, Glob, Test-`assert`) braucht jede Firn-Anwendung (Launcher, Installer, appkit); (c) Skripte ohne Python-Start laufen in <1 ms.
- **Vorgehen:** Lücken 7, 1, 3, 2, 4, 5 schließen (alle S/M; Aufwand in Wochen **nicht gemessen**), dann **nur** (i) neue Checks in Firn schreiben,
  (ii) `webcheck.py` portieren (entfällt: numpy, PIL, websocket-client), (iii) die Skripte mit externen Bindungen (`jsonschema`, `lxml`, `pypdf`, `yaml`, `tkinter`)
  in Python **lassen**. Die ~100 einfachen `check_*.py` nicht anfassen, solange sie grün laufen.

## 8. Nachmessen

```sh
export FIRNLIB=/root/firn/lib
F=/root/firn/compiler/target/release/firnc
$F -o /tmp/check_lists tools/script_port/check_lists.fi          # ~0,45 s
cd /root/projects/openplan && /tmp/check_lists /root/opw/b149/op   # gleiche Ausgabe wie: python3 tools/ci/check_lists.py /root/opw/b149/op
$F --timings -o /tmp/x tools/script_port/check_lists.fi            # Phasenzeiten
$F -o /tmp/pngdiff tools/script_port/pngdiff.fi && /tmp/pngdiff a.png b.png
FIRNC=$F tools/script_port/firn-run script.fi args…                # Cache-Wrapper (Prototyp)
```

`/root/opw/b149/op` ist ein fertiger Build eines anderen OpenPlan-Arbeitsbaums (nicht auf `tools/firn.version` gepinnt);
für ein genaues CI-Ergebnis `op` aus dem gepinnten Stand bauen (`firnc --package src/cli -o op`, `tools/ci.sh` Z. 239).

## Grenzen dieser Untersuchung

- Zeiten: ausgelastete Maschine (Load 12–16), Mittel aus 20–40 Läufen, kein statistischer Test.
- Nur **ein** Skript portiert; Hochrechnung auf 103 Skripte = **Schätzung**.
- Nicht geprüft (**unbekannt**): Cookie-Jar im HTTP-Client, `Ord` für `f64` in `vec_sort`, WS-Client gegen Chromium, Windows-/Android-Verhalten, Speicherbedarf großer JSON-Dokumente in `std.json`, Fundstelle des `#!`-Fehlers im Compiler.
- `tools/model/opmodel.py` (SDK, 2 628 Z.) ist eine **Bibliothek** (Python-SDK für Nutzer), kein CI-Skript — kein Ziel eines Ersatzes.
