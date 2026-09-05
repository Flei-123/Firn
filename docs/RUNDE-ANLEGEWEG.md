# Runde ANLEGEWEG -- der schnelle Pfad im Sammler

Zweig `anlegeweg`, Arbeitsbaum `/root/firn-anlegeweg`, abgezweigt von `main`
(`4af14c3ed`). 04.09.2026.

Der Auftrag kam aus der Runde TEMPO2, die die eine Zahl gemessen hat, an der
alles andere haengt:

```
    ein Objekt im Haufen anlegen ......... 161 ns
    eine leere Firn-Funktion rufen .......   3 ns
    eine Spalte des Syntaxbaums lesen ....   7 ns
    einen Namen nachschlagen .............  25 ns
```

und die Folgerung dazu benannt hat: *„161 ns auf 10 bis 20 ns zu bringen
waere allein ein Faktor 2 bis 3 auf die GANZE Maschine."*

Was diese Runde liefert:

```
  1. DIE ZAHL IST ZERLEGT. Fuenf Ablationsstufen, jede einzeln gemessen.
     Es war nie das Einsammeln. Es waren drei Dutzend FUNKTIONSAUFRUFE
     auf dem Weg -- und eine Groessenklasse, die die Laufzeit bei jedem
     Aufruf neu ausgerechnet hat, obwohl die ANFORDERUNGSSTELLE sie die
     ganze Zeit kannte.

  2. DER SCHNELLE WEG IST GEBAUT UND WIRD AN DER ANFORDERUNGSSTELLE
     EINGEBETTET -- in BEIDEN Uebersetzern (firnc0 in Rust, firnc1 in
     Firn).

         24 Oktette anlegen .......  153,9 ns  ->  54,9 ns   2,80 x
         56 Oktette anlegen .......  190,4 ns  ->  74,5 ns   2,55 x
         eine JS-Umgebung .........  566,9 ns  -> 210,9 ns   2,69 x
         mit 8192 lebenden Objekten  179,1 ns  ->  84,7 ns   2,11 x

  3. UND DER BEFUND, NACH DEM DIESE RUNDE NICHT GESUCHT HAT UND DER MEHR
     WERT IST ALS DER, NACH DEM SIE GESUCHT HAT: eine Adresse, die NICHT
     im Haufen liegt, kostete den Sammler einen Lauf ueber die ganze
     Brockenliste -- 279 ns. Und das ist der haeufigste Fall ueberhaupt,
     weil der KONSERVATIVE Stapellauf jedes Wort des Stapels an
     `__gc_mark` gibt.

         __gc_mark auf einen Zeiger .....  29 ns  ->  27 ns
         __gc_mark auf einen Nichtzeiger  279 ns  ->   5 ns
         gc_soak Durchsatz ..............          1,30 x
         gc_soak laengste Pause ......... 27,5 ms -> 10,0 ms

  4. EINE EHRLICHE ANTWORT AUF „FAKTOR 2 BIS 3 AUF DIE GANZE MASCHINE":
     NEIN. Von Ende zu Ende gemessen:

         zwoelf JS-Baenke, geometrisches Mittel ..  1,207 x
         Seitenaufbau xoffi.ai .................. 766,3 -> 674,7 ms
         Bild auf allen fuenf Seiten ............ BITGLEICH

     Warum das Anlegen 2,8 mal schneller wurde und JavaScript nur 1,2 mal,
     und was das fuer die Arbeitsliste heisst, steht in Abschnitt 9 -- mit
     den Zahlen, die die Entscheidung tragen.
```

---

## 0. Der Messstand

```
Bauwirt       AMD EPYC 7571, 20 Kerne sichtbar, LXC-Behaelter, Fremdlast.
              Deshalb steht jede Zahl, auf die es ankommt, als VORHER
              gegen NACHHER im selben Lauf, verschraenkt, Bestwert aus drei.
Uebersetzer   firnc0 (Rust) und firnc1 (selbstgehostet), Vorgabe-Baustufe
              `dev-fast` -- die, mit der Certus gebaut wird.
Firn          Arbeitsbaum /root/firn-anlegeweg, von 4af14c3ed
Certus        /root/certus-tempo2, vendor/firn/baum auf a58d17dd5;
              dieselben zwei Aenderungen dort fuer das A/B eingespielt
Baenke        tools/anlegeweg/mikro.fi   (die Grundvorgaenge)
              tools/anlegeweg/reject.fi  (was ein Nichtzeiger kostet)
              tools/gc_soak/run.sh       (Dauerlauf, Haufen, Pausen)
              tools/tempo2/jsbench.py    (zwoelf JS-Baenke)
              tools/tempo2/messen.py     (Seitenaufbau, fuenf Seiten)
              tools/tempo2/mikro_main.fi (dieselben Posten in Certus)
```

