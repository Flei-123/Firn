# Bericht Modul `rclib` — `Rc[T]` / `Weak[T]` (SPEC §3.4, Anforderung S7)

Runde 4 (Haertetest 2), Auftrag nach `PLAN.md` §5. Reines Firn, **keine
Compileraenderung**, keine Datei ausserhalb des eigenen Bestands angefasst.

## 1. Was gebaut wurde

| Datei | Inhalt |
|---|---|
| `tests/modules/rc.fi` | **die eine Implementierung** (Modul `rc`): Halde ueber `mmap`, Groessenklassen mit Freilisten, `Zaehlverweis[T]` (= `Rc[T]`), `Schwachverweis[T]` (= `Weak[T]`), fehlbare Allokation `AllocError!bool` |
| `lib/rc/rc.fi` | Symlink auf `tests/modules/rc.fi` (Bibliothekspfad ohne Codedopplung) |
| `lib/rc/parts/*.fi` | Rumpfe der Testprogramme |
| `lib/rc/gen_tests.sh` | erzeugt die Testprogramme aus Rumpf + Implementierung |
| `tests/550_rc_basic.fi` | anlegen/lesen/klonen/freigeben, Zaehlerstaende, Blockwiederverwendung, zweite Auspraegung der Vorlage |
| `tests/551_rc_weak.fi` | `weak_von`/`aufwerten`/`weak_freigeben`, Aufwertung nach dem Tod liefert **sichtbar leer**, Block wird erst mit dem letzten schwachen Verweis frei |
| `tests/552_rc_cycle_leak.fi` | **Pflichtnachweis: der Rc-Zyklus LECKT** (Ausgabe `1 1 2 128 200 12800 198 1 0`) |
| `tests/553_rc_fallible.fi` | fehlbare Allokation: `OutOfMemory` bei voller Halde, bei zu grosser Nutzlast, `try`-Kette, Wiederaufnahme nach Freigabe |
| `tests/554_rc_dauerlauf.fi` | 20.000 Runden ohne Zyklus: `20000 0 0 192 1` — kein Wachstum, alles frei |
| `tests/neg/rc_discarded.fi` | verworfenes `AllocError!bool` = Compilerfehler mit Zeile/Spalte |
| `tests/neg/rc_unveraenderlich.fi` | Schreibversuch durch `Rc` = Compilerfehler mit Zeile/Spalte |
| `docs/RC.md` | Handbuch, Beispiel, Leck-Erklaerung, vollstaendige Abweichungsliste |

## 2. Selbst gemessen

Alle Programme in **drei** Baustufen (`opt`, `--no-opt`, `--opt-level=dev-fast`),
jeweils dasselbe Ergebnis:

```
tests/550_rc_basic.fi     [opt|noopt|dev-fast] exit=0
tests/551_rc_weak.fi      [opt|noopt|dev-fast] exit=0
tests/552_rc_cycle_leak.fi [opt|noopt|dev-fast] exit=0  out=1 1 2 128 200 12800 198 1 0
tests/553_rc_fallible.fi   [opt|noopt|dev-fast] exit=0
tests/554_rc_dauerlauf.fi [opt|noopt|dev-fast] exit=0  out=20000 0 0 192 1
```

Negativtests (echte Compilerausgabe):

```
tests/neg/rc_discarded.fi:393:5
error: das ergebnis darf nicht verworfen werden: der typ 'AllocError!bool'
       ist mit #[must_consume] gekennzeichnet

tests/neg/rc_unveraenderlich.fi:396:5
error: linke seite ist kein zuweisbarer ausdruck (variable, feld, index oder '*zeiger')
```

### Stand der Gesamtsuite zum Zeitpunkt dieses Berichts

`bash test.sh` liess sich **nicht** zu Ende fahren: der Compiler im Baum
uebersetzte waehrend meiner Arbeit nicht, weil ein anderes Modul dieser Runde
mitten in seiner Aenderung steckt —

```
error[E0425]: cannot find function `emit_gc_addr` in module `crate::codegen_x86`
    --> src/regalloc.rs:1110:33
error[E0425]: cannot find function `emit_gc_addr` in this scope
    --> src/codegen_x86.rs:361:13
```

Das betrifft keine Datei von `rclib` (ich habe keine Compilerquelle angefasst).
Um trotzdem belastbar zu pruefen, habe ich den Compiler aus dem
**unveraenderten Ausgangsstand** gebaut
(`git archive HEAD compiler | tar -x -C .rcwork/rein`, `cargo build --release`
— null Warnungen) und damit alle Positivprogramme des Baumes in drei
Baustufen gefahren:

```
tests/*.fi + tests/opt/*.fi + examples/*.fi (Ausgangsstand + meine 5 neuen)
  148 Programme x 3 Baustufen -> PASS=444  FAIL=0
tests/neg/*.fi  55/58 wie erwartet
  (die drei Ausnahmen sind tests/neg/nogc_*.fi aus dem parallel laufenden
   Modul 'nogc'; sie brauchen dessen Compilerstand, nicht meinen)
```

Sobald der Baum wieder uebersetzt, ist fuer `rclib` nichts nachzuziehen:
meine Dateien sind reines Firn und haengen an keiner der offenen Baustellen.

