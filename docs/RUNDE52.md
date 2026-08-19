# Runde 52 — `profile kernel` wird echt: freistehend übersetzen

Branch `r52-freistehend`, Basis `cc1710f`.

Bis Runde 51 stand in `SPEC.md` §14, Punkt 6 dieser Satz:

> **`profile`-Deklaration** wird geparst und geprüft, hat aber keine Wirkung:
> erzeugt wird immer ein freistehendes Binary mit `_start` ohne libc.

Er stimmte. `sema.rs::check_profile` prüfte genau eine Sache — ob der Name
`kernel` oder `app` heißt — und tat dann nichts. Es gab keinen
Inline-Assembler, keine MMIO-Zugriffe, keine Interrupt-Einsprungpunkte, und
`main.rs` linkte immer fest mit `as --64` + `ld -n` zu einer ausführbaren
Datei. Diese Runde macht die Deklaration wahr.

**Ergebnis vorweg, alles selbst gemessen (19.08.2026):**

| | Basis `cc1710f` | Runde 52 |
|---|---|---|
| `bash ./test.sh` | 751/751 | **782/782** |
| `bash tools/self_compare.sh` | 213 / 0 abweichend / 0 fehlerhaft | **218 / 0 / 0** |
| `bash tools/fixpoint.sh` | zeichengleich, 427 401 Zeilen | **zeichengleich, 448 038 Zeilen** |
| `bash tools/freestanding/run.sh` | — | **41 / 41** |
| Kernel-Beispiel in QEMU gebootet | — | **ja, mit beiden Compilern** |

---

## 1. Was das Kernel-Profil jetzt garantiert

`--profile=kernel` (Kommandozeile, gewinnt) bzw. `profile kernel` in der
ersten Zeile der Wurzeldatei. Ohne Angabe gilt weiter `app`.

| SPEC §2 sagt | Runde 52 setzt durch | Meldung nennt |
|---|---|---|
| kein globaler Allokator, keine Laufzeit | `import std.*` abgelehnt | das Modul und warum |
| keine `Gc[T]` (Tracing-Sammler) | `gc class` abgelehnt | „`gc class` braucht den tracing-sammler" |
| keine Abwicklung / `throw` | `#[unwinds]` abgelehnt | die Funktion |
| keine versteckte Allokation | folgt aus beidem | — |
| Gleitkomma nur mit `#[allow_fp]` | `f64` **und** Gleitkommaliterale | „gleitkomma (der typ f64) … #[allow_fp]" |
| freistehend | `syscall` abgelehnt, kein `_start` | „unter einem freistehenden kernel liegt kein betriebssystem" |
| Ziel-Binärformat ELF-Objekt | `as --64 -o x.o`, **kein `ld`** | — |

Jede Verletzung ist ein Compilerfehler mit Zeile, Spalte, Markierung und einem
Hinweis, der die verbotene Sache **benennt**. Nachweis: 15 Negativtests
`tests/neg/frei_*.fi`, jeder mit erwarteter Position und erwartetem Text.

**`syscall` steht nicht in der Tabelle von SPEC §2** — es gehört trotzdem
dazu und ist die schärfste der sechs Regeln: unter einem freistehenden Kernel
liegt kein Betriebssystem, das einen Systemaufruf entgegennehmen könnte. Genau
diese eine Regel macht die gesamte Standardbibliothek im Kernel-Profil
unbenutzbar, denn dort geht jede Allokation über `mmap` und jede Ausgabe über
`write`.

Ein Kernel braucht keinen Einstiegspunkt: `fn main` ist im Kernel-Profil nicht
mehr Pflicht, `_start`, das Aufsetzen von `rsp` und der `exit`-Systemaufruf
entfallen ersatzlos.

## 2. Freistehende Ausgabe

```sh
firnc -c -o /tmp/x.o datei.fi        # nur `as --64 -o /tmp/x.o`, kein `ld`
firnc --objekt -o /tmp/x.o datei.fi  # dasselbe, ausgeschrieben
firnc -o /tmp/x.o kernel.fi          # `profile kernel` schaltet -c selbst ein
```