**Die Zahlen sind ROH**, genau wie in TEMPO2: der Leerlauf der Schleife wird
NICHT abgezogen, damit sie mit jenem Bericht unmittelbar vergleichbar sind.
Was die Schleife selbst kostet, steht als Posten 0 (2,7 ns).

---

# TEIL 1 -- WORAUS DIE 154 NANOSEKUNDEN BESTANDEN

## 1. Die Grundvorgaenge, einzeln

`tools/anlegeweg/mikro.fi`, je fuenf Millionen Runden, Bestwert aus drei:

```
                                        vorher      nachher
   0 Leerlauf (nur die Schleife)         2,706 ns    2,686 ns
   1 eine leere Funktion rufen           3,340 ns    3,247 ns
   2 __gc_ld64 (Aufruf + Ladebefehl)     3,048 ns    3,044 ns
   3 ein Wort direkt lesen               2,680 ns    2,712 ns
   4 __gc_st64 (Aufruf + Schreibbefehl)  4,762 ns    4,725 ns
   5 ein Wort direkt schreiben           0,669 ns    0,670 ns
   6 __gc_class_for(24)                  5,222 ns    3,444 ns
   6b __gc_class_for(200)               25,529 ns    3,093 ns
   7 __gc_classes_bytes(0)               3,046 ns    3,353 ns
   8 __gc_diag_inc                       7,213 ns    7,450 ns
   9 24 Oktette nullen ueber __gc_st64  14,227 ns   14,384 ns
  10 dieselben 24 Oktette direkt         4,502 ns    4,362 ns
  11 __gc_now_ns()                     395,519 ns  391,212 ns
  12 24 OKTETTE ANLEGEN                153,879 ns   54,900 ns   2,80 x
  13 56 OKTETTE ANLEGEN                190,384 ns   74,527 ns   2,55 x
  14 vier Objekte (eine Umgebung)      566,880 ns  210,928 ns   2,69 x
  15 anlegen mit 8192 lebenden         179,064 ns   84,693 ns   2,11 x
  16 __gc_alloc_raw(1, 24) unmittelbar 142,329 ns   63,017 ns   2,26 x
```

Drei dieser Zeilen sind die ganze Diagnose:

* **Zeile 2 gegen Zeile 3**: `__gc_ld64` ist eine FUNKTION. Jeder einzelne
  Wortzugriff im Sammler war ein Aufruf. Fuer sich genommen sind das 0,4 ns
  in einer Schleife mit viel Platz zum Ueberlappen -- in einer Kette von
  dreissig voneinander abhaengigen nicht.
* **Zeile 6b**: die Groessenklasse war eine SCHLEIFE, die je Runde
  `__gc_classes_bytes` RIEF. Fuer 200 Oktette sind das sechs Aufrufe:
  25,5 ns fuer eine Frage, deren Antwort vier Vergleiche sind.
* **Zeile 11**: `__gc_now_ns()` ist ein echter Systemaufruf, 395 ns. Sie
  wird nur gelesen, wenn es ueberhaupt Sammlerarbeit gibt -- aber das
  sollte wissen, wer sie irgendwo anders hinschreiben will.

## 2. Die Ablation: fuenf Stufen, jede gemessen

Dieselbe Bank, aber `lib/gc/gc.fi` Schritt fuer Schritt geaendert und der
Uebersetzer jedes Mal neu gebaut. Posten 12, mit laufendem Sammler und mit
hochgesetzter Sammelgrenze:

```
   Stufe                                     mit Sammler   entfesselt
   0  der Stand vor dieser Runde               144,4 ns     146,4 ns
   A  Groessenklasse als binaere Suche,
      kein Aufruf je Runde                     140,5 ns     131,3 ns
   B  die Wortzugriffe im Anlegeweg als
      nackte Speicherbefehle statt als
      __gc_ld64- / __gc_st64-AUFRUFE           103,7 ns      96,7 ns
   C  Diagnosezaehler und Nullschleife
      ebenso                                   106,9 ns      99,5 ns
   D  DER SCHNELLE WEG: eine Funktion, die
      den gewoehnlichen Fall in einem Zug
      prueft und dann selbst anlegt             68,2 ns      65,1 ns
   E  D plus BUMP-BEREICH fuer frische
      Brocken                                   80,6 ns      53,6 ns
```

**So ist die Tabelle zu lesen.** Die 154 ns waren, in runden Zahlen:

| Posten | ns |
|---|---|
| die Groessenklasse ueber eine Schleife mit einem Aufruf je Runde | 4 (24 Oktette), 22 (200 Oktette) |
| rund dreissig `__gc_ld64`/`__gc_st64`-AUFRUFE auf dem Weg | 37 |
| der lange Weg selbst: Phasenpruefungen, `__gc_alloc_in`/`_out`, `__gc_get_block`, `S_BLOCKGROESSE`, der Aufrufaufwand von alledem | 36 |
| das eigentliche Anlegen: Block ausklinken, Nutzlast nullen, Kopf schreiben, drei Zaehler fortschreiben | ~46 |
| die UEBERSETZUNG der Anforderung: Fehlerunion, `catch`, Nullpruefung an der Anforderungsstelle | ~9 |

Kein einziger dieser Posten ist der Sammler. Stufe D ist mit und ohne
Sammler gemessen, und der Unterschied betraegt 3 ns -- **es war nie das
Markieren und nie das Kehren.**

## 3. Was gemessen und dann VERWORFEN wurde: der Bump-Bereich

Ein Bump-Bereich ist das, was die Literatur an die erste Stelle setzt, und
er ist das, was ein TLAB in HotSpot, eine `mcache`-Spanne in Go und die
Kinderstube in SBCL sind: ein frischer Brocken wird nicht Block fuer Block
in die Freiliste gehaengt, sondern als ein zusammenhaengender Bereich
vermerkt, und Anlegen heisst dann „Zeiger laden, addieren, gegen die Grenze
pruefen, zurueckschreiben". Seine Bloecke kommen geradewegs von `mmap`,
sind also schon genullt, und **das Nullen der Nutzlast entfaellt ganz.**

Er wurde gebaut (Stufe E) und er wurde gemessen, verschraenkt, Bestwert aus
fuenf:

```
                                    nur Freiliste   mit Bump-Bereich
   24 Oktette anlegen, Sammler an       74,6 ns          81,1 ns   E/D 1,087
   56 Oktette anlegen, Sammler an      102,0 ns         107,1 ns   E/D 1,050
   vier Objekte, Sammler an            266,8 ns         268,3 ns   E/D 1,006
   anlegen mit 8192 lebenden           104,1 ns         103,9 ns   E/D 0,998
   24 Oktette anlegen, Sammler AUS      65,1 ns          53,6 ns   E/D 0,823
   neue Brocken ueber den Lauf             1613             2448
   Haufen                               1310720          1835008
```

**Urteil: verworfen.** Der Bump-Bereich gewinnt genau dort, wo nie
eingesammelt wird -- und in diesem Fall ist ein wirkliches Programm nie.
Sobald der Sammler laeuft, kommt jede Anforderung aus einem WIEDERVERWENDETEN
Block, die Freiliste ist die Quelle, der Bump-Bereich steht leer, und was
von ihm bleibt, ist ein um 40 % groesserer Haufen (7 Brocken statt 5) und
50 % mehr angelegte Brocken. Die Zahl steht hier, damit sie niemand ein
zweites Mal probiert.

*(Was sich LOHNEN wuerde, ist ein Kehren, das seine toten Bloecke als
zusammenhaengende Bereiche zurueckgibt statt als verkettete Liste -- so
macht es Immix. Das ist eine eigene Runde; sie aendert das Kehren, nicht das
Anlegen.)*

---

# TEIL 2 -- DER SCHNELLE WEG, UND WO ER STEHT

## 4. In der Laufzeit: `__gc_alloc_fast`

`lib/gc/gc.fi` bekommt eine Funktion, die den gewoehnlichen Fall in EINEM
Zug prueft. Jede der Bedingungen ist dieselbe, die auch der lange Weg
prueft; keine einzige wird uebersprungen:

```
   S_INIT  == 1          bereit (0 = gc_init() fehlt, 2 = in einem Finalisierer)
   S_MULTI == 0          ein Faden -- sonst Haltepunkt, Sperre, Freilisten
                         je Faden: alles davon unberuehrt
   S_PHASE == 0          kein Zyklus laeuft -- sonst muss das Objekt GRAU
                         werden und auf den Markierstapel
   S_SINCE <  S_LIMIT    die Anlegegrenze, der Ausloeser des Zyklus
   class < CLASSES       die Groesse hat eine Groessenklasse
   die Freiliste dieser Klasse ist nicht leer
```

