# Runde 45: Methoden — `impl`

**Basis: `f48e51c` (main nach Runde 42).** Zweig `r45-impl`.

Bis zu dieser Runde hatte Firn ausschließlich freie Funktionen. Die
Standardbibliothek aus Runde 42 liest sich entsprechend:

```firn
bytes_push(&b, 65 as u8)
let n: usize = laenge(trimme(quelle))
str16_set(&s, 0, 73 as u16)
```

Das Präfix ist der Typ — nur eben von Hand hingeschrieben und vom Compiler
nicht geprüft. Diese Runde macht daraus:

```firn
b.dazu(65 as u8)
let n: usize = quelle.trimme().laenge()
s.setze_bei(0, 73 as u16)
```

---

## 1. Die eine Entwurfsentscheidung: `impl` ist eine Schreibhilfe

Eine Methode ist in Firn **keine neue Art von Ding**. `impl` hat genau zwei
Wirkungen:

1. `impl T { fn m(*mut self, x: i32) … }` legt die **gewöhnliche Funktion**
   `T__m(self: *mut T, x: i32)` an. Sie steht danach in `Program::funcs` wie
   jede andere.
2. `x.m(a)` wird zum Aufruf dieser Funktion. Welche gemeint ist, entscheidet
   **allein der statische Typ des Empfängers**.

Kein dynamischer Versand, keine vtable, kein Suchen zur Laufzeit. Steht der
Typ fest, steht der Sprungbefehl fest. (Interfaces und dynamischer Versand
sind ausdrücklich **nicht** Teil dieser Runde.)

Der Gewinn dieser Bauweise ist nicht Bequemlichkeit, sondern **Reichweite bei
null Risiko für alles Nachgelagerte**: Monomorphisierung, `#[must_consume]`
über den Typ, Aufrufkonvention, Registerzuteilung, Optimierer, Debuginfo und
der Symbolplan haben nach dem Parsen eine ganz normale Funktion vor sich und
mussten **nicht angefasst** werden. Die gesamte Sprachänderung sitzt in
Parser, Typprüfer und dem einen Argument im Lowering.

---

## 2. Der Empfänger: `*self`, nicht `&self`

Die Aufgabe schlug `self` / `&self` / `&mut self` vor. Das passt nicht zu
dieser Sprache, und zwar aus einem Grund:

> **Firn hat keine Referenzen.** Es hat Zeiger (`*T`, `*mut T`) und den
> Adressoperator `&x`. Ein `&self` wäre ein Begriff, der in der ganzen
> übrigen Sprache nicht vorkommt — `&` heißt hier „Adresse von", nicht
> „Referenz auf".

Der Empfänger wird deshalb so geschrieben, wie in Firn jeder andere Parameter
geschrieben wird:

| Schreibweise | Typ von `self` | gedacht für |
|---|---|---|
| `fn m(self)` | `T` | kleine Werte, Sichten, reine Berechnungen |
| `fn m(*self)` | `*T` | lesende Methoden auf besitzenden Puffern |
| `fn m(*mut self)` | `*mut T` | ändernde Methoden |

Am Aufrufort gibt es genau **eine** Anpassung — die, die man sonst von Hand
schreibt:

* Methode will einen Zeiger, Empfänger liegt als **Wert** vor → der Compiler
  nimmt seine Adresse (`&x`).
* Methode will einen Zeiger, Empfänger ist **schon ein Zeiger** → er wird
  durchgereicht.

Mehr Automatik gibt es nicht:

* **Kein automatisches Dereferenzieren.** Will die Methode eine Kopie und
  liegt ein Zeiger vor, ist das ein Fehler mit Vorschlag: `(*z).erstes()`.
  (`tests/neg/impl_receiver_is_ptr.fi`)
* **Keine stille Zwischengröße.** `p.verschoben(1).summe()` ist ein Fehler,
  wenn `summe` einen Zeiger will: das Ergebnis eines Aufrufs hat keine
  Adresse. Erst binden, dann rufen. Ein stiller Zwischenwert wäre die
  gefährlichere Wahl — Änderungen daran kämen nirgends an.
  (`tests/neg/impl_receiver_without_address.fi`)
* **Keine Kette über mehrere Ebenen**, kein `**p`, kein Weg über Felder.

### Ehrlich benannt: `*self` und `*mut self` prüfen heute dasselbe

