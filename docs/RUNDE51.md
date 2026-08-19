# Runde 51 — realweb unter 1,3× (in Instruktionen gemessen)

Basis: `cc1710f` (Merge der Runden 46/47/48). Branch `r51-tempo`.
Revier: Optimierer, Registerzuteilung, Codeerzeugung, Tokenizer-Messlauf.

**Auftrag.** Den Tokenizer-Vergleich gegen `html5ever` auf dem Korpus
`realweb` sicher unter 2× halten und weiter druecken, Zielmarke **≤ 1,3×**,
gemessen in **Instruktionen mit callgrind** — nicht mit der Wanduhr.

**Ergebnis.** Sieben Aenderungen, jede einzeln gemessen, zusammen
**−26,99 %** auf `realweb` und **−23,43 %** auf `html5lib`.

| Korpus   | Instruktionen vorher | nachher       | Aenderung |
|----------|---------------------:|--------------:|----------:|
| realweb  |        957.989.680   |  699.459.494  | **−26,99 %** |
| html5lib |      2.149.257.366   | 1.645.729.694 | **−23,43 %** |

| Korpus   | Faktor vorher | Faktor nachher | Ziel |
|----------|--------------:|---------------:|-----:|
| realweb  |     **1,772×** |     **1,294×** | ≤ 1,30× ✅ |
| html5lib |     **1,034×** |     **0,791×** | ≤ 1,30× ✅ |

Alle sieben Aenderungen sitzen im **Compiler**, keine im Tokenizer. Der
Gewinn gilt also fuer jedes Firn-Programm, nicht nur fuer den Messlauf.

---

## 0. Was hier „Faktor" heisst — und warum das ein anderer Wert ist als in Runde 43

Runde 43 hat den Faktor mit `tools/tokenizer/durchsatz.sh` bestimmt, also mit
der **Wanduhr** (1,54× realweb). Diese Runde rechnet ihn aus
**Instruktionszahlen**:

```
valgrind --tool=callgrind --cache-sim=no --branch-sim=no  <binary>
```

* Firn:      `.tokenizer-work/tokenize_bench < korpus.<k>.auftrag`
* html5ever: `bench/tokenizer/target/release/html5ever_bench korpus.<k>.html`

Messwerte der Gegenseite (unveraendert, dieselbe Maschine):

| Korpus   | html5ever, Instruktionen |
|----------|-------------------------:|
| realweb  |              540.567.228 |
| html5lib |            2.079.365.558 |

Der Instruktionsfaktor ist **strenger** als der Wanduhrfaktor: 1,772× gegen
1,54× am selben Stand. Firn fuehrt also mehr Instruktionen aus als html5ever
und braucht trotzdem weniger Zeit je Instruktion. Beide Zahlen sind ehrlich,
sie messen nur Verschiedenes. Diese Runde druckt die Instruktionszahl, weil
sie auf die Instruktion genau reproduzierbar ist — die Wanduhr streute an
dieser Maschine zwischen 2,58× und 2,85× **fuer dieselbe Binary**
(docs/RUNDE43.md). Waehrend dieser Runde liefen zwei weitere Runden parallel
auf derselben Maschine; Wanduhrwerte werden deshalb **gar nicht** als Beleg
angefuehrt.

**Neues Werkzeug: `tools/tokenizer/muster.py`.** `profil.py` (Runde 43)
beantwortet „welche FUNKTION kostet?". Die neue Datei beantwortet die Frage
daneben — „welche FORM von Code kostet?": sie verbindet `objdump` mit der
instruktionsgenauen callgrind-Ausgabe (`--dump-instr=yes`) und gewichtet
Instruktions**muster** mit ihren echten Ausfuehrungszahlen. Aufruf:

```sh
objdump -d --no-show-raw-insn .tokenizer-work/tokenize_bench > dis.txt
valgrind --tool=callgrind --dump-instr=yes --cache-sim=no --branch-sim=no \
         --callgrind-out-file=cg.out .tokenizer-work/tokenize_bench < auftrag
python3 tools/tokenizer/muster.py dis.txt cg.out
```