Im App-Profil bleibt alles wie bisher (`as` + `ld` → ausführbare Datei).
`firnc1` kennt dieselben Schalter (`-c`, `--objekt`, `--profile=kernel|app`).

## 3. Inline-Assembler

```firn
asm("cli")
asm("out dx, al", in("dx") port, in("al") wert)
let alt: u64 = asm("rdtsc", out("rax"), clobber("rdx"))
```

```ebnf
asm_ausdruck = "asm" "(" str_lit { "," asm_op } ")" ;
asm_op       = "in"      "(" str_lit ")" ausdruck
             | "out"     "(" str_lit ")"
             | "clobber" "(" str_lit ")" ;
```

Fünf Entscheidungen, jede mit Preis:

1. **`asm` ist kein Schlüsselwort.** Der Parser erkennt die Form nur, wenn auf
   den Bezeichner `asm` unmittelbar `(` und ein Zeichenkettenliteral folgen.
   Damit ändert sich der Tokenstrom nicht (`tools/lex_compare.sh` bleibt
   unberührt) und `asm` bleibt als Name benutzbar. *Preis:* wer eine Funktion
   `asm(s: [u8; N])` schreibt und sie mit einem Literal aufruft, bekommt den
   Assembler statt seiner Funktion.
2. **Registerbindung statt Platzhalter.** Es gibt kein `{0}`. Operanden nennen
   ihr Register selbst; der Codegenerator legt vor dem Block
   `mov <reg>, <wert>` und liest danach `out`. *Preis:* die Vorlage ist an
   konkrete Register gebunden, der Zuteiler kann nichts optimieren.
3. **Nur caller-saved Register.** Erlaubt sind `rax rcx rdx rsi rdi r8..r11`
   samt schmalen Namen (`eax`/`ax`/`al`, `r8d`/`r8w`/`r8b`, …), in der
   Clobber-Liste zusätzlich `memory`. `rbx`, `rbp`, `rsp` und `r12`–`r15`
   werden mit eigener Meldung abgelehnt: sie tragen den Rahmen bzw. sind
   callee-saved. *Gewinn:* genau diese Menge zerstört auch ein gewöhnlicher
   `call` — die Registerzuteilung braucht **keine** Sonderregel, und eine
   Sonderregel im Zuteiler ist genau die Sorte Code, die den Fehler aus
   Runde 40 erzeugt hat.
4. **Höchstens ein `out`.** Sein Wert hat immer den Typ `u64`; wer weniger
   will, schreibt `as u8`. Ohne `out` hat der Block den Typ `()`.
5. **`volatile` ist nicht abwählbar.** Es gibt keine nicht-flüchtige Variante.

**Intel-Syntax**, weil der Codegenerator `.intel_syntax noprefix` ausgibt.
`\n` in der Vorlage trennt Assemblerzeilen.

### 3.1 Die R40-Falle, und was dagegen steht

Runde 40 hat der Optimierer Code entfernt, den er nicht entfernen durfte. Die
Gegenmaßnahmen sind klein und örtlich, wie verlangt:

| Stelle | Änderung |
|---|---|
| `fir.rs::Op::is_pure` | `Asm`, `MmioLoad`, `MmioStore` sind **nie** rein → kein DCE |
| `opt.rs::key_of` | kein CSE-Schlüssel (fällt in den bestehenden `_ => None`-Zweig) |
| `mem2reg.rs::is_untouchable` | wie `select`/`barrier`/`secure_zero`: Operanden werden nie umgeschrieben |
| `mem2reg.rs::clobbers_memory` | alle drei verändern Speicher → keine `load`-Weiterleitung darüber hinweg |
| `licm.rs::hebbare_op` | nicht hebbar (über `is_untouchable` und den `_ => false`-Zweig) |
| `regalloc.rs::unsupported_grund` | Funktionen mit Inline-Assembler, MMIO oder `#[interrupt]` gehen über den Grundpfad |

Nachgewiesen wird das an drei Stellen, und zwar **gemessen**, nicht behauptet:

* `tests/850`–`854` laufen wirklich, in allen drei Baustufen, in beiden
  Compilern. `851` liefert 0 statt 7, wenn der Block wegfällt; `852` liefert 2
  statt 3, wenn zwei wörtlich gleiche Blöcke zusammengelegt werden; `853`
  liefert 10 oder 18 statt 14, wenn MMIO-Zugriffe zusammengelegt werden.
* `tools/freestanding/volatile.fi` steht ganz in **einer** Funktion und ruft
  nichts — Einbetten kann die Zahlen also nicht verschieben. Gezählt wird in
  der FIR **nach** dem Optimierer (`--emit=fir`): `asm.void "pause"` = 3,
  `asm.u64 "rdtsc"` = 1, `mmio_load.u32` = 2, `mmio_store.u32` = 1, in jeder
  Baustufe.
* Sechs Rust-Modultests in `compiler/src/core.rs`.

### 3.2 Was Einbetten darf

`--opt-level=release-fast` bettet `out8`/`in8` in ihre Aufrufer ein; danach
steht `out dx, al` zehnmal statt einmal im Assembler. Das ist **richtig**:
Einbetten verdoppelt den Block samt Aufruf, es entfernt und verschmilzt ihn
nicht. `tools/freestanding/run.sh` prüft deshalb `cli` und `hlt` exakt (sie
stehen in `kern_start`, das niemand ruft) und `out`/`in` nur auf „mindestens
einmal"; die exakte Zählung macht `volatile.fi`.

## 4. MMIO

```firn
__mmio_read8(p)      __mmio_write8(p, w)
__mmio_read16(p)     __mmio_write16(p, w)
__mmio_read32(p)     __mmio_write32(p, w)
__mmio_read64(p)     __mmio_write64(p, w)
```

Acht eingebaute Namen mit reserviertem `__`-Präfix (wie `__atomar_addieren`,
Runde 47). Jeder wird zu **einer** Maschineninstruktion:

```
mmio_load.u32  %3      ->   mov rcx, qword ptr [rbp-8] ; mov eax, dword ptr [rcx]
mmio_store.u16 %4, %3  ->   mov rcx, … ; mov rax, … ; mov word ptr [rcx], ax
```

Kein Durchgang darf zwei Zugriffe zusammenlegen, einen entfernen oder ihn über
einen anderen Speicherzugriff hinweg verschieben. MMIO ist in **beiden**
Profilen verfügbar — auch eine Anwendung blendet gelegentlich ein Gerät ein.

## 5. Interrupt-Einsprungpunkte

```firn
#[interrupt]
fn timer_ih() {
    …
}
```

Eigene Aufrufkonvention: **14 Universalregister** werden gerettet
(`rax rcx rdx rbx rsi rdi r8`–`r15`; `rbp` rettet der gewöhnliche Prolog,
`rsp` der Prozessor), der Abschluss ist **`iretq`** statt `ret` — nur diese
Instruktion stellt `rflags`, `cs` und `rsp` des unterbrochenen Fadens wieder
her.

Bedingungen, jede mit eigener Meldung: nur im Kernel-Profil, keine Parameter,
kein Rückgabetyp, **nicht aufrufbar** (ein `call` würde in einem `iretq` enden
und den Stapel zerlegen; nur die IDT darf auf sie zeigen).

## 6. Der Nachweis: ein Kernel, der wirklich bootet

`demos/kernel/core.fi` — 130 Zeilen Firn: serieller Port COM1 über
`in`/`out`, VGA-Textpuffer bei `0xB8000` über MMIO, ein Interrupt-Einsprung.
`demos/kernel/start.s` (60 Zeilen, das einzige Nicht-Firn) trägt den
Multiboot-Kopf und den Weg in den Langen Modus; `demos/kernel/linker.ld`
bindet bei 1 MiB.

`bash tools/freestanding/run.sh` — **41 Prüfungen, 41 bestanden.** Auszug:

```
== 2. Es ist eine OBJEKTdatei, und sie ist freistehend ==
  OK    firnc0: ELF-Typ REL (verschiebbare Objektdatei)
  OK    firnc0: KEIN undefiniertes Symbol
  OK    firnc0: alle definierten Symbole sind eigene
  OK    firnc0: kein syscall im Maschinencode
  OK    firnc1: ELF-Typ REL (verschiebbare Objektdatei)
  OK    firnc1: KEIN undefiniertes Symbol
  OK    firnc1: alle definierten Symbole sind eigene
  OK    firnc1: kein syscall im Maschinencode
== 3. Gegen das Linkerskript binden (kein libc, keine crt-Dateien) ==
  OK    start.s assembliert (Multiboot-Kopf, Langer Modus)
  OK    firnc0: gelinkt, Einsprung 0x10000c
  OK    firnc0: das gebundene Abbild hat kein offenes Symbol
  OK    firnc1: gelinkt, Einsprung 0x10000c
  OK    firnc1: das gebundene Abbild hat kein offenes Symbol
== 3b. In QEMU booten (der eigentliche Beweis) ==
  OK    firnc0: gebootet, serielle Ausgabe erschienen
  OK    firnc1: gebootet, serielle Ausgabe erschienen
```

Von Hand nachvollziehbar:

```sh
$ compiler/target/release/firnc -o /tmp/kern0.o demos/kernel/core.fi
$ file /tmp/kern0.o
/tmp/kern0.o: ELF 64-bit LSB relocatable, x86-64, version 1 (SYSV), with debug_info, not stripped
$ nm /tmp/kern0.o
0000000000000030 T _F0.in8
000000000000078b T _F0.kern_start
0000000000000000 T _F0.out8
0000000000000193 T _F0.seriell_bereit
000000000000005e T _F0.seriell_init
000000000000029c T _F0.seriell_text
00000000000001f3 T _F0.seriell_zeichen
0000000000000685 T _F0.timer_ih
00000000000003aa T _F0.vga_leeren
00000000000004c0 T _F0.vga_text
000000000000030e T _F0.vga_zelle
$ nm -u /tmp/kern0.o            # undefinierte Symbole
$                                # — keine.
$ readelf -h /tmp/kern0.o | grep Type
  Type:                              REL (Relocatable file)
$ objdump -d /tmp/kern0.o | grep -c syscall
0
```

Und der Boot:

```sh
$ as --64 -o /tmp/start.o demos/kernel/start.s
$ ld -n -T demos/kernel/linker.ld --defsym=KERN_START=_F0.kern_start \
     -o /tmp/kern0.elf /tmp/start.o /tmp/kern0.o
$ objcopy -O elf32-i386 /tmp/kern0.elf /tmp/kern0.mb   # QEMUs Multiboot nimmt nur ELF32
$ qemu-system-x86_64 -kernel /tmp/kern0.mb -serial stdio -display none -no-reboot
FIRN: profile kernel ist
freistehend.
```

Dasselbe mit `./.firnc1 demos/kernel/core.fi -o /tmp/kern1.o` und
`--defsym=KERN_START=_F1.kern_start`: dieselbe Ausgabe.

## 7. Beide Compiler

| | `firnc0` (Rust) | `firnc1` (Firn) |
|---|---|---|
| `asm(…)` mit `in`/`out`/`clobber` | `compiler/src/core.rs` | `lib/firnc1/{kern,parser,sema,lower,codegen}.fi` |
| MMIO ×8 | ✓ | ✓ |
| `#[interrupt]` → `iretq` | ✓ | ✓ |
| `-c` / `--objekt` | ✓ | ✓ |
| `--profile=kernel|app` | ✓ | ✓ (Ausgabeformat) |
| Profilverbot `syscall` | ✓ mit Meldung | ✓ als Zurückweisung |
| Profilverbot `gc class` | ✓ mit Meldung | ✓ als Zurückweisung |
| Profilverbote `f64`/`import std.*`/`#[unwinds]` | ✓ mit Meldung | mittelbar, siehe §9 |

**FIR-Opcodes 50–52** (Runde 52 hat 50–59 reserviert):

```
O_ASM     = 50   asm.<ty> "vorlage" [out=reg] [in=[reg %v, …]] [clobber=[a, b]]
O_MMIOLD  = 51   mmio_load.<ty> %adr
O_MMIOST  = 52   mmio_store.<ty> %wert, %adr
```

