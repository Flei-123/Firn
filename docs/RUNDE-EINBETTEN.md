# Runde EINBETTEN — Funktions-Inlining im Firn-Übersetzer

Zweig `einbetten`, Arbeitsbaum `/root/firn-einbetten2`, abgezweigt von
`main` / `2a20c514`. Nicht nach `main` geführt. 14.09.2026.

---

## 0. Das Wichtigste zuerst: die Prämisse des Auftrags stimmt nicht

Der Auftrag sagt wörtlich: *„escape.rs, inliner gibt es NICHT"* und
*„Firn hat kein Inlining — deshalb kostet jeder Helferaufruf den vollen
Aufrufpreis, und deshalb verpufft der JIT."*

Beides ist nachprüfbar falsch, und das ist der eigentliche Ertrag dieser
Runde. Der Reihe nach:

```
  1. compiler/src/inline.rs STEHT SEIT RUNDE 92/94 IN main.
     537 Zeilen. Kann mehrere return-Pfade, Rekursionssperre,
     Phi-Umschluesselung, Debug-Positionen je Befehl. In opt.rs als
     Durchgang "inline" verdrahtet. Ein ganzer frueherer Zweig
     `inline` existiert dazu (nicht gemerged).

  2. ER LAEUFT NUR BEI release-*. Der Durchgang ist
     debug_preserving:false; die Vorgabestufe ist Level::DevFast.
     Die Runde JIT hat also mit AUSGESCHALTETEM Inlining gemessen
     und daraus geschlossen, Firn koenne kein Inlining.

  3. DIE 25,56 ns SIND NICHT DER AUFRUF. Ein echter Aufruf eines
     kleinen Rumpfes kostet GEMESSEN ~1,3 ns. Der Bericht der Runde
     JIT liegt damit um den Faktor 20 daneben.

  4. WAS ES WIRKLICH IST: die GC-SCHREIBSCHRANKE in `jh_set`.
     Isoliert gemessen 15,4 bis 19,0 ns fuer EINEN Zeigerschreib-
     zugriff. Das ist praktisch die ganze Luecke.

  5. FOLGE: #[inline] auf die JIT-Helfer zu setzen macht den D-Weg
     LANGSAMER, nicht schneller. Auch das ist gemessen, nicht
     vermutet. Die Runde verfehlt damit ihr wortwoertliches Ziel --
     und zwar, weil das Ziel auf einer falschen Ursache beruhte.
```

Was **wirklich** gefehlt hat, sind zwei andere Dinge — ein Attribut, mit dem
der Programmierer den Einbau erzwingen oder verbieten kann, und die
Blockplatzierung. Beide sind gebaut und beide sind gemessen.

---

## 1. Wie der Befund entstanden ist

### 1.1 Die Grundlinie reproduziert

`tools/jit/ruf_echt_main.fi` aus der Runde JIT, unverändert gebaut:

```
  A Deuter: Verteilung + add_values             259,87 ns
  B JIT:    nur add_values, keine Verteilung    225,17 ns
  D JIT-Weg: ueber den Helfer jh_add            252,82 ns
  C nur Verteilung, KEIN Aufruf                  23,87 ns
```

Die absoluten Zahlen liegen höher als im Bericht (200,54 / 166,28 / 191,84) —
die Maschine trägt Fremdlast. Die **Differenz**, auf die es ankommt, trifft:
`D-B = 27,65 ns` hier gegen `25,56 ns` dort.

### 1.2 Der erste Widerspruch: der Inliner läuft ja

`FIRNC_INLINE_DEBUG=1` zählt die Einbauten:

```
  dev-fast (Vorgabe) :     0 Einbauten
  release-fast       : 2000 Einbauten   (die Obergrenze MAX_INLINES)
```

Und bei `release-fast` steht in der Liste ausdrücklich
`inline: messen <- interp__jh_add (26 insts, 8 blocks)`. Der Helferrumpf
wird also längst eingebaut. Trotzdem ändert sich fast nichts:

```
  release-fast:  A 264,21   B 238,42   D 260,50   ->  D-B = 22,08 ns
```