`sema::compatible` vergleicht Zeiger **ohne** die Veränderlichkeit — `*T` und
`*mut T` sind in dieser Sprache füreinander einsetzbar (das gilt für jeden
Parameter, nicht erst für den Empfänger). `*mut self` sagt damit heute genau
so viel wie `*mut T` an einem gewöhnlichen Parameter: es ist **Absicht,
festgehalten im Quelltext**, kein Zwang durch den Compiler. Wird die
Veränderlichkeit von Zeigern eines Tages geprüft, gilt die Prüfung für den
Empfänger ohne eine weitere Zeile in dieser Runde mit.

Der Adressoperator selbst ist ebenfalls unverändert übernommen: `&x` verlangt
einen **Platz** (Variable, Feld, Arrayelement, `*p`) und liefert `*mut`. Die
Empfängerprüfung benutzt exakt dieselbe Menge — eine zweite, abweichende
Regel für Methoden gibt es nicht.

---

## 3. Wie ein Methodenaufruf durch beide Compiler läuft

Der Aufruf trägt bis zur Typprüfung den Namen `"methode m"`. Das Leerzeichen
macht ihn zu einem Namen, der aus keinem Bezeichner des Quelltextes entstehen
kann — dieselbe Bauart wie `"gc C"` in `gc.rs`.

| Phase | was passiert | `firnc0` | `firnc1` |
|---|---|---|---|
| Parser, Item | `impl T { … }` → Funktionen `T__m` | `impls.rs::hook_item` | `parser.fi::impl_deklaration` |
| Parser, Ausdruck | `x.m(a)` → `Aufruf("methode m", [x, a])` | `impls.rs::hook_methodenaufruf` | `parser.fi::nach_ausdruck` |
| Typprüfer | Empfängertyp → Strukturname → `T__m`, Empfänger anpassen, Argumente prüfen | `impls.rs::hook_call` | `sema.fi::methoden_ruf` |
| Typprüfer, `probe` | Rückgabetyp ohne zu melden (sonst bekommt `p.summe() != 42` keinen Literaltyp) | `sema.rs::probe_d` | `sema.fi::probe_t` |
| Lowering | Ziel **neu ableiten**, Empfänger ggf. als Adresse | `lower.rs::lower_call` | `lower.fi::ruf_voll` |

**Warum keine Seitentabelle zwischen Typprüfer und Lowering?** Weil es nichts
zu übertragen gibt: Empfängertyp und Methodenname stehen in beiden Phasen zur
Verfügung, die Ableitung ist reine Namensarithmetik. Eine Tabelle müsste in
`firnc1` mitgeschleppt und über die Monomorphisierung hinweg gültig gehalten
werden — Aufwand für eine Auskunft, die der Typ schon gibt. In `firnc0` steht
die Ableitung deshalb an **einer** Stelle (`impls::ziel_von`), die von
`probe`, `call` und dem Lowering gemeinsam benutzt wird; sie können nicht
auseinanderlaufen.

---

## 4. Namensauflösung: es gibt nichts zu verdecken

Methoden und freie Funktionen liegen in **einem** Namensraum, aber unter
**verschiedenen Namen**: die Methode `m` von `T` heißt `T__m`.

* `m(x)` findet nie eine Methode.
* `x.m()` findet nie eine freie Funktion — auch dann nicht, wenn deren erster
  Parameter genau der passende Zeiger ist
  (`tests/neg/impl_free_func_is_no_method.fi`).
* Beide dürfen deshalb gleich heißen. `tests/810` und `tests/modules/geo.fi`
  führen das vor: `summe(a, b)` und `Punkt.summe()`, `einheit()` und
  `Rechteck.einheit()` stehen nebeneinander. In `lib/str/std_impl.fi`
  passiert genau das im Ernstfall: die freie Funktion `laenge(s)` und die
  Methode `Spanne.laenge()` sind beide erreichbar und zeigen auf denselben
  Code.

**Module.** Im Modul `geo` wird aus `Rechteck` beim Zusammenführen
`geo__Rechteck` und aus `Rechteck__flaeche` entsprechend
`geo__Rechteck__flaeche`. Die Auflösung „Strukturname ++ `__` ++ Methode"
stimmt danach weiter, weil sie immer mit dem Namen rechnet, den der Typ zu
diesem Zeitpunkt trägt. Der Aufruf `r.flaeche()` braucht deshalb **kein**
Modulpräfix: die Methode gehört zum Typ, und der Typ trägt sein Modul schon
im Namen.

**Sichtbarkeit.** Eine Methode folgt dem Typ, nicht der `export`-Liste: wer
den Typ sieht, sieht seine Methoden. Das ist bewusst so — eine Methode, die
zwar am Typ hängt, aber nicht gerufen werden darf, wäre eine zweite
Sichtbarkeitsregel neben der bestehenden.