Die Textform ist **Vertrag**: `tools/fir_compare.sh` vergleicht
`firnc0 --emit=fir-raw` Oktett für Oktett mit `bin/firdump.fi`. Für
`tests/850`–`854` ist sie gleich.

In `firnc1` liegt das `asm`-Register **im Baum** (`ast.fi`, `asm_dazu`), nicht
in einer Seitentabelle — dieselbe Bauart wie `__match#N` im Musterregister. Im
Baum bleibt ein Aufruf `asm$<nummer>` mit den Eingabeausdrücken als
Argumenten; dadurch laufen Monomorphisierung, `#[no_gc]`-Prüfung und der
kanonische Druck unverändert darüber hinweg, und `--emit=ast-kanon` liefert
auf beiden Seiten denselben Text.

## 8. Abnahme, gemessen

```
$ rm -f .firnc1 .firnc2 .firnc3
$ bash tools/fixpoint.sh
STUFE 2: 2760 ms   2581456 Oktette
STUFE 3: 8004 ms   2581456 Oktette
FIXPUNKT:  Stufe 2 == Stufe 3, zeichengleich (448038 Zeilen Assembler)
  GLEICHES VERHALTEN: 218
  ABWEICHEND:         0
  FEHLERHAFT:         0
  NICHT KERN:         0
  DEFER:              0
  COMPTIME:           1
  CODEGEN FEHLT:      0
  UEBERSPRUNGEN:      18
KORPUS:    .firnc2 verhaelt sich wie firnc0

$ bash tools/freestanding/run.sh
FREISTEHEND: 41 bestanden, 0 fehlgeschlagen
```

```
$ bash ./test.sh
…
== 16. Der Compiler in Firn uebersetzt, das Ergebnis laeuft ==
   GLEICHES VERHALTEN: 218
   ABWEICHEND:         0
   FEHLERHAFT:         0
== 17. Der Fixpunkt: Firn uebersetzt sich selbst ==
   FIXPUNKT:  Stufe 2 == Stufe 3, zeichengleich (448038 Zeilen Assembler)
   KORPUS:    .firnc2 verhaelt sich wie firnc0
== 19. Freistehend ==
   FREISTEHEND: 41 bestanden, 0 fehlgeschlagen

PASS 782/782
```

Basis 751/751; dazu 5 neue Programme × 3 Baustufen = 15, 15 neue
Negativtests, und Abschnitt 19 `freistehend`.

Die Laufzeiten oben sind **keine** Leistungsaussage: die Maschine ist nicht
ruhig, es laufen mehrere Runden gleichzeitig.

## 9. Offenes — was für einen echten Kernel noch fehlt

Ehrlich und vollständig:

1. **Globale, veränderliche Daten.** SPEC §14, Punkt 5: es gibt nur `const`.
   Ohne sie kann ein Kernel keine IDT, keine GDT und keinen Tick-Zähler
   halten. `demos/kernel/core.fi` weicht deshalb aus und zählt im
   Bildspeicher. **Das ist der größte Blocker**, größer als alles andere in
   dieser Liste, und er gehört nicht dieser Runde (Revier: Profil,
   Ausgabeformat, Inline-Asm, MMIO, Interrupt-ABI).
2. **Die IDT trägt sich nicht selbst ein.** Der Einsprungpunkt steht bereit
   und endet korrekt mit `iretq`; die Tabelle muss noch von außen kommen —
   siehe (1). `lidt` ließe sich heute schon per `asm` schreiben, aber es gäbe
   keinen Ort für die Tabelle.
3. **`#[interrupt]` kennt keinen Fehlercode.** Vektoren 8, 10–14, 17, 21, 29
   und 30 legen einen Fehlercode auf den Stapel; `iretq` erwartet ihn dann
   nicht mehr dort. Für diese Vektoren braucht es eine Variante, die vor dem
   `iretq` acht Byte abräumt (`add rsp, 8`).
