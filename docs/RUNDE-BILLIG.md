# Runde BILLIG (Firn-Seite) — der Registerzuteiler für aarch64

Zweig `billig`, abgezweigt von `a58d17dd5` — dem Stand, den Certus in
`vendor/firn/COMMIT` festnagelt. Arbeitsbaum `/root/firn-billig`.
05.09.2026.

Diese Datei beschreibt den **Übersetzerteil** der Runde BILLIG. Die Runde
hatte drei Posten aus der JIT-Machbarkeitsstudie vom selben Tag; die
anderen zwei (die Baustufe, mit der Certus gebaut wird, und der
Android-Test für ausführbaren Speicher) sind Certus-Themen und stehen im
vollen Bericht `certus:docs/RUNDE-BILLIG.md`. Kurzfassung davon am Ende.

```
   codegen_a64.rs sagte in seinem eigenen Kopfkommentar seit Runde 80:

       "every FIR value gets its own 8-byte slot in the frame ...
        That is not fast, and it is not meant to be -- it is checkable.
        The linear scan of regalloc.rs is a second, separate step that
        x86 got in round 43; aarch64 can get it the same way later."

   Diese Runde ist das "later".

   ERGEBNIS, in Zahlen:

     Certus' Maler (lib/paint/b3_main.fi), fuer aarch64 uebersetzt
        statische Befehle   572 399  ->  431 234        -24,7 %
        Binaerdatei       2 646 224  ->  2 081 552      -21,3 %
        Seitenaufbau unter qemu                    1,29 x bis 1,45 x
        Bild auf allen drei Seiten                        BITGLEICH

     Elf Baenke (bench/firn/, dieselben wie Runde 43 und 90)
        statische Befehle    13 859  ->   11 488        -17,1 %
        Durchsatz unter qemu, geom. Mittel                 1,384 x
        Ergebnis identisch in allen elf Baenken                 JA

     Korrektheit
        tools/aarch64/run.sh: 304 von 304 vergleichbaren Faellen auf
        beiden Maschinen identisch (100 %), 0 DIFFERENT
        x86: der erzeugte Assemblertext ist BYTEGLEICH derselbe

   Zum Vergleich: Runde 43 mass fuer x86 -26,15 % ausgefuehrte Befehle.
```

---

## 1. Die Vorfrage: wie x86-verdrahtet ist `regalloc.rs`?

Der Auftrag verlangte, das zuerst zu prüfen und mit Belegen zu beantworten,
bevor irgendetwas gebaut wird. Nachgezählt an den Funktionsgrenzen von
`compiler/src/regalloc.rs` (5005 Zeilen):

| Teil | Zeilen | maschinenabhängig? |
|---|---:|---|
| Analyse: `compute_live`, `exact_crossings`, `promotable_cells`, `loop_depth`, `loop_ranges`, `Bits`, Intervallbildung, linearer Scan (`allocate`) | ~1580 | **nein** — reine FIR-Analyse |
| Registerbeschreibung: `CALLEE_SAVED`, `TEMP_REGS`, `ARG_SPARE`, `DIV_SPARE`, `reg_bit`, `inst_clobbers` | ~150 | ja, aber es sind **Tabellen** |
| Emission: `rn`, `Address::text`, `foldable_addresses`, `emit_with`, `emit_block`, `emit_inst`, `emit_bin`, `emit_div_const`, `checked_direct`, `descriptor_peephole` | ~2900 | **ja — x86-Assemblertext** |
| Tests | ~400 | ja |

Registernamen im selben Bestand gezählt: `rax` 230 ×, `rdx` 111 ×,
`rcx` 105 ×, `rbp` 55 ×, `r9` 38 ×, `r8` 33 ×.

**Die Antwort ist ein halbes Ja.** Der Kern arbeitet wirklich über eine
abstrakte Wertmenge, und die Vermutung der Aufgabe — *„dann ist es vor allem
eine Frage der Registerbeschreibung"* — stimmt für diesen Kern. Aber
`regalloc.rs` gibt keine Belegung zurück, die ein beliebiger Rückwärtsteil
benutzen könnte: `pub fn emit_func_ra(e, f) -> Option<String>` **ist** der
Rückwärtsteil, und zwei Drittel der Datei sind x86-Assemblertext.

Eine zweite Zielmaschine in dieselbe Datei zu ziehen hätte geheissen, den
Emitter zu abstrahieren — und der Emitter ist genau das, was ARM64 schon
hat.

