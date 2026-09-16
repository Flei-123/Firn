# Runde FIRN-ENV — Bauzeit-Umgebungsvariablen in der Sprache

*Vorgänger: FIRN-LUECKEN (508f6c796).*

## Der Anlass

Im OrientOS-Baum steht seit Commit `c3ecd95` eine Markenvariable, gebaut nach
der Vorlage `/root/projects/freeviewer/src/brand.rs`. In Rust ist das eine
Zeile:

```rust
pub const NAME: &str = match option_env!("FV_BRAND_NAME") {
    Some(s) => s,
    None => "FreeViewer",
};
```

In Firn ging das nicht. Der Worker musste sich mit `tools/marke-einsetzen.py`
behelfen: ein Skript, das die Werte vor dem Übersetzen in eine `/tmp`-Kopie des
Quelltextes schreibt. Sein Kommentar sagt es wörtlich — *„Firn kennt kein
`option_env!`, also tut es der Bau."* Diese Runde macht daraus Sprache.

## Was es jetzt gibt

| Schreibweise | ergibt | wenn nicht gesetzt |
|---|---|---|
| `__env_or("FIRN_X", "Vorgabe")` | `str` | das zweite Argument |
| `__env_has("FIRN_X")` | `bool` | `false` |

Die Form folgt dem, was die Sprache für `__v128_*` schon benutzt
(`compiler/src/simd.rs`): eine **Intrinsic-Funktion** mit einem Namen, mit dem
nichts kollidieren kann. Kein Operator, nichts Implizites. Beide Argumente
müssen Textliterale sein.

Dazu kommt, weil es ohne nichts nützt:

```firn
const NAME: str = __env_or("FIRN_X", "FreeViewer")     // neu: const mit Text
const VOLL: str = NAME + " " + "1.0"                    // zur Bauzeit gerechnet
```

`const` konnte bis zu dieser Runde Ganzzahlen, `bool` und (seit
FIRN-LUECKEN) Fließkommazahlen. Jetzt auch `str`.

## Wo das passiert: im Parser

`__env_or(a, b)` wird **im Parser** zu genau dem Knoten, den ein von Hand
geschriebenes Textliteral erzeugt (`ExprKind::Text` über dem Array-Literal
seiner Oktette). Alles hinter dem Parser sieht keinen Unterschied zu einem
Literal. Das beantwortet drei Forderungen auf einmal:

* der Wert funktioniert in `const`, in `static`, in einer Initialisierung und
  in einer Interpolation, ohne dass eine dieser Stellen einen neuen Fall lernt;
* ein Programm, das **läuft**, fragt die Umgebung nie wieder — die Oktette
  stehen im Binärprogramm, genau wie die eines Literals. Nachgeprüft mit
  `env -i` (`tools/env/run.sh`, Punkt 3);
* `firnc1` kann dasselbe an derselben Stelle tun
  (`lib/firnc1/parser.fi::env_call`), also drucken beide Übersetzer bei
  `--emit=ast-canon` denselben Text (`tools/parser_compare.sh`).

## Die Grenzen, und warum jede einzelne da ist

Ein Übersetzer, der **beliebige** Umgebung ins Binärprogramm schreibt, ist ein
Weg, die Geheimnisse einer Baumaschine in ein ausgeliefertes Programm zu
bekommen. Deshalb:

1. **Positivliste.** Ein Name wird nur gelesen, wenn er mit einem erlaubten
   Präfix beginnt. Ohne Option ist das allein `FIRN_`; der Bau nimmt seine
   eigenen mit `--env-allow=<präfix>` dazu (mehrfach oder kommagetrennt).
   Ein Name außerhalb der Liste ist ein **Fehler** — immer, egal ob die
   Variable gesetzt ist oder nicht. Das ist wichtig: ein Fehler, der von der
   Umgebung abhängt, wäre ein zweiter Weg, auf dem zwei Bauten sich
   unterscheiden.
2. **Gestalt des Namens.** `A-Z`, `0-9`, `_`, höchstens 64 Oktette. Nicht weil
   Kleinbuchstaben technisch schwer wären, sondern weil `__env_or("path", …)`
   neben `PATH` eine Falle ist.
3. **Wert.** Höchstens 4096 Oktette, gültiges UTF-8, keine Steuerzeichen. Zu
   lang oder kein UTF-8 ist ein Fehler und **kein stilles Abschneiden**: ein
   halbierter Markenname ist schlimmer als ein Bau, der stehen bleibt.
4. **Protokoll.** `--env-log` druckt jede Lesung mit Wert und Herkunft. Ohne
   die Option wird nichts gedruckt.

```
$ FIRN_TEST_BRAND=OrientOS firnc --env-log tests/1640_env_const.fi -o x
env: FIRN_TEST_BRAND = "OrientOS" (environment)
env: FIRN_TEST_BRAND ? true
```

Beide Übersetzer drucken diese zwei Zeilen zeichengleich.

