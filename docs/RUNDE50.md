# Runde 50: Schranken — Generik und Schnittstellen zusammengebracht

**Basis: `cc1710f` (main nach den Runden 46/47/48).** Zweig `r50-generik`.

Seit Runde 30 gibt es generische Vorlagen (`Vec[T]`, `Map[K,V]`), seit
Runde 46 Schnittstellen mit dynamischem Versand (`interface I`, `dyn I`).
Zwischen beiden lag eine Lücke, die man an einer Zeile sieht:

```firn
fn vec_sortiere[T: Scalar](v: *mut Vec[T]) { … a < b … }
```

`Scalar` sagt, welche **Form** `T` hat (Ganzzahl, `bool`, Zeiger) — nicht, was
`T` **kann**. Der Vergleich musste deshalb fest verdrahtet werden, und damit
konnte `Vec[T]` genau die Typen sortieren, für die der Übersetzer ein `<`
kennt. Ein `Vec[Person]` ließ sich anlegen und füllen, aber nicht sortieren.

Diese Runde schließt die Lücke: **der Name einer Schnittstelle ist eine
Schranke.**

```firn
interface Ord {
    fn kleiner(*self, b: *Self) -> bool
}

fn vec_sortiere[T: Ord](v: *mut Vec[T]) { … a.kleiner(b) … }
```

---

## 1. Die Syntax — und warum genau diese

```text
fn f[T: Ord](…)                 eine Schranke
fn f[T: Scalar + Ord](…)        mehrere, alle gelten gleichzeitig
struct Paar[T: Ord] { … }       auch an einem Typ, nicht nur an einer Funktion
fn f[K: Int, V: Ord](…)         je Parameter eigene Schranken
```

**Kein neues Zeichen, kein neues Schlüsselwort.** Die Stelle nach dem
Doppelpunkt gab es schon (`[T: Int]`, Runde 30); neu ist nur, dass dort
**jeder** Name stehen darf. `Any`, `Int` und `Scalar` bleiben die drei
eingebauten Schranken, jeder andere Name ist der Name einer Schnittstelle.
`+` als Trenner ist die einzige Zutat — und `+` kann an dieser Stelle nichts
anderes bedeuten, weil zwischen zwei Typparameter-Namen kein Ausdruck steht.

**Der Name wird beim Parsen NICHT aufgelöst.** `Bound::parse` liefert für
jeden unbekannten Namen `Bound::Iface(name)`, ohne zu fragen, ob es die
Schnittstelle gibt. Das ist Absicht: `interface Ord` darf weiter unten in
derselben Datei oder in einer ganz anderen stehen, und der Parser sieht immer
nur eine Datei. Ein Tippfehler fällt deshalb erst bei der Ausprägung auf —
dafür mit der Liste der bekannten Namen (§3).

**Geprüft wird bei der AUSPRÄGUNG**, in `mono::bind_params`, also bevor der
Typprüfer läuft. Zu diesem Zeitpunkt gibt es weder Structtabelle noch
aufgelöste Typen; was es gibt, sind Namen: die Registrierung aus `iface.rs`
und die Liste aller Funktionsnamen des zusammengeführten Programms. Genau
daraus wird die Meldung gebaut — und genau deshalb kann sie sagen, **welche
Methode fehlt**, statt später als „unbekannte methode" mitten in einer
ausgeprägten Kopie aufzuschlagen.

### `Self` — ohne das geht keine Ordnung

Eine Schnittstelle aus Runde 46 kennt nur konkrete Parametertypen:

```firn
interface Ord { fn kleiner(*self, b: *Punkt) -> bool }   // nur für Punkt
```

Eine Ordnung vergleicht aber **zwei Werte desselben Typs**. Deshalb darf die
Signatur jetzt `Self` nennen — den Typ, der die Schnittstelle umsetzt:

```firn
interface Ord { fn kleiner(*self, b: *Self) -> bool }
impl Ord for Punkt { fn kleiner(*self, b: *Punkt) -> bool { … } }
impl Ord for i32   { fn kleiner(*self, b: *i32)   -> bool { … } }
```

Die ganze Sonderbehandlung: eine Methode, deren Signatur `Self` nennt, wird
**nicht global** aufgelöst, sondern **je Umsetzung** (`resolve_mit_self` in
`iface.rs`, `aufloesen_self` in `iface.fi`). Global hätte `Self` gar keinen
Typ.

