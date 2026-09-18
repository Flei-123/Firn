# Runde XMM 3 -- die zweite Registerklasse

Stand 18.09.2026, Zweig `xmm-ra`. Das ist der Schritt, auf den `docs/TON2.md`
und `docs/XMM1.md` zeigen: **der Registerzuteiler kann jetzt Fliesskomma.**

## Der Satz, der vorher in `regalloc.rs` stand

```rust
// FLOATING POINT: this allocator knows only the integer registers.
if f.val_types.iter().any(|t| t.is_float()) {
    return Some("f64 in the value set".into());
}
```

Jede Funktion, in der auch nur ein `f32` vorkam, ging damit ueber den
Grundweg -- und der gibt JEDEM Zwischenwert einen eigenen Platz im Rahmen.
Sechs Speicherzugriffe fuer eine Multiplikation.

## Was diese Runde gebaut hat

1. **Ein zweiter Durchlauf des Linear Scan** fuer die SSE-Klasse
   (`xmm4`-`xmm15`; `xmm0`-`xmm3` bleiben Kratzregister). Er ist einfacher
   als der Ganzzahl-Durchlauf: alle zwoelf Register sind gleichwertig.
2. **Eine harte Regel statt einer Sicherung:** auf System V sind ALLE
   sechzehn `xmm` caller-saved. Ein Wert, dessen Lebensdauer einen Aufruf
   kreuzt, bekommt deshalb kein Register -- es gibt keines, das der Aufruf
   nicht zerstoert.
3. **`fp_taugt`** -- ein Wert bekommt nur dann ein `xmm`, wenn JEDE Stelle,
   die ihn erzeugt oder liest, im Fliesskommaweg der Ausgabe steht. Der
   Grund ist die Bauart des Erzeugers: an vielen Stellen steht
   `ra.load_full(e, "rax", v)`, und das erzeugt `mov rax, <platz>`. Stuende
   dort ein `xmm`, waere das ein stiller Fehler. Draussen bleiben damit
   Parameter, Aufrufergebnisse, `Select`, gepruefte Umwandlungen und alles,
   was als Aufrufargument dient.
4. **Die Ausgabe** fuer Fliesskomma im Zuteilerweg: Konstanten, `Copy`,
   Laden und Speichern (auch aus befoerderten Zellen), die vier
   Grundrechenarten, Vergleiche (mit der NaN-Regel ueber das Paritaetsbit),
   alle Umwandlungen, Aufrufe, Rueckgabe und der Vorspann.
5. **Die Aufrufkonvention richtig gezaehlt:** System V fuehrt zwei
   Registerfolgen unabhaengig voneinander (Ganzzahlen in `rdi`…`r9`,
   Fliesskomma in `xmm0`-`xmm7`). Der Zuteilerweg zaehlte vorher stur die
   Position -- richtig, solange keine Funktion mit Fliesskomma ankam.

## Vier Fehler, die das Messen gefunden hat

1. **Fliesskomma-Konstanten standen in der Liste der unmittelbaren
   Operanden.** SSE hat keine Form mit Konstante; das Bitmuster als Zahl in
   einen `movss`-Operanden zu setzen ergibt Unsinn. (`immediate_consts`
   ueberspringt sie jetzt.)
2. **`&t` auf eine lokale Gleitzahl** -- eine `alloca`, deren Adresse fest
   im Rahmen steht, hat gar keinen Platz, in dem die Adresse stuende. Sie
   mit `load_full` holen zu wollen, liest einen nie beschriebenen Platz:
   Zeiger ins Nichts, Absturz.
3. **Adressrechnungen, die ganz in den Speicherzugriff gewandert sind**
   (`foldable_addresses`) -- derselbe Fall, andere Ursache. Beides loest
   jetzt `addr_mem`.
4. **NaN.** `ucomis*` setzt bei NaN ZF *und* PF. `sete` allein saehe
   `NaN == NaN` als wahr an, `setne` saehe `NaN != NaN` als falsch. Das
   Paritaetsbit korrigiert es -- und weil die Korrektur zwischen Vergleich
   und Sprung nicht passt, wird fuer Gleitzahl-Gleichheit nicht mehr
   verschmolzen.

## Die Messung

Alles `--opt-level=release-fast`, dieselbe Maschine, dasselbe Programm.

| Messfall | Anfang | nach XMM1+2 | **nach XMM3** | C (`gcc -O2`) |
|---|---|---|---|---|
| Messkern `f32`, 2 Mio Durchlaeufe | 0,50 s | 0,21 s | **0,05 s** | 0,03 s |
| MP3-Dekoder, 60 s Audio | 2,07 s | 1,79 s | **1,28 s** | 0,34 s |

Der Abstand zu C beim reinen Rechenkern faellt damit von **17x auf 1,7x**.
Der Dekoder liegt bei 3,8x; was dort noch fehlt, ist keine
Fliesskommaarbeit mehr, sondern Huffman-Bits und Adressrechnung.

Richtigkeit: die Ausgabe des Dekoders ist weiterhin **bitgleich**
(`tools/ton_bauen.sh` -> PASS 4/4), und die Fliesskomma-Tests des Repos
(1101-1104, 1182, 1453, 111, 1002) bestehen in `dev` wie in
`release-fast`.

`FIRN_NO_FP_RA=1` laesst die Gleitzahlen auf ihren Plaetzen -- der Schalter
bleibt, weil er beim Eingrenzen der vier Fehler oben die halbe Arbeit
gemacht hat.

## Was als Naechstes moeglich waere

* **Sichern um Aufrufe herum:** Werte, deren Lebensdauer einen Aufruf
  kreuzt, koennten ein Register behalten, wenn der Erzeuger sie vor dem
  Aufruf schreibt und danach liest. Lohnt sich nur, wenn gemessen ist, dass
  die betroffenen Werte heiss sind.
* **Parameter in Registern:** heute schreibt der Vorspann jeden
  Fliesskomma-Parameter in seinen Platz.
* **Die restlichen Ops** (`Select`, gepruefte Umwandlung, `Un`) in den
  Fliesskommaweg holen, damit weniger Funktionen ueber den Grundweg gehen.
