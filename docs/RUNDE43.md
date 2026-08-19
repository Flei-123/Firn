# Runde 43 — das letzte Tempo-Ziel: realweb unter 2x

**Auftrag.** Der Firn-Tokenizer sollte auf dem Korpus `realweb` (acht echte
Seiten, 4,70 MB) hoechstens doppelt so lange brauchen wie `html5ever`. Der
Stand zu Rundenbeginn: `html5lib` (pathologisch) war mit 1,33x abgenommen,
`realweb` stand bei 2,68x. Als naechster Hebel war aus Runde 40/41
**Intervall-Splitting im Registerverteiler** notiert.

**Ergebnis.** Das Ziel ist erreicht — und zwar an einer ganz anderen Stelle,
als der notierte Hebel vermuten liess. Intervall-Splitting wurde in dieser
Runde **nicht** angefasst; es blieb nicht noetig. Gemessen hat entschieden.

| Korpus   | Instruktionen vorher | nachher       | Aenderung |
|----------|---------------------:|--------------:|----------:|
| realweb  |    1.297.226.150     |  965.887.079  | **-25,54 %** |
| html5lib |    2.481.675.238     | 2.150.834.784 | **-13,33 %** |

| Korpus   | Faktor vorher | Faktor nachher | Ziel |
|----------|--------------:|---------------:|-----:|
| realweb  |     **2,85x** |      **1,47x** | <= 2,00x |
| html5lib |     **1,23x** |      **0,94x** | <= 2,00x |

Die Faktoren stammen aus `tools/tokenizer/durchsatz.sh` (bester von sieben
Laeufen je Seite), **beide Staende unmittelbar nacheinander auf derselben
Maschine gemessen**, damit die bekannte Schwankung von rund 30 % nicht in den
Vergleich eingeht. Der ausgewiesene Startwert 2,85x weicht deshalb leicht vom
notierten 2,68x aus Runde 41 ab: dieselbe Binary, andere Tagesform der
Maschine. Die Instruktionszahlen darueber sind reproduzierbar auf die
Instruktion genau und der eigentliche Beleg.

Auf `html5lib` ist der Firn-Tokenizer damit **schneller als html5ever**.

---

## 1. Das Werkzeug, das gefehlt hat

Firn-Binaries sind statisch gelinkt und haben keinen Dynamik-Abschnitt.
`callgrind_annotate` findet darin keine Symbole und nennt jede Funktion nur
nach ihrer Anfangsadresse:

    fn=(748) 0x000000000041d33b

Ein Profil aus 900 solchen Zeilen ist unbrauchbar. Die Namen stehen aber sehr
wohl in `.symtab` — `nm` liest sie. Neu in dieser Runde:

**`tools/tokenizer/profil.py <binary> <callgrind-out> [anzahl]`** — liest die
callgrind-Ausgabe, loest jede `fn=`-Adresse ueber die Symboltabelle auf und
gibt Selbst- und Inklusivkosten je Funktion aus.

Zwei Fallen, die beim Bau des Werkzeugs zugeschnappt sind und im Kopf der
Datei benannt stehen:

* Mit `positions: line` ist die **erste** Zahl einer Kostenzeile die
  Zeilennummer, **nicht** die Adresse. Wer sie als Adresse liest, bekommt ein
  plausibel aussehendes, komplett falsches Profil.
* Die Zeile unmittelbar nach `calls=` ist die **inklusive** Kostenzeile des
  Aufrufs, nicht Selbstkosten der aufrufenden Funktion.

Ohne dieses Werkzeug waere der groesste Posten dieser Runde erneut unentdeckt
geblieben. Das ist die eigentliche Lehre: **ein Profil, das keine Namen zeigt,
ist kein Profil.**

## 2. Das Profil vorher (realweb, 1.297.226.150 Ir)

