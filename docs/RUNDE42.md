# Runde 42: die Standardbibliothek

Reine Bibliotheksarbeit. An Lexer, Parser, Sema, Lowering und Codegen ist
**keine Zeile** geändert — die beiden Compiler sind exakt die des
Basis-Commits. Was hier steht, ist Firn-Code in `lib/`, dazu sieben
Testprogramme.

**Basis: `fe31d13` (Runde 41).** Gebaut und begonnen wurde auf `da3b0d9`;
weil Runde 41 währenddessen einen Miscompile in `firnc0` behoben hat, der
`test.sh` in Abschnitt 12 endlos hängen ließ (Befund D unten), ist dieser
Zweig darauf umgesetzt (`git rebase main`) und die Abnahme **vollständig neu
gemessen** — mit einem selbst gebauten `firnc0` und frisch gebauten
Hilfsbinaries, ohne jeden Handgriff von außen.

---

## 1. Bestandsaufnahme: was die std vor dieser Runde konnte

`lib/std/` entstand in Runde 39 als Fassade über den vorhandenen Bausteinen.
Sie war vollständig **in der Breite** (jedes Thema hatte ein Modul) und dünn
**in der Tiefe** (jedes Modul hatte das Nötigste).

| Modul | Herkunft | Konnte vorher | Fehlte |
|---|---|---|---|
| `std.io` | Hand, 92 Z. | `print`, `eprint`, `read_file`, `write_file`, `Fmt` mit `fmt_neu/text/zahl/len/druck/frei` | Zeilenumbruch, C-Zeichenketten, Anhängen an Dateien, stdin, **Zeilen lesen**, Zeichen/Hex/u64/bool im Builder, Ergebnis in einen Puffer statt auf stdout |
| `std.math` | Hand, 116 Z. | `PI`, `E`, `abs`, `min`, `max`, `clamp`, `isqrt`, `pow` (Ganzzahl), `sqrt`, `powi` (f64) | `floor/ceil/round/trunc`, `fabs/fmin/fmax/fclamp`, `fmod`, `hypot`, `ldexp/frexp`, `exp/ln/log2/log10`, Trigonometrie, `gcd/lcm`, Zweierpotenzen, `INF/NAN/EPSILON`, vorzeichenlose Varianten |
| `std.str` | **erzeugt** aus `lib/str`, 741 Z. | `Bytes`, `Str16`, WTF-8/UTF-16-Umwandlungen, `utf8_is_valid`, `AtomTable` | alles, was man täglich mit Text macht: vergleichen, suchen, trimmen, teilen, verbinden, ersetzen, Groß/Klein, auffüllen — und eine **Iteration über Zeichen** |
| `std.num` | **erzeugt** aus `lib/num`, 1203 Z. | `Bn` (16384-bit), `dtoa`, `strtod` — beide exakt | Ganzzahl ↔ Text in **beide** Richtungen, andere Basen, Vorzeichen, Auffüllen, Überlauferkennung, eine f64-Hülle über `dtoa`/`strtod` (die Kerne rechnen auf Bitmustern, weil sie älter sind als `f64`) |
| `std.vec` | Symlink `lib/rt/vec.fi`, 137 Z. | `vec_neu/frei/leeren/reserve/push/at/setzen/len/kap/ptr/letztes/pop` | suchen, einfügen, entfernen, kopieren, vergleichen, umkehren, **sortieren**, binär suchen |
| `std.map` | Symlink `lib/rt/map.fi`, 298 Z. | `map_neu/frei/leeren/setzen/hol/hat/loeschen/len/kap/reserve` + Platzzugriffe | **Iteration**, „hol oder Vorgabe", „hol und sag ob da war", herausnehmen, Zählmuster, Grabsteine aufräumen |
| `std.mem` | Hand, 54 Z. | `alloc`/`free`, `heap_*` der rc-Halde | — (in dieser Runde nicht angefasst) |
| `std.rt`, `std.intern`, `std.rc` | Symlinks | Laufzeitkern, Interner, Zählverweise | — (nicht angefasst) |

Kurz: man konnte mit der std **rechnen und Speicher verwalten**, aber nicht
bequem **Text verarbeiten**, nicht **sortieren** und nicht **über eine Map
laufen**.

---

## 2. Wohin der neue Code gehört — die eine Bauentscheidung dieser Runde

Drei verschiedene Sorten Modul verlangen drei verschiedene Wege:

**`std.str` und `std.num` werden ERZEUGT.** `lib/str` und `lib/num` sind
Einbindungs-Bibliotheken aus Stufe 0: ihre Dateien verweisen sich textuell
(`//#include`) und tragen keine Modulstruktur. `tools/strlib/expand.py` setzt
daraus je **ein** Modul zusammen und schreibt es nach `lib/std/`. Eine
Erweiterung von Hand in `lib/std/str.fi` wäre bei der nächsten Erzeugung weg;
eine Erweiterung in `lib/str/bytes.fi` läge auch jedem `html`/`dom`-Nutzer im
Binary. Deshalb gibt es **zwei neue Quelldateien, die ausschließlich die
Fassade einbindet**:

```
lib/str/std_facade.fi   <- nur aus tools/strlib/src/std_str.fi
lib/num/std_facade.fi   <- nur aus tools/strlib/src/std_num.fi
```

`lib/str/*.fi` und `lib/num/*.fi` sind **unverändert**; die erzeugten Tests
300–308 bekommen kein Byte dazu.

**`std.vec` und `std.map` sind Symlinks auf `lib/rt/vec.fi` und
`lib/rt/map.fi` — und `lib/firnc1/vec.fi` ist derselbe Symlink.** Alles, was
dort dazukommt, ist deshalb **generisch**: eine Vorlage wird erst zu Code,
wenn ein Programm sie für einen Typ benutzt (Monomorphisierung, Runde 2).
Nachgemessen: der Fixpunkt (`.firnc2.s`) hat nach dem Ausbau **exakt dieselbe
Zeilenzahl** wie vorher — der selbstgehostete Compiler trägt von den 21 neuen
`Vec`- und 11 neuen `Map`-Funktionen nichts.