Der Leck-Nachweis in Zahlen (`tests/552`): nach dem Verwerfen beider aeusseren
Griffe stehen beide starken Zaehler weiter auf **1**, es bleiben **2** lebende
Bloecke / **128** Byte gehalten; nach 100 angelegten und verworfenen Zyklen
sind es **200** Bloecke / **12.800** Byte. Mit `Weak` statt des zweiten starken
Verweises: Aufwertung sichtbar leer, am Ende **0** belegte Bloecke.

## 3. Abweichungen (gehoeren nach SPEC §14.1 — Modul `mess` traegt sie ein)

1. **Typnamen `Zaehlverweis[T]` / `Schwachverweis[T]` statt `Rc[T]` / `Weak[T]`.**
   `Rc`, `Arc` und `Weak` sind im Parser als noch nicht umgesetzte
   Typkonstruktoren reserviert (`compiler/src/parser.rs::nicht_umgesetzter_typ`,
   Fehler „'Rc[T]' ist in Stufe 0 nicht umgesetzt"); ein reines Firn-Modul kann
   die Namen nicht belegen, und die Compilerquellen gehoeren in dieser Runde
   anderen Modulen. Die **Funktionsnamen** aus dem Vertrag sind unveraendert:
   `rc_neu`, `rc_lesen`, `rc_klonen`, `rc_freigeben`, `rc_stark_zahl`,
   `weak_von`, `aufwerten`, `weak_freigeben`. Sobald jemand die Reservierung
   aufhebt, ist es eine reine Umbenennung in einer Datei.
2. **`rc_neu(…)` statt `Rc[T].neu(…)`** — Stufe 0 kennt keine Methoden.
3. **`h: *mut RcHeap` statt `inout alloc`** — Stufe 0 kennt kein `inout`.
4. **`AllocError!bool` + Ausgabezeiger statt `AllocError!Rc[T]`.** Die
   Monomorphisierung setzt Typargumente in der Nutzlast einer Fehlerunion nicht
   ein: `fn f[T: Any](..) -> AllocError!Zaehlverweis[T]` meldet „unbekannter typ
   'Zaehlverweis__T'", ebenso `AllocError!T` („unbekannter typ 'T'"). Die
   Allokation bleibt vollstaendig fehlbar und `#[must_consume]`.
   *Meldung an `gckern`: falls die Monomorphisierung in dieser Runde ohnehin
   angefasst wird, waere das eine kleine, lohnende Ergaenzung.*
5. **`Arc[T]` ist nicht gebaut** — offen. Stufe 0 hat keine Faeden und keine
   atomaren Befehle; ein „Arc", der nicht atomar zaehlt, waere eine Luege.
6. **Keine Destruktoren** (`drop`, SPEC §3.3, ist nicht gebaut): Verweise, die
   *in* einem Wert liegen, muessen von Hand geloest werden. Das betrifft nur
   Werte, die selbst Verweise enthalten, und ist in `tests/552` sichtbar.
7. **Testprogramme enthalten die Implementierung woertlich.** Stufe 0 loest
   generische Vorlagen nicht ueber Modulgrenzen auf — weder
   `rc.Zaehlverweis[T]` in Typstellung noch `rc.rc_neu[T](..)` im Aufruf
   uebersetzen (`hook_generic_call` verlangt einen `Ident`, das Modulsystem
   liefert dort einen `Field`-Ausdruck). Deshalb dasselbe Verfahren wie bei
   `lib/str` (`tools/strlib/expand.py`): eine Quelle im Baum,
   `bash lib/rc/gen_tests.sh` setzt die Programme zusammen. `import
   modules.rc` funktioniert damit **nicht** — das ist die ehrliche Lage.

## 4. Fuer Modul `mess`: die Doppelung der Fehlermenge

`error AllocError { OutOfMemory }` steht **in `tests/modules/rc.fi`** (also
auch in den erzeugten `tests/55*_rc_*.fi` und `tests/neg/rc_*.fi`).
Fehlermengennamen sind programmweit. Sobald die GC-Laufzeit (`gckern`) dieselbe
Menge programmweit bereitstellt, gibt es zwei Moeglichkeiten:

* Die Rc-Programme benutzen **keinen** `gc class`-Wert und ziehen die
  GC-Laufzeit deshalb gar nicht ein — dann kollidiert nichts und beide
  Fassungen koennen stehen bleiben.
* Sollte die Laufzeit doch in jedes Programm eingezogen werden, genuegt es,
  die eine `error`-Zeile in `tests/modules/rc.fi` zu loeschen und
  `bash lib/rc/gen_tests.sh` erneut laufen zu lassen. Kein weiterer
  Eingriff noetig.

Geprueft wurde gegen den Stand des Compilers zu Beginn der Runde; ein `Gc`-
oder `gc class`-Bezug kommt in keiner Rc-Datei vor.

## 5. Was NICHT gebaut wurde

* `Arc[T]` (atomarer Zaehler) — offen, siehe Abweichung 5.
* Destruktoren / `drop` — nicht Teil dieser Runde.
* Kein Nachwachsen der Halde; `rc_heap_init` legt einmal fest, wie viel es gibt.