```
          SELBST   ANTEIL         INKLUSIV  FUNKTION
     526.900.669   40,62%      772.044.290  tokenizer__tokenize
     332.932.201   25,66%    1.297.226.146  main
     192.243.336   14,82%      192.243.336  dekodiere
     105.033.364    8,10%      105.787.926  tokens__tok_attr_value_push
      47.342.985    3,65%       47.693.872  tokens__sink_fehler_bei
      18.589.583    1,43%       18.590.675  tokens__tok_attr_name_push
      11.641.019    0,90%       16.293.464  tokens__tok_attr_finish
      10.700.120    0,82%       24.125.970  tokens__tok_emit
```

`main` mit 25,66 % war die Ueberraschung. Auffaellig: der Wert ist auf
1.000 Instruktionen genau derselbe wie auf dem anderen Korpus
(332.931.511 gegen 332.932.201), obwohl sich die Eingaben in Groesse und
Inhalt unterscheiden. Also **konstante Arbeit, die nichts mit dem Inhalt zu
tun hat**.

Ein zweiter Durchlauf mit `--dump-instr=yes` zeigte, wo genau: **100 % der
Selbstkosten von `main` liegen in einem einzigen Block von 109
Instruktionen**, 8.323.079 mal ausgefuehrt, im Mittel 40 Instruktionen je
Durchlauf. Der disassemblierte Block ist eine byteweise Kopierschleife.

8.323.079 ist keine Zufallszahl: `8.388.608 - 65.536`. Das ist genau die
Summe aller Umkopierungen, wenn ein Puffer von 64 KiB durch Verdoppeln auf
8 MiB waechst.

## 3. Hypothese 1 — das Einlesen der Eingabe, nicht der Tokenizer

**Hypothese.** `mem.read_all_stdin` liest die Eingabe in 64-KiB-Haeppchen und
laesst den Puffer dabei verdoppeln. Jede Verdoppelung ruft `mem_copy`, und
`mem_copy` kopiert **byteweise**. Zusammen 8.323.072 Byte zu je 40
Instruktionen.

**Warum 40 Instruktionen fuer ein Byte?** Weil `main` ueber den Grundpfad
ohne Registerzuteilung uebersetzt wurde (dazu Hypothese 2). Jeder
Zwischenwert ging durch einen Stack-Platz:

```
mov  rcx, QWORD PTR [rbp-0xd50]     ; Zeiger auf die Zelle von i
mov  rax, QWORD PTR [rcx]           ; i
mov  QWORD PTR [rbp-0xd68], rax     ; -> Zwischenplatz
mov  rax, QWORD PTR [rbp-0xd68]     ; <- sofort wieder zurueck
...
movzx eax, BYTE PTR [rcx]           ; ein Byte lesen
mov  BYTE PTR [rcx], al             ; ein Byte schreiben
```

**Ist das ueberhaupt ein erlaubtes Ziel?** Ja, und die Frage muss gestellt
werden. Der ausgewiesene Faktor misst die Laufzeit des ganzen Programms, und
die Gegenseite (`bench/tokenizer`, html5ever) liest ihre Datei mit
`read_to_tendril` in einem Zug. Der bisherige Faktor war also zu einem knappen
Drittel eine Messung der Firn-Speicherschicht und nicht des Tokenizers.
**Ehrlich benannt: dieser Teil des Gewinns macht den Tokenizer nicht
schneller — er raeumt einen Messfehler zugunsten von html5ever weg.**
Der Tokenizer selbst wird durch Hypothese 2 und die frueheren Runden
schneller.

**Umsetzung.** `mem_copy` kopiert acht Byte je Durchgang, solange ein volles
Wort in `n` passt; der Rest byteweise. Beide Bereiche sind mindestens `n` Byte
gross, ein Zugriff bei `i + 8 <= n` liegt also vollstaendig innerhalb, und
x86-64 erlaubt unausgerichtete 8-Byte-Zugriffe. Kopiert wird weiterhin
vorwaerts; ueberlappende Bereiche mit `dst > src` waren schon vorher nicht
erlaubt.

**Messwerte.**

| Korpus   | vorher        | nachher       | Aenderung |
|----------|--------------:|--------------:|----------:|
| realweb  | 1.297.226.150 | 1.008.764.928 | -288.461.222 (-22,24 %) |
| html5lib | 2.481.675.238 | 2.185.519.179 | -296.156.059 (-11,93 %) |