**`Self` und `dyn` schließen einander aus.** Über `dyn I` steht erst zur
Laufzeit fest, welcher Typ dahintersteckt; `*Self` wäre für jeden ein anderer
Typ, und der Aufrufer könnte das Argument nicht bilden. Ein solcher Aufruf ist
deshalb ein Fehler — mit dem Hinweis auf den Weg, der geht:

```
error: 'Ord.kleiner' nennt 'Self' und ist deshalb nicht ueber 'dyn Ord' aufrufbar
   = hinweis: rufe sie ueber eine schranke auf: 'fn f[T: Ord](x: *T)' — dort steht der typ fest
```

Das ist die Objektsicherheitsregel dieser Sprache, in einem Satz und an einer
Stelle. `dyn I` bleibt für Schnittstellen ohne `Self` unverändert erlaubt —
auch für dieselbe Schnittstelle, solange nur ihre `Self`-freien Methoden über
`dyn` gerufen werden.

### `impl I for <Grundtyp>`

`vec_sortiere[i32]` muss weiter gehen. Also darf seit dieser Runde auch ein
**eingebauter Typ** eine Schnittstelle umsetzen:

```firn
impl Ord for i32 { fn kleiner(*self, b: *i32) -> bool { return *self < *b } }
```

Das legt die gewöhnliche Funktion `i32__kleiner(self: *i32, b: *i32)` an —
dasselbe Namensschema wie für einen Struct (Runde 45). Zwei Folgen:

* **Methoden eines Grundtyps gelten programmweit.** `modules.rs` benennt sie
  nicht um. Der Typ `i32` gehört keinem Modul, seine Methoden also auch
  keinem; hieße die Methode in `std.vec` `vec__i32__kleiner`, suchte die
  Auflösung weiter `i32__kleiner` und fände nichts. `firnc1` tat das schon
  immer so — `eigene_suchen` überspringt `impl`-Blöcke —, hier sind beide
  Compiler jetzt aus demselben Grund gleich.
* **Ein Grundtyp bekommt keine Methodentafel.** Eine Tafel gibt es nur je
  Struct-Umsetzung, denn nur ein Struct kann hinter einem `dyn I` stehen
  (`hook_cast` verlangt einen Zeiger auf einen Struct). `(&n) as dyn Zeigbar`
  mit `n: i64` ist ein Fehler, `tests/neg/schranke_dyn_grundtyp.fi`.

---

## 2. Was daraus folgt: der Versand ist statisch

Das ist der eigentliche Gewinn, und er kostete **keine Zeile im Codegenerator
und keinen neuen FIR-Opcode**. Nach der Monomorphisierung steht in

```firn
fn kleineres[T: Ord](a: *T, b: *T) -> *T { if a.kleiner(b) { return a } … }
```

für `T = Punkt` ein gewöhnlicher Methodenaufruf auf `*Punkt` — und den löst
`impls.rs` seit Runde 45 allein aus dem statischen Typ auf. Es gibt an dieser
Stelle nichts zu versenden.

```asm
; fn f[T: Ord] (--no-opt)        | ; dyn OrdnungD (--no-opt)
    mov rdi, qword ptr [rbp-40]  |     mov rcx, qword ptr [rbp-64]   ; Tafel
    mov rsi, qword ptr [rbp-48]  |     mov rax, qword ptr [rcx]      ; Eintrag 0
    call _F0.Punkt__kleiner      |     mov qword ptr [rbp-176], rax
                                 |     mov rdi, qword ptr [rbp-136]  ; Datenzeiger
                                 |     mov rsi, qword ptr [rbp-176]
                                 |     mov rax, qword ptr [rbp-168]
                                 |     call rax
```

`tools/schranken/run.sh` hält das fest — und zwar nicht mit der Uhr, sondern
am erzeugten Code. Zwei Programme, dieselbe Arbeit; geprüft wird:

| Prüfung | Schranke | `dyn` |
|---|---|---|
| indirekte Aufrufe (`call <register>`) im Assembler | **0** | ≥ 1 |
| `lea … .L__iface…` (Adresse einer Methodentafel) | **nein** | ja |
| `calli` in der FIR | **0** | ≥ 1 |
| `vtab` in der FIR | **0** | ≥ 1 |
| namentlicher `call … Punkt__kleiner` | ja | — |