Im Maschinencode ist der Aufruf nachweislich weg
(`objdump` zeigt in `messen` keinen `call …jh_add` mehr). **Wenn der Aufruf
verschwindet und die Zeit bleibt, ist der Aufruf nicht die Ursache.**

### 1.3 Was ein Aufruf wirklich kostet

`/root/einbetten-mess/kosten_main.fi`: derselbe kleine Rumpf einmal über
einen Aufruf, einmal von Hand eingebettet, 200 Mio. Durchgänge.

```
  dev-fast (Inlining aus):   ueber Aufruf 2,36 ns   von Hand 1,07 ns
  release-fast            :  ueber Aufruf 1,07 ns   von Hand 1,07 ns
```

Ein Aufruf kostet also rund **1,3 ns**, und bei `release-fast` ist er ohnehin
schon weg. Für 25,56 ns braucht es etwas anderes.

### 1.4 Die Zerlegung — und der Schuldige

`jh_add` ist kein „ein Aufruf". Der Rumpf ruft selbst weiter:

```
  jh_add -> ctx_of
         -> jh_reg, jh_reg
         -> add_values
         -> jh_set -> __gc_barrier     <-- die Schreibschranke
```

`jh_set` schreibt durch einen Zeiger in ein Halden-Feld. `gc_lower.rs`
(`hook_assign`) setzt dafür zwingend die Einfügeschranke — richtig so, genau
hier macht ein JIT den Sammler sonst blind.

Vier getrennte Binäre, je EIN Weg pro Programm
(`/root/einbetten-mess/weg{B,D,E,F}_main.fi`), verschränkt gemessen:

| Weg | was er tut | ns je Befehl |
|---|---|---:|
| **B** | nur `add_values` (Idealfall) | **239,75** |
| **D** | über den Helfer `jh_add` | **258,31** |
| **E** | `jh_add`-Rumpf von Hand, **ohne** das Zurückschreiben | **232,50** |
| **F** | wie E **+ genau ein Zeigerschreibzugriff** | **251,51** |

```
  D - B  = 18,6 ns   die Luecke, um die es geht
  F - E  = 19,0 ns   EIN Zeigerschreibzugriff mit Schranke
  E      < B         der ganze uebrige Helferrumpf kostet NICHTS
```

**Das geht ohne Rest auf.** Der gesamte Helferrumpf — zwei Registerladungen,
`ctx_of`, der Befehlszähler, zwei Abfragen — ist schneller als der Idealfall
B. Die Lücke ist zu 100 % die Schreibschranke.

Die Firn-Laufzeit sagt es an dieser Stelle selbst (`lib/gc/gc.fi`, bei
`__gc_barrier`):

> *„Der Teil der Schranke, der NUR während eines Zyklus läuft, liegt in einer
> eigenen Funktion, damit `__gc_barrier` klein genug bleibt, um eingebettet
> zu werden (gemessen: sonst kostet jeder Zeigerschreibzugriff einen echten
> Aufruf)."*

Genau dieser Einbau findet bei `dev-fast` nicht statt.

### 1.5 Warum `#[inline]` auf `jh_add` **schadet**

Mit `#[inline]` auf `jh_add`/`jh_reg`/`jh_set` gemessen:

```
  ohne #[inline] :  B 239,75   D 258,31   ->  D-B = 18,6 ns
  mit  #[inline] :  B 237,48   D 270,87   ->  D-B = 33,4 ns
```

Der Grund steht im Maschinencode. Der Einbau entfernt **einen** Aufruf
(`jh_add`) und legt **drei** frei, die vorher in dessen eigenem Rahmen
steckten:

```
  messen OHNE Einbau: ... 1x jh_add ...
  messen MIT  Einbau: ... 1x ctx_of  2x add_values  1x __gc_barrier ...

  Stapelzugriffe in messen : 341 -> 417
  Rahmengroesse            : 0xc40 -> 0xe20  (+480 Oktette)
```

Ein Rumpf, der selbst ruft, verschiebt die Aufrufe nur nach oben und
vergrößert dabei den Rahmen des Aufrufers. **Inlining hilft dort, wo der
Rumpf rechnet — nicht dort, wo er weiterruft.**

---

## 2. Was gebaut wurde

