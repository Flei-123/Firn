# Runde 46: Schnittstellen — `interface` und dynamischer Versand

**Basis: `a492d26` (main nach den Runden 43/44/45).** Zweig `r46-interfaces`.

Runde 45 brachte Methoden — aber ausdrücklich nur als Schreibhilfe: `x.m(a)`
wurde zu `Typ__m(&x, a)`, entschieden allein vom **statischen** Typ. Diese
Runde ergänzt den anderen Fall, den `SPEC.md` §6.2 seit v0.1 fordert und den
`DESIGNZIELE.md` §1 für `Io` voraussetzt: **eine Aufrufstelle, viele Typen.**

```firn
interface Flaeche {
    fn flaeche(*self) -> i64
    fn skaliere(*mut self, k: i64)
}

impl Flaeche for Rechteck { … }
impl Flaeche for Kreis    { … }

let f: dyn Flaeche = (&r) as dyn Flaeche
f.flaeche()          // über die Methodentafel — welcher Code läuft,
                     // steht erst zur Laufzeit fest
```

---

## 1. Die Darstellung — verbindlich

```text
interface I      -> Struct "dyn I" in types::TypeCtx, 16 Byte:
                      daten: *mut u8   (Versatz 0) — der Wert selbst
                      tafel: *mut u8   (Versatz 8) — die Methodentafel
dyn I            -> genau dieser Struct (ein FETTER ZEIGER, kein Zeiger)
impl I for T     -> Methodentafel `.L__iface.I.T` in `.rodata`:
                      .quad T__m1
                      .quad T__m2      (Reihenfolge = Reihenfolge in `I`)
```

Der interne Structname trägt ein **Leerzeichen** (`"dyn I"`) — dieselbe Bauart
wie `"gc C"` (gc.rs) und `"methode m"` (impls.rs). Er kann aus keinem
Bezeichner des Quelltextes entstehen, und `TypeCtx::name_of` gibt ihn
unverändert aus: in jeder Fehlermeldung steht `dyn I`, so wie man es schreibt.

### Warum ein Struct und kein eigener `Type`

Ein `Type::Dyn(..)` hätte **jede** Fallunterscheidung über `Type` angefasst:
Layout, ABI, Monomorphisierung, Optimierer, Debuginfo, Codegenerator. Als
Struct ist der Schnittstellenwert ein gewöhnliches 16-Byte-Aggregat:
`abi::classify` gibt ihm zwei INTEGER-Wörter, er wird kopiert, übergeben,
zurückgegeben und im Rahmen abgelegt wie jeder andere Struct. Deshalb gilt
alles, was `tests/820` durchprobiert — Parameter, Rückgabewert, Structfeld,
Arrayelement, Kopie —, ohne dass dafür eine Zeile geschrieben wurde.

Die ganze Sprachänderung sitzt damit in Parser, Typprüfer und **zwei**
FIR-Instruktionen.

---

## 2. Zwei neue FIR-Instruktionen

| Instruktion | Textform | Bedeutung |
|---|---|---|
| `Op::CallIndirect { target, args }` | `calli.i64 %7(%3, %4)` | Aufruf über einen Zeiger; sonst gilt alles wie bei `Call` |
| `Op::VtabAddr { tafel }` | `vtab.ptr @Flaeche.Kreis` | Adresse einer Methodentafel (`.rodata`) |

Beide erzeugen in **beiden** Codewegen von `firnc0` (Grundpfad und
Registerzuteilung) und in `firnc1` denselben Befehl:

```asm
    lea rax, [rip + .L__iface.Flaeche.Kreis]
    …
    call rax
```

**Warum `rax` und nicht `r11`:** `r11` ist in `regalloc.rs` ein
Arbeitsregister (`TEMP_REGS`) und kann die **Heimat eines Wertes** sein — ein
Argument, das gerade dort liegt, wäre beim Laden des Ziels zerstört. `rax` ist
nie Heimat eines Wertes (Kopf von `regalloc.rs`) und kein Argumentregister;
deshalb wird das Ziel dorthin und **zuletzt** geladen, nach allen Argumenten.
Das ist die einzige Stelle dieser Runde, an der eine falsche Entscheidung
still falschen Code erzeugt hätte.