**Der Ausweg, und er ist der Grund, warum das in einem Tag ging:**

```
$ grep -n '\.off(' compiler/src/codegen_a64.rs
337:    let off = fr.off(v);        <- EINE Stelle, in fn at()
```

Jeder Wertzugriff des A64-Rückwärtsteils läuft durch `at()`, und `at()` wird
nur von `load_full`, `load_ext`, `store_dst`, `load_fp`, `store_fp` und den
zwei SIMD-Funktionen gerufen. Eine Handvoll Funktionen registerbewusst zu
machen erledigt den ganzen Rückwärtsteil.

**Aufwandsschätzung vorher: 400–700 Zeilen. Tatsächlich: `regalloc_a64.rs`
330 Zeilen, plus rund 180 geänderte Zeilen in `codegen_a64.rs`.**

---

## 2. Sechs Register, und die Zahl kommt vom Sammler

AAPCS64 hat zehn aufrufergesicherte Allzweckregister, `x19`–`x28`.
Ausgeteilt werden **sechs**, `x19`–`x24`:

> `codegen_a64::emit_gc_addr` schreibt bei jedem `Op::GcAddr { regs: true }`
> **x19–x24** in den Zustandsblock des Sammlers, weil SPEC §3.5.3 einen
> konservativen Lauf über Stapel **und Register** verspricht. Der Block hat
> Platz für sechs Wörter (`gc::REG_SAVE_OFF = 3968`) — dieselben sechs, die
> `codegen_x86::emit_gc_addr` mit `rbx`, `rbp`, `r12`–`r15` füllt.

Ein Zeiger in `x25`–`x28` wäre für diesen Lauf unsichtbar. Der Sammler gäbe
ein noch lebendes Objekt frei, und der Fehler zeigte sich als kaputter
Haufen an einer ganz anderen Stelle — die gefährlichste Fehlerart, die es
gibt. **Sechs Register, die der Sammler sieht, sind mehr wert als zehn, die
er nicht sieht.** Den Block zu verbreitern ist eine eigene Änderung an drei
Dateien (`gc.rs`, der eingebettete Laufzeittext, `codegen_x86.rs`),
abgesichert durch `gc::tests::offsets_match_the_runtime`, und wird hier
nicht nebenbei mitgenommen.

**Der eine Punkt, an dem ARM64 einfacher ist als x86:** weil alles
Ausgeteilte aufrufergesichert ist, braucht kein Intervall eine
Kreuzungsanalyse. `bl`, `svc` und jede Hilfsfolge dieses Rückwärtsteils
lassen `x19`–`x24` in Ruhe. Auf x86 dürfen `r11`, `r10`, `rsi`, `rdi` und
`rdx` nur an Intervalle gehen, die nichts kreuzen — dafür stehen die
90 Zeilen `exact_crossings` aus Runde 87.

### Die drei Sperren

* **`#[interrupt]`.** `INT_SAVE_A64` rettet `x0`–`x18` und `x30` und
  verlässt sich wörtlich auf *„x19-x28 are callee-saved and this backend
  never hands them out"*. Diese Runde macht den Satz falsch, also bleiben
  Behandlungsroutinen auf dem alten Weg.
* **`asm`-Blöcke.** Der A64-Inline-Assembler (Runde ARM-FREESTANDING) lässt
  jeden Registernamen als Operand oder in der Zerstörungsliste zu,
  `x19`–`x28` eingeschlossen (`core.rs:184`). Eine Funktion mit `asm` fällt
  ganz auf den alten Weg zurück — dieselbe Linie, die
  `regalloc.rs::unsupported_basic` auf x86 zieht.
* **`secret` (SPEC §9.2), `f32`/`f64`/`v128`.** Bleiben im Rahmen.

Ausserdem: die Nummerierung der Blöcke muss `id == index` erfüllen (dieselbe
Vorbedingung wie `regalloc::allocate`), und bei `nv * nb > 8 000 000` bleibt
es beim Stapelmodell.

---

## 3. Drei Stufen, jede einzeln messbar

### Stufe 1 — der Wert liegt im Register

`load_full`, `load_ext` und `store_dst` fragen zuerst die Belegung. Aus
`ldr x9, [sp, #48]` wird `mov x9, x22`, aus `str x9, [sp, #48]` wird
`mov x22, x9`. Gleiche Befehlszahl, aber keine Speicherberührung mehr.