Dazu im Arbeitsverzeichnis (nicht eingecheckt) `.r51/messe.sh`, das callgrind
auf beide Korpora fahrt und die Instruktionszahl ablegt.

**Die Lehre aus Runde 43 hat sich wieder bewaehrt** und diesmal in die andere
Richtung: statische Haeufigkeit sagt nichts, aber ein *dynamisch gewichtetes
Muster* sagt sehr viel. Die drei groessten Posten dieser Runde standen nach
20 Minuten Werkzeugbau als Tabelle da:

```
jmp direkt hinter jcc (Blocklayout)          29.258.200 Ir   3,05%    614 Stellen
setcc-Kette statt direktem Sprung           164.130.198 Ir  17,13%    345 Stellen
Store+Reload derselben Zelle                 98.393.560 Ir  10,27%    294 Stellen
```

---

## 1. Das Profil vorher (realweb, 957.989.680 Ir)

```
          SELBST   ANTEIL         INKLUSIV  FUNKTION
     526.801.688   54,99%      754.292.482  tokenizer__tokenize
     192.243.334   20,07%      192.243.334  dekodiere
     100.031.790   10,44%      100.144.102  tokens__tok_attr_value_push
      47.342.985    4,94%       47.469.496  tokens__sink_fehler_bei
      17.682.777    1,85%       17.683.869  tokens__tok_attr_name_push
      11.448.890    1,20%      957.989.676  main
```

Nach Mnemonics: **51,7 % aller ausgefuehrten Instruktionen waren
Datenbewegungen** (`mov`/`movzx`/`movabs`; das reine `mov` allein 43,2 %),
darunter **129.637.675 Ir Rahmen-Loads (13,53 %)** und **126.722.224 Ir
Rahmen-Stores (13,23 %)**. Der Tokenizer verbrachte also mehr als die Haelfte
seiner Instruktionen damit, Werte hin und her zu schieben, und ueber ein
Viertel allein zwischen Registern und dem Stapelrahmen.

---

## 2. Messtabelle — sieben Aenderungen, sieben Messungen

| # | Aenderung | realweb Ir | Δ | html5lib Ir | Δ |
|---|---|---:|---:|---:|---:|
| — | Basis `cc1710f` | 957.989.680 | — | 2.149.257.366 | — |
| H1 | Sprungfaedelung durch Bool-Zellen (`thread-bool`) | 790.898.007 | **−17,44 %** | 1.824.956.636 | **−15,09 %** |
| H2 | `switch`-Wert aus dem Register, kein `mov eax, eax` | 775.569.867 | −1,94 % | 1.818.386.969 | −0,36 % |
| H3 | Blocklayout entlang von Spuren | 747.247.528 | **−3,65 %** | 1.758.320.039 | −3,30 % |
| H4 | kein `xor eax, eax` bei `void`-Rueckgabe | 743.411.513 | −0,51 % | 1.756.522.359 | −0,10 % |
| H5 | Deskriptor-Nachpass mit Nullerweiterungs-Verfolgung | 733.566.422 | −1,32 % | 1.748.021.433 | −0,48 % |
| H6 | volle x86-Adressierung `[basis+index*faktor+k]` | 712.941.773 | **−2,81 %** | 1.683.365.701 | −3,70 % |
| H7 | Adressfaltung auch mit Basis/Index aus dem Rahmen | 699.459.494 | −1,89 % | 1.645.729.694 | −2,24 % |
| | **gesamt** | | **−26,99 %** | | **−23,43 %** |

Die Ausgabe blieb bei jedem Schritt gleich: realweb 187.473 Token / 1 Auftrag,
html5lib 8.511 / 1.

---

## 3. H1 — Sprungfaedelung durch Bool-Zellen (`compiler/src/faedeln.rs`, neu)

**Beobachtung.** 17,13 % aller Instruktionen steckten in Ketten der Form

```
setb   %al                       ; Bool herstellen
movzbl %al,%r11d
mov    %r11b,-0xae1(%rbp)        ; in eine Zelle schreiben
movzbl -0xae1(%rbp),%r11d        ; sofort wieder herausholen
test   %r11b,%r11b
je     ...
```

