# Bericht Modul `nogc` — `#[no_gc]` scharf gemacht (SPEC §3.5.4)

Stand: Runde „Haertetest 2". Alle Ausgaben unten sind **echte** Ausgaben des
gebauten Compilers (`compiler/target/release/firnc`), nicht nacherzaehlt.

## 1. Was jetzt gilt

`#[no_gc]` ist im Attributregister (`compiler/src/attrs.rs`) auf
`umgesetzt: true` gesetzt und wird von `compiler/src/nogc.rs` geprueft
(angebunden ueber `// HOOK nogc` in `sema::Checker::run`, `sema.rs` wurde
nicht angefasst). Geprueft wird nach der Typpruefung, weil Regel (iii) die
Typtabelle braucht.

```
$ ./compiler/target/release/firnc --list-attrs | head -5
Attribute

NAME            ZIEL         ARGS  STUFE 0     ZWECK
must_consume    fn, struct   0     umgesetzt   Ergebnis darf nicht verworfen werden (SPEC 3.3, 5.1)
no_gc           fn           0     umgesetzt   kein Sammellauf in diesem Aufrufbaum (SPEC 3.5.4)
```

Die uebrigen Attribute bleiben unveraendert abgelehnt — `constant_time`,
`unwinds`, `packed`, `align`, `layout`, `no_move`, `abi_stable`, `frozen`,
`hot` melden weiter „attribut '…' ist in Stufe 0 nicht umgesetzt"
(`tests/neg/attr_nicht_umgesetzt.fi` unveraendert gruen, zusaetzlich der
Modultest `attrs::tests::nicht_umgesetzte_attribute_melden_weiter_einen_fehler`).
Der Test `nur_must_consume_ist_umgesetzt` wurde mitgezogen und verlangt jetzt
genau `["must_consume", "no_gc"]` — die Klammer, die verhindert, dass ein
Attribut still „umgesetzt" wird.

### Die drei Regeln

In einer `#[no_gc]`-Funktion sind verboten:

| Regel | Was | Woher die Auskunft kommt |
|---|---|---|
| (i) | GC-Allokation / Aufruf, der einen Sammellauf ausloesen kann | `crate::gc::ist_gc_alloc_aufruf(name)` |
| (ii) | Aufruf einer Funktion **ohne** `#[no_gc]` | Attributtabelle des Gesamtprogramms |
| (iii) | Schreiben eines `Gc[T]`/`GcWeak[T]`-Zeigers in ein Feld | `crate::gc::ist_gc_zeiger(typ)` |

Die beiden GC-Abfragen sind der **Vertrag von Modul `gckern`** und wurden
nicht veraendert; `nogc.rs` fragt ausschliesslich diese zwei Funktionen und
kennt den GC sonst nicht.

Die Pruefung ist transitiv: weil jede gerufene Funktion selbst `#[no_gc]`
tragen muss, gilt die Zusage fuer den ganzen Aufrufbaum, ueber beliebig viele
Ebenen und ueber Modulgrenzen hinweg.

### Gehaertet gegenueber der Ausgangsfassung

* **`match`-Faelle werden mitgeprueft.** Die Rumpfbloecke der Faelle liegen
  nicht im AST, sondern in der Registrierung von `sema_match.rs`
  (`__match#N`). Ohne den Abstieg dorthin waere jede Zustandsmaschine ein
  blinder Fleck — also genau der Code, fuer den `#[no_gc]` gedacht ist.
  Nachweis: `tests/neg/nogc_match_fall.fi`.
* **Modulqualifizierte Aufrufe.** `modul.funktion` heisst nach der Umschrift
  durch `modules.rs` intern `modul__funktion`; die Meldung zeigt wieder die
  Schreibweise aus dem Quelltext (`nogc_kalt.aufwaendig`).
  Nachweis: `tests/neg/nogc_modulgrenze.fi`.
* **Keine Fehlalarme.** Compilerintern erzeugte Aufrufnamen (`__match#N`,
  `__try#`, `__catch#`, `Enum::Variante`) sind keine Funktionsaufrufe und
  loesen nichts aus; ihre Argumente werden trotzdem durchsucht. Aufrufe eines
  Namens, den es gar nicht gibt, meldet die Typpruefung selbst — hier gibt es
  keinen zweiten, verwirrenden Fehler. Nachweis:
  `nogc::tests::interne_namen_loesen_keinen_fehler_aus`.