Danach: den Block ausklinken, die Nutzlast nullen, den Kopf schreiben
(Typkennung, das aktuelle Weiss, Seriennummer), die drei Zaehler
fortschreiben. Alles mit nackten Speicherzugriffen, und die Adressrechnung
mit `+%` -- das sind Adressen im Zustandsblock und in einem Brocken, ein
Ueberlauf ist dort bauartbedingt unmoeglich, und die Pruefung auf jeder
einzelnen davon war ein messbarer Teil dessen, was den Weg langsam machte.

`__gc_class_for` und `__gc_classes_bytes` sind binaere Suchen aus derselben
Tabelle geworden (`6b`: 25,5 ns -> 3,1 ns). `__gc_class_and_bytes` gibt
Klasse und Blockgroesse in EINEM Aufruf zurueck.

## 5. An der Anforderungsstelle: der Uebersetzer faltet sie zu Konstanten

Das ist der Punkt der Runde. Die Laufzeit muss bei jedem Aufruf ausrechnen,
was die ANFORDERUNGSSTELLE die ganze Zeit weiss -- welche Groessenklasse das
Objekt hat und wie gross sein Block ist. Deshalb steht der schnelle Weg
jetzt AN der Anforderungsstelle, in beiden Uebersetzern
(`compiler/src/gc_lower.rs`, `lib/firnc1/lower.fi`), mit `class` und `step`
zu Konstanten gefaltet und der Nullschleife abgewickelt.

Was der Codeerzeuger aus `gc Cell { a: i, b: 0, c: 0 }` (24 Oktette,
Klasse 0, Block 48) in der VORGABE-Baustufe macht:

```asm
    lea  rax, [rip + .L__gc_state]
    mov  r9, qword ptr [r8+8]         ; S_INIT
    cmp  r9, 1
    jne  langsam
    mov  r10, qword ptr [r8+1992]     ; S_MULTI
    cmp  r10, 0
    jne  langsam
    mov  r10, qword ptr [r8+320]      ; S_PHASE
    cmp  r10, 0
    jne  langsam
    mov  r10, qword ptr [r8+96]       ; S_LIMIT
    lea  r11, [r8+88]                 ; S_SINCE
    mov  rdx, qword ptr [r11]
    cmp  rdx, r10
    jae  langsam
    lea  r10, [r8+168]                ; S_FREE + 0*8
    mov  rdi, qword ptr [r10]
    cmp  rdi, 0
    je   langsam
    lea  rsi, [rdi+16]                ; die Nutzlast
    mov  r12, qword ptr [rsi]         ; ausklinken
    mov  qword ptr [r10], r12
    mov  qword ptr [rsi], 0           ; nullen, abgewickelt
    mov  qword ptr [rsi+8], 0
    mov  qword ptr [rsi+16], 0
    lea  r10, [r8+104]                ; S_SERIES
    mov  rax, qword ptr [r10]
    mov  r12, rax
    add  r12, 1
    mov  qword ptr [r10], r12
    mov  dword ptr [rdi], 1           ; die Typkennung, eine Konstante
    mov  eax, dword ptr [r8+360]      ; das aktuelle Weiss
    mov  dword ptr [rdi+4], eax
    mov  qword ptr [rdi+8], r12       ; die Seriennummer
    lea  r10, [rdx+48]                ; S_SINCE += 48
    mov  qword ptr [r11], r10
    lea  r10, [r8+296]                ; S_TOTAL += 48
    mov  r11, qword ptr [r10]
    lea  rdx, [r11+48]
    mov  qword ptr [r10], rdx
    lea  r11, [r8+1528]               ; S_D_ALLOK += 1
    mov  r8, qword ptr [r11]
    lea  r10, [r8+1]
    mov  qword ptr [r11], r10
    mov  qword ptr [r15], rsi         ; in die __val-Zelle der Fehlerunion
```

**41 Befehle, kein Aufruf, jeder Abstand eine Konstante.** Vor dieser Runde
war dieselbe Quellzeile ein `call __gc_alloc_raw`, und dahinter lagen rund
140 Befehle und drei Dutzend weitere Aufrufe.

Das Ergebnis reist durch die `__val`-Zelle DER FEHLERUNION, nicht durch
eine eigene Stapelzelle. FIR hat kein Phi, also muss eine Zelle her -- aber
sie darf keine NEUE sein: eine eigene Zelle hielt den zuletzt an dieser
Stelle angelegten Zeiger, SOLANGE DIE FUNKTION LIEF, und niemand raeumte sie
je auf. Der Stapellauf ist KONSERVATIV, so eine Zelle ist eine Wurzel. Die
`__val`-Zelle ist ohnehin schon eine, dort gehoert der Zeiger hin, und der
Fehlerzweig ueberschreibt sie gleich danach mit dem Nullwert.