### 2.1 `#[inline]` und `#[no_inline]` (Commit 1)

`fir::Func` bekommt `inline_hint: Option<bool>`:

* `None` — der Übersetzer entscheidet nach seiner Größenregel (wie bisher).
* `Some(true)` — `#[inline]`: die **Größengrenzen entfallen**.
* `Some(false)` — `#[no_inline]`: **nie** einbetten.

`attrs.rs` meldet beide an, `core.rs::inline_hint` liest sie, `lower.rs`
setzt sie (`// HOOK einbetten`). Beide zugleich ist ein Widerspruch; dann
gewinnt das **Verbot** — die sichere Seite.

In `inline.rs` trennt die neue Funktion `darf_grundsaetzlich()` sauber die
**harten Sperren** von der Größenregel. `#[inline]` hebt die harten Sperren
**nie** auf, denn das sind Richtigkeitsfragen, keine Geschmacksfragen:

| Sperre | warum sie bleibt |
|---|---|
| `#[constant_time]` / `secret` | die Prüfung im Codeerzeuger arbeitet **je Funktion** (SPEC §9.2). Eingebettet in einen Aufrufer ohne die Marke fiele sie weg — eine Zeitseitenkanal-Zusage wäre still gebrochen. |
| `#[interrupt]` | eigene Aufruffolge, endet mit `iretq` statt `ret` (Runde 52). Gehört nicht in einen gewöhnlichen Rahmen. |
| Rekursion (auch indirekt) | ein Einbau rollt eine Ebene aus und verschiebt ihre Rahmen in den Aufrufer. Code, dessen Wirkung an der **Stapeltiefe** hängt (`__gc_scrub_deep`, das Stapelschrubben des konservativen Sammlers), verliert dabei seine Wirkung — in Runde 37 gemessen, `tests/520_gc_weak.fi` starb mit Exit 6. |
| unfertiger Block / Blocknummern nicht der Reihe nach | dann stimmt `blockmap` nicht. |

Auch die **Aufrufergrenze** (`MAX_CALLER_INSTS`) sperrt einen ausdrücklich
verlangten Einbau nicht mehr aus — sonst hinge `#[inline]` davon ab, wie groß
der Aufrufer zufällig ist.

### 2.2 Der Durchgang für die Vorgabestufe (Commit 1)

Der springende Punkt aus Abschnitt 0.2: eine Zusage an den Programmierer darf
nicht davon abhängen, mit welchem Schalter gebaut wird — sonst misst man,
wie die Runde JIT, still etwas anderes, als man gebaut hat.

Neuer Durchgang **`inline-verlangt`** (`debug_preserving: true`), der auf
**jeder** Stufe außer `dev` läuft und **ausschließlich** die Stellen einbaut,
die der Programmierer selbst mit `#[inline]` ausgezeichnet hat. Die
Größenregel bleibt wie bisher `release-*` vorbehalten — sie macht den
Aufrufstapel unlesbar, und `dev-fast` ist die Stufe, auf der man mit dem
Fehlersucher arbeitet.

**Schalter** (alle schon vorhanden, jetzt auch für den neuen Durchgang):

```
  --no-pass=inline-verlangt    nur die ausdruecklichen Einbauten aus
  --no-pass=inline             nur die Groessenregel aus
  --no-opt                     alles aus
  --list-passes                zeigt beide Durchgaenge mit ihrer Beschreibung
```

### 2.3 Die Blockplatzierung (Commit 3) — der zweite echte Befund

`Func::add_block` hängt nur **an**. Ein eingebauter Rumpf landete damit hinter
**allen** anderen Blöcken, auch wenn die Aufrufstelle in der ersten Schleife
steht. Das ist kein Schönheitsfehler:

`regalloc.rs` bildet die Lebendintervalle als
`[kleinste Position, größte Position]` über die **lineare** Blockfolge
(`live.block_start` / `live.block_end`). Liegt der Rumpf am Ende, spannt das
Intervall jedes Werts, der in den Rumpf hinein und wieder heraus lebt, über
die **ganze** Funktion — auch über fremde Schleifen, mit denen er nichts zu
tun hat. Dort kollidiert er mit deren Werten und wird ausgelagert.

