# FIR — die Zwischensprache von `firnc0`

FIR ("Frontend Intermediate Representation") ist die eigene, typisierte
Zwischensprache des Compilers. Sie steht zwischen dem Typprüfer und dem
x86_64-Backend:

```
Quelle -> Lexer -> Parser -> AST -> Typprüfer -> [ FIR ] -> Optimierer -> x86_64
```

FIR ist **kein** umbenannter AST: Ausdrücke sind in Einzelinstruktionen mit
eigenen Wert-Ids zerlegt, Kontrollfluss existiert nur noch als Basisblöcke mit
Terminatoren, Variablen sind Speicherplätze (`alloca`) und jede Instruktion hat
einen expliziten Maschinentyp. Umgekehrt enthält FIR nichts Maschinenspezifisches:
Register, Stackrahmen, Aufrufkonvention und Befehlsauswahl entstehen erst im
Backend.

Textform ausgeben:

```
firnc --emit=fir-raw datei      # FIR direkt nach dem Lowering
firnc --emit=fir-opt datei      # FIR nach Konstantenfaltung + Entfernen toten Codes
firnc --emit=fir     datei      # dasselbe wie --emit=fir-opt
```

Implementierung: `compiler/src/fir.rs` (Datenstruktur + Textform),
`compiler/src/lower.rs` (AST -> FIR), `compiler/src/opt.rs` (Optimierer).

---

## 1. Aufbau

Ein **Modul** ist eine Liste von **Funktionen**. Eine Funktion hat einen Namen,
eine Liste von Parametertypen, einen Rückgabetyp und eine Liste von
**Basisblöcken**. Der erste Block (`bb0`) ist der Eintrittsblock. Ein Block ist
eine Folge von **Instruktionen** und genau **ein Terminator** am Ende.

Jede Instruktion definiert höchstens einen Wert `%n`. Werte sind fortlaufend
nummeriert und werden genau einmal definiert (SSA-artig). Die Parameter einer
Funktion mit *n* Parametern belegen die Werte `%0 … %(n-1)`.

Textform (jede Zeile ist eine Instruktion, Blocklabels stehen in Spalte 1):

```
; FIR v0
fn @name(%0: i32, %1: ptr) -> i32 {
bb0:
  %2 = const.i32 7
  ret %2
}
```

Die erste Zeile `; FIR v0` ist die Formatkennung; `;` leitet keinen Kommentar
in der Instruktionssyntax ein, sie kommt nur in dieser Kopfzeile vor.

---

## 2. Typen

| FIR-Typ | Bedeutung | Breite |
|---|---|---|
| `i8 i16 i32 i64` | vorzeichenbehaftete Ganzzahl | 8/16/32/64 Bit |
| `u8 u16 u32 u64` | vorzeichenlose Ganzzahl | 8/16/32/64 Bit |
| `bool` | Wahrheitswert, ausschließlich 0 oder 1 | 8 Bit |
| `ptr` | Adresse, in FIR untypisiert | 64 Bit |
| `void` | kein Wert (z. B. `store`, `copymem`, Aufruf ohne Rückgabe) | – |

Abbildung der Quelltypen (identisch in Typprüfer, Lowering und Backend):

* `i8…i64` -> `i8…i64`, `u8…u64` -> `u8…u64`
* `usize` -> `u64`, `isize` -> `i64`
* `bool` -> `bool` (1 Byte, nur 0/1)
* `*T`, `*mut T` -> `ptr` (der Zielttyp verschwindet; Elementgrößen sind beim
  Lowering bereits in konstante Byte-Offsets aufgelöst)

**Aggregate (Structs, Arrays) sind keine FIR-Werte.** Sie existieren nur als
Adresse: `alloca` legt den Platz an, `ptradd` rechnet Feld- und Elementadressen
aus, `load`/`store` greifen auf skalare Felder zu, `copymem` kopiert ganze
Aggregate. Deshalb gibt es in FIR keinen Aggregattyp und keine
Aggregat-Argumente.