Die Abstaende des Zustandsblocks sind jetzt ein VERTRAG zwischen drei
Dateien. Sie stehen nicht auf Treu und Glauben zweimal da: der Test
`gc::tests::offsets_match_the_runtime` liest sie aus dem eingebetteten
Laufzeittext (`include_str!`) zurueck und wird rot, sobald sich einer
bewegt.

**Codegroesse ist hier kein Einwand, und das ist nachgesehen und nicht
geraten:** Certus hat 105 `gc C{ }`-Stellen in ganz `lib/`, der Firn-Baum
96. Hundert Stellen zu vierzig Befehlen sind 20 KiB.

## 6. Warum es nicht 10 bis 20 ns sind

54,9 ns fuer 41 Befehle sind 2,7 Takte je Befehl. Dasselbe Programm mit
`--opt-level=release-safe` braucht fuer dieselbe Zeile **33,0 ns**. Der Rest
ist also nicht mehr der Anlegeweg, sondern der Codeerzeuger in der
Vorgabestufe -- oertliche Groessen liegen im Speicher, eine Adresse mit zwei
Gebrauchsstellen wird zu einem eigenen `lea` statt zu einem Speicheroperanden.
Das ist ein Befund fuer eine Runde ueber den Codeerzeuger, und er ist eine
Zahl und keine Meinung: 54,9 gegen 33,0.

---

# TEIL 3 -- GETAGGTE SOFORTWERTE, UND WAS BEIM HINSEHEN HERAUSKAM

## 7. Die Behauptung in `lib/js/val.fi`, geprueft

Der Kopf von `lib/js/val.fi` sagt darueber, warum Certus kein getaggtes Wort
hat:

> *„A packed small integer would be harmless in the other direction
> (`__gc_mark` finds no block for it), but the pointer case is fatal."*

**Ueber die Korrektheit hat der Satz recht.** `__gc_mark` ruft
`__gc_block_of`, das findet fuer einen Wert, der nicht im Haufen liegt,
keinen Block, und der Wert wird uebergangen. Geprueft, es stimmt.

**Ueber den Preis hat der Satz unrecht, und zwar deutlich.**
`__gc_block_of` LAEUFT DIE BROCKENLISTE AB. Sein Ein-Eintrag-Speicher trifft
bei einem echten Zeiger fast immer; bei etwas, das nicht im Haufen liegt,
trifft er NIE -- jeder solche Wert kostet also einen vollstaendigen Lauf
ueber jeden Brocken des Haufens.

`tools/anlegeweg/reject.fi`, auf einem Haufen von 9,7 MiB (200 000 lebende
Objekte, rund 37 Brocken):

```
                                        vorher      nachher
   __gc_mark auf einen echten Zeiger      29 ns      27 ns
   __gc_mark auf einen Sofortwert        279 ns       5 ns
   __gc_block_of auf einen Sofortwert    276 ns       3 ns
```

## 8. Und das ist kein Randfall, sondern der Normalfall

Der Sammler laeuft den Stapel KONSERVATIV ab (SPEC 3.5.3). Er gibt JEDES
Wort des Stapels und jedes gerettete Register an `__gc_mark`, und die
allerwenigsten davon sind Zeiger. Jedes dieser Woerter hat einen Lauf ueber
die ganze Brockenliste bezahlt.

Die Abhilfe sind vier Befehle: der Zustandsblock merkt sich die NIEDRIGSTE
und die HOECHSTE Adresse, die der Haufen je belegt hat, und `__gc_block_of`
und `__gc_chunk_of` weisen alles ausserhalb sofort ab. Das Paar wird nur
geweitet, nie verengt -- ein Brocken, der ans Betriebssystem zurueckgeht,
laesst die Grenzen stehen. Die Antwort bleibt KORREKT (eine Adresse
innerhalb der Grenzen wird weiterhin ordentlich gesucht), sie ist nur etwas
weniger scharf; sie zu verengen brauchte einen Lauf ueber die Liste genau in
dem Augenblick, der billig werden soll.

Was das wert ist, auf dem Dauerlauf des Sammlers
(`tools/gc_soak/run.sh`, Mode 0, 90 s Budget, wechselnde Objektgroessen):

```
                                 vorher         nachher
   Runden in 90 s               558 976         727 744     1,302 x
   Oktette durch den Anleger      1,9 GiB         2,3 GiB
   Sammellaeufe                     490             637
   Haufen / lebend, Median         2,61 x          2,56 x
   laengste Pause                27,54 ms         9,98 ms    2,76 x
   RSS-Hoechstwert              42 940 KiB      43 316 KiB
   Urteil                        beschraenkt     beschraenkt
```