**Ursache.** FIR hat **keine Phi-Knoten** — eine ausdrueckliche Invariante.
Die Kurzschlussoperatoren `&&` und `||` muessen ihr Ergebnis deshalb ueber
eine `alloca` zusammenfuehren. Aus `if c0 < 0x80 && c0 != 13` wird:

```text
bbA: %1 = cmp.lt %c, 128 ; store.bool %1, %zelle ; brcond %1, bbB, bbJ
bbB: %2 = cmp.ne %c, 13  ; store.bool %2, %zelle ; br bbJ
bbJ: %3 = load.bool %zelle ; brcond %3, bbT, bbE
```

`mem2reg` kann diese Zelle nicht aufloesen (zwei Schreibzugriffe, kein Phi),
und die bestehende Verschmelzung `cmp`+`jcc` in `regalloc.rs` greift nicht,
weil zwischen Vergleich und Terminator der `store` steht.

**Umsetzung.** Ein neuer Durchgang `thread-bool` faedelt die Kante am
Zusammenfluss vorbei. Ein **Weichenblock** besteht aus genau einer Instruktion
`%v = load.bool %zelle` und endet mit `brcond %v, T, E`. Ein Vorgaenger, der
unmittelbar vor seinem Terminator `store.bool %x, %zelle` ausfuehrt, kennt den
Inhalt der Zelle auf dieser Kante bereits:

* `br J`            → `brcond %x, T, E`
* `brcond %x, A, J` → `brcond %x, A, E`  (auf der J-Kante ist `%x` falsch)
* `brcond %x, J, B` → `brcond %x, T, B`

Danach ist der Weichenblock unerreichbar (`dce`), die Zelle wird nirgends mehr
gelesen (`mem2reg::remove_dead_stores` raeumt `store` und `alloca` weg), und
der Vergleich steht wieder unmittelbar vor dem Terminator — die bestehende
Verschmelzung macht daraus `cmp` + `jcc`. Aus sieben Instruktionen werden zwei.

**Warum das nicht die Fehlerklasse aus Runde 40/41 ist.** Dort wurde eine
Lebensspanne ueber eine `call`-Grenze gedehnt, ohne dass der Registerverteiler
davon wusste. Hier entsteht **keine neue Spanne ueber einen Block hinaus**:
`%x` ist bereits Operand des `store` im selben Block, der Terminator liest es
eine Instruktion spaeter — und `Term::BrCond` gehoert ohnehin zur
Lebensdaueranalyse des Verteilers. Zusaetzlich abgesichert:

* zwischen `store` und Terminator darf keine Instruktion mit Speicherwirkung
  stehen (`store`, `call`, `syscall`, `copymem`, `atomicadd`, `securezero`),
* die Zelle muss eine `alloca` sein, deren Zeiger **nicht entkommt**,
* `secret`-Werte und `#[constant_time]`-Funktionen bleiben unberuehrt
  (SPEC §9.2),
* `store` und `alloca` bleiben stehen; erst der bestehende Durchgang fuer tote
  Speicherungen entfernt sie. Der Durchgang ist damit debugerhaltend.

Acht Modultests in `faedeln.rs` decken das ab, darunter „Aufruf zwischen
`store` und Sprung blockiert", „fremder `store` dazwischen blockiert",
„Zelle, deren Zeiger entkommt", „`constant_time`", „geheimer Wert" und
„zweiter Lauf aendert nichts mehr" (Fixpunkt).

**Wirkung im Binary:** die Muster „setcc-Kette" fielen von 164.130.198 Ir auf
847.126 Ir.

## 4. H2 — der `switch`-Wert kam ueber den Rahmen

**Beobachtung.** Der Zustandsversand des Tokenizers — einmal je Zeichen,
5.109.380 Durchlaeufe — sah so aus:

```
mov %r12d,%r9d          ; Zustand aus der Zelle
mov %r9,%rax
mov %rax,-0x260(%rbp)   ; nur, damit emit_switch ihn findet
mov -0x260(%rbp),%eax   ; und sofort wieder heraus
cmp $0x48,%eax
```

`codegen_switch::emit_switch` konnte den Wert nur aus dem Rahmen lesen; der
Registerpfad musste ihn deshalb erst dorthin schreiben.

**Umsetzung.** `emit_switch` bekommt eine `Wertquelle`: entweder `Rahmen(fr)`
(Grundpfad) oder `Geladen(f)` — dann laedt der Aufrufer den Wert selbst nach
`rax`. Dazu entfaellt das `mov eax, eax` vor der Sprungtabelle: auf x86-64
nullt **jeder** Schreibzugriff auf ein 32-Bit-Register die oberen 32 Bit, und
jeder Zweig von `load_ext` schreibt `eax`. Der Registerpfad prueft zur
Sicherheit, dass der Wert nicht selbst in `rax` liegt (`rax` wird nie
vergeben).

## 5. H3 — Blocklayout entlang von Spuren

**Beobachtung.** 614 Stellen, 28.414.304 Ir (3,66 %), sahen so aus:

```
cmp  -0x18(%rbp),%r8
jae  40dbd4          ; then
jmp  40dbe0          ; else — haette Fallthrough sein koennen
```

Die Bloecke wurden in ihrer FIR-Nummerierung ausgegeben; war weder `then` noch
`else` zufaellig der naechste Block, kostete jeder bedingte Sprung einen
zweiten, unbedingten.

**Umsetzung.** `emissionsreihenfolge()` legt gierige Spuren: ab `bb0` dem
bevorzugten Nachfolger folgen, solange der noch frei ist; reisst die Spur ab,
beim kleinsten noch nicht platzierten Block weitermachen. Bevorzugt wird
`else` — `emit_block` dreht die Bedingung selbst um, wenn stattdessen `then`
folgt.

Das betrifft **ausschliesslich die Ausgabe**. Lebendigkeitsanalyse,
Intervalle und Registerwahl arbeiten weiter auf der FIR-Reihenfolge; jeder
Block hat einen expliziten Terminator, und ein Sprung faellt nur weg, wenn
sein Ziel wirklich unmittelbar folgt. Abschaltbar mit `FIRN_NO_LAYOUT=1`.

**Wirkung:** das Muster fiel von 28.414.304 Ir auf 10.021 Ir; `jmp` insgesamt
von 46.639.178 auf 18.316.839 Ir.

## 6. H4 — `void` braucht kein `xor eax, eax`

Jede Funktion mit Rueckgabetyp `void` setzte vor dem Epilog `rax` auf null.
System V laesst `rax` in diesem Fall undefiniert, und in FIR liest niemand das
Ergebnis eines void-Aufrufs (`Op::Call` ohne `dst`). Bei 4.229.623 Aufrufen im
Messlauf ist das eine Instruktion je Aufruf fuer nichts. In beiden Pfaden
gestrichen (Grundpfad und Registerpfad).

## 7. H5 — der Deskriptor-Nachpass lernt Nullerweiterung

Runde 43 hatte diesen Punkt ausdruecklich **zurueckgestellt** (§6): schmale
Reloads sind nur dann ueberfluessig, wenn das Register „bereits nullerweitert"
ist, und ohne diese Information waere das Streichen genau die Bauform, die in
Runde 40 den Miscompile erzeugt hat.