* **Regel (iii) genauer.** Gemeldet wird das Schreiben in ein Feld, in ein
  Element hinter einem Feld und hinter einem Zeiger. Die Zuweisung an eine
  oertliche Veraenderliche auf dem Stapel braucht keine Einfuegebarriere und
  bleibt erlaubt.
* Meldungen mit Zeile **und** Spalte, mit Quelltextzeile, `^^^`-Markierung und
  Hinweis, was zu tun ist; Befunde deterministisch nach Datei/Zeile/Spalte
  sortiert; doppelte Meldungen unterdrueckt.
* Rekursionsschranke `MAX_TIEFE = 256` fuer geschachtelte `match`-Rumpfbloecke
  (zweite Sicherung neben der Parser-Grenze von 200).

## 2. Negativtests — echte Compilerausgaben

```
$ ./compiler/target/release/firnc -o /dev/null tests/neg/nogc_aufruf_ohne_attribut.fi
error: 'heiss' ist #[no_gc], ruft aber 'langsam' ohne #[no_gc]
   --> tests/neg/nogc_aufruf_ohne_attribut.fi:11:12
    |
 11 |     return langsam(a)
    |            ^^^^^^^ hier
    = hinweis: SPEC 3.5.4: die Zusage gilt transitiv fuer den ganzen Aufrufbaum — schreibe #[no_gc] vor 'langsam' oder rufe es hier nicht auf

$ ./compiler/target/release/firnc -o /dev/null tests/neg/nogc_transitiv.fi
error: 'mitte' ist #[no_gc], ruft aber 'unten' ohne #[no_gc]
   --> tests/neg/nogc_transitiv.fi:21:17
    |
 21 |         s = s + unten(a)
    |                 ^^^^^ hier
    = hinweis: SPEC 3.5.4: die Zusage gilt transitiv fuer den ganzen Aufrufbaum — schreibe #[no_gc] vor 'unten' oder rufe es hier nicht auf

$ ./compiler/target/release/firnc -o /dev/null tests/neg/nogc_match_fall.fi
error: 'schritt' ist #[no_gc], ruft aber 'protokoll' ohne #[no_gc]
   --> tests/neg/nogc_match_fall.fi:17:35
    |
 17 |         Zustand::Ende => { return protokoll(c) }
    |                                   ^^^^^^^^^ hier
    = hinweis: SPEC 3.5.4: die Zusage gilt transitiv fuer den ganzen Aufrufbaum — schreibe #[no_gc] vor 'protokoll' oder rufe es hier nicht auf

$ ./compiler/target/release/firnc -o /dev/null tests/neg/nogc_modulgrenze.fi
error: 'heiss' ist #[no_gc], ruft aber 'nogc_kalt.aufwaendig' ohne #[no_gc]
   --> tests/neg/nogc_modulgrenze.fi:10:12
    |
 10 |     return nogc_kalt.aufwaendig(a)
    |            ^^^^^^^^^^^^^^^^^^^^ hier
    = hinweis: SPEC 3.5.4: die Zusage gilt transitiv fuer den ganzen Aufrufbaum — schreibe #[no_gc] vor 'nogc_kalt.aufwaendig' oder rufe es hier nicht auf
```

In `tests/neg/nogc_transitiv.fi` steht der Bruch bewusst **eine Ebene tiefer**
als der markierte Einstieg (`oben` → `mitte` → `unten`): gemeldet wird die
Stelle, an der die Kette reisst.

## 3. Positivtests

| Datei | was sie zeigt | Ergebnis |
|---|---|---|
| `tests/540_no_gc_aufrufbaum.fi` | markierter Aufrufbaum ueber vier Ebenen, Schleifen, Verzweigungen; eine unmarkierte Funktion darf eine markierte rufen | `expect_exit: 42` |
| `tests/541_no_gc_zustandsmaschine.fi` | `#[no_gc]` + `match` mit vier Faellen, Aufrufe aus den Fallrumpfen heraus | `expect_exit: 99` |
| `tests/542_no_gc_modul.fi` (+ `tests/modules/nogc_heiss.fi`) | `#[no_gc]` ueber die Modulgrenze | `expect_exit: 100` |