Dazu die **beförderte Zelle** (`promotable_cells` aus `regalloc.rs`, Schritt
2): ein `alloca` von höchstens acht Oktetten, dessen Adresse nie den
direkten Operanden eines `load`/`store` verlässt, IST ein Register.
**Sie ist am Ende dieser Runde wieder ausgeschaltet — Abschnitt 5b sagt,
warum, und was es kostet.** Gebaut bleibt sie; `FIRN_A64_CELLS=1` schaltet
sie zum Messen wieder an. Der
`Op::Alloca` selbst erzeugt dann gar keinen Code mehr.

Beim Lesen aus einer Zelle wird mit **Nullen** verbreitert (`uxtb`/`uxth`/
`mov w`), weil das die Regel ist, die `ldrb`/`ldrh`/`ldr w` aus dem Speicher
befolgen — ein als `u8` abgelegtes −1 muss als 255 zurückkommen und nicht
als −1. Geschrieben wird dagegen das volle Wort, genau wie `strb` es tut.

### Stufe 2 — die Konstanten fressen sonst das Registerfeld

Der erste Lauf zeigte die Falle sofort. Rauchprobe:

```firn
while i < n { if i % 3 == 0 { s = s + i * 2 } else { s = s - i }; i = i + 1 }
```

Belegung nach Stufe 1: `x24` = Parameter `n`, `x23` = Konstante 0,
`x22` = Konstante 3, `x21` = Konstante 2, `x20` = Konstante 1, `x19` = `s`
— **und der Schleifenzähler `i` lag auf dem Stapel.**

Eine Konstante, die `imm_into` in **einem** Befehl schreibt
(`mov r, xzr` / `movz` / `movn`), bekommt jetzt weder Platz noch Register,
sondern wird an der Gebrauchsstelle neu gebaut (`cheap_const`). Das ist das
A64-Gegenstück zu `regalloc.rs::immediate_consts`, mit derselben
Vorbedingung aus Runde 92: nur ein Wert mit **genau einer** Definition IST
die Konstante, die sein `const` nennt — nach `phi.rs` kann ein Wert aus
mehreren Blöcken geschrieben werden.

Nebenwirkung, die einen Test kostete: `codegen_a64::tests::
start_block_prologue_and_exit` verlangte `movz x9, #42`. `fn main() -> i32
{ return 42 }` ist jetzt `movz x0, #42` — ein Befehl statt drei und zwei
Speicherzugriffen. Der Test prüft das jetzt, und zusätzlich, dass die
Konstante **nicht** mehr durch den Rahmen geht.

### Stufe 3 — das Register IST der Operand

Erst hier kommt der Gewinn von Runde 43. Vier kleine Funktionen —
`src`, `src_ext`, `dreg`, `commit` — und sie werden **nur** dort benutzt,
wo die ganze Rechnung EIN Befehl ist, der alle Quellen liest, bevor er sein
Ziel schreibt. Dann ist `d == a` harmlos und braucht keine
Reihenfolgeregel. Das ist die Sicherheitsbedingung dieser Stufe, und sie
steht so im Quelltext.

Angewandt auf `emit_bin` (`add`/`sub`/`and`/`orr`/`eor`/`mul`/`sdiv`/`udiv`/
`msub`/`lsl`/`lsr`/`asr`), `Op::Cmp`, `Op::Load`, `Op::Store`,
`Op::PtrAdd`, `Op::Copy`, `Op::Const`.

`src_ext` gibt das belegte Register nur dann direkt zurück, wenn der Typ
schon mindestens so breit ist wie die Rechenbreite — sonst müsste
`sxtb`/`uxth` in ein Register schreiben, das noch etwas anderes hält.

Dazu die Sofortwertform von `add`/`sub`/`cmp` (zwölf Bit, vorzeichenlos;
`cmp_imm` kennt auch die negative Form über `cmn`). **Ausdrücklich nicht**
für `and`/`orr`/`eor`: deren Sofortwertfeld ist die Bitmaskenkodierung
`N:immr:imms`, die 0xff ausdrücken kann und 3 nicht. Wer das verwechselt,
baut ein Programm, das läuft und etwas anderes ausrechnet.

### Was daraus wird