**`std.math` und `std.io` sind handgeschriebene Moduldateien** und werden
direkt erweitert; ihre `export`-Listen sind mitgewachsen und nach Runden
gegliedert.

---

## 3. Was dazugekommen ist — Modul für Modul

### 3.1 `std.str` (+863 Zeilen, `lib/str/std_facade.fi`)

Zwei Typen, eine Regel, an jeder Signatur ablesbar:

* **`Spanne { p: *mut u8, n: usize }`** ist die **lesende** Sicht. Sie
  besitzt nichts, alloziert nichts, wird als Wert weitergereicht — wie
  `ReadOnlySpan<byte>`. Jede *Frage* über Text nimmt eine `Spanne`.
* **`Bytes`** (aus `lib/str/bytes.fi`) bleibt der **besitzende** Puffer. Jede
  Funktion, die Text *erzeugt*, schreibt in ein `*mut Bytes`.

```firn
fn npos() -> usize                                  // "nicht gefunden"

// Sichten
fn spanne(p: *mut u8, n: usize) -> Spanne
fn spanne_leer() -> Spanne
fn spanne_von_bytes(b: *mut Bytes) -> Spanne
fn spanne_von_c(p: *mut u8) -> Spanne
fn c_laenge(p: *mut u8) -> usize
fn ist_leer(s: Spanne) -> bool
fn laenge(s: Spanne) -> usize
fn zeichen(s: Spanne, i: usize) -> u8
fn spanne_teil(s: Spanne, von: usize, wieviel: usize) -> Spanne
fn spanne_ab(s: Spanne, von: usize) -> Spanne
fn spanne_bis(s: Spanne, bis: usize) -> Spanne

// Zeichenklassen (ASCII)
fn ist_leerraum(c: u8) -> bool          fn ist_ziffer(c: u8) -> bool
fn ist_hexziffer(c: u8) -> bool         fn ist_grossbuchstabe(c: u8) -> bool
fn ist_kleinbuchstabe(c: u8) -> bool    fn ist_buchstabe(c: u8) -> bool
fn ist_alphanumerisch(c: u8) -> bool    fn hexwert(c: u8) -> i32
fn gross_zeichen(c: u8) -> u8           fn klein_zeichen(c: u8) -> u8

// Vergleiche
fn gleich(a: Spanne, b: Spanne) -> bool
fn vergleiche(a: Spanne, b: Spanne) -> i32          // -1 / 0 / 1
fn gleich_ohne_fall(a: Spanne, b: Spanne) -> bool
fn vergleiche_ohne_fall(a: Spanne, b: Spanne) -> i32
fn beginnt_mit(s: Spanne, teil: Spanne) -> bool
fn endet_mit(s: Spanne, teil: Spanne) -> bool

// Suchen
fn finde_zeichen(s: Spanne, c: u8) -> usize
fn finde_zeichen_ab(s: Spanne, c: u8, ab: usize) -> usize
fn finde_zeichen_rueck(s: Spanne, c: u8) -> usize
fn finde(s: Spanne, teil: Spanne) -> usize
fn finde_ab(s: Spanne, teil: Spanne, ab: usize) -> usize
fn finde_rueck(s: Spanne, teil: Spanne) -> usize
fn enthaelt(s: Spanne, teil: Spanne) -> bool
fn enthaelt_zeichen(s: Spanne, c: u8) -> bool
fn zaehle_zeichen(s: Spanne, c: u8) -> usize
fn zaehle_teil(s: Spanne, teil: Spanne) -> usize
fn finde_nicht_aus(s: Spanne, menge: Spanne, ab: usize) -> usize

// Trimmen
fn trimme(s: Spanne) -> Spanne
fn trimme_links(s: Spanne) -> Spanne
fn trimme_rechts(s: Spanne) -> Spanne
fn trimme_menge(s: Spanne, menge: Spanne) -> Spanne
fn ohne_praefix(s: Spanne, teil: Spanne) -> Spanne
fn ohne_suffix(s: Spanne, teil: Spanne) -> Spanne

// Teilen (Kursor, keine Liste)
struct Teiler { quelle: Spanne, trenner: Spanne, i: usize, modus: u32, fertig: bool }
fn teiler_neu(s: Spanne, trenner: Spanne) -> Teiler
fn teiler_leerraum(s: Spanne) -> Teiler
fn teiler_naechst(t: *mut Teiler, aus: *mut Spanne) -> bool
fn teile_zaehle(s: Spanne, trenner: Spanne) -> usize

// Text bauen
fn anhaengen(aus: *mut Bytes, s: Spanne)
fn anhaengen_zeichen(aus: *mut Bytes, c: u8)
fn setze(aus: *mut Bytes, s: Spanne)
fn wiederhole(aus: *mut Bytes, s: Spanne, mal: usize)
fn verbinde_teil(aus: *mut Bytes, teil: Spanne, trenner: Spanne)
fn fuelle_links(aus: *mut Bytes, s: Spanne, breite: usize, fueller: u8)
fn fuelle_rechts(aus: *mut Bytes, s: Spanne, breite: usize, fueller: u8)

// Groß/Klein und Ersetzen
fn nach_gross(aus: *mut Bytes, s: Spanne)
fn nach_klein(aus: *mut Bytes, s: Spanne)
fn gross_hier(b: *mut Bytes)
fn klein_hier(b: *mut Bytes)
fn ersetze(aus: *mut Bytes, s: Spanne, alt: Spanne, neu: Spanne) -> usize
fn ersetze_erstes(aus: *mut Bytes, s: Spanne, alt: Spanne, neu: Spanne) -> bool
fn ersetze_zeichen_hier(b: *mut Bytes, alt: u8, neu: u8) -> usize

// UTF-8
struct Zeichenfund { cp: u32, laenge: usize, gueltig: bool }
fn ist_folgeoktett(c: u8) -> bool
fn utf8_lies(s: Spanne, i: usize) -> Zeichenfund
fn utf8_naechst(s: Spanne, i: usize) -> usize
fn utf8_vorher(s: Spanne, i: usize) -> usize
fn utf8_ist_grenze(s: Spanne, i: usize) -> bool
fn utf8_zaehle(s: Spanne) -> usize
fn utf8_teil(s: Spanne, von_zeichen: usize, anzahl: usize) -> Spanne
fn utf8_anhaengen(aus: *mut Bytes, cp: u32)
```