Gemessen, dieselbe Rechnung, derselbe Einbau, **nur die Blockentfernung
unterscheidet sich**:

| Fall | ohne Einbau | mit Einbau | |
|---|---:|---:|---|
| `iso_main.fi` — **eine** Schleife, Rumpf landet daneben | 2,35 ns | **1,08 ns** | Faktor **2,2 besser** |
| `kosten_main.fi` — **zwei** Schleifen, Rumpf landet hinter der zweiten | 2,37 ns | **3,44 ns** | **schlechter** |

`bloecke_umsortieren()` zieht Rumpf und Fortsetzungsblock unmittelbar hinter
den Aufrufblock. Weil die Blocknummer überall im Übersetzer ein **Index** ist
(`b.id as usize == i` in `mem2reg.rs`, `opt.rs`, `regalloc.rs`,
`rangecheck.rs`), werden Nummer, Sprungziele **und die Blockangaben in jedem
`phi`** zusammen umgeschrieben. Stimmt die Blockzahl nicht, wird
vorsichtshalber gar nicht umsortiert.

**Danach:**

```
  kosten_main.fi (zwei Schleifen), dev-fast:
     ohne Einbau                2,36 ns
     mit Einbau, VORHER         3,44 ns   (schlechter)
     mit Einbau, NACHHER        1,07 ns   <- trifft den Idealwert
```

Der von Hand eingebettete Vergleichswert im selben Programm ist 1,07 ns. Der
Einbau trifft ihn jetzt **genau**, statt ihn um das Dreifache zu verfehlen.

---

## 3. Was bewusst NICHT gemacht wurde

**Fehler-Unions, `defer`/Aufräumcode und GC-Wurzeln sind nicht angefasst**
worden, und zwar begründet:

* **Fehler-Unions** brauchen nichts Eigenes. `E!T` ist zum FIR-Zeitpunkt
  längst ein gewöhnlicher Wert (`lower_errors.rs`), `try`/`catch` sind
  gewöhnliche Blöcke und Sprünge. Der vorhandene Einbau kopiert sie wie jedes
  andere Kontrollflussgerüst mit; die Messwege D/E/F oben laufen **alle**
  durch den `AllocError!`-Kanal und rechnen richtig.
* **GC-Wurzeln** sind genau die Stelle, an der Inliner erfahrungsgemäß
  kaputtgehen — Runde 37 hat das mit `tests/520_gc_weak.fi` und Exit 6 schon
  einmal bezahlt. Die bestehende Sperre (nichts Selbst-Erreichbares einbetten,
  damit das Stapelschrubben seine Tiefe behält) bleibt **unverändert** in
  Kraft, und `#[inline]` hebt sie nicht auf.
* **Debug-Info** war bereits erledigt: Runde 94 hat `Inst::like` mit der
  Position des **Aufgerufenen** eingeführt, damit der Fehlersucher nicht die
  Zeile des Aufrufers für fremden Code meldet. Echte
  `inlined_subroutine`-Einträge in DWARF wären eine eigene Runde; solange sie
  fehlen, ist der neue Durchgang **absichtlich** auf das beschränkt, was der
  Programmierer selbst angeordnet hat.

---

## 4. Die Zahlen

### 4.1 Der eigentliche Nachweis — `ruf_echt`

Der Auftrag verlangt: *„Mit Inlining muss der Weg D in Richtung B rutschen.
Wenn D-B nicht deutlich kleiner wird als 25,56 ns, hat die Runde ihr Ziel
verfehlt — dann schreib das hin statt es zu bemänteln."*

**Hier steht es hin: D-B wird nicht kleiner. Es wird größer.**

```
  ohne #[inline] :  B 239,75   D 258,31   ->  D-B = 18,6 ns
  mit  #[inline] :  B 237,48   D 270,87   ->  D-B = 33,4 ns
```

Und der Grund ist gemessen und nicht vermutet (Abschnitt 1.4/1.5): die Lücke
ist die **GC-Schreibschranke**, nicht der Aufruf. `F - E = 19,0 ns` für
**einen** Zeigerschreibzugriff deckt die 18,6 ns der Lücke vollständig ab. Ein
Inliner kann diese Kosten nicht wegnehmen — die Schranke muss stehen, sonst
wird der Sammler blind.