Vorzeichen ist eine Eigenschaft des **Typs**, nicht des Befehls: `div.i32` ist
vorzeichenbehaftet (`idiv`), `div.u32` nicht (`div`); `shr.i64` ist arithmetisch
(`sar`), `shr.u64` logisch (`shr`).

---

## 3. Instruktionen

Schreibweise unten: `%d = ` steht für den definierten Wert (fehlt bei
`void`-Instruktionen), `T` ist der Instruktionstyp.

| Textform | Bedeutung |
|---|---|
| `%d = const.T c` | Konstante `c`, auf `T` zurechtgestutzt |
| `%d = add.T %a, %b` | Addition (umlaufend) |
| `%d = sub.T %a, %b` | Subtraktion |
| `%d = mul.T %a, %b` | Multiplikation |
| `%d = div.T %a, %b` | Division; Vorzeichen aus `T` |
| `%d = rem.T %a, %b` | Rest; Vorzeichen aus `T` |
| `%d = and.T %a, %b` | bitweises Und |
| `%d = or.T %a, %b` | bitweises Oder |
| `%d = xor.T %a, %b` | bitweises Exklusiv-Oder |
| `%d = shl.T %a, %b` | Linksverschiebung |
| `%d = shr.T %a, %b` | Rechtsverschiebung; arithmetisch bei signiertem `T` |
| `%d = cmp.OP.T %a, %b` | Vergleich, `OP` ∈ `eq ne lt le gt ge`; `T` ist der **Operandentyp**, Ergebnis ist immer `bool` |
| `%d = neg.T %a` | arithmetische Negation |
| `%d = not.T %a` | bitweises Nicht; bei `T = bool` logisches Nicht (0↔1) |
| `%d = cast.FROM.TO %a` | Umwandlung: `FROM` ist der Quelltyp, `TO` der Instruktionstyp. Verbreitern erweitert nach Vorzeichen von `FROM` (signed: vorzeichen-, sonst nullerweitert), Verschmälern schneidet ab. `ptr` zählt als 64-Bit-Wert. |
| `%d = alloca.ptr size=N align=A` | `N` Byte Stackspeicher, `A`-Byte ausgerichtet; Ergebnis ist die Adresse. **Nur im Eintrittsblock.** |
| `%d = load.T %a` | lädt `T` von Adresse `%a` |
| `store.T %v, %a` | speichert Wert `%v` vom Typ `T` an Adresse `%a` (kein Ergebnis) |
| `%d = ptradd.ptr %b, %o` | Adresse `%b` + `%o` **Bytes** (`%o` ist `i64` oder `u64`) |
| `%d = call.T @f(%a, …)` | Aufruf; `T` ist der Rückgabetyp, `void` bei Aufruf ohne Rückgabewert (dann ohne `%d = `) |
| `%d = syscall.i64 %nr, %a1, …` | Linux-Syscall: erstes Argument ist die Nummer, danach bis zu 6 Argumente, alle `i64`; Ergebnis ist `i64` |
| `copymem %dst, %src, size=N` | kopiert `N` Byte von `%src` nach `%dst` (kein Ergebnis, keine Überlappung) |

**Rein/unrein:** `const`, `add`…`shr`, `cmp`, `neg`, `not`, `cast`, `ptradd`,
`load` und `alloca` sind rein — der Optimierer darf sie entfernen, wenn ihr
Ergebnis unbenutzt ist. `store`, `call`, `syscall` und `copymem` haben
Seiteneffekte und bleiben immer stehen (`Op::is_pure` in `fir.rs`).

## 4. Terminatoren

| Textform | Bedeutung |
|---|---|
| `br bbN` | unbedingter Sprung |
| `brcond %c, bbT, bbF` | Sprung nach `bbT`, wenn `%c` (Typ `bool`) ungleich 0, sonst nach `bbF` |
| `ret %v` | Rücksprung mit Wert |
| `ret` | Rücksprung ohne Wert |
| `<unset>` | **darf nach dem Lowering nicht vorkommen** — nur Bauzustand |

---

## 5. Invarianten

Diese Zusagen macht das Lowering dem Optimierer und dem Backend; der Optimierer
erhält sie. `lower.rs` prüft (1) und (2) am Ende selbst nach und meldet einen
Compilerfehler statt still etwas Kaputtes weiterzureichen.