`main` faellt von 332.932.201 auf 45.786.357 Selbstkosten. Ausgabe
unveraendert (realweb 187473 Token / 1 Auftrag, html5lib 8511 / 1).

**Schluss.** Bestaetigt. Der zweitgroesste Posten des Messlaufs war eine
byteweise `memcpy` beim Einlesen.

## 4. Hypothese 2 — sechs Funktionen ohne Registerzuteilung

**Beobachtung.** Der Code in `main` sah nicht aus wie der in `tokenize`:
`tokenize` benutzt `rbx`, `r12`–`r15`, `main` nur `rax`/`rcx`. Das ist die
Handschrift des Grundpfads in `codegen_x86.rs` — der Registerverteiler war
fuer `main` gar nicht zustaendig.

**Warum nicht?** `regalloc::supported()` lieferte bis dahin nur ein stummes
`false`. Neu: `unsupported_grund()` nennt den Grund, sichtbar mit
`FIRN_RA_WARN=1`:

```
RA-Grundpfad: tokens__out_wort            — 10 Parameter
RA-Grundpfad: tokens__tok_emit            — Aufruf mit 10 Argumenten
RA-Grundpfad: tokens__sink_flush_chars    — Aufruf mit 10 Argumenten
RA-Grundpfad: tokens__sink_end            — Aufruf mit 10 Argumenten
RA-Grundpfad: tokens__out_fehlerliste     — Aufruf mit 10 Argumenten
RA-Grundpfad: main                        — Aufruf mit 10 Argumenten
```

**Eine einzige Signatur** — `tokens.out_wort(s, a, b, c, d, e, f, g, h, i)` —
hat sechs Funktionen aus der Registerzuteilung geworfen, darunter `main` mit
seinen 25 % und die gesamte Ausgabekette des Sinks. Der Registerpfad konnte
System V nur bis zum sechsten Argument; alles darueber ging an den
Grundpfad, der es laengst beherrscht.

**Umsetzung** (`compiler/src/regalloc.rs`, wortgleich mit dem Grundpfad):

* **Prolog.** Parameter ab dem siebten kommen aus dem Rahmen des Aufrufers
  (`[rbp+16]`, `[rbp+24]`, …). Sie werden **erst nach** den parallelen
  Registerbewegungen geholt: ihr Zielregister darf sonst eine noch
  gebrauchte Quelle ueberschreiben. `rax` ist nie Heimat eines Wertes und
  dient als Zwischenlager.
* **Aufruf.** Argumente ab dem siebten werden **zuerst** abgelegt
  (`sub rsp, raum` + `mov qword ptr [rsp+k*8], rax`), danach erst die
  Argumentregister gefuellt — dann kann der Aufbau der Argumentliste keinen
  noch gebrauchten Wert mehr zerstoeren. `raum` ist auf 16 aufgerundet, damit
  `rsp` an der `call`-Grenze ausgerichtet bleibt; danach `add rsp, raum`.
  Die Quellen sind rbp-relativ oder Register und bleiben von `sub rsp`
  unberuehrt.

**Messwerte** (auf Hypothese 1 aufsetzend).

| Korpus   | vorher        | nachher      | Aenderung |
|----------|--------------:|-------------:|----------:|
| realweb  | 1.008.764.928 |  965.887.079 | -42.877.849 (-4,25 %) |
| html5lib | 2.185.519.179 | 2.150.834.784 | -34.684.395 (-1,59 %) |

Selbstkosten der betroffenen Funktionen (realweb):
`main` 45.786.357 -> 11.448.891 (-75 %), `tok_emit` 10.700.120 -> 5.198.342
(-51 %), `sink_flush_chars` faellt aus den ersten zwoelf heraus.
`selbst_vergleich.sh` meldet jetzt „CODEGEN FEHLT: 0".

**Absicherung.** Diese Aenderung fasst die Aufrufkonvention an — der Bereich,
in dem ein Fehler nicht auffaellt, sondern erst drei Runden spaeter als
Miscompile auftaucht. Deshalb:

* `tests/331_stapelargumente.fi` (neu): sieben Argumente (**ein**
  Stapelwort, also mit Fuellung — der Fall, der die 16-Byte-Ausrichtung
  bricht, wenn man sie vergisst), zehn Argumente (vier Stapelworte), ein
  Stapelargument, das selbst aus einem Aufruf kommt, und Rekursion mit einem
  **weiteren Aufruf nach** dem Stapelaufruf — ein verschobenes `rsp` faellt
  sonst erst dort auf. Jedes Argument hat eine eigene Dezimalstelle im
  Ergebnis, damit auch eine vertauschte Reihenfolge auffliegt.
  `test.sh` uebersetzt jede Datei in drei Optimierungsstufen; mit `--no-opt`
  geht die Funktion wegen der Debugzeilen ohnehin ueber den Grundpfad —
  derselbe Test prueft damit **beide** Pfade gegeneinander.
* Zwei Modul-Tests umgestellt (`zu_viele_parameter_gehen_an_den_grundpfad`
  wird zu `viele_parameter_bleiben_im_registerpfad`, dazu
  `aufruf_mit_acht_argumenten_legt_zwei_auf_den_stapel`).

**Schluss.** Bestaetigt. Der Rueckfall auf den Grundpfad war teuer und lag an
einer einzigen Signatur.

## 5. Was NICHT gemacht wurde — und warum

### Intervall-Splitting (der notierte Hebel)

Nicht angefasst. Nach Hypothese 1 und 2 war das Ziel erreicht; jede weitere
Aenderung am Verteiler haette Risiko ohne Bedarf bedeutet. Die Warnung aus
Runde 41 (der Zellen-Alias, der monatelang falschen Code erzeugte, weil eine
verlaengerte Lebensspanne dem Verteiler unbekannt blieb) gilt weiter: eine
Aenderung, die Lebensspannen aufteilt, muss gegen genau diese Klasse
abgesichert werden. Das ist Arbeit fuer eine Runde, die sie noetig hat.

### Peephole fuer schmale Reloads — geprueft und zurueckgestellt

`deskriptor_peephole` streicht heute nur 64-Bit-Reloads aus **Wert-Slots**.
Eine Auswertung des Basis-Binaries mit den realweb-Gewichten fand 1.054
weitere Stellen der Form „Speichern und sofort in dasselbe Register
zurueckladen" mit zusammen 90.200.508 Ir (6,95 %). Davon lagen aber rund
50 Mio in genau der Kopierschleife, die Hypothese 1 beseitigt hat; es bleiben
rund 40 Mio (~4 %).

Zurueckgestellt, weil die verbleibenden Faelle **nicht sicher zu streichen**
sind: sie betreffen schmale Breiten.

```
mov   BYTE PTR [rbp-0xae1], r11b
movzx r11d, BYTE PTR [rbp-0xae1]      ; NUR dann ueberfluessig, wenn r11
                                      ; bereits nullerweitert ist
```

`mov eax, DWORD PTR [X]` nullt die oberen 32 Bit von `rax`; ein Streichen
waere nur mit einer mitgefuehrten „ab welchem Bit ist das Register garantiert
null"-Information korrekt. Machbar, aber ein Textnachpass mit
Halbwissen ueber Registerinhalte ist genau die Bauform, die in Runde 40 den
Miscompile erzeugt hat. Ohne Not nicht.

Ausserdem sitzt der Loewenanteil dieser Paare an **Blockgrenzen**
(`mov BYTE PTR [X], r11b` / Label / `movzx r11d, BYTE PTR [X]`), also an einem
echten Zusammenfluss zweier Pfade — das ist ein Phi in Speicherform und
gehoert nach `mem2reg`, nicht in einen Nachpass. 1.022 der 3.427 Label im
Tokenizer (29,8 %) sind allerdings **gar kein Sprungziel**; sie loeschen den
Zustand des Nachpasses ohne Grund. Das ist ein sauberer, kleiner Hebel fuer
die naechste Runde.

### Bereichspruefung je Byte in `dekodiere`