Festgelegte Regeln, damit nichts geraten werden muss:

* **Teilen** folgt C# ohne `RemoveEmptyEntries`: `"a,b,,c"` mit `","` ergibt
  vier Stücke, `""` ergibt genau ein leeres, ein leerer Trenner ergibt genau
  ein Stück (die ganze Quelle). `teiler_leerraum` fasst Leerraumfolgen
  zusammen und liefert nie ein leeres Stück.
* **Ein leerer Suchtext** wird an der Startposition *gefunden* — dieselbe
  Regel wie in C# und Rust. Sie macht `ersetze` mit leerem `alt` zu einer
  Nulloperation statt zu einer Endlosschleife.
* **`utf8_lies` prüft dieselben Regeln wie `utf8_is_valid`**: überlange
  Kodierungen, Surrogate und alles über U+10FFFF sind ungültig. Ein
  ungültiges Oktett wird als **ein** Zeichen mit `gueltig = false` und
  `cp = U+FFFD` gemeldet — so kommt jede Schleife garantiert voran.
* **Die erzeugenden Funktionen leeren `aus` und verlangen, dass `aus` nicht
  im Speicher der Quelle liegt.** Wer an Ort und Stelle arbeiten will, nimmt
  die `_hier`-Formen.
* **`verbinde_teil` erkennt „da steht schon etwas" an der Länge des
  Zielpuffers.** Ein *erstes* leeres Stück bekommt deshalb keinen Trenner
  nachgestellt: `["", "a"]` mit `"-"` ergibt `"a"`, nicht `"-a"`. Für den
  Regelfall (nicht leere Stücke, oder ein Puffer, in den vorher schon etwas
  geschrieben wurde) ist das richtig; wer leere Stücke verbinden muss, zählt
  selbst mit und ruft `anhaengen` mit dem Trenner. Bewusst so gelassen: die
  Alternative wäre ein Zustand *im* Aufrufer oder ein vierter Parameter, und
  beides wäre für den Regelfall schlechter.

### 3.2 `std.num` (+422 Zeilen, `lib/num/std_facade.fi`)

Eine Regel für die Richtung, an jedem Namen ablesbar: `schreibe_*` hängt den
Text einer Zahl an ein `*mut Bytes` an, `lies_*` liest Text und gibt die Zahl
der **verbrauchten** Oktette zurück, `text_zu_*` ist die strenge Form (der
ganze Text oder `false`).

```firn
fn u64_max() -> u64          fn i64_max() -> i64        fn i64_min() -> i64
fn ziffernwert(c: u8) -> i32
fn ziffernzahl(v: u64, basis: u32) -> usize

fn schreibe_basis(aus: *mut Bytes, v: u64, basis: u32, gross: bool)
fn schreibe_u64(aus: *mut Bytes, v: u64)
fn schreibe_i64(aus: *mut Bytes, v: i64)
fn schreibe_hex(aus: *mut Bytes, v: u64, mindest: usize)
fn schreibe_hex_gross(aus: *mut Bytes, v: u64, mindest: usize)
fn schreibe_binaer(aus: *mut Bytes, v: u64, mindest: usize)
fn schreibe_oktal(aus: *mut Bytes, v: u64, mindest: usize)
fn schreibe_breit_u64(aus: *mut Bytes, v: u64, breite: usize, fueller: u8)
fn schreibe_breit_i64(aus: *mut Bytes, v: i64, breite: usize, fueller: u8)

fn lies_u64_basis(p: *mut u8, n: usize, ab: usize, basis: u32,
                  aus: *mut u64, ueberlauf: *mut bool) -> usize
fn lies_u64(p: *mut u8, n: usize, ab: usize, aus: *mut u64,
            ueberlauf: *mut bool) -> usize
fn lies_i64(p: *mut u8, n: usize, ab: usize, aus: *mut i64,
            ueberlauf: *mut bool) -> usize
fn text_zu_u64_basis(p: *mut u8, n: usize, basis: u32, aus: *mut u64) -> bool
fn text_zu_u64(p: *mut u8, n: usize, aus: *mut u64) -> bool
fn text_zu_i64(p: *mut u8, n: usize, aus: *mut i64) -> bool
fn text_zu_u64_auto(p: *mut u8, n: usize, aus: *mut u64) -> bool   // 0x/0b/0o

fn f64_bits(x: f64) -> u64            fn bits_f64(b: u64) -> f64
fn f64_ist_nan(x: f64) -> bool        fn f64_ist_unendlich(x: f64) -> bool
fn f64_ist_null(x: f64) -> bool       fn f64_vorzeichen(x: f64) -> bool
fn dtoa_arbeitsbytes() -> usize       fn strtod_arbeitsbytes() -> usize
fn schreibe_f64(aus: *mut Bytes, x: f64) -> bool
fn schreibe_f64_bits(aus: *mut Bytes, bits: u64) -> bool
fn lies_f64(p: *mut u8, n: usize, aus: *mut f64) -> usize
fn lies_f64_bits(p: *mut u8, n: usize, aus: *mut u64) -> usize
fn text_zu_f64(p: *mut u8, n: usize, aus: *mut f64) -> bool
fn text_zu_f64_bits(p: *mut u8, n: usize, aus: *mut u64) -> bool
```