**Das Ziel der Runde, so wie es formuliert war, ist damit nicht erreichbar,
und zwar aus einem sachlichen Grund, nicht aus Aufwandsgründen.**

### 4.2 Wo Inlining sehr wohl wirkt

Da, wo ein Rumpf **rechnet**, statt weiterzurufen:

```
  iso_main.fi,   dev-fast:  2,35 ns -> 1,08 ns   = Faktor 2,2
  kosten_main.fi,dev-fast:  2,36 ns -> 1,07 ns   = Faktor 2,2
```

Das ist der Fall, für den `#[inline]` da ist — und er funktioniert jetzt auch
auf der Vorgabestufe.

### 4.3 Größe

```
  Certus web1, gebaut mit main        : 8.383.448 Oktette
  Certus web1, gebaut mit einbetten   : 8.383.448 Oktette
  Unterschied                         : 0
```

Das ist **kein Zufall und der beste Beleg für Rückwärtskompatibilität**: in
den Quellen von Certus steht kein einziges `#[inline]`, also hat der neue
Durchgang nichts zu tun, und die Größenregel bei `release-*` verhält sich
unverändert. Wer die Marke nicht benutzt, merkt von dieser Runde nichts.

Der Preis an Platz entsteht erst mit der Marke und ist dann örtlich: je
Einbaustelle der Rumpf ein weiteres Mal. Für das „brutal leichtgewichtige"
OrientOS-Abbild heißt das: solange dort kein `#[inline]` steht, wächst
nichts.

---

## 5. Korrektheit

| Prüfung | Ergebnis |
|---|---|
| Modultests des Übersetzers (`cargo test --release`) | **275 / 275 grün** (270 vorher + 5 neue) |
| `tests/` + `tests/opt/` + `examples/` in **allen vier** Baustufen (`release-fast`, `--no-opt`, `dev-fast`, `release-safe`) | **1304 / 1304 grün, 0 rot** |
| `tests/neg/` — 193 Programme müssen mit Meldung scheitern | **193 / 193 grün**, keine Rust-Panik |
| Certus `web1` baut (8,4 MB Binär, der ganze Browser) | **grün**, byte-gleich groß wie mit `main` |
| Attributwache `only_must_consume_is_implemented` | angeschlagen wie vorgesehen, Liste + Merkbuch nachgezogen |

Fünf neue Modultests halten fest, was die Marken dürfen und was nicht:

```
  no_inline_verbietet_den_einbau             das Verbot gilt auch fuer
                                             winzige Ruempfe
  inline_hebt_die_groessengrenze_auf         ein Rumpf ueber
                                             MAX_CALLEE_INSTS wird genommen
  inline_hebt_die_harten_sperren_nicht_auf   #[constant_time] bleibt
                                             gesperrt (SPEC 9.2)
  inline_bricht_die_rekursionssperre_nicht   Rekursion bleibt gesperrt
  nur_verlangt_nimmt_nur_markierte           der Vorgabe-Durchgang laesst
                                             unmarkierte Aufrufe stehen
```

### Offen geblieben — ehrlich benannt

* **OrientOS baut nicht** — aber **nicht wegen dieser Runde**. Der Bau bricht
  mit `cannot read 'kernel/user/paint/canvas.fi'` ab; das Verzeichnis
  `kernel/user/paint/` existiert im osum-Repo (`3660b24`) überhaupt nicht und
  hat laut `git log --all` nie existiert. **Gegenprobe mit dem unveränderten
  Übersetzer aus `main`: derselbe Fehler, Zeile für Zeile.** Der Schaden liegt
  im osum-Repo, nicht im Zweig `einbetten`. Damit konnten auch
  `tools/usbimg/run.sh` (Bootprobe) und die Abbildgröße nicht gemessen werden.
* **Der Selbstbau (`firnc` baut sich selbst)** steht noch aus — siehe unten.
* **`tools/check-ui.sh`** wurde nicht gefahren — das Certus-Binär baut, die
  Bildprüfung braucht eine X11-Sitzung.
* **DWARF `inlined_subroutine`** fehlt weiterhin (siehe Abschnitt 3).