1. **Genau ein Terminator je Block**, immer am Ende. Kein `Term::Unset`.
   Unerreichbarer Code nach `return` landet in einem neuen Block, der ebenfalls
   ordentlich terminiert wird (`ret` mit `const 0` bzw. `ret` bei `void`).
2. **Alle `alloca` stehen im Eintrittsblock `bb0`**, vor der ersten
   Nicht-`alloca`-Instruktion. Damit ist der Stackrahmen im Backend statisch
   berechenbar; auch `alloca`s aus tiefen Blöcken (z. B. der Ergebnisplatz eines
   `&&`) wandern dorthin. Deswegen sind die Wert-Ids im Eintrittsblock nicht
   zwingend aufsteigend.
3. **Jeder Wert wird genau einmal definiert** und vor jeder Verwendung — mit
   Ausnahme von Rückwärtskanten, wo nur Werte aus dominierenden Blöcken benutzt
   werden. Werte fließen nie über einen Blockrand hinweg neu zusammen (siehe 4).
4. **Keine Phi-Knoten.** Jede lokale Variable und jeder Parameter hat einen
   eigenen `alloca`-Slot; Lesen ist `load`, Schreiben ist `store`. Begründung:
   Stufe 0 soll klein und nachprüfbar sein — ohne Phis braucht das Lowering
   weder Dominanzberechnung noch SSA-Konstruktion, und das Backend kann jeden
   Wert einfach in einen Stack-Slot legen. Der Preis sind mehr `load`/`store`;
   das ist bewusst so, denn die Registerzuteilung von Stufe 0 ist ohnehin naiv.
   Eine SSA-Konstruktion (`mem2reg`) kann später ergänzt werden, ohne die
   Instruktionsmenge zu ändern.
5. **Typtreue:** Beide Operanden einer Binäroperation haben den Typ der
   Instruktion (Ausnahme: der Verschiebungsbetrag wird beim Lowering auf den Typ
   des linken Operanden gebracht); beide Operanden eines `cmp` haben den in der
   Instruktion genannten Operandentyp; `store.T`/`load.T` passen zur Breite des
   gespeicherten Wertes; Adressen sind immer `ptr`; `bool` enthält nur 0 oder 1.
6. **Blocknummern sind Indizes**: `bbN` ist der `N`-te Block der Funktion.
   Sprungziele sind immer gültige Blöcke derselben Funktion.

---

## 6. Wie der AST nach FIR abgebildet wird

* **Variablen/Parameter:** `alloca` im Eintrittsblock; Parameter werden dort aus
  ihren Wert-Ids (`%0…`) in ihren Slot gestored. Jeder Zugriff ist `load`/`store`.
* **lvalues** werden zu Adressrechnungen:
  * Bezeichner -> Slotadresse,
  * `a.f` -> `ptradd base, const OFFSET` (Offset aus dem Struct-Layout; Offset 0
    erzeugt kein `ptradd`),
  * `a[i]` -> Index nach `u64` casten, `mul.u64` mit der Elementgröße,
    `ptradd`,
  * `*p` -> der Zeigerwert selbst.
* **`if`/`else`, `while`** werden zu Basisblöcken mit `brcond`/`br`.
* **`&&`, `||`** werden **kurzschließend** aufgelöst: ein `alloca`-Slot vom Typ
  `bool` nimmt das Ergebnis auf, der linke Operand landet im Slot und steuert
  ein `brcond`; nur im "muss noch geprüft werden"-Zweig wird der rechte Operand
  ausgewertet und überschreibt den Slot. Es entsteht **keine** `and.bool`- oder
  `or.bool`-Instruktion.
* **Struct-/Array-Literale** werden feld- bzw. elementweise in ihren Zielplatz
  geschrieben; Zuweisung eines ganzen Aggregats (`let p2: Point = p1;`) wird zu
  `copymem`.
* **`as`** wird zu `cast`. Sonderfall: `x as bool` wird als `cmp.ne` gegen 0
  gelowert, damit `bool` garantiert nur 0/1 enthält. `bool as iN` ist ein `cast`
  (nullerweitert).