* **Überlauf wird gemeldet, nicht verschwiegen.** Stufe 0 prüft in der
  Arithmetik nirgends (SPEC §14.1.3); beim Einlesen *fremder* Eingaben wäre
  ein stiller Umbruch eine Lücke, kein Schönheitsfehler. `lies_*` setzt
  `ueberlauf` und liest die Ziffern trotzdem zu Ende (der Aufrufer muss
  wissen, wo der Text weitergeht), `text_zu_*` gibt `false`.
* **Die f64-Hülle kostet zwei `mmap` je Aufruf** (14 KiB Arbeitsspeicher für
  `dtoa`, ein Zwischenpuffer, weil `dtoa` sein Ziel leert). Das ist die
  bequeme, nicht die schnelle Form; wer viele Zahlen schreibt, ruft `dtoa`
  direkt mit eigenem Arbeitsspeicher. Steht so im Kopf der Funktion.
* `f64_bits`/`bits_f64` deuten das **Bitmuster** um (über den Speicher).
  `as` wäre eine Wert-Umwandlung: `1.0 as u64` ist `1`.

### 3.3 `std.vec` (+312 Zeilen, generisch, `lib/rt/vec.fi`)

```firn
fn vec_ist_leer[T](v: *mut Vec[T]) -> bool
fn vec_kuerzen[T](v: *mut Vec[T], n: usize)
fn vec_fuellen[T](v: *mut Vec[T], wert: T)
fn vec_tausche[T](v: *mut Vec[T], i: usize, j: usize) -> bool
fn vec_umkehren[T](v: *mut Vec[T])
fn vec_index_von[T](v: *mut Vec[T], wert: T) -> usize      // len = nicht da
fn vec_enthaelt[T](v: *mut Vec[T], wert: T) -> bool
fn vec_zaehle[T](v: *mut Vec[T], wert: T) -> usize
fn vec_einfuegen[T](v: *mut Vec[T], i: usize, wert: T) -> bool
fn vec_entfernen[T](v: *mut Vec[T], i: usize) -> T          // Reihenfolge bleibt
fn vec_entfernen_schnell[T](v: *mut Vec[T], i: usize) -> T  // O(1)
fn vec_anhaengen[T](ziel: *mut Vec[T], quelle: *mut Vec[T]) -> bool
fn vec_kopie[T](quelle: *mut Vec[T], ziel: *mut Vec[T]) -> bool
fn vec_gleich[T](a: *mut Vec[T], b: *mut Vec[T]) -> bool
fn vec_min[T](v: *mut Vec[T]) -> T
fn vec_max[T](v: *mut Vec[T]) -> T
fn vec_senken[T](v: *mut Vec[T], wurzel: usize, ende: usize)
fn vec_sortiere[T](v: *mut Vec[T])
fn vec_ist_sortiert[T](v: *mut Vec[T]) -> bool
fn vec_untere_schranke[T](v: *mut Vec[T], wert: T) -> usize
fn vec_binaersuche[T](v: *mut Vec[T], wert: T) -> usize     // len = nicht da
fn vec_sortiert_einfuegen[T](v: *mut Vec[T], wert: T) -> bool
```

**Warum Heapsort und nicht Quicksort** — drei nachprüfbare Gründe:
O(n log n) auch im schlechtesten Fall (Quicksort entartet bei bereits
sortierter oder gleichverteilter Eingabe zu O(n²) — genau die kommen in einem
Compiler dauernd vor), kein Zusatzspeicher (Mergesort bräuchte n Elemente),
keine Rekursion (die Tiefe hinge sonst an der Eingabe, und Stufe 0 hat keine
Stapelprüfung). Der Preis steht dabei: **nicht stabil**.

Die Ordnung ist die des Typs (`<` auf `T`) — bei `u64` also die
vorzeichenlose. `tests/802` sortiert deshalb bewusst auch ein `Vec[u64]` mit
2⁶³ und 2⁶⁴−1 darin.

### 3.4 `std.map` (+139 Zeilen, generisch, `lib/rt/map.fi`)

```firn
fn map_ist_leer[K, V](m: *mut Map[K, V]) -> bool
fn map_tot[K, V](m: *mut Map[K, V]) -> usize
fn map_naechster[K, V](m: *mut Map[K, V], ab: usize) -> usize   // kap = Ende
fn map_erstes[K, V](m: *mut Map[K, V]) -> usize
fn map_hol_oder[K, V](m: *mut Map[K, V], k: K, vorgabe: V) -> V
fn map_hol_wenn[K, V](m: *mut Map[K, V], k: K, aus: *mut V) -> bool
fn map_wert_adresse[K, V](m: *mut Map[K, V], k: K) -> u64        // 0 = fehlt
fn map_setzen_wenn_neu[K, V](m: *mut Map[K, V], k: K, v: V,
                             eingefuegt: *mut bool) -> bool
fn map_erhoehen[K, V](m: *mut Map[K, V], k: K, delta: V) -> V
fn map_nehmen[K, V](m: *mut Map[K, V], k: K, aus: *mut V) -> bool
fn map_aufraeumen[K, V](m: *mut Map[K, V]) -> bool
```

Iteration als **Kursor über die Plätze**, nicht als Feld:

```firn
var i: usize = map_naechster[K, V](&m, 0)
while i < map_kap[K, V](&m) {
    let k: K = map_platz_schluessel[K, V](&m, i)
    let w: V = map_platz_wert[K, V](&m, i)
    i = map_naechster[K, V](&m, i + 1)
}
```

Grund: `lib/rt/map.fi` bindet bewusst kein `vec` ein. Ein zweiter Import in
einem Modul, das der Compiler selbst benutzt, ist Gewicht ohne Gegenwert, und
`firnc1` dedupliziert Importpfade als Zeichenkette (Runde 39) — wer `std.map`
und `rt.vec` mischt, lädt sonst dieselbe Datei zweimal. Wer die Schlüssel doch
als Feld will, schiebt sie in dieser Schleife in ein `Vec[K]`; dann steht der
Import beim Aufrufer, wo er hingehört.

Dokumentierte Warnung: **während der Iteration nicht einfügen** (ein
Einfügen kann neu streuen). Löschen ist unbedenklich.