## Der Fixpunkt

`bin/firnc1.fi` benutzt keine der beiden Intrinsics und hat keine
Positivliste über die Vorgabe hinaus — die Umgebung erreicht die
Selbstübersetzung also gar nicht. Stufe 2 und Stufe 3 bleiben zeichengleich,
was immer gesetzt ist. Das ist kein Glück, sondern der Grund, warum die
Positivliste standardmäßig leer ist.

## Was NICHT geht (ehrlich)

* **`static NAME: str = "…"`** geht nicht. Ein `str` ist ein Zeiger und eine
  Länge; ein Zeiger in einem Datenabschnitt braucht eine Relokation, und die
  hat Stufe 0 nicht. Die Meldung sagt das sauber. Was geht:
  `static NAME: [u8; 10] = __env_or(…)` — als Array, mit **exakt** passender
  Länge.
* **`[u8; _]` für `static`** geht nicht (Runde 79 hat die Längenableitung nur
  für `let`/`var` freigeschaltet). Deshalb muss man bei einem Array-`static`
  die Länge kennen.
* **`__env_or` mit berechnetem Namen** geht nicht und soll nicht: der Name wird
  beim Parsen gelesen. Was gerechnet werden muss, gehört in `comptime`.
* **Kein `__env_int_or`.** Eine Zahl aus der Umgebung wäre dieselbe Faltung mit
  `ExprKind::Int` statt `Text` — sie ist nicht gebaut, weil sie niemand
  gebraucht hat.
* **`comptime` rechnet weiter nur mit `i128`.** Die Zeichenketten dieser Runde
  liegen in den Konstantenwalks von `sema` (`const_octets`), nicht im
  `comptime`-Interpreter. Ein `comptime`-Programm kann mit Texten also
  weiterhin nicht rechnen; `emit_raw` nimmt nach wie vor nur Literale.

## Für OrientOS: wie die Zeile dort aussieht

Nachgewiesen, nicht umgebaut — der Umbau gehört dem OrientOS-Chat.

Heute (`kernel/marke.fi` + `marke.conf` + `tools/marke-einsetzen.py`):

```firn
static mut s_produkt: [u8; 32] = "???????????????????????????????\0"
// … und ein Python-Skript ersetzt die Fragezeichen in einer /tmp-Kopie
```

Mit dieser Runde, **im Kernprofil, ohne Sammler** — nachgeprüft mit
`firnc --profile=kernel -c`:

```firn
// Der Text ist ein Zeiger und eine Länge. Jede Struktur dieser Gestalt
// (Runde 88 nennt sie eine "Sicht" auf `str`) nimmt ein Textliteral --
// und damit auch ein `__env_or`.
struct Text { p: *mut u8, n: usize }

fn produkt() -> Text {
    let t: Text = __env_or("OSUM_MARKE_PRODUKT", "OrientOS")
    return t
}
```

Im App-Profil geht die kürzere Form, der direkte Zwilling von `brand.rs`:

```firn
const PRODUKT: str = __env_or("OSUM_MARKE_PRODUKT", "OrientOS")
```

Der Bauaufruf bekommt eine Option dazu:

```sh
OSUM_MARKE_PRODUKT="Xoffi OS" firnc --env-allow=OSUM_MARKE_ kernel/marke.fi …
```

Damit entfallen `tools/marke-einsetzen.py` (192 Zeilen), die `/tmp`-Kopie des
Kernbaums und die Fragezeichen-Platzhalter. `marke.conf` kann bleiben, wenn die
Vorgaben in einer Datei stehen sollen — dann liest der Bau sie und setzt sie als
Umgebung; die Sprache braucht sie nicht mehr. Der Längentest, den das Skript
macht (`MAX`/`MAX_URL`), wird überflüssig: ein `Text` trägt seine Länge selbst.

## Dateien dieser Runde

| Datei | was |
|---|---|
| `compiler/src/env.rs` | neu — Positivliste, Grenzen, Protokoll (6 Modultests) |
| `compiler/src/parser.rs` | `env_call`, `literal_octets`, `text_from_octets` |
| `compiler/src/sema.rs` | `const` mit `str`, `const_octets`, Zahlensperre |
| `compiler/src/lower.rs` | Textkonstante materialisieren wie ein Literal |
| `compiler/src/main.rs` | `--env-allow=`, `--env-log` |
| `lib/firnc1/parser.fi` | Zwilling: `env_call` und die Grenzen |
| `lib/firnc1/sema.fi` | Zwilling: `const_octets`, `k_tdata` |
| `lib/firnc1/lower.fi` | Zwilling: Textkonstante materialisieren |
| `bin/firnc1.fi` | die zwei Optionen, das Protokoll |
| `tests/1640_env_const.fi` | der Vorgabefall im Korpus |
| `tests/neg/1641…1645` | fünf Grenzen, je eine Meldung |
| `tools/env/run.sh` | beide Übersetzer, beide Fälle, `env -i` |