| | vorher | nachher |
|---|---|---|
| Schleifenkopf `i < n` | `ldr x9,[sp,#48]` / `mov x10,x24` / `cmp x9,x10` / `cset w9,lt` / `str x9,[sp,#216]` / `ldrb w9,[sp,#216]` / `cbnz` / `b` — **8** | `cmp x22, x24` / `cset w21, lt` / `uxtb w9, w21` / `cbnz` / `b` — **5** |
| Zählerschritt `i = i + 1` | `ldr x9,[sp,#48]` / `movz x10,#1` / `add x9,x9,x10` / `str x9,[sp,#48]` — **4** | `add x22, x22, #1` — **1** |
| `i % 3 == 0` | `ldr` / `movz` / `sdiv` / `msub` / `str` / `ldr` / `mov` / `cmp` / `cset` / `str` — **10** | `movz x10,#3` / `sdiv x11,x22,x10` / `msub x21,x11,x10,x22` / `cmp x21,#0` / `cset w20,eq` — **5** |

---

## 4. Der Rahmen: wohin die geretteten Register kommen

`sp` wird in diesem Rückwärtsteil **einmal** im Vorspann gesetzt und bewegt
sich danach nie wieder — das ist es, was jeden Platzzugriff zu einem Befehl
macht. Die geretteten Register können also nicht auf den Stapel geschoben
werden.

Sie bekommen einen eigenen Bereich **direkt über den ausgehenden
Argumenten** und unter den Wertplätzen:

```
        +--------------------+ <- x29 + 16 + 8k : eingehende Stapelargumente
        | gerettete x29, x30 | <- x29
        | Wertplaetze        |
        | alloca-Speicher    |
        | GERETTETE x19..x24 | <- sp + outgoing      NEU
        | ausgehende Args    | <- sp + 0
        +--------------------+ <- sp
```

Die Lücke dort war bis zu dieser Runde Füllung. `size` wächst um
`align_up(8 * anzahl, 16)`; die Plätze liegen weiterhin bei
`sp + (size - slot)` und damit garantiert über dem neuen Bereich.

Der Vorspann rettet **nach** `sub sp` (der Bereich wird von `sp` aus
adressiert) und **vor** dem Ablegen der Parameter (ein Parameter kann selbst
in einem dieser Register landen). Der Nachspann holt sie **vor** `add sp`
zurück; der Rückgabewert steht da schon in `x0`/`d0`/`v0`, und `at_base`
benutzt nur `x12`.

---

## 5. Korrektheit

### 5.1 Die härteste Prüfung, die der Baum hat

`tools/aarch64/run.sh` übersetzt **jeden** Fall aus `tests/*.fi` zweimal —
x86-64 und aarch64 — lässt beide laufen und vergleicht Standardausgabe
Zeichen für Zeichen und Rückgabewert:

```
  build stage:    dev-fast
  SAME:           304
  DIFFERENT:      0
  NOT SUPPORTED:  0
  ENVIRONMENT:    1 (bewiesen, tools/aarch64/environment.txt)
  x86 already:    4
  RESULT: 304 of 304 comparable cases identical on both machines (100%)
  PASS no case differs between x86-64 and aarch64
```

Die vier `x86 already` sind Fälle, die schon auf x86 ihre eigene Erwartung
verfehlen (`028_cast_narrow`, `030_wrap_u8`, `054_i16_ops`,
`1334b_type_truncation`) — sie gehören zu den 23 roten Punkten der
Grundlinie und nicht zu dieser Runde.

### 5.2 x86 ist nachweislich unangetastet

An gemeinsamem Code wurden **nur Sichtbarkeiten** geändert
(`fn compute_live` → `pub(crate) fn compute_live`, dito `loop_depth`,
`promotable_cells`, `struct Live` und seine Felder). Beweis statt
Behauptung — der erzeugte x86-Assemblertext der vier grossen
Certus-Programme, alter gegen neuer Übersetzer, beide Baustufen:

```
   GLEICH   b3_main     [dev-fast]      458 366 Zeilen
   GLEICH   b3_main     [release-safe]  514 003 Zeilen
   GLEICH   run_main    [dev-fast]      440 428 Zeilen
   GLEICH   run_main    [release-safe]  489 578 Zeilen
   GLEICH   parse_main  [dev-fast]      118 144 Zeilen
   GLEICH   parse_main  [release-safe]  170 402 Zeilen
   GLEICH   layout_main [dev-fast]      387 714 Zeilen
   GLEICH   layout_main [release-safe]  436 522 Zeilen
```