* **`const`-Deklarationen** sind zur Übersetzungszeit ausgewertet und erscheinen
  als `const`-Instruktion an der Verwendungsstelle.
* **`syscall(...)`** wird zu `syscall.i64`; jedes Argument wird vorher auf `i64`
  erweitert (signierte Quelle: vorzeichen-, sonst nullerweitert).

---

## 7. Vollständiges Beispiel

Quellprogramm:

```
struct Point {
    x: i32,
    y: i32,
}

const LIMIT: i32 = 10;

fn summe(n: i32) -> i32 {
    var s: i32 = 0;
    var i: i32 = 1;
    while i <= n {
        s = s + i;
        i = i + 1;
    }
    return s;
}

fn main() -> i32 {
    var p: Point = Point{ x: 3, y: 4, };
    p.y = summe(LIMIT);
    if p.x > 0 && p.y > 0 {
        return p.x + p.y;
    }
    return 0;
}
```

Dazugehörige Ausgabe von `--emit=fir-raw` (unoptimiert, wörtlich kopiert; der
Test `lower::tests::doku_beispiel_stimmt` vergleicht diesen Block mit dem, was
das Lowering wirklich erzeugt):

```firdump
; FIR v0
fn @sum(%0: i32) -> i32 {
bb0:
  %1 = alloca.ptr size=4 align=4
  %2 = alloca.ptr size=4 align=4
  %4 = alloca.ptr size=4 align=4
  store.i32 %0, %1
  %3 = const.i32 0
  store.i32 %3, %2
  %5 = const.i32 1
  store.i32 %5, %4
  br bb1
bb1:
  %6 = load.i32 %4
  %7 = load.i32 %1
  %8 = cmp.le.i32 %6, %7
  brcond %8, bb2, bb3
bb2:
  %9 = load.i32 %2
  %10 = load.i32 %4
  %11 = add.i32 %9, %10
  store.i32 %11, %2
  %12 = load.i32 %4
  %13 = const.i32 1
  %14 = add.i32 %12, %13
  store.i32 %14, %4
  br bb1
bb3:
  %15 = load.i32 %2
  ret %15
bb4:
  %16 = const.i32 0
  ret %16
}
fn @main() -> i32 {
bb0:
  %0 = alloca.ptr size=8 align=4
  %9 = alloca.ptr size=1 align=1
  %1 = const.i32 3
  store.i32 %1, %0
  %2 = const.i64 4
  %3 = ptradd.ptr %0, %2
  %4 = const.i32 4
  store.i32 %4, %3
  %5 = const.i64 4
  %6 = ptradd.ptr %0, %5
  %7 = const.i32 10
  %8 = call.i32 @sum(%7)
  store.i32 %8, %6
  %10 = load.i32 %0
  %11 = const.i32 0
  %12 = cmp.gt.i32 %10, %11
  store.bool %12, %9
  brcond %12, bb1, bb2
bb1:
  %13 = const.i64 4
  %14 = ptradd.ptr %0, %13
  %15 = load.i32 %14
  %16 = const.i32 0
  %17 = cmp.gt.i32 %15, %16
  store.bool %17, %9
  br bb2
bb2:
  %18 = load.bool %9
  brcond %18, bb3, bb4
bb3:
  %19 = load.i32 %0
  %20 = const.i64 4
  %21 = ptradd.ptr %0, %20
  %22 = load.i32 %21
  %23 = add.i32 %19, %22
  ret %23
bb4:
  br bb5
bb5:
  %24 = const.i32 0
  ret %24
bb6:
  br bb5
bb7:
  %25 = const.i32 0
  ret %25
}
```

Zeile für Zeile das Wichtigste:

* `@sum` hat einen Parameter: `%0` ist der übergebene Wert, `%1` sein
  Stack-Slot. Die drei `alloca` (`%1` Parameter, `%2` = `s`, `%4` = `i`) stehen
  wie vorgeschrieben vorn im Eintrittsblock — dass `%3` (die Konstante `0`)
  eine kleinere Id hat als `%4`, ist Folge von Invariante 2.