### 3.5 `std.math` (+633 Zeilen)

```firn
// Konstanten und Bitmuster
fn TAU() -> f64      fn SQRT2() -> f64   fn LN2() -> f64    fn LN10() -> f64
fn INF() -> f64      fn NAN() -> f64     fn EPSILON() -> f64
fn math_bits(x: f64) -> u64              fn math_f64(b: u64) -> f64
fn ist_nan(x: f64) -> bool   fn ist_unendlich(x: f64) -> bool
fn ist_endlich(x: f64) -> bool

// Ganzzahl
fn sign(x: i64) -> i64
fn min_u(a: u64, b: u64) -> u64          fn max_u(a: u64, b: u64) -> u64
fn clamp_u(x: u64, lo: u64, hi: u64) -> u64
fn abs_diff(a: i64, b: i64) -> u64       fn gcd(a: u64, b: u64) -> u64
fn lcm(a: u64, b: u64) -> u64            fn ilog2(v: u64) -> usize
fn ilog10(v: u64) -> usize               fn ist_zweierpotenz(v: u64) -> bool
fn naechste_zweierpotenz(v: u64) -> u64  fn pow_u(b: u64, e: u64) -> u64

// Gleitkomma, EXAKT
fn fabs(x: f64) -> f64    fn fmin(a: f64, b: f64) -> f64
fn fmax(a: f64, b: f64) -> f64            fn fclamp(x: f64, lo: f64, hi: f64) -> f64
fn trunc(x: f64) -> f64   fn floor(x: f64) -> f64   fn ceil(x: f64) -> f64
fn round(x: f64) -> f64   fn fmod(x: f64, y: f64) -> f64
fn hypot(x: f64, y: f64) -> f64
fn ldexp(x: f64, k: i64) -> f64           fn frexp(x: f64, e: *mut i64) -> f64

// Gleitkomma, GENÄHERT
fn exp(x: f64) -> f64     fn ln(x: f64) -> f64
fn log2(x: f64) -> f64    fn log10(x: f64) -> f64
fn powf(b: f64, e: f64) -> f64
fn sin(x: f64) -> f64     fn cos(x: f64) -> f64     fn tan(x: f64) -> f64
fn atan(x: f64) -> f64    fn atan2(y: f64, x: f64) -> f64
fn nahe(a: f64, b: f64, eps: f64) -> bool
```

* **`trunc/floor/ceil/round` rechnen nicht, sie schneiden Bits ab.** Damit
  stimmen sie auch bei 2⁵² und darüber, wo jede Formel mit `+0.5` falsch ist,
  und `round(0.49999999999999994)` ist `0.0` statt fälschlich `1.0`.
* **`fmod` ist exakt**: der Betrag des Teilers wird verdoppelt, bis er über
  dem Rest liegt, dann halbierend abgezogen — jede Zwischenzahl liegt auf
  demselben Bitraster, also ist jede Subtraktion exakt.
* **`fmin`/`fmax` übergehen NaN**, statt es weiterzureichen; sonst vergiftet
  ein einziges NaN jedes Minimum über einem Feld.
* **`exp/ln/sin/cos/tan/atan/atan2` sind Reihen mit Bereichsreduktion, ohne
  Tabellen. Sie sind NICHT korrekt gerundet.** Gemessene Schranke im
  geprüften Bereich: **relativ 1e-12**; bei `sin`/`cos` mit großem Argument
  (|x| = 100) nur noch **1e-10** — die Reduktion rechnet in doppelter
  Genauigkeit und verliert dort Stellen. Das ist die übliche Grenze ohne
  Payne-Hanek-Reduktion.
* **Namensregel, bewusst beibehalten:** mathematische Funktionen tragen ihren
  internationalen Namen (`sqrt`, `floor`, `exp`, `sin`) — das Modul heißt
  seit Runde 39 so, und ein `wurzel` neben dem bestehenden `sqrt` wären zwei
  Namen für eine Sache. Alles, was kein eingebürgerter Funktionsname ist,
  heißt deutsch (`ist_zweierpotenz`, `naechste_zweierpotenz`, `nahe`).

### 3.6 `std.io` (+285 Zeilen)

```firn
fn println(p: u64, n: usize)             fn eprintln(p: u64, n: usize)
fn print_c(p: u64)                       fn eprint_c(p: u64)
fn print_zeile()                         fn print_fd(fd: i64, p: u64, n: usize) -> bool
fn read_stdin(aus: *mut rt.Buf) -> bool
fn append_file(pfad: u64, p: u64, n: usize) -> bool      // O_APPEND
fn file_exists(pfad: u64) -> bool

struct Zeilenleser { p: u64, n: usize, i: usize }
fn zeilen_neu(p: u64, n: usize) -> Zeilenleser
fn zeilen_von_buf(b: *mut rt.Buf) -> Zeilenleser
fn zeilen_naechst(z: *mut Zeilenleser, ap: *mut u64, an: *mut usize) -> bool
fn zeilen_zahl(p: u64, n: usize) -> usize

fn fmt_zeichen(f: Fmt, c: u8) -> Fmt     fn fmt_bool(f: Fmt, w: bool) -> Fmt
fn fmt_u64(f: Fmt, v: u64) -> Fmt        fn fmt_hex(f: Fmt, v: u64, mindest: usize) -> Fmt
fn fmt_c(f: Fmt, p: u64) -> Fmt          fn fmt_wiederhole(f: Fmt, c: u8, mal: usize) -> Fmt
fn fmt_zeile(f: Fmt) -> Fmt              fn fmt_breit(f: Fmt, v: i64, breite: usize, fueller: u8) -> Fmt
fn fmt_anhaengen(f: Fmt, h: Fmt) -> Fmt  // h wird angehängt UND freigegeben
fn fmt_inhalt(f: Fmt, aus: *mut rt.Buf)  // Inhalt in einen Puffer, f frei
fn fmt_druck_zeile(f: Fmt)               fn fmt_eprint(f: Fmt)
fn fmt_eprint_zeile(f: Fmt)              fn fmt_in_datei(f: Fmt, pfad: u64) -> bool
```