**Ein Drittel mehr Durchsatz und die schlimmste Pause auf ein Drittel** --
aus einem Befund, der aus einer Frage nach getaggten Sofortwerten kam.

## 9. Was Sofortwerte noch braechten: gemessen, und es ist wenig

Die schnelle Abweisung macht sie fuer den Sammler SICHER und BILLIG. Aber
bevor 31 500 Zeilen `lib/js` umgeschrieben werden, ist die Frage, was am
Ende steht. Zwei Messungen sagen: nicht viel.

**Erstens** haelt `realm_num` in Certus schon einen Speicher fuer die
kleinen Ganzzahlen 0..1024 -- eine Zahl ausserhalb kostet ein Haufenobjekt,
eine darin kostet ein Nachschlagen. Dieselbe Bank mit Zahlen innerhalb und
ausserhalb dieses Fensters (`jsrun` nach dieser Runde, 200 000 Runden):

```
   arith,   im Speicher      235 598 Op/s
   arith,   ausserhalb       202 800 Op/s      1,16 x
   Zaehler, im Speicher      270 679 Op/s
   Zaehler, ausserhalb       248 116 Op/s      1,09 x
```

**Zweitens** der Grund: eine Runde von
`while (i < N) { s = i & 1023; i = i + 1; }` kostet in diesem Deuter
**3,7 Mikrosekunden**. TEMPO2 hat 264 bis 1266 ns je Knoten des Syntaxbaums
gemessen. Dagegen ist eine Zahl -- 104 ns zum Anlegen, 37 ns zum Holen aus
dem Speicher, rund 2 ns als Sofortwert -- ein paar Prozent und kein Faktor.

**Schluss, gegen die Arbeitsliste von TEMPO2:** Posten 2 (Sofortwerte) und
Posten 3 (Bytecode) sollten die Plaetze tauschen. Solange ein
Schleifendurchgang 3,7 Mikrosekunden kostet, sind Sofortwerte Politur. Der
Baumlauf ist die Sache.

---

# TEIL 4 -- DAS ERGEBNIS

## 10. (a) Der Anlegeweg

```
   Firn, tools/anlegeweg/mikro.fi, Vorgabe-Baustufe
       24 Oktette anlegen        153,9 ns  ->   54,9 ns   2,80 x
       56 Oktette anlegen        190,4 ns  ->   74,5 ns   2,55 x
       vier Objekte (Umgebung)   566,9 ns  ->  210,9 ns   2,69 x
       mit 8192 lebenden         179,1 ns  ->   84,7 ns   2,11 x
       __gc_alloc_raw direkt     142,3 ns  ->   63,0 ns   2,26 x

   Certus, tools/tempo2/mikro_main.fi (dieselben Posten wie in TEMPO2)
       eine Zahl anlegen         165 ns    ->  104 ns     1,59 x
       eine Umgebung anlegen     635 ns    ->  245 ns     2,59 x
       eine Zahl aus dem Speicher 39 ns    ->   37 ns
       einen Namen nachschlagen   24 ns    ->   26 ns
       eine leere Funktion rufen   4 ns    ->    4 ns
```

`realm_num` gewinnt weniger als das nackte Anlegen, weil das meiste, was es
tut, nicht das Anlegen ist: drei Gleitkommavergleiche, `math_bits`, ein
Hin und Zurueck zwischen `u64` und `f64` mit gepruefter Umwandlung und zwei
Stufen Fehlerunion.

## 11. (b) Die zwoelf JS-Baenke

`tools/tempo2/jsbench.py`, N = 200 000, Certus gegen Certus:

```
   Bank                    vorher  nur Anlegeweg    x    Endstand       x
   leere_schleife         418 567     448 218   1,071     456 019   1,089
   arith                  189 981     207 102   1,090     210 875   1,110
   funktionsaufruf        106 947     123 514   1,155     143 325   1,340
   eigenschaft_lesen      474 867     505 182   1,064     526 558   1,109
   eigenschaft_schreiben  324 283     343 953   1,061     347 055   1,070
   methodenaufruf         137 241     152 199   1,109     161 114   1,174
   feld_index             198 427     205 209   1,034     203 612   1,026
   prototypkette          148 299     163 387   1,102     178 154   1,201
   zeichenketten          249 368     261 821   1,050     257 261   1,032
   objekt_anlegen         107 635     124 515   1,157     134 712   1,252
   closure                102 071     118 331   1,159     139 165   1,363
   fibonacci               46 825      51 672   1,104      91 271   1,949
   ---------------------------------------------------------------------
   geometrisches Mittel                         1,095               1,207
```