Der Aufruf selbst läuft durch **denselben** Pfad wie jeder andere Aufruf
(`lower::lower_call`): Aggregate in Registern, versteckter Rückgabezeiger ab
9 Byte, Stapelargumente ab dem siebten Wort. Es gibt keinen zweiten
Aufrufpfad, der auseinanderlaufen könnte.

---

## 3. Wie ein Schnittstellenwert entsteht

**Ausdrücklich, nie still.** `SPEC.md` §6.2 verlangt die dynamische Auflösung
„ausdrücklich hingeschrieben", und §4.5 streicht implizite Umwandlungen
generell. Eine stille Umwandlung an einer Zuweisung, die zusätzlich eine
Methodentafel anhängt, wäre die schlechteste Stelle, damit anzufangen.

```firn
let f: dyn Flaeche = (&r) as dyn Flaeche
```

Die Klammern um `&r` sind Pflicht — `as` bindet stärker als die unären
Operatoren (`SPEC.md` §14.1 Punkt 12). Für einen Wert, der schon ein Zeiger
ist (`Gc[K]`, `*mut T`), entfallen sie.

Im Lowering sind das genau zwei Wörter:

```text
store.ptr %zeiger, [%ziel + 0]
%t = vtab.ptr @Flaeche.Kreis
store.ptr %t,     [%ziel + 8]
```

Der Datenzeiger wird **nicht verändert** — kein Versatz, keine Verschleierung.
Das ist die Voraussetzung für Abschnitt 5.

---

## 4. Der Aufruf

```text
%b = <adresse des schnittstellenwertes>
%d = load.ptr [%b + 0]      ; der Wert selbst
%t = load.ptr [%b + 8]      ; die Methodentafel
%z = load.ptr [%t + 8*k]    ; die k-te Methode der Schnittstelle
%r = calli.i64 %z(%d, …)
```

Drei Ladebefehle und ein indirekter Sprung je Aufruf. Der Preis steht damit
sichtbar im Quelltext des Compilers und in der FIR — `SPEC.md` §1, Leitsatz 1
(„nichts versteckt") und Leitsatz 4 („wer nicht bestellt, zahlt nicht"): ein
Programm ohne `interface` bekommt keine einzige dieser Instruktionen und keine
`.rodata`-Tafel.

### Gemessen (callgrind, `--tool=callgrind`, 2.000.000 Aufrufe in einer Schleife)

| | Instruktionen gesamt | je Durchlauf |
|---|---|---|
| statischer Aufruf, mit Optimierer | 10.000.017 | **5** |
| dynamischer Aufruf, mit Optimierer | 52.000.037 | **26** |
| statischer Aufruf, `--no-opt` | 150.000.066 | **75** |
| dynamischer Aufruf, `--no-opt` | 182.000.082 | **91** |

Ehrlich gelesen: der **reine** Versand kostet 16 Instruktionen (`--no-opt`,
beide Seiten ohne Inlining). Mit Optimierer klafft die Lücke weiter auf — 5
gegen 26 —, und zwar **nicht**, weil der Versand teurer würde, sondern weil
der statische Aufruf ganz verschwindet: `inline.rs` setzt ihn ein, der Rest
fällt der Konstantenfaltung zum Opfer. Genau das ist der Grund, warum `dyn`
in dieser Sprache ausdrücklich hingeschrieben wird und nicht die Voreinstellung
ist.

---

## 5. Der Sammler und der fette Zeiger

Der Datenzeiger ist ein gewöhnlicher Zeiger auf den **Anfang** des Wertes.
Ein `dyn I` liegt im Rahmen oder in einem callee-saved Register; beides
durchsucht der Sammler konservativ (`SPEC.md` §3.5.3), und vor einem
Sammellauf rettet `Op::GcAddr { regs: true }` die Register in den
Zustandsblock. Damit hält ein Schnittstellenwert sein Objekt am Leben, auch
wenn es sonst keine Wurzel mehr gibt.

`tests/822_iface_gc_core.fi` weist das nach, ohne sich auf Zufall zu
verlassen:

* 64 `gc class`-Zellen werden erzeugt und **ausschließlich** als `dyn Zaehler`
  in einem Array im Rahmen abgelegt,
* danach entstehen 20.000 unerreichbare Zellen (`muell`), die Laufzeit sammelt
  dabei von selbst,
* nach zwei ausdrücklichen `gc_collect()` leben **höchstens 200** Objekte —
  die 20.000 sind also wirklich gefegt —, und die 64 Werte sind unverändert,
* danach wird über die Schnittstelle geschrieben (`dazu`) und wieder gelesen.

Wären die fetten Zeiger keine Wurzeln, wären die 64 Zellen mitgefegt und ihr
Speicher an den Abfall vergeben worden; die Summe danach wäre Müll. Der Test
läuft in allen drei Übersetzungsarten (`release-fast`, `--no-opt`,
`dev-fast`) und unter `firnc1`.

**Die eine Stelle, an der das nicht trägt, ist der Heap:** dort verfolgt der
Sammler PRÄZISE anhand des Feldlayouts und kennt den Datenzeiger in einem
`dyn I`-Feld nicht. Ein `dyn I` als Feld einer `gc class` ist deshalb ein
Fehler — und zwar an der Stelle, an der die Regel schon lebt: `gc.rs` lässt in
einer Klasse ohnehin nur Ganzzahlen, `bool`, Zeiger, `Gc[T]`, `GcWeak[T]` und
Arrays davon zu, also auch keinen Struct und damit auch kein `dyn I`
(`tests/neg/iface_dyn_in_gc_class.fi` hält das fest). Eine zweite Prüfung
daneben wäre eine zweite Wahrheit gewesen.

### `impl I for <gc class>`

Damit der Nachweis oben überhaupt möglich ist, darf eine `gc class` eine
Schnittstelle umsetzen. Der Empfänger `*self` ist dann `Gc[K]` — ein
`gc class`-Wert existiert nur auf dem Heap, ein Zeiger darauf ist der einzige
Weg, ihn anzufassen (`SPEC.md` §3.5.1). Technisch trägt der Empfänger dafür
den internen Structnamen `"gc K"`; `self` als **Kopie** ist für eine Klasse ein
Fehler mit klarer Ansage.

**Einschränkung, offen benannt:** ob `K` eine Klasse ist, steht in der
Registrierung von `gc.rs`, und die wird beim **Parsen** gefüllt. `gc class K`
muss deshalb vor dem `impl`-Block stehen (in derselben Datei oder in einer
früher eingelesenen). Steht es dahinter, meldet der Typprüfer „'K' ist eine
gc-klasse und kann kein wert sein" — verständlich, aber nicht die Meldung, die
man sich wünscht. Beide Compiler verhalten sich hier gleich, weil beide
dieselbe Registrierung zum selben Zeitpunkt lesen.

---

## 6. Was geprüft wird — und wo

| Prüfung | Stelle | Negativtest |
|---|---|---|
| alle Methoden der Schnittstelle vorhanden | `iface::pruefe_umsetzung` | `iface_method_missing.fi` |
| Rückgabetyp passt | dieselbe | `iface_signature_ret.fi` |
| Parametertyp passt | dieselbe | `iface_signature_parameter.fi` |
| Parameterzahl passt (ohne Empfänger gezählt) | dieselbe | `iface_parameterzahl.fi` |
| Empfänger ist ein Zeiger auf genau diesen Typ | dieselbe | — |
| keine zwei `impl I for T` | dieselbe | `iface_duplicate_impl.fi` |
| die Schnittstelle gibt es | dieselbe | `iface_unknown.fi` |
| `dyn I` mit unbekanntem `I` | `iface::hook_resolve_ty` | `iface_dyn_unknown.fi` |
| Empfänger einer Schnittstellenmethode ist `*self`/`*mut self` | Parser | `iface_receiver_value.fi` |
| Schnittstellenmethode hat keinen Rumpf | Parser | `iface_body.fi` |
| nur Methoden der Schnittstelle sind über `dyn` erreichbar | `iface::hook_methode` | `iface_no_method.fi` |
| Umwandlung nur aus einem Zeiger auf einen Struct | `iface::hook_cast` | `iface_no_ptr.fi` |
| der Typ setzt die Schnittstelle wirklich um | dieselbe | `iface_does_not_impl.fi` |
| `*dyn I` ist kein Schnittstellenwert | `iface::hook_methode` | `iface_ptr_receiver.fi` |
| `dyn I` nicht im GC-Heap | `gc.rs` (Feldtypen einer Klasse) | `iface_dyn_in_gc_class.fi` |