Damit sind die **zwei Lücken geschlossen, die `docs/RUNDE39.md` am Ende
ausdrücklich benannt hat**: `fmt_zeichen` (ein Zeichen als *Buchstabe*, nicht
als Dezimalzahl — `f"{c}"` zeigt sonst `65` statt `A`) und `fmt_inhalt`
(Ergebnis in einen Puffer statt auf stdout).

Der `Zeilenleser` ist ein Kursor über einen bereits gelesenen Block: jede
Zeile kommt als Zeiger+Länge **innerhalb** des Blocks, ohne Kopie. Der
Umbruch gehört nicht zur Zeile, ein vorangehendes `\r` fällt weg, und ein
abschließendes `\n` erzeugt **keine** leere Schlusszeile — die Regel, die
`wc -l` und jeder Editor benutzen.

---

## 4. Was bewusst NICHT gebaut wurde — und warum

1. **Keine generische `Option[T]`/`Result[T]`-Schicht.** Die
   Monomorphisierung setzt Typargumente in der **Nutzlast einer Fehlerunion**
   nicht ein (`docs/RC.md`, Abweichung A4: `fn f[T](..) -> AllocError!Zaehlverweis[T]`
   meldet „unbekannter typ"), und `enum` ist nicht generisch (SPEC §14.1.types
   T3). Ein echtes `Option[T]` braucht eine **Kernänderung** — die war für
   diese Runde ausgeschlossen. Statt eines vierten, halben Weges nutzt die
   Bibliothek durchgehend die drei Formen, die die Sprache heute *hat*, und
   zwar konsequent:
   * **Ausgabezeiger + `bool`**, wenn „da/nicht da" und der Wert beide zählen
     (`map_hol_wenn`, `text_zu_u64`, `teiler_naechst`)
   * **Sonderwert**, wenn ein Index gesucht wird (`npos()` in `std.str`,
     `vec_len` in `std.vec`, `map_kap` in `std.map` — jeweils ein Index, der
     nie ein Treffer sein kann)
   * **Vorgabe**, wenn der Aufrufer den Ersatz kennt (`map_hol_oder`)
2. **Kein Zahlen-Parser in `std.str`.** Text → Zahl steht ausschließlich in
   `std.num`. Zwei Parser wären zwei Wahrheiten.
3. **Groß/Klein nur ASCII.** Unicode-Fallabbildung braucht die UCD-Tabellen;
   die liegen im comptime-Zweig (`tests/602_comptime_ucd.fi`) und gehören
   nicht in die Kernfassade.
4. **`teiler_*` liefert eine Folge, keine Liste.** `lib/str` ist eine
   Einbindungs-Bibliothek ohne Modulstruktur und kann `Vec[T]` nicht
   einbinden. Der Kursor ist ohnehin die Form, die nichts alloziert.
5. **Kein Sortieren nach eigenem Vergleich.** Firn kennt keine
   Funktionszeiger (`docs/SELBSTHOSTING.md`, Zeile 1576). `vec_sortiere`
   ordnet nach `<` auf `T`; alles andere bräuchte ein Sprachmittel.
6. **Kein stabiles Sortieren.** Siehe 3.3 — bei Skalaren nicht
   unterscheidbar, bei Paaren wäre es eine Zusage, die hier fehlt.
7. **Kein `Str`-Typ (geprüftes UTF-8) über der `Spanne`.** `Spanne` ist roh;
   `utf8_is_valid` sagt, ob eine Folge Text ist. Ein eigener Typ ohne
   Sprachmittel zur Erzwingung wäre eine Zusage ohne Deckung.
8. **Keine korrekt gerundeten Elementarfunktionen.** Das braucht Tabellen und
   ist eine eigene Runde. Die tatsächliche Schranke steht in 3.5 und wird in
   `tests/805` gemessen.
9. **`read_stdin` wird nicht im Test aufgerufen** — der Testläufer gibt dem
   Programm keine Eingabe, ein Lesen würde blockieren. Die Funktion ist eine
   Weiterleitung auf `rt.lies_stdin` (durch `bin/firnc1.fi` seit Runde 29 im
   Einsatz).
10. **`std.mem`, `std.rc`, `std.intern`, `std.rt` blieben unberührt.** Sie
    sind vollständig für ihren Zweck; hinzugefügte Namen hätten nur die
    Oberfläche vergrößert.

---

## 5. Zwei Befunde aus dem Bau (benannt, nicht umgebaut)

**A. `f"..."` in der Bedingung eines `if`.**

```firn
if !io.fmt_in_datei(f"zahl {255}", pfad) { ... }
//                  ^^^^^^^^^^^^^ error: unbekannter name '_fseg549'
```

Der Parser hebt die versteckten Textsegmente (`let _fsegN: [u8; N]`) vor die
umgebende **Anweisung**; bei einem `if` ist die Bedingung aber schon Teil
dieser Anweisung, und der Name ist dort nicht sichtbar. Ein Zwischenwert löst
es (`let inhalt: io.Fmt = f"..."`, siehe `tests/806`). Eine Behebung wäre eine
Parser-Änderung und gehört damit nicht in diese Runde.

**B. `firnc0` und `firnc1` runden 17-stellige Gleitkommaliterale nicht
gleich.** Gemessen mit `num.f64_bits` auf beiden Compilern:

| Literal | `firnc0` | `firnc1` | korrekt (IEEE) |
|---|---|---|---|
| `0.30000000000000004` | 4599075939470750516 | 4599075939470750**517** | 4599075939470750516 |
| `9007199254740993.0` | 4845873199050653696 | 4845873199050653**697** | 4845873199050653696 |
| `0.49999999999999994` | 4602678819172646911 | 4602678819172646**909** | 4602678819172646911 |
| `2.718281828459045` | 4613303445314885481 | 4613303445314885481 | gleich |
| `0.1` | 4591870180066957722 | 4591870180066957722 | gleich |