Die Spalte „nur Anlegeweg" ist die Fassung OHNE die schnelle Abweisung aus
Abschnitt 8, „Endstand" die mit beidem. Bestwert aus sechs Laeufen vorher und
zwei je Fassung nachher.

Die schnelle Abweisung ist hier mehr wert als der schnelle Anlegeweg, und
der Grund ist in beiden Faellen derselbe: `fibonacci` und `closure` bauen
die tiefsten Stapel, und es ist der KONSERVATIVE STAPELLAUF, der fuer jedes
Wort bezahlt.

## 12. (c) Der Seitenaufbau

`tools/tempo2/messen.py`, Zuwachs je Ausfertigung, 412 x 915, drei Laeufe:

```
   Seite                 vorher    nachher    Bild
   xoffi.ai             766,3 ms   674,7 ms   derselbe SHA-256   1,14 x
   xoffi.ai/100         345,4 ms   352,3 ms   derselbe SHA-256   0,98 x
   news.ycombinator     199,1 ms   171,4 ms   derselbe SHA-256   1,16 x
   de.wikipedia          68,7 ms    63,6 ms   derselbe SHA-256   1,08 x
   example.com            4,4 ms     4,7 ms   derselbe SHA-256
```

Je Abschnitt auf xoffi.ai: Stilblaetter 72,7 -> 63,4 ms, Kaskade
82,5 -> 80,8 ms, HTML 92,5 -> 91,2, Fluss 42,8 -> 41,2, **Rastern
499,7 -> 511,2**. Die Streuung des Bauwirts ist hier gross -- ein zweiter
Lauf gab 740,8 -> 727,2 ms statt 766,3 -> 674,7; belastbar ist nur die
Richtung (ein paar Prozent) und das Bild. Der Seitenaufbau besteht zu 62 % aus dem Rastern, und das
Rastern legt kaum etwas an; deshalb sind die 2,8 x auf das Anlegen hier 2 %
und nicht mehr. Der Abstand zu Chromium auf xoffi.ai bleibt bei den 6,2 x
aus TEMPO2.

**Jedes Bild ist bitgleich zur Fassung vor der Runde.** Das ist die
Abnahme, nicht die Zeit.

## 13. Die eingefrorenen Zahlen

```
Firn, bash test.sh                     Abschnitt 14
tools/gc_soak/run.sh                   BESTANDEN, beschraenkt, Gegenprobe
                                       WAECHST; Durchsatz 1,302 x, laengste
                                       Pause 27,54 -> 9,98 ms,
                                       Haufen/lebend 2,61 -> 2,56

Certus, tools/layout/run.sh --fast
   eigene Faelle    1087 / 1087 Kaesten in 146 Faellen  (Grenze 1087)
   gegen Chromium   1087 / 1087, Abweichung 0,00 %      (Grenze 1087)
   Malreihenfolge   5171 / 5171 Sondenpunkte           (Grenze 5171)
   LAYOUT OK

Certus, tools/js/run.sh --fast
   test262 Zerteiler 3051   (TEMPO2: 3051 -- GLEICH)
   test262 Maschine  2693   (TEMPO2: 2693 -- GLEICH)

Certus, tools/tempo2/messen.py
   der SHA-256 des Bildes auf allen fuenf Seiten: unveraendert
```

**Zwei Tests mussten robuster werden, und das ist kein Schoenreden.**
`tests/842_gcmap_basic.fi` (Schritt 5) und `tests/843_collections_interplay.fi`
(Teil A und B) beweisen, dass etwas NICHT mehr gehalten wird. Der
KONSERVATIVE Stapellauf darf tote Objekte halten -- `lib/gc/gc.fi` sagt das
bei `gc_stack_clean()` woertlich und verlangt, den toten Stapel vorher zu
saeubern. Schritt 4 desselben Tests 842 tut das schon; Schritt 5 tat es
nicht. Gemessen, und das ist die Zahl, um die es geht:

```
   842, Ueberschuss ueber `vor2`, --opt-level=release-fast
       vor dieser Runde, ohne gc_stack_clean()      24   (Grenze: 24)
       nach dieser Runde, ohne gc_stack_clean()     25   ROT
       nach dieser Runde, MIT gc_stack_clean()       0
```

Der Test stand also schon vorher GENAU auf der Grenze. Was ihn kippt, ist
nicht mehr gehaltener Muell, sondern eine andere Rahmenaufteilung -- genau
das, wovor `gc.fi` an dieser Stelle warnt. Beide Tests bekommen deshalb den
Aufruf, den die Laufzeit selbst vorschreibt, und **beide bestehen danach in
allen fuenf Baustufen mit dem UEBERSETZER VON VORHER UND MIT DEM VON
NACHHER.** Es ist keine Grenze angehoben worden.