`dekodiere` kostet 192.243.336 Ir fuer 4.931.819 Byte = 39 Instruktionen je
Byte, obwohl der ASCII-Schnellweg nur laden, vergleichen und schreiben muss.
Ein Teil davon ist die Bereichspruefung in `byte_bei`, die der Aufrufer
eigentlich schon kennt. Nicht angefasst: `dekodiere` steht wortgleich in
`tokenize_bench.fi` und `tokenize_main.fi`, eine Aenderung muss beide treffen
und ihre Semantik bei abgeschnittener Eingabe (Bytes jenseits des Endes lesen
sich als 0) exakt erhalten. Lohnt, war aber fuer das Ziel nicht noetig.

## 6. Profil nachher (realweb, 965.887.079 Ir)

```
          SELBST   ANTEIL         INKLUSIV  FUNKTION
     526.900.669   54,55%      762.188.529  tokenizer__tokenize
     192.243.336   19,90%      192.243.336  dekodiere
     105.033.364   10,87%      105.145.676  tokens__tok_attr_value_push
      47.342.985    4,90%       47.469.496  tokens__sink_fehler_bei
      18.589.583    1,92%       18.590.675  tokens__tok_attr_name_push
      11.641.019    1,21%       16.293.464  tokens__tok_attr_finish
      11.448.891    1,19%      965.887.075  main
```

`tokenize` ist mit 54,55 % jetzt klar der einzige grosse Posten:
526.900.669 Ir auf 4.917.779 Codepunkte sind **107 Instruktionen je
Zeichen**. Darin steckt der naechste Hebel, und er ist immer noch der aus
Runde 40 — zu wenige Register fuer eine Funktion mit einem Rahmen von
43.104 Byte.

## 7. Abnahme

| Pruefung | Ergebnis |
|---|---|
| `bash ./test.sh` | **PASS 676/676** (Basis 673/673; +3 durch `tests/331_stapelargumente.fi` in drei Optimierungsstufen) |
| `cargo test --release` (Modul-Tests) | 142/142 |
| `bash tools/selbst_vergleich.sh` | **197 gleiches Verhalten, 0 abweichend, 0 fehlerhaft** (Basis 196; +1 durch die neue Testdatei), CODEGEN FEHLT: 0 |
| `bash tools/fixpunkt.sh` | **Stufe 2 == Stufe 3, zeichengleich, 309.468 Zeilen** |
| `bash tools/tokenizer/run.sh` | **6810/6810 = 100,00 %** |
| Lexer/Parser/Layout/Sema/FIR-Vergleich | unveraendert (je 1 bekannte und benannte Abweichung, Layout 0) |

Messwerkzeuge dieser Runde: `tools/tokenizer/profil.py` (neu),
`.r43/messe.sh` (Arbeitsverzeichnis, nicht eingecheckt),
`tools/tokenizer/durchsatz.sh`, `valgrind --tool=callgrind`.

## 8. Offene Punkte

1. **`tokenize` mit 107 Instruktionen je Zeichen.** Der Rahmen ist
   43.104 Byte gross, es gibt neun vergebbare Register. Hier liegt
   Intervall-Splitting (Runde 40/41) weiterhin richtig — jetzt mit
   klarem Anteil: 54,55 % des Messlaufs.
2. **Label ohne Sprungziel loeschen** (1.022 von 3.427). Kostet nichts,
   verlaengert die Reichweite jedes Nachpasses und ist trivial zu belegen.
3. **Schmale Reloads** (~4 %) — braucht eine Nullerweiterungs-Verfolgung im
   Nachpass oder, besser, ein `mem2reg`, das Bool-Zellen an Zusammenfluessen
   promoviert.
4. **`dekodiere`** (19,90 %): Bereichspruefung je Byte, in beiden Treibern
   gleichzeitig zu aendern.
5. **`out_wort` mit zehn Parametern** ist jetzt nicht mehr teuer, aber
   immer noch eine Signatur, die Zeichen einzeln durchreicht. Ein
   Zeichenkettenliteral waere billiger und lesbarer.