Alle drei laufen in `test.sh` in **drei** Baustufen (`opt`, `--no-opt`,
`--opt-level=dev-fast`) mit demselben Ergebnis.

## 4. Der HTML5-Tokenizer ist jetzt `#[no_gc]`

Das ist die Zusage aus SPEC §3.5.4 an Tokenizer, Rasterizer und Krypto —
und hier ist sie an echtem Code eingeloest. **Jede** Funktion in `lib/html/`
traegt `#[no_gc]`, einschliesslich `main` des Treibers:

| Datei | markierte Funktionen |
|---|---|
| `lib/html/mem.fi` | 31 |
| `lib/html/tokens.fi` | 58 |
| `lib/html/tokenizer.fi` | 17 |
| `lib/html/entities.fi` | 19 |
| `lib/html/entities_data.fi` | 14 |
| `lib/html/fehler_codes.fi` | 1 |
| `lib/html/tokenize_main.fi` | 3 (mit `main`) |
| `lib/html/entities_probe.fi` | 4 |
| `lib/html/entities_ausfall.fi` | 12 |
| **Summe** | **159** |

Damit steht statisch fest: im ganzen Tokenizer-Programm kann keine Sammlung
stattfinden, es gibt keine Barriere und keine Pause.

Die Quote ist **nicht** schlechter geworden (`bash tools/tokenizer/run.sh`):

```
== 3. Gleiche Bilanz in allen drei Baustufen ==
   noopt: 6810 ohne / 6809 mit Fehlercodes — gleich
   devfast: 6810 ohne / 6809 mit Fehlercodes — gleich

== 5. Regressionsschranke ==
   ohne Fehlercodes: 6810 / 6810   (Schranke: 6810)
   mit  Fehlercodes: 6809 / 6810   (Schranke: 6809)
OK: 6810 / 6810 ohne, 6809 / 6810 mit Fehlercodes bestanden
```

### Gegenprobe: greift die Markierung wirklich?

Ein Attribut, das nichts tut, waere wertlos. Probe: `#[no_gc]` vor
`mem.buf_at` **entfernt**, danach uebersetzen — der Compiler bricht sofort ab
(danach wieder hergestellt):

```
$ ./compiler/target/release/firnc -o .test-work/tk lib/html/tokenize_main.fi
error: 'lies_u32' ist #[no_gc], ruft aber 'mem.buf_at' ohne #[no_gc]
   --> lib/html/tokenize_main.fi:32:19
    |
 32 |         v = v | ((mem.buf_at(b, off + i) as u32) << (8 * i as u32))
    |                   ^^^^^^^^^^ hier
    = hinweis: SPEC 3.5.4: die Zusage gilt transitiv fuer den ganzen Aufrufbaum — schreibe #[no_gc] vor 'mem.buf_at' oder rufe es hier nicht auf
error: 'dekodiere' ist #[no_gc], ruft aber 'mem.buf_at' ohne #[no_gc]
   --> lib/html/tokenize_main.fi:45:23
    |
 45 |         let c0: u32 = mem.buf_at(b, off + i) as u32
    |                       ^^^^^^^^^^ hier
    = hinweis: SPEC 3.5.4: die Zusage gilt transitiv fuer den ganzen Aufrufbaum — schreibe #[no_gc] vor 'mem.buf_at' oder rufe es hier nicht auf
```

## 5. Modultests des Compilers

`cargo test --release --manifest-path compiler/Cargo.toml` (Abschnitt 2 von
`test.sh`), Ausschnitt:

```
test nogc::tests::echte_regeln_sind_die_aus_gc_rs ... ok
test nogc::tests::hat_no_gc_erkennt_das_attribut ... ok
test nogc::tests::interne_namen_loesen_keinen_fehler_aus ... ok
test nogc::tests::modulname_wird_lesbar_gemeldet ... ok
test nogc::tests::regel1_gc_allokation_ist_verboten ... ok
test nogc::tests::regel2_aufruf_ohne_no_gc_ist_verboten ... ok
test nogc::tests::regel2_markierter_aufruf_ist_erlaubt ... ok
test nogc::tests::regel3_schreiben_in_gc_feld_ist_verboten ... ok
test nogc::tests::regel3_zuweisung_an_oertliche_veraenderliche_ist_erlaubt ... ok
test nogc::tests::tiefe_verschachtelung_wird_erreicht ... ok
test nogc::tests::unmarkierte_funktion_wird_nicht_geprueft ... ok
test attrs::tests::nicht_umgesetzte_attribute_melden_weiter_einen_fehler ... ok
test attrs::tests::nur_must_consume_ist_umgesetzt ... ok
test result: ok. 134 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## 6. Ehrlich offen — was NICHT geht

* **Regel (i) und (iii) koennen heute in keinem Firn-Programm ausgeloest
  werden**, weil `compiler/src/gc.rs` (Modul `gckern`) in der Skelettfassung
  fuer beide Abfragen `false` liefert und es damit weder `gc class` noch
  `Gc[T]` gibt. Die Pruefung ist verdrahtet und **im Compiler nachgewiesen**
  (die drei Modultests oben setzen fuer die beiden Abfragen Vorhersagen ein
  und pruefen Meldung, Zeile und Spalte); der Compiler selbst benutzt immer
  `Regeln::echt()`, also `gc.rs` (Test `echte_regeln_sind_die_aus_gc_rs`).
  Sobald `gc.rs` antwortet, greifen (i) und (iii) ohne weitere Aenderung.
  Die beiden fertigen Negativprogramme dafuer liegen in
  `tests/nogc_wartet_auf_gc/` samt `LIESMICH.md`; sie gehoeren dann
  unveraendert nach `tests/neg/`. Sie stehen bewusst **nicht** schon dort:
  `test.sh` wuerde sie sonst gegen eine Meldung des Parsers pruefen und damit
  etwas anderes belegen, als draufsteht.
* Der Pruefer sieht das **ganze, flache Programm** nach Modulzusammenfuehrung
  und Monomorphisierung. Getrennte Uebersetzungseinheiten mit
  Schnittstellendateien gibt es nicht (Grenze des Modulsystems, `modules.rs`),
  also auch keine `#[no_gc]`-Pruefung ueber Bibliotheksgrenzen ohne Quelltext.
* Aufrufe ueber Funktionszeiger gibt es in Stufe 0 nicht (`ExprKind::Call`
  traegt immer einen Namen). Sobald es sie gibt, braucht Regel (ii) eine
  Erweiterung — heute ist dort kein Schlupfloch, aber auch keine Vorsorge.
* `#[no_gc]` erzeugt **keinen** Code und aendert nichts am Lowering; es ist
  reine statische Zusage. Der Sammler selbst, `gc.stats()`, die
  Einfuegebarriere und inkrementelles Sammeln gehoeren zu `gckern`.

## 7. Selbst gefahren

```
cargo build --release --manifest-path compiler/Cargo.toml   # 0 Warnungen
cargo test  --release --manifest-path compiler/Cargo.toml   # 134 passed
bash tools/tokenizer/run.sh                                 # 6810/6810, 6809/6810
bash test.sh                                                # Abschnitte 1-9
```

Ergebnis des vollstaendigen Laufs zum Zeitpunkt dieser Fertigmeldung:

```
== 9. HTML5-Tokenizer gegen html5lib (tools/tokenizer/run.sh) ==
   GESAMT                      6810 /  6810 100.00 %    6809 /  6810  99.99 %

PASS 510/510
```

(Die Gesamtzahl waechst mit den Tests der anderen Module dieser Runde; von
`nogc` kommen 3 Positivprogramme x 3 Baustufen und 4 Negativtests dazu.)

Nebenbefund, damit er nicht untergeht: in `tests/neg/` lag eine leere Datei mit
dem woertlichen Namen `*.fi` (0 Byte, offensichtlich aus einer verunglueckten
Umleitung). Sie liess `test.sh` fehlschlagen und wurde geloescht.

Kein `#[allow(...)]`, keine Sammelunterdrueckung, kein `todo!()`, keine
externen Crates, keine feste Adresse.