**Zwei `impl`-Blöcke für denselben Typ sind erlaubt** (`tests/modules/geo.fi`
macht es): `impl` legt Funktionen an, keine geschlossene Tafel. Zwei Methoden
gleichen Namens am selben Typ sind derselbe Fehler wie zwei gleichnamige
Funktionen — „funktion 'T__m' ist bereits deklariert".

---

## 5. Was in `firnc0` geändert wurde

| Datei | Zeilen | was |
|---|---|---|
| `compiler/src/impls.rs` | **490 neu** | alles Eigene dieser Runde: Parserhaken, Empfängerformen, Auflösung, Fehlermeldungen, `ziel_von` |
| `compiler/src/parser.rs` | +15/-1 | zwei Haken (Itemebene, Nachausdruck); `cont` wird `pub(crate)` |
| `compiler/src/sema.rs` | +57/-24 | Haken in `call`, Zweig in `probe_d`, `pruefe_argument` herausgezogen |
| `compiler/src/lower.rs` | +32/-2 | Ziel neu ableiten, Empfänger als Adresse |
| `compiler/src/nogc.rs` | +17 | Methodenaufruf im `#[no_gc]`-Baum wird abgelehnt |
| `compiler/src/main.rs` | +1 | `mod impls;` |

`pruefe_argument` ist **wörtlich** der bisherige Schleifenrumpf aus `call` —
die Meldungen gewöhnlicher Aufrufe sind unverändert, nur die Nummer im Text
zählt bei Methoden ohne den Empfänger (`v.push(x)` hat **ein** Argument).

## 6. Was in `firnc1` geändert wurde

| Datei | Zeilen | was |
|---|---|---|
| `lib/firnc1/parser.fi` | +225/-25 | `impl`-Block, Empfänger, `ruf_argumente_in`, `methodenrufname`, Vorabsuche |
| `lib/firnc1/sema.fi` | +159 | `methoden_ruf`, `empf_struct`, `methodenfunktion`, `ist_platz`, Zweig in `probe_t` |
| `lib/firnc1/lower.fi` | +63 | dieselbe Ableitung, Empfänger als Adresse |
| `lib/firnc1/nogc.fi` | +13 | dieselbe Ablehnung wie in `nogc.rs` |
| `lib/rt/intern.fi` | +26/-1 | `intern_praefix`, `intern_ab` |

`intern_ab` (der Name ab Byte *n*, also `"methode m"` → `"m"`) kopiert den
Text **zuerst heraus**: `intern_nummer` kann den Textpuffer umziehen lassen,
und dann zeigte `intern_zeiger` ins Leere. Das steht so schon am Kopf von
`intern.fi` und ist hier zum ersten Mal wirklich relevant.

---

## 7. Zwei Befunde aus dem Bau

### A. Die Klammer in der nächsten Zeile

Der erste Bau von `firnc1` schlug fehl — an dieser Stelle im eigenen
Quelltext:

```firn
let gemerkt: usize = (*p).kein_slit
(*p).kein_slit = 0
```

Der neue Haken sah nach `.kein_slit` eine öffnende Klammer und machte daraus
einen Methodenaufruf `(*p).kein_slit((*p))`. Die Klammer stand aber in der
**nächsten Zeile** — und in Firn beendet der Zeilenumbruch die Anweisung
(SPEC §10, Semikolon optional).

Die Prüfung dafür gibt es längst: `Parser::cont` (in `firnc1`: `weiter`)
beantwortet genau die Frage „darf der Ausdruck mit diesem Token
weitergehen?". Sie stand im Nachausdruck vor der Schleife, nicht vor der
Klammer. Behoben in beiden Compilern mit einer Bedingung.

**Die Lehre:** eine neue Postfix-Form muss dieselbe Zeilenregel beantworten
wie alle anderen. Der Fehler wurde nicht durch einen Test gefunden, sondern
dadurch, dass der Compiler sich selbst übersetzt — der Selbstbau ist an
solchen Stellen der schärfere Test, weil er 18 000 Zeilen echten Quelltext
gegen die neue Regel hält.

### B. Zwei Wege zur Modulumbenennung, die auseinanderliefen

`firnc0` benennt Modulnamen **nach** dem Parsen um (`modules.rs`), `firnc1`
**während** des Parsens (`parser.fi`). Dafür sucht `firnc1` vorab alle selbst
deklarierten Namen — ein Tokenlauf, der auf `fn`/`struct`/`const` + Bezeichner
achtet. Der findet in einem `impl`-Block auch `fn push` und hätte `push` in
die Umbenennungsliste des Moduls aufgenommen; `firnc0` sieht dort nur die
fertige Funktion `Bytes__push` und niemals `push`.