`cmp` ist auf allen acht Paaren still. Damit sind die Certus-Abnahmen auf
x86 per Konstruktion unverändert.

### 5.3 Einheitentests

`cargo test --release`: **281 bestanden, 0 rot.** Einer musste angepasst
werden — siehe Stufe 2.

---


## 5b. Der zweite Fund: der Registerzuteiler macht den Sammler blind

`tools/aarch64/run.sh --no-opt` meldete einen einzigen Unterschied:

```
  DIFF tests/901_dom_tree_gc.fi :: exit code x86=0 aarch64=3
```

Rueckgabewert 3 heisst in diesem Fall: *„nach dem Einsammeln sind noch zu
viele Objekte am Leben"*. Kein falscher Code -- ein **staendiger
Wurzelpunkt**. Nachgemessen mit einer Fassung des Falls, die den
Unterschied als Zahl zurueckgibt:

```
   zurueckgehalten nach einem Einsammeln, das alles freigeben muesste
   (der Baum hat 4680 Knoten):

     mit befoerderten Zellen ....... 4600
     ohne befoerderte Zellen .......    0
     ohne Register (nur Konstanten)     0
     alter Uebersetzer .............    0
```

**Die Ursache, und sie ist kein Versehen, sondern eine Eigenschaft:** eine
befoerderte Zelle ist eine oertliche Groesse, die ihre GANZE Funktion lang
in einem aufrufergesicherten Register wohnt (Intervall `[0, letzter
Zugriff]`, die Regel aus Runde 90). Und jedes dieser sechs Register schreibt
`emit_gc_addr` bei JEDEM Sicherungspunkt in den Zustandsblock des Sammlers,
weil SPEC §3.5.3 einen konservativen Lauf ueber Stapel **und Register**
verspricht. Eine tote Baumwurzel in so einer Zelle haelt damit ihren ganzen
Baum fest.

**Zwei Reparaturversuche, beide gemessen, beide verworfen:**

1. *Am Sicherungspunkt die toten Register mit `xzr` ueberschreiben.* Sauber
   begruendbar (Intervalle sind eine Obermenge der Lebendigkeit, wer
   durchfaellt, ist wirklich tot), und es feuerte an 2 von 114
   Sicherungspunkten. Der Grund: eine Zelle gilt ab Stelle 0 als lebendig,
   also ueberdeckt sie fast jeden Sicherungspunkt. **4600 Objekte blieben.**
2. *An der Sterbestelle `mov rN, xzr`.* 1265 solcher Leerungen wurden
   erzeugt -- und der erste Anlauf brachte das Programm zum **Haengen**,
   weil ein Intervall, das auf `block_end` endet, LIVE-OUT dieses Blocks ist
   und ueber die Rueckwaertskante an den Schleifenkopf weiterfaehrt. Nach
   der Korrektur (nur echte Befehlsstellen) lief es wieder, und es half
   trotzdem nicht: **4600 Objekte blieben.** Auch der Versuch, nur
   `FTy::Ptr` auszuschliessen, half nicht -- `Gc[T]` ist in FIR eine ZAHL
   und kein Zeigertyp, und eine Zahl, die ein Zeiger ist, kann FIR gar nicht
   von einer Zahl unterscheiden. Das ist der Preis eines konservativen
   Sammlers und keine Nachlaessigkeit.

**Die Entscheidung: befoerderte Zellen bleiben auf ARM64 aus.** Alles
andere -- Werte in Registern, neu gebaute Konstanten, das Register als
Operand -- bleibt an. `FIRN_A64_CELLS=1` schaltet sie zum Messen wieder ein.

Was das kostet, ist gemessen und nicht geschaetzt:

```
                                mit Zellen   ohne Zellen
   b3_main statische Befehle      -27,32 %      -24,66 %
   elf Baenke statisch            -19,47 %      -17,11 %
   elf Baenke Durchsatz (qemu)     1,358 x       1,384 x   <- BESSER
   Certus example                  1,42 x        1,45 x    <- BESSER
   Certus hackernews               1,18 x        1,29 x    <- BESSER
```

Die Zellen kosten also 2,7 Prozentpunkte Befehle und bringen beim Durchsatz
**nichts** -- eher das Gegenteil, weil sie sechs Register ueber die ganze
Funktion binden und damit den kurzlebigen Werten wegnehmen. Der Verzicht
ist billiger, als er aussieht.