in **drei Baustufen** (`release-fast`, `--no-opt`, `dev-fast`) und in **beiden
Compilern**. Die Gegenprobe mit `dyn` ist Teil des Tests: ohne sie würde er
auch dann bestehen, wenn er gar nichts misst. Der Lauf hängt als Schritt 8c in
`test.sh` (dort ohne die callgrind-Messung, damit er kurz bleibt).

### Gemessen (callgrind, 2.000.000 Aufrufe in einer Schleife)

| | Instruktionen gesamt | je Durchlauf |
|---|---|---|
| Schranke, `release-fast` | 10.000.019 | **5** |
| `dyn`, `release-fast` | 58.000.046 | **29** |
| Schranke, `--no-opt` | 182.000.106 | **91** |
| `dyn`, `--no-opt` | 208.000.145 | **104** |

Ehrlich gelesen: der **reine** Versand kostet 13 Instruktionen je Aufruf
(`--no-opt`, beide Seiten ohne Inlining — die drei Ladebefehle aus
docs/RUNDE46.md §4 plus der indirekte Sprung und das, was er an
Registerrettung nach sich zieht). Mit Optimierer klafft die Lücke weiter auf,
5 gegen 29, und zwar **nicht**, weil der Versand teurer würde, sondern weil
der statische Aufruf ganz verschwindet: `inline.rs` setzt ihn ein, der Rest
fällt der Konstantenfaltung zum Opfer. Ein indirekter Sprung kann das nicht.

Das ist derselbe Befund wie in Runde 46 (dort 5 gegen 26 mit einer
Schnittstelle ohne Argument) — neu ist, dass man die statische Seite jetzt
**mit einer Schnittstelle** hinschreiben kann und nicht nur ohne.

---

## 3. Die Fehlermeldungen

Jede nennt Zeile, Spalte und im Hinweis, was zu tun ist. Die interessanten
sind die, die eine Methode benennen.

**Kein `impl` — die Meldung sagt, welche Methode fehlen würde:**

```
error: typ 'Kreis' setzt die schnittstelle 'Ordnung' nicht um — schranke am typparameter 'T' von 'kleineres'
  --> tests/neg/schranke_kein_impl.fi:28:21
   = hinweis: es fehlt 'fn kleiner(*self, *Self) -> bool' in 'impl Ordnung for Kreis { … }'
```

**Der Typ hat schon einen Teil — dann steht nur der Rest da.** `Punkt` hat
`kleiner` aus einem gewöhnlichen `impl`-Block, aber keinen `impl Ordnung
for`-Block; genannt wird nur `gleich`:

```
error: typ 'Punkt' setzt die schnittstelle 'Ordnung' nicht um — schranke am typparameter 'T' von 'f'
   = hinweis: es fehlt 'fn gleich(*self, *Self) -> bool' in 'impl Ordnung for Punkt { … }'
```