* Die `while`-Schleife hat drei Blöcke: `bb1` Bedingung, `bb2` Rumpf (springt
  mit `br bb1` zurück), `bb3` danach. `bb4` ist der unerreichbare Block hinter
  dem `return`; er wird trotzdem terminiert (Invariante 1) und vom Optimierer
  entfernt.
* In `@main` ist `%0` der 8 Byte große `Point`; `x` liegt bei Offset 0 (deshalb
  `store.i32 %1, %0` ohne `ptradd`), `y` bei Offset 4.
* `%9` ist der Ergebnisplatz des `&&`. `bb1` wertet den rechten Operanden nur
  aus, wenn der linke wahr war — echter Kurzschluss.
* `LIMIT` erscheint als `%7 = const.i32 10`: Konstanten werden beim Lowering
  eingesetzt.
* `bb4`, `bb6` und `bb7` sind unerreichbar bzw. leer. Genau solche Blöcke räumt
  das Entfernen toten Codes weg — im Vergleich `--emit=fir-raw` gegen
  `--emit=fir-opt` gut zu sehen.

---

## 8. Was FIR (noch) nicht hat

Bewusst weggelassen in Stufe 0: Phi-Knoten und SSA-Konstruktion, Aggregate als
Werte, Funktionszeiger/indirekte Aufrufe, globale Variablen (nur `const`, und
die werden eingesetzt), Aliasinformation, Schleifeninformation
(Dominanzbaum/Schleifenerkennung), Debug-Metadaten, Aufruf-Attribute,
Gleitkommatypen und Vektortypen. Die Instruktionsmenge ist so gewählt, dass sie
für diese Erweiterungen Platz lässt, ohne dass Bestehendes umgeschrieben werden
muss.

---

## 9. Vom FIR zum x86_64-Code

Das Backend (`compiler/src/codegen_x86.rs`) uebersetzt FIR direkt in
GNU-Assembler-Text (Intel-Syntax) — ohne LLVM, ohne Cranelift, ohne C.
Die Abbildung ist bewusst einfach und dadurch pruefbar:

* Jeder FIR-Wert `%n` bekommt einen eigenen 8-Byte-Stack-Slot; gerechnet wird in
  `rax`/`rcx` (`rdx` fuer Division/Rest, `rdi`/`rsi`/`rcx` fuer `copymem`).
  Dadurch lebt ueber einen `call` hinweg nie ein Wert in einem Register und die
  callee-saved Register `rbx`, `r12`-`r15` werden nie angefasst.
* `alloca` wird zu einem Bereich im Stackrahmen; die Instruktion selbst ist ein
  `lea` der Adresse in den Slot. Deshalb muessen alle `alloca` im Eintrittsblock
  stehen (Invariante 2) — der Rahmen ist damit statisch gross.
* Ein Basisblock `bbN` von `@f` wird zum Label `.Lf__bbN`; `br` ist `jmp`,
  `brcond` ist `test al, al` + `jnz`/`jmp`, `ret` ist Epilog + `ret`.
* Der Rahmen ist immer ein Vielfaches von 16 Byte. Beim Eintritt gilt
  `rsp % 16 == 8`, `push rbp` gleicht das aus — damit ist der Stack an jeder
  Aufrufstelle 16-ausgerichtet (System-V-AMD64).
* Vorzeichen kommt aus dem FIR-Typ: `div.iN` -> `idiv`, `div.uN` -> `div`,
  `shr.iN` -> `sar`, `shr.uN` -> `shr`, `cmp.lt.iN` -> `setl`,
  `cmp.lt.uN` -> `setb`. `cast` wird zu `movsx`/`movsxd`/`movzx`/`mov`.
* `syscall.i64` legt Argument 0 nach `rax` und die restlichen nach
  `rdi, rsi, rdx, r10, r8, r9` — das Linux-ABI, nicht das Aufruf-ABI.
* `_start` ruft `main` auf und uebergibt `eax` an `exit` (freistehend, ohne libc).

Zu sehen mit `firnc --emit=asm datei.fi -o datei.s` bzw. `--keep-asm`.