---

## 6. Was daraus folgt

Der Auftrag stellte die Frage: *„Wie bekommt Firn Inlining, damit der JIT
seinen Helferaufruf loswird?"* Die richtige Antwort ist nicht die erwartete:

```
  1. FIRN HAT INLINING, seit Runde 92/94. Es war nur auf der
     Vorgabestufe aus. Das allein erklaert den ganzen Befund der
     Runde JIT.

  2. DER HELFERAUFRUF WAR NIE DAS PROBLEM. Ein Aufruf kostet 1,3 ns,
     nicht 25,56. Die Luecke ist die GC-SCHREIBSCHRANKE -- 19,0 ns
     fuer EINEN Zeigerschreibzugriff, isoliert gemessen.

  3. WER DEN JIT SCHNELLER MACHEN WILL, muss an die Schranke oder an
     die Anlage im Haufen, nicht an den Inliner. Die Runde JIT hat
     das in ihrem eigenen Abschnitt 9 schon richtig aufgeschrieben:
     die Anlage im Haufen kostet 134 ns je Schleifendurchgang --
     siebenmal mehr, als ein perfekter Baseline-JIT je sparen
     koennte.

  4. WAS DIESE RUNDE TROTZDEM BRINGT: eine Marke, mit der man den
     Einbau erzwingen und verbieten kann, auf JEDER Baustufe -- und
     eine Blockplatzierung, ohne die ein Einbau in einer Funktion
     mit mehreren Schleifen das Programm LANGSAMER macht (3,44 ns
     statt 1,07 ns). Der zweite Punkt ist ein echter Fehler im
     vorhandenen Inliner, den erst die Messung sichtbar gemacht hat.
```

**Die Lehre ist dieselbe wie in der Runde JIT selbst: eine Ursache, die man
nicht gemessen hat, ist eine Vermutung.** Der Bericht der Runde JIT hat
25,56 ns einem Aufruf zugeschrieben, ohne den Aufruf je einzeln zu messen —
und der Auftrag dieser Runde hat die Vermutung als Befund übernommen. Eine
einzige Messung (`F - E`) hat gereicht, um sie umzustoßen.

---

## 7. Reproduktion

```bash
# Uebersetzer
cd /root/firn-einbetten2/compiler && cargo build --release && cargo test --release

# Was ein Aufruf wirklich kostet (1,3 ns)
cd /root/einbetten-mess
firnc --opt-level=dev-fast     -o k kosten_main.fi && ./k   # Aufruf / von Hand
firnc --opt-level=release-fast -o k kosten_main.fi && ./k

# Die Wirkung von #[inline] auf der Vorgabestufe (Faktor 2,2)
firnc --opt-level=dev-fast                        -o iso  iso_main.fi && ./iso
firnc --opt-level=dev-fast --no-pass=inline-verlangt -o iso2 iso_main.fi && ./iso2

# Die Zerlegung: B / D / E / F -- der Nachweis, dass es die Schranke ist
cd /root/certus-jit
for W in wegB wegD wegE wegF; do
  FIRNLIB=/root/jit-firnlib firnc -o /root/einbetten-mess/$W \
      /root/einbetten-mess/${W}_main.fi
done
cd /root/einbetten-mess && for i in 1 2 3; do ./wegE; ./wegF; done   # F-E = die Schranke

# Certus baut (8,4 MB)
cd /root/certus-jit && FIRNLIB=/root/jit-firnlib \
  firnc -o .bau/einbetten/web1 lib/browser/web1_main.fi
```

Die Messprogramme liegen bewusst außerhalb des Repos in
`/root/einbetten-mess/` — sie sind erzeugt und jederzeit reproduzierbar.

**Warum nicht `ruf_echt_main.fi` direkt?** Dort stehen fünf Messschleifen in
**einer** Funktion `messen`. Die spillt schon ohne jeden Einbau 2466-mal, und
ein eingebauter Rumpf verschärft das, statt zu helfen (Abschnitt 1.5). Für
eine Aussage über **einen** Weg braucht es **ein** Binär je Weg — sonst misst
man die Registernot des Prüfstands und nicht die Sache.