Dafür liest `schranke_pruefen` die Namen aller Funktionen des
zusammengeführten Programms und fragt für jede Methode der Schnittstelle, ob
`<Typ>__<Methode>` existiert. Hätte der Typ **alle** Methoden und nur den
Block nicht, sagt die Meldung genau das („es fehlt der block `impl … { … }`").

**Schranke auf einer Schnittstelle, die es nicht gibt:**

```
error: unbekannte schnittstelle 'Ordnunng' als schranke am typparameter 'T' von 'f'
   = hinweis: bekannt sind: Ordnung (eingebaut: Any, Int, Scalar)
```

**Ein Zeiger als Typargument** — hier ist die Ursache eine andere, und die
Meldung sagt es:

```
error: typargument '*Punkt' erfuellt die schranke 'Ordnung' des typparameters 'T' von 'f' nicht
   = hinweis: eine schnittstelle wird mit 'impl Ordnung for <typ>' umgesetzt;
              ein zeiger- oder feldtyp hat keinen namen, unter dem das stehen koennte
```

**Dieselbe Schranke zweimal** — beim Parsen, nicht erst bei der Ausprägung:

```
error: die schranke 'Ordnung' steht zweimal an 'T'
   = hinweis: jede schranke wird hoechstens einmal genannt
```

**Mehrere Schranken, eine verletzt** — gemeldet wird die **erste**; eine
zweite Meldung zu demselben Typargument sagte nichts Neues.

### Die 15 Negativtests

| Datei | Fall |
|---|---|
| `schranke_kein_impl.fi` | Typ ohne `impl` (Meldung nennt die Methode) |
| `schranke_methode_teilweise.fi` | Typ hat eine von zwei Methoden — genannt wird nur die fehlende |
| `schranke_grundtyp_ohne_impl.fi` | Grundtyp ohne Umsetzung |
| `schranke_unbekannt.fi` | Schranke auf unbekannter Schnittstelle |
| `schranke_doppelt.fi` | dieselbe Schranke zweimal (Parser) |
| `schranke_widerspruch.fi` | `Int + Ordnung`, `Int` verletzt |
| `schranke_zweite_schnittstelle.fi` | `Ordnung + Anzeige`, zweite verletzt |
| `schranke_verschachtelt.fi` | Verletzung erst in der ZWEITEN Ausprägungsstufe |
| `schranke_struct.fi` | Schranke an einem generischen Struct |
| `schranke_zeigerargument.fi` | Zeiger als Typargument |
| `schranke_signatur.fi` | `impl` da, Signatur passt nicht (`Self` ≠ `i64`) |
| `schranke_self_dyn.fi` | `Self`-Methode über `dyn` gerufen |
| `schranke_dyn_grundtyp.fi` | `as dyn I` auf einem Grundtyp |
| `schranke_doppelte_umsetzung_grundtyp.fi` | zwei `impl Ord for i32` |
| `methode_ohne_typ.fi` | Methode auf einem Feldtyp (hat keinen Namen) |

Dazu geändert: `generic_anforderung.fi` (Wortlaut „anforderung" → „schranke")
und `impl_kein_struct.fi` — dessen alte Meldung („methoden gibt es nur fuer
struct-typen") ist seit dieser Runde falsch; er prüft jetzt, dass `i32.summe()`
sauber als „typ 'i32' hat keine methode 'summe'" abgelehnt wird.

Alle 15 werden auch von `firnc1` abgelehnt (nachgemessen: 109 von 115
Negativtests lehnt `firnc1` ab; die 6 Ausnahmen sind dieselben wie vor dieser
Runde und betreffen sie nicht).

---

## 4. Die Standardbibliothek: vorher / nachher

`lib/rt/vec.fi` (identisch als `lib/std/vec.fi`, Symlink).

| | vorher | nachher |
|---|---|---|
| `vec_sortiere` | `[T: Scalar]`, im Rumpf `a > b` | `[T: Ord]`, im Rumpf `a.kleiner(b)` |
| `vec_binaersuche` | `[T: Scalar]`, `<` und `==` | `[T: Ord]`, Gleichheit aus der Ordnung |
| `vec_ist_sortiert`, `vec_untere_schranke`, `vec_sortiert_einfuegen`, `vec_senken` | `[T: Scalar]` | `[T: Ord]` |
| `vec_min`, `vec_max` | `[T: Scalar]` | `[T: Scalar + Ord]` |
| sortierbare Typen | die Skalare, für die der Übersetzer `<` kennt | **jeder Typ mit `impl Ord`** |
| Ordnung wählbar | nein | ja, sie gehört dem Typ |

**Was dafür nötig war — `vec_zeiger[T]`.** `vec_at[T]` liefert jenseits des
Endes `0 as T`, und `0 as T` gibt es nur für Skalare. Genau daran hing die
Schranke `T: Scalar`, und genau deshalb ließ sich ein `Vec[Punkt]` nicht
sortieren. Ein **Zeiger** hat für jeden Elementtyp einen Nullwert; alles, was
ordnet (Sortieren, Suchen, Tauschen, Einfügen), arbeitet jetzt darüber. `vec_at`
bleibt unverändert `[T: Scalar]` — es ist die bequeme Fassung für Skalare.

**Was das kostet.** `interface Ord` und zehn Umsetzungen (`i8`…`isize`) stehen
in `lib/rt/vec.fi` und sind damit in jedem Programm da, das `vec` einbindet.
Das sind zehn Funktionen mit je einem Vergleich; eine Methodentafel entsteht
für keine davon (Grundtyp). Nachgemessen an einem Programm, das nie sortiert — dem Compiler
selbst: `firnc0 --emit=asm bin/firnc1.fi` liefert **209.388** Zeilen mit dem
alten `vec.fi` und **209.579** mit dem neuen. Der Preis für `Ord` und zehn
Umsetzungen in einem Programm, das sie nicht benutzt, sind also **191 Zeilen
Assembler (+0,09 %)**.

**Was das bringt** — `tests/831_schranken_std_kern.fi` fährt beide Seiten:
dieselben Funktionen mit `i32` (wie bisher) und mit

```firn
struct Person { alter: i64, nummer: i64 }
impl Ord for Person {
    fn kleiner(*self, b: *Person) -> bool {
        if (*self).alter != (*b).alter { return (*self).alter < (*b).alter }
        return (*self).nummer < (*b).nummer
    }
}
```

Zwei Schlüssel, absteigend nach dem zweiten wäre genauso möglich — darum geht
es: die Ordnung gehört dem Typ, nicht dem Sortierer. Vorher war dieses
Programm nicht schreibbar.

**Gleichheit aus der Ordnung.** `vec_binaersuche` prüft `!(a<b) && !(b<a)`
statt `a == b`. Das ist keine Bequemlichkeit, sondern die einzige Gleichheit,
die zu einer Binärsuche passt: sie muss dieselbe sein, nach der sortiert wurde.
Sonst fände die Suche eine Stelle, an der nach der Ordnung nichts steht.

**Warum `Ord` nicht über einen Schlüssel geht.** Der einfachere Entwurf wäre
`interface Ord { fn schluessel(*self) -> i64 }` gewesen — ohne `Self`, ohne
Umsetzungen für Grundtypen. Er scheitert an einem Wert, der schon im
Testkorpus steht: `tests/802_std_vec_kern.fi` sortiert `u64` und sucht
`9223372036854775808`. Der passt in kein `i64`. Ein Schlüssel hätte die
Ordnung für die Hälfte aller `u64` still falsch gemacht.

---

## 5. Was geändert wurde

| `firnc0` (Rust) | Zeilen | was |
|---|---|---|
| `compiler/src/iface.rs` | +410/−45 | Schrankenprüfung mit Methodennamen, `Self`, Grundtyp-Umsetzungen |
| `compiler/src/mono.rs` | +84/−26 | alle Schranken je Parameter, Weg zu `iface.rs` |
| `compiler/src/sema_generic.rs` | +53/−22 | `Bound::Iface`, `+`-Listen, doppelte Schranke |
| `compiler/src/impls.rs` | +49/−16 | Empfänger darf ein Grundtyp sein |
| `compiler/src/modules.rs` | +9 | Grundtyp-Methoden nicht umbenennen |

| `firnc1` (Firn) | Zeilen | was |
|---|---|---|
| `lib/firnc1/iface.fi` | +222/−48 | `Self`, Grundtyp-Umsetzungen, `if_umsetzung_da` |
| `lib/firnc1/mono.fi` | +107/−12 | Schrankenlisten, Schnittstellenschranken |
| `lib/firnc1/parser.fi` | +55/−17 | `+`-Listen, `Self`-Erkennung |
| `lib/firnc1/typen.fi` | +32 | `grundtyp_name` (die Umkehrung von `grundtyp`) |
| `lib/firnc1/sema.fi`, `lower.fi`, `codegen.fi` | +41/−16 | Methoden auf Grundtypen, keine Tafel für einen Grundtyp |

**Keine neuen FIR-Opcodes.** Die Opcode-Regel dieser Runde (Nummern 30–39
reserviert) wurde nicht gebraucht: statischer Versand ist ein gewöhnlicher
`Call`, und die Schranke ist eine Prüfung, keine Instruktion. Der reservierte
Bereich bleibt unangetastet — `fir.rs`/`fir.fi` sind unverändert.

---

## 6. Abnahme

Gemessen auf `r50-generik` nach `rm -f .firnc1 .firnc2 .firnc3` (kein
wiederverwendetes Binary), eigenes `mktemp -d` in jedem Werkzeug.

| Prüfung | Basis `cc1710f` | jetzt |
|---|---|---|
| `bash ./test.sh` | 751/751 | **PASS 773/773** |
| `bash tools/selbst_vergleich.sh` | 213 gleich / 0 abweichend / 0 fehlerhaft | **215 gleich / 0 abweichend / 0 fehlerhaft** |
| `bash tools/fixpunkt.sh` | zeichengleich, 427.401 Zeilen | **Stufe 2 == Stufe 3, zeichengleich, 431.972 Zeilen** |

Die +22 in `test.sh` erklären sich Datei für Datei: 2 neue Programme x 3
Baustufen (`830`, `831`) = 6, 15 neue Negativtests = 15, der neue Schritt 8c
(`tools/schranken/run.sh`) = 1. Die +2 im Selbstvergleich sind dieselben zwei
Programme; `tests/modules/schranken.fi` zählt nicht mit (`firnc0` übersetzt ein
Modul nicht einzeln).

Beide Vergleichszahlen stammen aus einem EINZELN gestarteten Lauf des
jeweiligen Skripts, jeweils nach `rm -f .firnc1 .firnc2 .firnc3` — kein
wiederverwendetes Binary. `tools/fixpunkt.sh` und `tools/schranken/run.sh`
legen ihr Arbeitsverzeichnis mit `mktemp -d` an; feste `/tmp`-Namen gibt es
in dieser Runde keine.

**Der Fixpunkt hält.** `.firnc2` (von einem Compiler erzeugt, der aus Rust
kam) und `.firnc3` (von einem, der aus Firn kam) sind Oktett für Oktett
gleich — 2.483.328 Oktette. Die Sprachänderung dieser Runde ist damit in
beiden Compilern dieselbe, nicht nur ähnlich.

Der Assembler von `.firnc2` wuchs von 427.401 auf 431.972 Zeilen (+1,07 %) —
das ist die ganze Runde, überwiegend `iface.fi` und `mono.fi`.

---

## 7. Bewusst weggelassen

* **Prüfung des Vorlagenrumpfes gegen die Schranke.** Die Schranke ist heute
  eine Zusage an den **Aufrufer**, keine Beschränkung des Rumpfes. Der Rumpf
  wird erst nach der Ausprägung geprüft, also gegen den **konkreten** Typ:
  eine Vorlage `fn f[T: Ord](a: *T)` darf `a.etwas_anderes()` schreiben, und
  das geht durch, solange der ausgeprägte Typ diese Methode hat. Eine Vorlage,
  die nie ausgeprägt wird, wird gar nicht geprüft. Das ist die Kehrseite der
  Monomorphisierung ohne getrennte Typprüfung der Vorlage; wer es ändern will,
  braucht einen Prüflauf über den Rumpf mit `T` als abstraktem Typ — eine
  eigene Runde, und eine, die die Fehlermeldungen aller bestehenden Vorlagen
  anfasst.
* **Statisch unerfüllbare Schrankenmengen.** `[T: Int + Ordnung]` ist erlaubt,
  auch wenn keine Umsetzung von `Ordnung` je ein Ganzzahltyp ist. Gemeldet
  wird bei der Ausprägung, nicht bei der Deklaration. Das zu erkennen hieße,
  alle Umsetzungen zu zählen — und die dürfen später noch dazukommen.
* **Vererbung zwischen Schnittstellen** (`interface A: B`) und
  **Vorgabemethoden** — beides steht seit Runde 46 offen und ist es geblieben.
* **`dyn I` für Grundtypen.** Ein Schnittstellenwert trägt Datenzeiger und
  Methodentafel; eine Tafel entsteht nur je Struct-Umsetzung.
* **Schranken an `gc class`-Typen.** `impl I for <gc class>` geht seit
  Runde 46; als **Typargument** einer Vorlage steht eine Klasse nur als
  `Gc[K]` zur Verfügung, und das ist ein Zeigertyp — siehe die nächste Zeile.
* **Ordnung für `f64` und `bool`.** `lib/rt/vec.fi` setzt `Ord` nur für die
  zehn Ganzzahltypen um. `f64` hätte Gleitkommacode in eine Datei gebracht,
  die `firnc1` selbst übersetzt (dessen Codegenerator kann kein Gleitkomma);
  `bool` hat keine Ordnung, die jemand erwartet.

---

## 8. Verworfene Ansätze

**Schranken beim Parsen auflösen.** Erster Entwurf: unbekannter Name =
Fehler, wie bisher bei `Any/Int/Scalar`. Verworfen, sobald `interface Ord`
unter `fn vec_sortiere[T: Ord]` stehen soll — und in `lib/rt/vec.fi` steht es
genau so, weil die Umsetzungen für die Grundtypen dazwischen liegen. Der
Parser sieht immer nur eine Datei; er kann diese Frage nicht beantworten.

**Strukturelle Erfüllung** („der Typ hat die Methoden, also erfüllt er die
Schranke"). Bequem und falsch: `impl I for T` ist die Stelle, an der der
Typprüfer die **Signaturen** prüft. Ohne Block gibt es diese Prüfung nicht,
und eine zufällig gleichnamige Methode mit anderer Bedeutung wäre stillschweigend
akzeptiert worden. Die Namen der vorhandenen Methoden werden trotzdem gelesen —
aber nur, um die **Meldung** brauchbar zu machen.

**`Ord` über einen Schlüssel** (`fn schluessel(*self) -> i64`). Siehe §4:
scheitert an `u64`-Werten oberhalb von `i64::MAX`, und die stehen schon im
Testkorpus.

**Grundtyp-Methoden pro Modul umbenennen.** Wäre die Regel gewesen, die
`modules.rs` sonst anwendet — und hätte `i32__kleiner` aus `std.vec` in
`vec__i32__kleiner` verwandelt, während die Auflösung am Aufrufort weiter
`i32__kleiner` sucht (sie rechnet aus dem **Typ**, und der Typ heißt in jedem
Modul `i32`). Verworfen zugunsten der Regel „programmweiter Typ, programmweite
Methoden" — dieselbe, die für `interface`, `enum`, `gc class` und generische
Vorlagen schon gilt.

**`struct_idx` als Schlüssel für „doppelte Umsetzung".** Funktionierte, solange
jede Umsetzung einen Struct hatte. Ein Grundtyp hat keinen; alle hätten
`usize::MAX` getragen und wären als **dieselbe** Umsetzung gezählt worden.
Verglichen wird jetzt der Methodenpräfix (`i32`, `geo__Punkt`) — er ist für
beide Fälle da und eindeutig.

**Die Endungsregel auch für Grundtypen.** `iface::typ_struct` sucht als
dritten Schritt „genau einen Struct, dessen Name auf `__<Name>` endet" (Typ aus
einem Modul). Für `i32` findet diese Regel `Vec__i32` — den Struct `Vec[i32]`.
Der Fehler war echt und stand nach fünf Minuten im Testlauf
(`'Vec__i32' setzt die methode 'Ord.kleiner' nicht um`). Deshalb wird der
Grundtyp **zuerst** gefragt, und die Endungsregel gilt nur für Namen, die
keiner sind.

---

## 9. Offen geblieben — und wo es weh tut

**Typargumente aus einem Modul gehen nicht.** Das ist ÄLTER als diese Runde
(seit Runde 30) und fiel hier auf, weil `tests/830` es zuerst versucht hat:

```firn
groesster[schranken.Marke](&a, &b)      // error: unbekannter typ 'schranken.Marke'
```

Ursache: der Ausprägungsname (`groesster__schranken.Marke`) entsteht beim
**Parsen** und steht danach als Aufrufname im Baum; die Modulumbenennung läuft
erst **danach** und fasst die Typargumente in der Registrierung nicht an. Der
substituierte Typ heißt anschließend `schranken.Marke`, der Struct aber
`schranken__Marke`. Dasselbe gilt für einen modullokalen Typ, der innerhalb
seines eigenen Moduls als Typargument benutzt wird. Nicht repariert, weil die
Reparatur die Ausprägungsnamen im ganzen Baum umschreiben müsste — das ist
eine eigene Runde und berührt `ast_kanon`. `tests/modules/schranken.fi` prüft
deshalb, was geht: Schnittstelle und Vorlage im Modul, Umsetzung und
Typargument in der Wurzeldatei, plus `impl Reihe for u16` im Modul.

**Kleine Falle, festgehalten:** `*self as i64` ist `*(self as i64)` — `as`
bindet stärker als die unären Operatoren (SPEC §14.1 Punkt 12). Richtig ist
`(*self) as i64`. Gekostet hat das einen Fehlversuch in
`tests/modules/schranken.fi`, mit einer Meldung, die auf eine leere Zeile zeigte.