Folge wäre gewesen: ein Modul, das eine Methode `push` **und** irgendwo einen
freien Aufruf `push(..)` hat, hätte in den beiden Compilern verschiedene
Namen aufgelöst — genau die Sorte Abweichung, die erst im Selbstvergleich
auffällt. Die Vorabsuche überspringt `impl`-Blöcke jetzt vollständig
(`ist_impl_bei`/`impl_ende`).

---

## 8. Das Anwendungsbeispiel: `std.str` bekommt eine Hülle

`lib/str/std_impl.fi` (102 Zeilen, nur von `tools/strlib/src/std_str.fi`
eingebunden, erzeugt `lib/std/str.fi`) gibt drei Typen Methoden:

* **`Spanne`** — die lesende Sicht, Empfänger als Wert: `laenge`, `ist_leer`,
  `zeichen`, `teil`, `ab`, `bis`, `gleich`, `vergleiche`, `gleich_ohne_fall`,
  `beginnt_mit`, `endet_mit`, `finde`, `finde_rueck`, `finde_zeichen`,
  `enthaelt`, `zaehle_zeichen`, `zaehle_teil`, `trimme`, `trimme_links`,
  `trimme_rechts`, `ohne_praefix`, `ohne_suffix`, `utf8_zeichen`, `utf8_teil`
* **`Bytes`** — der besitzende Puffer, `*self` bzw. `*mut self`: `laenge`,
  `bei`, `ist_text`, `gleich`, `spanne`, `schreibe`, `dazu`, `leeren`,
  `frei`, `anhaengen`, `setze`, `wiederhole`, `gross_hier`, `klein_hier`
* **`Str16`**: `laenge`, `bei`, `gleich`, `dazu`, `dazu_cp`, `setze_bei`,
  `leeren`, `frei`

**Ausschließlich Hüllen.** Jede Methode ruft genau die freie Funktion auf,
die es schon gibt, und tut sonst nichts. Damit ist die Umstellung
nachweislich verhaltensgleich — es gibt keine zweite Umsetzung, die
auseinanderlaufen könnte. Die freien Funktionen bleiben unverändert
erreichbar und werden von der Bibliothek selbst weiter benutzt.

`tests/812_impl_std_core.fi` rechnet beide Wege **nebeneinander** aus
(`core.finde(welt) != str.finde(kern, welt)` → Rückgabe 12) und gibt am Ende
eine Zeile aus, die `firnc0` und der selbst übersetzte `firnc1` zeichengleich
erzeugen müssen.

---

## 9. Testabdeckung

**`tests/810_impl_core.fi`** — 21 geprüfte Punkte, jeder mit eigenem
Rückgabewert: die drei Empfänger; Adressnahme bei Wert-Empfängern; Empfänger,
der schon ein Zeiger ist; `(*z)` für die ausdrückliche Kopie;
Aggregatrückgabe und Bindung davor; Methode ruft Methode; Aggregat als
Argument; Empfänger als **Feld** (`r.ecke.summe()`), als **Arrayelement**
(`feld[i].setze(..)`) und geschachtelt (`r.eckensumme()` ruft
`(*self).ecke.summe()`); freie Funktion und Methode gleichen Namens.

**`tests/811_impl_module_core.fi`** mit **`tests/modules/geo.fi`** — 12 Punkte
über die Modulgrenze: Methode ohne Modulpräfix, schreibende Methode, die
ihrerseits eine Methode ruft, zweiter `impl`-Block für denselben Typ, freie
Funktion `einheit()` neben `Rechteck.einheit()`, Empfänger über einen Zeiger.

**`tests/812_impl_std_core.fi`** — 24 Punkte auf `std.str`, siehe oben.

**`tests/neg/impl_*.fi`** — acht Negativtests, jeder mit Position, Meldung
und Fehlerzahl:

| Datei | Meldung |
|---|---|
| `impl_no_method.fi` | `typ 'Punkt' hat keine methode 'differenz'` (Hinweis nennt die vorhandenen) |
| `impl_free_func_is_no_method.fi` | dieselbe Meldung für `p.punkt_summe()` |
| `impl_receiver_without_address.fi` | `der empfaenger von 'Punkt.summe' braucht eine adresse, dieser ausdruck hat keine` |
| `impl_receiver_is_ptr.fi` | `'Punkt.erstes' erwartet den empfaenger als wert, gefunden *mut Punkt` |
| `impl_no_struct.fi` | `methode 'summe' auf einem wert vom typ i32 — methoden gibt es nur fuer struct-typen` |
| `impl_argument_ty.fi` | `argument 1 von 'Punkt.setze_x' hat typ bool, erwartet i32` |
| `impl_argumentzahl.fi` | `methode 'Punkt.setze_x' erwartet 1 argument(e), gefunden 2` |
| `impl_without_receiver.fi` | `der erste parameter einer methode ist der empfaenger: 'self', '*self' oder '*mut self'` |