`firnc0` stimmt mit der korrekten Rundung überein, `firnc1` weicht um 1–2 ULP
ab. Das ist die **eine** bekannte Abweichung, die `tools/lex_compare.sh`
seit Langem als „UNGLEICH: 1 (bekannt und benannt: 1) / GLEITKOMMA außerhalb
des schnellen Pfades: 1" meldet — die neue `std.num` macht sie nur zum ersten
Mal *sichtbar*, weil sie Bitmuster ausdrucken kann. Die Tests dieser Runde
vermeiden solche Literale bewusst und **rechnen** die Werte statt dessen
(`0.1 + 0.2`, `ldexp(1.0, 51) + 0.5`); die Behebung gehört in eine
Lexer-Runde.

**C. Ein modulqualifizierter Aufruf im Ausdruck eines `f"..."` ergibt
verschiedene Syntaxbäume.**

```text
firnc0 --emit=ast-kanon :  (ruf str.utf8_zaehle (id u))
./.astdump (firnc1)     :  (ruf str__utf8_zaehle (id u))
```

`firnc1` setzt beim Neu-Lexen des Ausdruckssegments schon den **internen**
Namen ein (`modul__name`, SPEC §14.1.15), `firnc0` den geschriebenen. Das
*Verhalten* ist identisch — `tools/self_compare.sh` meldet für alle
betroffenen Dateien `GLEICH`, und die Programme drucken dieselbe Zeile.
`tools/parser_compare.sh` vergleicht aber den Baum Oktett für Oktett, und
dort fällt es auf. Die Tests binden solche Werte deshalb vor der
Interpolation an einen Namen. Behebung wäre eine Parser-Änderung.

**D. `.astdump` hing auf `da3b0d9` bei JEDEM `||` in einer Endlosschleife —
`test.sh` kam nie über Abschnitt 12 hinaus.** Hier gefunden, von Runde 41
behoben (`fe31d13`).

Fehlerbild und Eingrenzung aus dieser Runde:

```firn
fn f(a: bool, b: bool) -> bool { if a || b { return a } return b }
```

`./.astdump` auf diese sechs Zeilen: läuft ewig, kein Byte Ausgabe. Damit
hängt `tools/parser_compare.sh` beim ersten Quelltext mit `||` — und das
ist praktisch jeder. Drei Messungen haben gezeigt, dass es **nicht** an
dieser Runde liegt:

1. mit auf `da3b0d9` **zurückgesetztem** `lib/rt/vec.fi` und `lib/rt/map.fi`
   und frisch gebautem `.astdump`: hängt genauso;
2. dasselbe `bin/astdump.fi`, gebaut mit dem `firnc0` aus dem Arbeitsbaum der
   Runde 41 (dort unterschied sich nur `compiler/src/regalloc.rs`): läuft
   durch;
3. `.firnc1` selbst war nie betroffen — der Selbstvergleich und der Fixpunkt
   liefen auch auf `da3b0d9` grün.

Die Ursache steht in `fe31d13`: die Optimierung „Zellen-Alias" (Runde 40,
`regalloc.rs`) ließ einen Load das Zellenregister direkt lesen, obwohl
zwischen Load und Verwendung ein anderer Wert genau dieses Register
beschrieb. In `bin/print.fi`/`drucke_binop` wurde aus `43 - start` ein
`43 - &tab[start]`, die Länge lief unter Null, und `rt.buf_wachse` drehte
sich ewig. Aufgefallen ist es erst jetzt, weil die Dump-Binaries vorher
veraltet wiederverwendet wurden — dieselbe Falle, vor der Abschnitt 7 warnt.

Für diesen Zweig ist damit nichts mehr offen: er sitzt auf `fe31d13`, und
die Abnahme in Abschnitt 7 ist mit dem eigenen, korrigierten `firnc0`
gemessen.

---

## 6. Testabdeckung

Sieben neue Programme in `tests/`, jedes läuft in `test.sh` **dreimal**
(`opt` / `noopt` / `dev-fast`) und zusätzlich in `tools/self_compare.sh`
gegen `firnc1`. Jede einzelne Erwartung steht als `return <code>` im Programm
— schlägt eine fehl, endet der Test mit genau diesem Code und `test.sh` nennt
ihn; die gedruckte Zeile ist zusätzlich der Vergleichspunkt zwischen den
Compilern. Die Spalte „Fehlerausgänge" zählt genau diese `return <code>`
(ohne das abschließende `return 0`); viele davon prüfen mit `||` mehrere
Dinge auf einmal, die Zahl der geprüften Zusagen liegt also höher.
Zusammen: 310.

| Test | Inhalt | Zahlen |
|---|---|---|
| `800_std_str_core.fi` | trimmen, teilen (fester Trenner **und** Leerraum), verbinden, suchen (vorwärts/rückwärts/zählen), ersetzen, Groß/Klein, auffüllen, vergleichen, Zeichenklassen, UTF-8 vorwärts/rückwärts/nach Zeichen geschnitten, ungültiges Oktett | 49 Fehlerausgänge |
| `801_std_num_core.fi` | Basis 2/8/10/16/36, Auffüllen, Breite, u64::MAX, **u64::MAX+1 als Überlauf**, i64::MIN, Präfixe `0x`/`0b`/`0o`, Teillesen mit Rest, dtoa/strtod-Hülle, `1e21`, Rundreise `0.1+0.2` | 43 Fehlerausgänge |
| `802_std_vec_core.fi` | zwei Ausprägungen (`i32`, `u64`), suchen, sortieren, binär suchen, untere Schranke, einfügen/entfernen (beide Formen), kopieren/anhängen/vergleichen, 200 Elemente absteigend und 200 gleiche | 41 Fehlerausgänge |
| `803_std_map_core.fi` | Kursor über 50 Paare (Summe der Schlüssel und Werte), Entry-Helfer, Wert an Ort und Stelle, herausnehmen, **4000 Einfügungen mit jeder dritten Löschung** und anschließendem Aufräumen, zweite Ausprägung `Map[u32, i32]` | 36 Fehlerausgänge |
| `804_std_math_core.fi` | der **exakte** Teil, alles mit `==`: Ganzzahl-Helfer, `fabs/fmin/fmax/fclamp`, `trunc/floor/ceil/round` samt dem größten Double unter 0,5 und dem Raster bei 2⁵¹, `fmod`, `ldexp`, `frexp` (auch subnormal), `hypot`, Sonderwerte | 57 Fehlerausgänge |
| `805_std_math_f64.fi` | der **genäherte** Teil gegen benannte Schranken; dazu zwei Schleifen: sin²+cos²=1 an 41 Stellen, `tan(atan(x)) == x` an 30 Stellen | 54 Fehlerausgänge |
| `806_std_io_core.fi` | schreiben/anhängen/gibt-es-sie, Zeilen mit `\r\n` und ohne Schlussumbruch, der ganze `Fmt`-Ausbau; die Funktionen, die **selbst** einen Umbruch schreiben (`println`, `print_zeile`, `fmt_druck_zeile`), laufen mit über `dup2` umgebogenem Deskriptor 1 und werden aus der Datei zurückgelesen — ausgeführt, nicht behauptet | 30 Fehlerausgänge |