Nachgemessen, welcher Teil der Aenderung es war: bei 842 die Einbettung an
der Anforderungsstelle (nur mit der Laufzeitaenderung: wieder 24), bei 843
die Laufzeit selbst -- `__gc_alloc_raw` ruft jetzt zuerst
`__gc_alloc_fast`, und schon das verschiebt die Rahmen.

**Und ein Rotes, das nicht dieser Runde gehoert.** `tools/js/soak.sh` in der
`--fast`-Gestalt (20 000 Runden) verlangt, dass die absichtlich leckende
Gegenprobe um mindestens 4596 KiB waechst. Sie ist flatterig, und sie ist es
VOR der Aenderung genauso -- vier Laeufe, verschraenkt:

```
   vorher   4864 KiB  BESTANDEN     vorher   3336 KiB  DURCHGEFALLEN
   nachher  3332 KiB  DURCHGEFALLEN nachher  3332 KiB  DURCHGEFALLEN
```

Mit den vollen 40 000 Runden bestehen beide Seiten deutlich (vorher
14 400 KiB, nachher 12 892 KiB). Die Schwelle sitzt auf einer RSS-Probe, die
genommen wird, wenn der Lauf schon gewachsen ist; bei 20 000 Runden ist der
Lauf kurz genug, dass die erste Probe hinter dem Wachstum landet. Das gehoert
in Ordnung gebracht, und zwar in einer eigenen Runde, weil es eine Messung
ist und kein Sammler.

---

## 14. Was in diesem Zweig liegt

```
lib/gc/gc.fi              __gc_alloc_fast (der schnelle Weg), die
                          Groessenklassen als binaere Suchen,
                          __gc_class_and_bytes, und die schnelle Abweisung
                          (S_HEAPLO / S_HEAPHI) in __gc_block_of und
                          __gc_chunk_of
compiler/src/gc.rs        die Abstaende des Zustandsblocks als Vertrag,
                          class_for, und die zwei Tests, die beides
                          ehrlich halten
compiler/src/gc_lower.rs  alloc_fast: der schnelle Weg AN der
                          Anforderungsstelle
lib/firnc1/gc.fi          dieselben Abstaende und dieselbe Tabelle fuer den
                          selbstgehosteten Uebersetzer
lib/firnc1/lower.fi       gc_alloc_fast: dasselbe FIR
lib/firnc1/gctext.fi      neu erzeugt (tools/gen_gctext.sh)
tools/anlegeweg/mikro.fi  NEU -- die Grundvorgaenge in Nanosekunden
tools/anlegeweg/reject.fi NEU -- was ein Nichtzeiger den Sammler kostet
tools/anlegeweg/mikro.sh  NEU -- drei Laeufe, Bestwert je Zeile
tests/842_gcmap_basic.fi  gc_stack_clean() vor der Pruefung in Schritt 5
tests/843_collections_interplay.fi
                          dasselbe vor den beiden Pruefungen "muss tot sein"
docs/RUNDE-ANLEGEWEG.md   dieser Bericht
```

## 15. Die Arbeitsliste nach dieser Runde

```
 1. DER CODEERZEUGER in der Vorgabe-Baustufe. In dieser Runde gemessen:
    dasselbe Anlegen kostet 54,9 ns bei `dev-fast` und 33,0 ns bei
    `release-safe`. Oertliche Groessen im Speicher, eine Adresse mit zwei
    Gebrauchsstellen als eigenes `lea` statt als Speicheroperand.
    WIRKUNG: auf alles.

 2. BYTECODE statt Baumlauf (TEMPO2, Posten 3). Ein Durchgang einer
    JS-Schleife kostet 3,7 Mikrosekunden. In dieser Runde gemessen:
    Zahlen als Sofortwerte waeren dagegen 1,09 bis 1,16 x wert. Also
    erst Bytecode, dann Sofortwerte -- andersherum als TEMPO2 es hatte.

 3. DAS KEHREN gibt seine toten Bloecke als verkettete Liste zurueck.
    Sie als ZUSAMMENHAENGENDE BEREICHE zurueckzugeben (Immix) wuerde das
    Bump-Anlegen aus Abschnitt 3 doch noch lohnend machen -- es ist hier
    nur daran gescheitert, dass im eingeschwungenen Zustand die
    Freiliste die Quelle ist.

 4. tools/js/soak.sh --fast: die Schwelle der Gegenprobe haengt an einer
    RSS-Probe. Auch vor dieser Runde flatterig (Abschnitt 13).
```