4. **Der Unterbrechungsrahmen ist nicht lesbar.** Eine `#[interrupt]`-Funktion
   hat keine Parameter, kommt also nicht an `rip`/`cs`/`rflags` heran. Für
   einen Seitenfehlerbehandler ist das zu wenig.
5. **Kein `#[naked]`.** Jede Funktion bekommt Prolog und Epilog. Für den
   allerersten Einsprung nach dem Bootlader braucht es das nicht (der
   32-Bit-Vorspann in `start.s` erledigt es), für einen Kontextwechsel schon.
6. **Kein callee-saved Register im `asm`.** `rbx` und `r12`–`r15` sind
   gesperrt. Auflösen ließe sich das mit `push`/`pop` um den Block herum; die
   Runde hat bewusst die kleinere, nachweisbare Variante genommen.
7. **Kein `#[allow_fp]`-Effekt über Aufrufgrenzen.** Das Attribut wirkt je
   Funktion; wer eine `#[allow_fp]`-Funktion aus einer ohne aufruft, bekommt
   keinen Fehler. Für den FPU-Zustand ist das noch nicht genug.
8. **Die Registerzuteilung ist im Kernel-Code aus.** Jede Funktion mit
   Inline-Assembler, MMIO oder `#[interrupt]` geht über den Grundpfad
   (`codegen_x86.rs`) und hält jeden Wert im Rahmen. Das ist korrekt und
   langsam. Der Zuteiler müsste dafür feste Registerbindungen und
   Clobber-Mengen lernen — eine eigene Runde, und eine, die den Fehler aus
   Runde 40 wiederholen kann, wenn man sie schlampig macht.
9. **`firnc1` weist zurück, es erklärt nicht.** Der Typprüfer in Firn zählt
   Fehler, er formuliert sie nicht — das ist seit Runde 30 so und gilt für
   *alle* Prüfungen, nicht nur für diese. `syscall` und `gc class` im
   Kernel-Profil lehnt er ab (Rückgabewert 1), `import std.*` mittelbar
   (jedes std-Modul benutzt `syscall`). **Nicht** geprüft sind dort
   Gleitkomma ohne `#[allow_fp]` — dafür müsste `#[allow_fp]` bis in den
   Baum durchgereicht werden — und `#[unwinds]`, das `firnc1` schon in der
   Vorabsuche als nicht portierte Erweiterung ablehnt. Die ausformulierten
   Meldungen mit Zeile, Spalte und Hinweis gibt es nur in `firnc0`; genau
   dort prüfen die 15 Negativtests sie auch.
10. **Kein `#[max_stack]`.** SPEC §2 führt es für beide Profile; es gibt es
    weiterhin nicht.
11. **Panik ruft kein `karst_panic`.** SPEC §2 nennt das für das
    Kernel-Profil; Stufe 0 hat überhaupt keine Bereichsprüfung (SPEC §14,
    Punkt 3).

## 10. Geänderte Dateien

```
compiler/src/core.rs            neu   Inline-Assembler, MMIO, #[interrupt]
compiler/src/prof.rs          neu   Profilauflösung und -durchsetzung
compiler/src/fir.rs                   Op::Asm/MmioLoad/MmioStore, Func.interrupt
compiler/src/{opt,mem2reg,licm,inline,regalloc}.rs   volatile-Schutz
compiler/src/codegen_x86.rs           kein _start im Kernel-Profil, iretq-Epilog
compiler/src/main.rs                  -c/--objekt, --profile=
compiler/src/{sema,lower,parser,modules,attrs}.rs    Hooks
lib/firnc1/core.fi              neu   Registertabelle, MMIO-Namen, asm-Nummern
lib/firnc1/{ast,parser,sema,lower,fir,codegen}.fi    dieselbe Sprache in Firn
bin/firnc1.fi                         -c/--objekt, --profile=
demos/kernel/{core.fi,start.s,linker.ld}   neu   der Nachweis
tools/freestanding/{run.sh,volatile.fi}         neu   41 Prüfungen
tests/85{0,1,2,3,4}_*.fi                       neu   laufende Programme
tests/neg/frei_*.fi (15)                       neu   jede Verbotsmeldung
test.sh                               Abschnitt 19
```