Jede Meldung nennt Zeile, Spalte und im Hinweis die **erwartete Signatur**:

```
error: 'Kreis' setzt die methode 'Flaeche.skaliere' nicht um
  --> tests/neg/iface_method_missing.fi:13:6
   = hinweis: erwartet wird 'fn skaliere(*mut self, i64)' im block
```

Alle 14 Negativtests werden auch von `firnc1` abgelehnt. Bei
`iface_dyn_unknown.fi` endet `firnc1` mit 5 („das Lowering gibt auf") statt
mit 1 — das ist **nicht** neu und nicht schnittstellenspezifisch: ein
unbekannter Typname liefert in `firnc1` seit jeher einen Fehlertyp ohne eigene
Meldung (`let x: Unbekannt = 1` verhält sich genauso).

---

## 7. Namensauflösung

Methoden einer Schnittstelle sind **gewöhnliche Funktionen**: `impl I for T`
legt genau dieselben `T__m` an wie `impl T` aus Runde 45. Die Schnittstelle
sagt nur zusätzlich, was darin stehen **muss**. Daraus folgt:

* Der statische Aufruf bleibt möglich (`r.flaeche()`), und er ist derselbe
  Code, den die Tafel nennt.
* Ein zweiter, schnittstellenloser `impl`-Block für denselben Typ ist erlaubt.
* Eine freie Funktion darf weiter genauso heißen (`tests/820`: `flaeche(a, b)`
  neben `Rechteck.flaeche()`).

**Über `dyn` ist nur erreichbar, was in der Schnittstelle steht** — auch dann,
wenn der konkrete Typ mehr kann. Sonst wäre die Tafel nicht die ganze
Wahrheit.

### Module

`interface`-Namen gelten **programmweit** und werden nicht umbenannt — wie
Aufzählungen und `gc class` (`SPEC.md` §14.1 T6). Ein `impl Groesse for Kreis`
in einem Modul erzeugt dagegen `zeichnen__Kreis__groesse`, und genau dieser
Name muss in der Tafel stehen.

Der Typname in der Registrierung steht so da, wie er im Quelltext geschrieben
wurde — `firnc0` benennt **nach** dem Parsen um (`modules.rs`), `firnc1`
**während** des Parsens (`parser.fi`), und keiner von beiden fasst die
Registrierung an. Beide Compiler suchen den Struct deshalb in derselben
Reihenfolge (`iface.rs::typ_struct`, `iface.fi::typ_struct`):

1. der Name selbst,
2. `gc <Name>`,
3. genau **ein** Struct, dessen Name auf `__<Name>` endet (der Fall „Typ in
   einem Modul"). Mehrere Treffer sind ein Fehler — raten wäre die
   gefährlichere Wahl.

`tests/821_iface_module_core.fi` fährt beides gleichzeitig: zwei Umsetzungen im
Modul, eine in der Wurzeldatei, alle über dieselbe Schnittstelle.

---

## 8. Was in beiden Compilern geändert wurde

| `firnc0` (Rust) | Zeilen | was |
|---|---|---|
| `compiler/src/iface.rs` | **+1010 neu** | alles Eigene: Registrierung, Parser, Prüfungen, Auflösung, Lowering-Bausteine, `.rodata`-Tafeln |
| `compiler/src/impls.rs` | +100/−28 | `impl I for T`, Methodenpräfix ohne `"gc "`, Empfänger einer Klasse |
| `compiler/src/fir.rs` | +29 | `CallIndirect`, `VtabAddr`, Textform |
| `compiler/src/regalloc.rs` | +55/−2 | beide Instruktionen, Aufrufsperren erweitert |
| `compiler/src/codegen_x86.rs` | +47 | dieselben Instruktionen im Grundpfad, Tafeln |
| `compiler/src/lower.rs` | +59/−3 | Versand im gewöhnlichen Aufrufpfad, `as dyn I` |
| `compiler/src/sema.rs`, `parser.rs`, `mem2reg.rs`, `inline.rs`, `main.rs` | +46 | Haken |

| `firnc1` (Firn) | Zeilen | was |
|---|---|---|
| `lib/firnc1/iface.fi` | **+558 neu** | dieselbe Registrierung, dieselbe Namensarithmetik |
| `lib/firnc1/parser.fi` | +299/−13 | `interface`, `impl … for …`, `dyn I`, Vorabsuche |
| `lib/firnc1/codegen.fi` | +168/−1 | `O_CALLI`, `O_VTAB`, Methodentafeln |
| `lib/firnc1/lower.fi` | +143/−7 | Versand, `as dyn I` |
| `lib/firnc1/sema.fi` | +126/−1 | Anmeldung, Prüfung, Aufruf, Umwandlung |
| `lib/firnc1/fir.fi` | +29 | dieselben zwei Instruktionen, gleiche Textform |
| `bin/firnc1.fi`, `bin/semadump.fi`, `bin/firdump.fi` | +21 | Registrierung durchreichen |

`bin/astdump.fi` und `bin/layoutdump.fi` bekommen die Registrierung
**bewusst nicht**: ihr Maßstab (`ast_canon.rs`, `layout_canon.rs`) kennt nur
die Wurzeldatei, dort ist `dyn I` — wie `Gc[C]` und `E!T` — ein unbekannter
Name und wird zu `?`. Beide Seiten tun dasselbe, und der Vergleich bleibt
exakt.

---

## 9. Bewusst weggelassen

* **Vorgabemethoden** (`fn m(*self) -> i64 { … }` in der Schnittstelle). Sie
  bräuchten eine Funktion ohne Typ dahinter und eine Regel, welcher Name sie
  trägt. Heute ein Fehler mit klarer Ansage statt eines Syntaxfehlers.
* **Statische Auflösung über Schnittstellen** (`fn f[T: Flaeche](x: T)`,
  `SPEC.md` §6.2, erster Halbsatz). Das ist die Monomorphisierungsseite und
  gehört zu den Anforderungen an Typparameter (§14.1 T7) — eine eigene Runde.
* **Schnittstellen als Anforderung an eine generische Vorlage**, aus demselben
  Grund.
* **Vererbung zwischen Schnittstellen** (`interface A: B`).
* **`dyn I` im GC-Heap** — siehe Abschnitt 5, das ist eine Zusage, keine
  Lücke.
* **Umgekehrte Prüfung** (`x.as?[T]` von `dyn I` auf den konkreten Typ). Dafür
  müsste die Tafel eine Typkennung tragen; heute enthält sie nur
  Methodenzeiger. Der Platz dafür ist da (ein Wort vor der Tafel), die
  Entscheidung ist vertagt.
* **`#[no_gc]` und Schnittstellen.** Ein Aufruf über `dyn` ist in einer
  `#[no_gc]`-Funktion abgelehnt — dieselbe Regel wie für Methoden in Runde 45,
  und hier sogar zwingend: welche Funktion läuft, weiß der Prüfer nicht.
* **Tafeln nur für benutzte Umsetzungen.** Erzeugt wird eine Tafel je
  vollständiger Umsetzung, auch wenn kein `as dyn` sie je anfasst. Der Inhalt
  hängt allein an der Deklaration; eine Tafel, die nur manchmal entsteht, wäre
  die Sorte Zustand, die man beim Fehlersuchen nicht sehen will.

---

## 10. Verworfene Ansätze

**Ein eigener `Type::Dyn`.** Erster Entwurf, nach dem Durchzählen der
Fallunterscheidungen verworfen: `types.rs`, `abi.rs`, `layout.rs`, `mono.rs`,
`sema.rs`, `lower.rs` und `codegen_x86.rs` hätten je einen neuen Zweig
gebraucht, und `firnc1` dieselben noch einmal. Der Struct mit dem
Leerzeichen-Namen kostet null davon.

**Stille Umwandlung an Zuweisung und Argument.** Bequemer, aber gegen
`SPEC.md` §4.5 und §6.2 — und sie hätte die Frage aufgeworfen, welche der
möglichen Schnittstellen gemeint ist, sobald ein Typ mehrere umsetzt.

**Der Empfänger als eigene Typform** (`TypeExpr::Named("impl T")`), um `impl`
für `gc class` ohne Reihenfolgeregel zu ermöglichen. Verworfen, weil
`modules.rs` diesen Namen nicht mitbenennt: ein Modultyp wäre danach
unauffindbar gewesen — die Modulunterstützung aus Runde 45 wäre für einen
Randfall zerbrochen.

**`r11` als Sprungregister.** Naheliegend (caller-saved, kein Argumentregister)
und falsch: `regalloc.rs` vergibt `r11` als Heimat eines Wertes. Gefunden beim
Lesen von `TEMP_REGS`, nicht durch einen Testfehler — deshalb steht die
Begründung jetzt an beiden Codestellen.

**Eine Seitentabelle zwischen Typprüfer und Lowering.** Wie in Runde 45 nicht
gebaut: Empfängertyp und Methodenname stehen in beiden Phasen zur Verfügung,
die Ableitung ist reine Namensarithmetik. Eine Tabelle müsste `firnc1`
mitschleppen, ohne etwas zu können, was der Typ nicht schon sagt.

---

## 11. Abnahme

Gemessen auf `r46-interfaces` mit frisch gebautem `firnc0` und frisch
gebauten Hilfsbinärdateien (`.firnc1`, `.astdump`, `.semadump`, `.firdump`,
`.layoutdump`).

| Prüfung | Basis `a492d26` | jetzt |
|---|---|---|
| `bash ./test.sh` | 696/696 | **719/719** |
| `tools/self_compare.sh` | 201 / 0 / 0 | **204 gleich / 0 abweichend / 0 fehlerhaft** |
| `tools/fixpunkt.sh` | zeichengleich | **Stufe 2 == Stufe 3, zeichengleich (344.864 Zeilen Assembler)** |
| Parser (`parser_compare.sh`) | 240 gleich, 1 bekannt | 254 gleich, 1 bekannt |
| Layout/ABI (`types_compare.sh`) | 190 gleich, 0 ungleich | 204 gleich, 0 ungleich |
| Typprüfer (`sema_compare.sh`) | 147 gleich, 1 bekannt | 148 gleich, 1 bekannt |
| Lowering (`fir_compare.sh`) | 146 gleich, 1 bekannt | 147 gleich, 1 bekannt |

Die eine bekannte Abweichung ist unverändert `tests/590_f64.fi` (Literal
`1e308`, Rundungsfall aus Runde 20 — kein Parserfehler).

**Wie gemessen.** Die Basiswerte stammen aus einem eigenen Auszug von
`a492d26` (`git archive`, frisch gebaut, `bash ./test.sh` -> `PASS 696/696`),
die vier Vergleichszahlen auf beiden Seiten aus einem EINZELN gestarteten
Lauf des jeweiligen Skripts. Innerhalb von `test.sh` zählen `parser_` und
`typen_vergleich` je neun Dateien mehr (Zwischenstände, die die früheren
Schritte im Baum ablegen); das gilt für beide Seiten gleichermaßen und ist
kein Effekt dieser Runde — verglichen wird deshalb gleich mit gleich.

Der Zuwachs erklärt sich Datei für Datei: die vierzehn Negativtests kommen
durch den Parser (nur der Typprüfer lehnt sie ab) und werden dort mitgezählt;
`tests/820`, `tests/821`, `tests/modules/draw.fi` und `lib/firnc1/iface.fi`
kommen hinzu, `tests/822` zählt als „nicht Kern" (gc). Für Typprüfer und
Lowering bleibt nur `tests/820` übrig — die übrigen neuen Dateien kann
`firnc0` nicht einzeln prüfen (Modul, gc) oder sie sind Negativtests.

## 12. Zeilen

| | Zeilen |
|---|---|
| `firnc0` (Rust) | +1422 / −33 |
| `firnc1` (Firn) | +1326 / −22 |
| Tests (3 Programme, 1 Modul, 14 Negativtests) | +652 |