Nicht abgedeckt und hier benannt: `read_stdin` (siehe 4.9), die
Speichermangel-Zweige (`heap_alloc` liefert 0) — die lassen sich ohne
Einspeisung eines fehlschlagenden `mmap` nicht auslösen.

---

## 7. Abnahme

Gemessen auf **`fe31d13`** (Basis dieses Zweiges), mit selbst gebautem
`firnc0` und frisch gebauten Hilfsbinaries.

| Messung | Wert | Ausgangslage `fe31d13` |
|---|---|---|
| `bash ./test.sh` | **PASS 673/673**, `RC=0` | 652/652 |
| `bash tools/self_compare.sh` | **GLEICHES VERHALTEN 196 · ABWEICHEND 0 · FEHLERHAFT 0 · CODEGEN FEHLT 0**, `RC=0` | 189 / 0 / 0 |
| `bash tools/fixpunkt.sh` | **Stufe 2 == Stufe 3, zeichengleich (309468 Zeilen Assembler)** · Korpus: `.firnc2` verhält sich wie `firnc0`, `RC=0` | zeichengleich, 309468 Zeilen |

Die 673 sind 652 + 21: sieben neue Programme × drei Durchläufe
(`opt` / `noopt` / `dev-fast`). Die 196 sind 189 + 7. Die **309468 Zeilen sind
unverändert** — der selbstgehostete Compiler trägt von den 32 neuen
generischen `Vec`/`Map`-Funktionen kein einziges Byte, weil er keine davon
benutzt (Monomorphisierung).

(Zwischenstand auf der alten Basis `da3b0d9`, der Vollständigkeit halber:
670/670, 195/0/0, Fixpunkt zeichengleich bei 289096 Zeilen. Dieselbe Aussage,
nur vor dem Rebase.)

Die Vergleichswerkzeuge im Einzelnen (aus demselben Lauf):

| Werkzeug | gleich | ungleich |
|---|---|---|
| `lex_vergleich` | 361 | 1 (bekannt: `tests/590_f64.fi`, Literal `1e308`) |
| `parser_vergleich` | 235 | 1 (dieselbe bekannte) |
| `typen_vergleich` | 185 | 0 |
| `sema_vergleich` | 144 | 1 (dieselbe bekannte) |
| `fir_vergleich` | 143 | 1 (dieselbe bekannte) |

Kein Werkzeug hat eine **neue** Ausnahme bekommen: die Liste der bekannten
Abweichungen (`BEKANNT=` in den Skripten) ist unangetastet.

Vor der Messung wurden **alle** Hilfsbinaries der Vergleichswerkzeuge
gelöscht (`.astdump`, `.lexdump`, `.firdump`, `.semadump`, `.layoutdump`,
`.firnc1..3`). `lib/firnc1/vec.fi` ist ein *Symlink* auf `lib/rt/vec.fi`, und
`find -newer` sieht die Änderung an der Zieldatei nicht — ein stehen
gelassenes `.astdump` hätte den Stand von vor dem Ausbau gemessen und wäre
grün gewesen, ohne etwas zu beweisen.

**Eines gehört noch zur Ehrlichkeit dieser Messung:**

1. **Die Messung lief in einem eigenen Mount-Namensraum mit privatem `/tmp`**
   (`unshare --mount` + `tmpfs`). `tools/lex_compare.sh` und die anderen
   Vergleicher benutzen feste Pfade wie `/tmp/lexv_a.txt`; läuft in einem
   zweiten Arbeitsbaum gleichzeitig dieselbe Suite (hier: Runde 41), schreiben
   beide in dieselben Dateien und die Ergebnisse sind Zufall. Ohne
   Namensraum meldete `lex_vergleich` einmal 72 Abweichungen, mit Namensraum
   genau die eine bekannte. Runde 41 ist derselben Falle begegnet
   (`/tmp/parv_a.txt`, „das sah wie 148 echte Abweichungen aus"). Das ist ein
   Werkzeugmangel, der hier festgehalten wird — die Skripte sollten `mktemp`
   benutzen; solange sie es nicht tun, darf immer nur EINE Suite gleichzeitig
   laufen.

## 8. Zeilen

| Datei | vorher | nachher |
|---|---|---|
| `lib/std/str.fi` (erzeugt) | 741 | 1604 |
| `lib/std/num.fi` (erzeugt) | 1203 | 1625 |
| `lib/std/math.fi` | 116 | 749 |
| `lib/std/io.fi` | 92 | 377 |
| `lib/rt/vec.fi` (= `std.vec`) | 137 | 449 |
| `lib/rt/map.fi` (= `std.map`) | 298 | 437 |
| **Summe** | **2587** | **5241** |

Dazu sieben Testprogramme mit zusammen rund 1400 Zeilen.