Alle acht werden auch von `firnc1` abgelehnt (Exit 1). Jeder von ihnen meldet
**genau einen** Fehler: eine kaputte Methode bricht den ganzen `impl`-Block
ab, damit die erste — die einzige, die etwas erklärt — nicht in einer Kaskade
untergeht.

---

## 10. Bewusst weggelassen

* **Zugeordnete Funktionen ohne `self` (`Typ::neu(..)`).** Teil 4 der
  Aufgabe, „nur wenn Zeit bleibt". Sie sind nicht gebaut, und zwar sichtbar:
  eine Methode ohne Empfänger ist ein Fehler mit klarer Ansage
  (`impl_without_receiver.fi`) statt eines ratlosen Syntaxfehlers. Der Aufruf
  bräuchte zusätzlich `Typ::name` als Ausdrucksform; `::` ist im Tokenisierer
  heute kein eigenes Zeichen (`Enum::Variante` wird als zwei `:` gelesen).
* **Generische Typen.** `impl Vec[T]` gibt es nicht. Eine Ausprägung heißt
  nach der Monomorphisierung anders als die Vorlage; die Methoden müssten
  mitmonomorphisiert werden. Das ist eine eigene Runde wert und der Grund,
  warum das Anwendungsbeispiel `std.str` ist und nicht `std.vec`.
* **Attribute an Methoden.** `#[no_gc]` oder `#[must_consume]` vor einer
  Methode ist ein Syntaxfehler; ein Attribut vor `impl` wird ausdrücklich
  abgelehnt. `#[must_consume]` am **Typ** wirkt weiter, auch wenn eine
  Methode ihn liefert — diese Prüfung geht über den Typ, nicht über den
  Namen.
* **`#[no_gc]` und Methoden.** Ein Methodenaufruf in einer `#[no_gc]`-Funktion
  wird **abgelehnt**. Die Prüfung läuft ohne die Typtabelle der Empfänger und
  kann deshalb nicht sagen, welche Funktion dahintersteht. Ein Loch in einer
  Zusage wäre schlimmer als eine fehlende Bequemlichkeit; wer beides braucht,
  ruft `Typ__methode(..)` direkt auf.
* **Dynamischer Versand, Interfaces, Operatorüberladung.** Nicht Teil dieser
  Runde (Aufgabenstellung).
* **Methodenketten über Zeiger-Empfänger.** Siehe §2: bewusst kein stiller
  Zwischenwert.

---

## 11. Abnahme

Gemessen auf `r45-impl` mit selbst gebautem `firnc0` und frisch gebauten
Hilfsbinärdateien (`.firnc1`, `.astdump`, …).

| Prüfung | Basis `f48e51c` | jetzt |
|---|---|---|
| `bash ./test.sh` | 673/673 | **690/690** |
| `tools/self_compare.sh` | 196 gleich / 0 abweichend / 0 fehlerhaft | **199 / 0 / 0** |
| `tools/fixpunkt.sh` | Stufe 2 == Stufe 3, 309 468 Zeilen | **Stufe 2 == Stufe 3, 315 088 Zeilen** |
| Parser (`parser_compare.sh`) | 236 gleich, 1 bekannt ungleich | 248 gleich, 1 bekannt ungleich |
| Layout/ABI (`types_compare.sh`) | 186 gleich, 0 ungleich | 198 gleich, 0 ungleich |
| Typprüfer (`sema_compare.sh`) | 145 gleich, 1 bekannt | 146 gleich, 1 bekannt |
| Lowering (`fir_compare.sh`) | 144 gleich, 1 bekannt | 145 gleich, 1 bekannt |

Die eine bekannte Abweichung ist unverändert `tests/590_f64.fi` (Literal
`1e308`, Rundungsfall aus Runde 20 — kein Parserfehler).

## 12. Zeilen

| | Zeilen |
|---|---|
| `firnc0` (Rust) | +591 / −21 |
| `firnc1` (Firn, mit `lib/rt/intern.fi`) | +481 / −5 |
| Bibliothek (`lib/str/std_impl.fi`, erzeugte `lib/std/str.fi`) | +205 |
| Tests (3 Programme, 1 Modul, 8 Negativtests) | +543 |