**Warum das auf x86 nicht auffaellt:** dort gibt es vier Registervorraete,
und nur EINER (`rbx`, `rbp`, `r12`-`r15`) landet im Zustandsblock. Alles,
was keinen Aufruf kreuzt, bekommt `r11`/`r10`/`rsi`/`rdi`/`rdx` und ist fuer
den Sammler unsichtbar. Auf ARM64 sind alle sechs ausgeteilten Register im
Block.

**Der Weg zurueck ist bekannt** und steht in „Was NICHT getan wurde":
`x25`-`x28` als nicht-sammlersichtbare Haelfte, sobald der Registerblock des
Sammlers von sechs auf zehn Woerter waechst. Dann koennen Zellen ohne
Zeigerinhalt dorthin -- und die 2,7 Prozentpunkte kommen zurueck.

## 6. Die Messung, und was sie wert ist

**Die Warnung gehört an den Anfang:** Es hängt kein ARM64-Rechner an diesem
System. Gemessen wird unter `qemu-aarch64` im Benutzermodus. TCG übersetzt
Blöcke und führt sie als x86-Code aus — **die Wanduhr dort folgt der Zahl
der ausgeführten Befehle viel enger, als echte ARM64-Hardware es täte**
(kein Sprungvorhersager, keine Ausführung ausser der Reihe, andere
Cachehierarchie). Was daneben steht — die **statische** Befehlszahl aus
`--emit=asm` und die **exakt ausgeführte** aus `-singlestep -d cpu` — ist
dagegen exakt und hängt von gar keiner Maschine ab.

### 6.1 Certus' Maler

```
   lib/paint/b3_main.fi, --target=aarch64-linux, --opt-level=dev-fast
   statische Befehle   572 399  ->  431 234     -24,66 %
   Binaerdatei       2 646 224  ->  2 081 552   -21,34 %
```

Seitenaufbau unter qemu (`tools/tempo2/messen.py`, drei Läufe, Bestwert):

| Seite | alt | neu | Faktor | Bild |
|---|---:|---:|---:|:--:|
| `example` | 559,5 ms | 386,4 ms | **1,45 x** | bitgleich |
| `wikipedia-firn` | 1662,4 ms | 1238,6 ms | **1,34 x** | bitgleich |
| `hackernews` | 3333,2 ms | 2579,9 ms | **1,29 x** | bitgleich |

### 6.2 Die elf Bänke

`tools/billig/arm_bench.py`, `bench/firn/*.fi`, `dev-fast`, Bestwert aus
drei Läufen:

| Bank | statische Befehle alt | neu | Unterschied | Wanduhr qemu alt | neu | Faktor |
|---|---:|---:|---:|---:|---:|---:|
| `fib` | 801 | 646 | -19.4 % | 0.506 s | 0.422 s | **1.197 x** |
| `sieve` | 1114 | 949 | -14.8 % | 1.511 s | 1.221 s | **1.238 x** |
| `matmul` | 1185 | 977 | -17.6 % | 5.057 s | 3.478 s | **1.454 x** |
| `bytecount` | 1057 | 892 | -15.6 % | 12.778 s | 10.274 s | **1.244 x** |
| `bubblesort` | 1191 | 1007 | -15.4 % | 2.945 s | 2.419 s | **1.217 x** |
| `statemachine` | 1319 | 1083 | -17.9 % | 3.658 s | 1.913 s | **1.912 x** |
| `bitmap` | 1311 | 1102 | -15.9 % | 3.566 s | 2.544 s | **1.402 x** |
| `xxhash` | 1511 | 1289 | -14.7 % | 8.685 s | 5.987 s | **1.451 x** |
| `jsonscan` | 2172 | 1685 | -22.4 % | 5.976 s | 4.851 s | **1.232 x** |
| `memstride` | 992 | 831 | -16.2 % | 1.513 s | 1.150 s | **1.316 x** |
| `branchy` | 1206 | 1027 | -14.8 % | 5.404 s | 3.105 s | **1.740 x** |
| **zusammen / geom. Mittel** | **13859** | **11488** | **-17.11 %** | | | **1.384 x** |

```
   statische Befehle gesamt: 13859 -> 11488   (-17.11 %)
   Durchsatz unter qemu, geometrisches Mittel:  1.384 x
   Ergebnis (Ausgabe + Rueckgabewert) identisch in allen 11 Baenken: JA
```