Die fehlende Information ist eine Eigenschaft von x86-64: **jeder
Schreibzugriff auf ein 32-Bit-Register nullt die oberen 32 Bit.** Der Nachpass
fuehrt jetzt `nullab[r] = k` mit („ab Bit k ist `r` garantiert null"):

* `movzx r32, byte ptr …` → 8, `movzx r32, word ptr …` → 16,
* jeder andere Schreibzugriff mit 32-Bit-Ziel → 32,
* alles uebrige, `call`, `syscall`, `div`, `setcc`, Blockgrenzen → unbekannt.

Dazu merkt sich der Nachpass die **Breite** jeder Speicherung. Gestrichen wird
ein Reload nur, wenn Ziel- und Quellregister gleich sind, die gespeicherte
Breite ausreicht und `nullab` die Erweiterung belegt. `movsx`/`movsxd` bleiben
aussen vor.

Im heissen Pfad von `dekodiere` verschwinden dadurch zwei von 29
Instruktionen je Byte:

```
movzbl (%rcx),%eax
mov    %rax,-0x658(%rbp)
movzbl -0x658(%rbp),%eax   <- entfaellt (rax ist schon <= 0xFF)
mov    %rax,-0x660(%rbp)
mov    -0x660(%rbp),%eax   <- entfaellt (rax ist schon nullerweitert)
```

## 8. H6/H7 — die x86-Adressierung endlich ganz benutzen

Runde 43 hatte nur den **konstanten Versatz** in den Speicherzugriff gefaltet
(`faltbare_versaetze`). Die dynamische Messung zeigte, was daneben liegen
blieb:

| Muster                                   |          Ir | Anteil |
|------------------------------------------|------------:|-------:|
| `shl k` + `lea (b,i,1)` + Zugriff        |  28.840.310 |  3,93 % |
| `lea (b,i,1)` + Zugriff                  |  16.231.553 |  2,21 % |
| `lea off(b)` + Zugriff                   |  14.432.184 |  1,97 % |

Aus `faltbare_versaetze` wurde `faltbare_adressen`, das den vollen
x86-Operanden `[basis + index*faktor + versatz]` erzeugt. Aus

```
mov  r8, qword ptr [rbp-416]
shl  r8, 2
lea  r8, [r9+r8]
mov  r8d, dword ptr [r8]
```

wird

```
mov  r8, qword ptr [rbp-416]
mov  r8d, dword ptr [r9+r8*4]
```

**H7** nimmt zusaetzlich die Faelle dazu, in denen Basis oder Index **im Rahmen**
liegen. Dann bleibt von der Adressrechnung genau das Fuellen ihres ohnehin
vorhandenen Zielregisters uebrig (`vorlader`) — eine Instruktion statt zwei
bis drei.

**Die Bedingungen sind eng gehalten**, weil jede Lockerung die Lebensspanne
der Basis verlaengert:

* adressbildend ist `ptradd` oder ein **64-Bit**-`add` — bei 32 Bit wuerde die
  Adressierung den Ueberlauf nicht abschneiden, den FIR verlangt;
* das Ergebnis wird **genau einmal** gelesen, und dieser Leser ist der
  **unmittelbar folgende** `load`/`store` desselben Blocks;
* Skalierung ist ein 64-Bit-`shl` mit 0..3 bzw. `mul` mit 1/2/4/8, steht
  **unmittelbar davor** und wird ebenfalls genau einmal gelesen;
* Basis, Index und Skalierung sind weder Rahmenadresse noch befoerderte Zelle
  noch Zellen-Alias noch `secret` (SPEC §9.2);
* wird ein Register vorgeladen, darf es nicht zugleich Index oder gespeicherter
  Wert sein — dieser Fall wird ausdruecklich geprueft, weil der Verteiler
  Register von Werten mit beruehrenden Intervallen wiederverwenden darf.

Damit verschiebt sich der Lesezeitpunkt von Basis und Index um genau die ein
bis zwei Instruktionen, die dabei **ganz entfallen**; dazwischen liegt danach
nichts mehr, insbesondere kein `call`. Abschaltbar mit `FIRN_NO_FALTUNG=1`.

**Neuer Test `tests/332_adressierung.fi`** (in drei Baustufen, Rueckgabe 77),
gebaut gegen genau die Warnung aus der Rundenvorgabe:

* jede Skalierung 1/2/4/8 (u8/u16/u32/u64), lesend und schreibend,
* dieselbe Adresse zweimal gelesen (darf nicht gefaltet werden),
* **Aufrufe mitten in der Kette** Basis → Index → Zugriff, mit einer Funktion,
  die alle sechs Argumentregister beschreibt,
* ein Index, der selbst aus einem Aufruf kommt,
* eine Kette, in der das Ergebnis eines Zugriffs sofort wieder Index ist
  (Ziel- und Indexregister kollidieren).

Im Tokenizer entstehen dadurch **419 skalierte Speicheroperanden**; `lea`
faellt von 78.473.907 auf 55.831.640 Ir (−28,9 %) und die Zahl der statischen
`lea`-Stellen von 2.697 auf 1.010.

---

## 9. Verworfen — mit Zahlen

### 9.1 Groessere Inlining-Grenzen: **widerlegt**

`tok_attr_value_push` kostet 37 Instruktionen je Aufruf, davon 13 reine
Rahmenverwaltung, und wird 2.500.787 mal gerufen. Naheliegende Hypothese: die
Grenzen des Inliners (`MAX_CALLEE_INSTS = 40`, `MAX_CALLEE_BLOCKS = 8`) sperren
genau die kleinen heissen Funktionen aus, weil sie durch vorheriges Inlining
selbst gewachsen sind (`cp_push` wandert in `tok_attr_value_push`, das dadurch
ueber die Grenze rutscht).

Gemessen auf dem Stand nach H3:

| Grenzen | realweb Ir | Binary | Uebersetzung |
|---|---:|---:|---:|
| 40 / 8 (Basis) | 747.247.528 | 206.904 B | 1,4 s |
| 80 / 14 | 730.776.445 (−2,20 %) | 397.584 B (**+92 %**) | 6,6 s |
| 120 / 20 | **1.327.230.572 (+77,7 %)** | 531.680 B | 11,9 s |

Bei 120/20 **explodiert** die Instruktionszahl: was in `tokenizer__tokenize`
eingebettet wird, treibt den Registerdruck der ohnehin groessten Funktion so
weit hoch, dass alles in den Rahmen ausgelagert wird. Und 80/14 kauft 2,2 %
mit fast doppelter Binaergroesse und der fuenffachen Uebersetzungszeit.
**Verworfen; die Grenzen bleiben bei 40/8.** Das ist zugleich der Beleg
dafuer, dass mehr Inlining ohne besseren Verteiler nichts bringt.

### 9.2 `leave` statt `mov rsp, rbp` + `pop rbp`: **bewusst nicht gemacht**

`leave` tut genau dasselbe wie die beiden Instruktionen und wuerde die
gemessene Zahl um 4.229.623 Ir (0,6 %) druecken — ohne dass das Programm
weniger arbeitet. Die Instruktionszahl ist hier das Messmittel fuer Arbeit,
nicht das Ziel. Ein solcher Tausch wuerde die Metrik schoener machen und die
Aussage kaputt. Ausdruecklich unterlassen und hier vermerkt, damit es niemand
spaeter „vergisst" zu erwaehnen.

### 9.3 Nicht angefasst: `sink_fehler_bei`

41.444.572 Ir (5,93 %) bei nur 1.503 Aufrufen — die Funktion rechnet je
Parse-Fehler Zeile und Spalte aus, indem sie den Eingabestrom nachscannt.
`html5ever` tut das im Messlauf **nicht**. Das ist eine echte Asymmetrie des
Vergleichs zu Firns Ungunsten, aber sie gehoert dem Tokenizer, nicht dem
Compiler; sie hier wegzuoptimieren hiesse, den Messlauf zu aendern statt den
Uebersetzer. **Benannt, nicht angefasst.**

---

## 10. firnc1

`lib/firnc1` hat **keinen Optimierer und keine Registerzuteilung** — jeder
Wert liegt dort im Rahmen (so steht es im Kopf von
`tools/selbst_vergleich.sh`). Alle sieben Aenderungen dieser Runde liegen
genau in diesen beiden Teilen und haben in firnc1 kein Gegenstueck; es gibt
dort nichts zu spiegeln. Die geforderte Gleichheit wird deshalb dort
nachgewiesen, wo sie in diesem Aufbau nachweisbar ist:

* `tools/selbst_vergleich.sh` — **214 gleiches Verhalten, 0 abweichend,
  0 fehlerhaft**: jedes Testprogramm, von firnc1 uebersetzt, liefert denselben
  Rueckgabewert und dieselbe Ausgabe wie von firnc0 uebersetzt.
* `tools/fixpunkt.sh` — Stufe 2 == Stufe 3, **zeichengleich, 427.401 Zeilen**:
  der von firnc0 uebersetzte Compiler erzeugt denselben Assembler wie der von
  sich selbst uebersetzte.
* `tools/fir_vergleich.sh` — 42.472 FIR-Instruktionen gleich (1 bekannte,
  benannte Abweichung wie vor der Runde).

Neue FIR-Opcodes waren nicht noetig; der Nummernbereich 40–49 bleibt frei.

---

## 11. Profil nachher (realweb, 699.459.494 Ir)

```
          SELBST   ANTEIL         INKLUSIV  FUNKTION
     355.533.636   50,83%      559.683.789  tokenizer__tokenize
     128.322.175   18,35%      128.322.175  dekodiere
      92.529.491   13,23%       92.711.427  tokens__tok_attr_value_push
      41.444.572    5,93%       41.584.990  tokens__sink_fehler_bei
      16.322.580    2,33%       16.323.516  tokens__tok_attr_name_push
      11.448.594    1,64%      699.459.490  main
      10.234.104    1,46%       14.808.079  tokens__tok_attr_finish
```

| Kennzahl | vorher | nachher |
|---|---:|---:|
| `tokenize` je Zeichen | 107 Ir | **72,3 Ir** |
| `dekodiere` je Byte | 39 Ir | **26,0 Ir** |
| `tok_attr_value_push` je Aufruf | 40 Ir | **37,0 Ir** |
| Datenbewegungen (`mov`/`movzx`/`movabs`) | 494.930.767 (51,7 %) | 371.999.741 (53,2 %) |
| Rahmen-Loads | 129.637.675 (13,53 %) | 94.014.270 (13,44 %) |
| Rahmen-Stores | 126.722.224 (13,23 %) | 80.695.439 (11,54 %) |

Der **Anteil** der Datenbewegungen steigt, obwohl ihre absolute Zahl um
24,8 % faellt: alles andere ist staerker geschrumpft. Die Rahmen-Stores gehen
um 36,3 % zurueck, die Rahmen-Loads nur um 27,5 % — genau das Bild, das man
erwartet, wenn ueberfluessige Zwischenspeicherungen verschwinden, der Grund
fuer die Auslagerung aber bleibt. Der verbleibende Engpass ist damit
unveraendert benannt: **zu wenige Register** (§13.2).

## 12. Abnahme

| Pruefung | Ergebnis | Basis |
|---|---|---|
| `bash ./test.sh` | **PASS 754/754** | 751/751 (+3 durch `tests/332_adressierung.fi` in drei Baustufen) |
| `cargo test --release` (Modultests) | **169/169** | 155 (+8 `faedeln.rs`, +6 `regalloc.rs`) |
| `bash tools/selbst_vergleich.sh` | **214 gleich, 0 abweichend, 0 fehlerhaft**, CODEGEN FEHLT 0 | 213/0/0 (+1 neue Testdatei) |
| `bash tools/fixpunkt.sh` | **Stufe 2 == Stufe 3, zeichengleich, 427.401 Zeilen** | 427.401 |
| `bash tools/tokenizer/run.sh` | **6810/6810 = 100,00 %**, mit Fehlern **6809/6810** | unveraendert |
| Lexer/Parser/Layout/Sema/FIR-Vergleich | unveraendert (je 1 bekannte, benannte Abweichung; Layout 0) | unveraendert |
| DOM-Dauerlauf, Pakete, atomares Primitiv | bestanden | unveraendert |

Vor jeder Abnahme wurden `.firnc1 .firnc2 .firnc3` geloescht (Falle (a) der
Rundenvorgabe). Alle Zwischendateien lagen unter `.r51/` bzw.
`.tokenizer-work/` im eigenen Worktree — kein `/tmp` (Falle (b)). Wanduhrwerte
werden nirgends als Beleg angefuehrt (Falle (c)).

Zustand des Messlaufs am Ende, mit `tools/tokenizer/muster.py`:

```
Rahmenverwaltung (callee-saved sichern/holen)   36.414.390 Ir   5,21%    932 Stellen
Store+Reload derselben Zelle                    28.215.979 Ir   4,03%    118 Stellen
Rahmenverwaltung (push/pop/ret)                 12.688.869 Ir   1,81%   1177 Stellen
lea + Zugriff (Adressierungsmodus ungenutzt)     9.467.272 Ir   1,35%    215 Stellen
Rahmenverwaltung (rsp<->rbp)                     8.459.246 Ir   1,21%    664 Stellen
Rahmenverwaltung (call)                          4.229.623 Ir   0,60%   3949 Stellen
setcc-Kette statt direktem Sprung                  565.336 Ir   0,08%     16 Stellen
jmp direkt hinter jcc (Blocklayout)                 10.021 Ir   0,00%     39 Stellen
```

## 13. Offene Punkte

1. **Store/Reload an Blockgrenzen: 28.215.979 Ir (4,03 %), 118 Stellen.** Das
   ist der Rest des Phi-in-Speicher-Problems: ein Zusammenfluss, an dem jeder
   Vorgaenger in eine Zelle schreibt und der Nachfolger sie sofort liest. H1
   loest das fuer `bool` (weil dort der Leser ein `brcond` ist); fuer Werte
   geht es nur ueber echte Phi-Knoten in FIR, Tail-Duplizierung des
   Zusammenflusses oder eine Zellen-Befoerderung, die genug Register haette.
2. **Registerdruck ist der Engpass.** In `tokenize` (Rahmen 43 KiB) und
   `dekodiere` werden Schleifeninvarianten wie `off`, `len`, `basis` bei
   jedem Durchlauf aus dem Rahmen geholt. Ursache ist der Linear Scan ohne
   Intervall-Splitting: wer einen `call` kreuzt, bekommt nur eines der fuenf
   callee-saved Register — auch dann, wenn der Aufruf auf einem **kalten
   Zweig** liegt und der Wert dort gar nicht lebt. `crosses_call` wird heute
   als „irgendein Aufruf liegt zwischen `start` und `end`" bestimmt; eine
   Berechnung aus der echten Lebendigkeit (`live_out` an der Aufrufstelle,
   ohne das Ergebnis des Aufrufs, plus seine Argumente) waere praeziser und
   ohne Splitting zu haben. Das ist der naechstgroesste Hebel.
3. **Rahmenverwaltung: 66.021.751 Ir (9,44 %)** bei 4.229.623 Aufrufen —
   Prolog, Epilog, Sichern und Zurueckholen der callee-saved Register
   (36.414.390 Ir davon). `tok_attr_value_push` sichert vier Register, obwohl
   eines davon nur auf dem kalten Zweig gebraucht wird; „shrink wrapping"
   waere der Fachbegriff. Mehr Inlining ist es ausdruecklich **nicht** (§9.1).
4. **Adressfaltung: 16.188.606 Ir (2,3 %) blieben stehen**, weil das Ergebnis
   der Adressrechnung mehr als einmal gelesen wird — typisch mehrere Felder
   desselben Structs. Das zu falten hiesse, die Lebensspanne der Basis ueber
   mehrere Instruktionen zu verlaengern; dafuer muesste der Verteiler die
   Verlaengerung **vor** der Zuteilung kennen. Unveraendert offen seit
   Runde 43.
5. **`sink_fehler_bei`** (5,93 %) rechnet Zeile/Spalte je Parse-Fehler durch
   Nachscannen des Stroms; ein mitlaufender Zaehler waere billiger. Gehoert
   dem Tokenizer, nicht dem Compiler (§9.3).
6. **Der Grundpfad** (`codegen_x86.rs`) hat kein Blocklayout und keine
   Adressfaltung. Fuer `--no-opt` ist das richtig so; falls `dev-fast`
   einmal auf ihn zurueckfaellt, kostet es unnoetig.