Der Streubereich ist gross und er hat einen Grund: `statemachine`
(**1,912 x**), `branchy` (**1,740 x**) und `matmul` (**1,454 x**) sind
Schleifen mit vielen kleinen Werten, die vorher alle im Rahmen lagen --
genau das, wofuer sechs Register reichen. `fib` (1,197 x) haengt am Aufruf,
`bubblesort` (1,217 x) und `sieve` (1,238 x) am Speicher; da ist wenig zu
holen und es wird auch wenig geholt.


### 6.3 Exakt ausgeführte Befehle

```
   PROBE                          alt          neu     Unterschied
   ---------------------------------------------------------------
   tools/billig/icount_probe.fi  2 707 467    2 465 517    -8,94 %
   die reine Rechenschleife         37 490       26 372   -29,66 %
```

**Warum die zwei Zahlen so weit auseinander liegen, und das ist die
ehrlichste Aussage dieser Runde:** Stufe 3 wirkt auf die Formen, die sie
anfasst -- Arithmetik, Vergleich, `load`/`store`, `ptradd`, Kopie,
Konstante. Die reine Rechenschleife besteht fast nur daraus und gewinnt
29,7 %. Die groessere Probe hat dazu einen Aufruf je Durchgang, einen
Feldzugriff mit Bereichspruefung, Strukturfelder im Speicher und geprueftes
Rechnen -- alles Wege, die Stufe 1 und 2 mitnehmen, Stufe 3 aber noch
nicht. Sie gewinnt 8,9 %.

**Der reale Fall liegt dazwischen und naeher am oberen Ende**: Certus'
Maler verliert 24,7 % seiner statischen Befehle, die elf Baenke 17,1 %.

Warum nicht alle elf Baenke exakt ausgezaehlt wurden: `qemu -singlestep
-d cpu` schreibt rund tausend Oktette je ausgefuehrtem Befehl, gezaehlt
werden davon etwa 66 000 Befehle je Sekunde. `fib` allein hatte nach
420 Sekunden erst 27 638 241 Befehle beisammen und war noch nicht fertig.
Deshalb steht daneben die statische Zahl -- die ist exakt und kostet
nichts.

---

## 7. Was NICHT getan wurde

* **`x25`–`x28`** — vier weitere Register, sobald der Registerblock des
  Sammlers von sechs auf zehn Wörter wächst.
* **`d8`–`d15`** — unter AAPCS64 ebenfalls aufrufergesichert, liegen
  genauso brach. Für den Maler (Verläufe, Transformationen) der nächste
  lohnende Posten.
* **Kein Operandenweg für `Op::Un`, `Op::Cast`, `Op::Select`, die geprüfte
  Arithmetik und die Aufrufargumente** — sie haben Stufe 1 und 2, nicht
  Stufe 3. Wieviel da noch liegt, ist nicht gemessen.
* **Keine Intervallteilung** — ein Wert lebt ganz im Register oder ganz auf
  dem Stapel, wie auf x86 auch.
* **Kein echtes ARM64-Silizium.**

## 8. Dateien

| Datei | was |
|---|---|
| `compiler/src/regalloc_a64.rs` | neu, 330 Zeilen: der Zuteiler |
| `compiler/src/codegen_a64.rs` | die drei Stufen im Rückwärtsteil, Rahmen, Vor-/Nachspann |
| `compiler/src/regalloc.rs` | **nur** Sichtbarkeiten (`pub(crate)`) |
| `compiler/src/main.rs` | `mod regalloc_a64;` |
| `tools/billig/arm_bench.py` | die Messung: statisch, ausgeführt, Wanduhr |
| `tools/billig/arm_icount.sh` | exakt ausgeführte Befehle über eine benannte Röhre |

## 9. Die anderen zwei Posten der Runde, in drei Zeilen

* **Baustufe `release-safe` für Certus: NEIN.** Gemessen 1,042 x auf
  JavaScript, 0,988 x auf den Seitenaufbau, bei +115 % Übersetzungszeit.
  Die 1,66 x aus ANLEGEWEG waren eine Mikromessung und tragen nicht.
* **Android und ausführbarer Speicher: ERLAUBT.** In der echten Domäne
  `untrusted_app`, Android 15, SELinux Enforcing: alle drei Fragen aus
  STUDIE-JIT §2.4 mit Ja, Sprung in selbst geschriebenen Code gelingt.
* Beides mit allen Belegen in `certus:docs/RUNDE-BILLIG.md`.
