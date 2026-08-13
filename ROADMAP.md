# Firn — Fahrplan

**Stand:** 2026-08-13 · **Bezug:** `SPEC.md` · Zeitangaben = Arbeitsaufwand einer
Person mit KI-Unterstützung, nicht Kalenderzeit.

---

## Wie realistisch ist das?

Zum Vergleich, ohne Schönfärberei:

| Sprache | Erster Compiler | Version 1.0 / stabil | Dauer |
|---|---|---|---|
| Rust | 2006 (Graydon, in OCaml) | Mai 2015 | **9 Jahre** |
| Zig | 2015 | noch nicht (0.16, 2026) | **11+ Jahre** |
| Go | 2007 | März 2012 | 5 Jahre, mit Google-Team |
| Odin | 2016 | noch nicht stabil | 10 Jahre |

Firn wird nicht schneller fertig, nur weil KI mitschreibt. KI beschleunigt das
Tippen, nicht die Entwurfsentscheidungen und nicht das Finden der Fehler, die
erst auftauchen, wenn 50.000 Zeilen echter Code in der Sprache geschrieben sind.
Was KI wirklich ändert: Die frühen Phasen (Parser, Typprüfer, Codegen für eine
Teilmenge) schrumpfen von Monaten auf Tage. Die späten Phasen (Selbst-Hosting,
Optimierer, Stabilität, Ökosystem) schrumpfen kaum.

**Ehrliche Erwartung:** *Nutzbar für kleine Karstos-Systemprogramme* in 6–12
Monaten. *Ein selbst-hostender Compiler* in 1–2 Jahren. *Der karst-Kernel in
Firn statt Rust* eher 3–5 Jahre — und nur, wenn das Projekt durchgehalten wird.

---

## Phase 0 — Spezifikation ✔ (fertig)

* `SPEC.md`: Profile, Speichermodell (Ownership + zweitklassige Referenzen),
  Fehlerbehandlung, `comptime`, Backend-Strategie, Bootstrap-Stufen, Grammatik.
* Entscheidung gegen LLVM im Bootstrap-Pfad, Entscheidung gegen GC — beide
  begründet und mit benanntem Preis.

## Phase 1 — `firnc0`: Prototyp in Rust *(diese Runde)*

**Ziel:** Die Teilmenge aus SPEC §12 wirklich bis zum laufenden Binary.

* Lexer, rekursiv absteigender Parser, gute Fehlermeldungen (Zeile/Spalte/Auszug)
* Typprüfer für Ganzzahlen fester Breite, `bool`, Zeiger, Structs, Arrays
* FIR (eigene IR, Basisblöcke) + Konstantenfaltung + Entfernen toten Codes
* x86_64-Codegen ohne LLVM → Assembler für `as`/`ld`
* `syscall(...)` eingebaut → Ausgabe ohne libc
* ≥ 40 `.fi`-Testprogramme, `test.sh` baut und fährt alles
* **Aufwand:** 1–2 Wochen · **Ergebnis:** kompilierbare Sprache, kein Werkzeug

## Phase 2 — v0.2: benutzbar für kleine Programme

* Zeichenketten (`[]u8` + Länge), `for`-Schleife, `break`/`continue`
* Aufzählungen mit Nutzdaten, `match` mit Erschöpfungsprüfung
* Fehlerunionen `!T`, `try`, `catch`, `errdefer`
* `defer`, `drop`, Move-Prüfer (§3 der Spezifikation — das Herzstück)
* Referenztypen `&T` / `inout T` mit Zweitklassigkeitsprüfung
* Bessere Registerzuteilung (Lebendigkeitsanalyse statt naiv)
* **Aufwand:** 4–8 Wochen

## Phase 3 — v0.3: Module und `comptime`

* Modulsystem, `import`, `export`-Listen, getrennte Übersetzung
* `comptime`-Auswertung im Compiler (Interpreter über FIR)
* Generics durch Monomorphisierung
* Minimale Standardbibliothek: `Arena`, `Vec[T]`, `Str`, `io`
* **Stufe 1 beginnt:** Lexer und Parser werden in Firn neu geschrieben
* **Aufwand:** 2–4 Monate

## Phase 4 — v0.4/0.5: Selbst-Hosting

* Der gesamte Compiler in Firn: `firnc1` übersetzt `firnc2`, `firnc2` übersetzt
  sich selbst, Ergebnis bit-identisch (Fixpunkt)
* Rust wird zum Bootstrap-Archiv; `firnc0` eingefroren
* aarch64-Backend (Raspberry Pi, ARM-Server)
* Debug-Informationen (DWARF-Grundlagen), damit `gdb` benutzbar wird
* **Aufwand:** 6–12 Monate · **Das ist der Punkt, an dem Firn eine echte Sprache ist**

## Phase 5 — v0.6: WASM und Web

* wasm32-Backend aus FIR (Stackifier für strukturierten Kontrollfluss)
* Minimale DOM-Anbindung, damit Frontend ohne JavaScript möglich ist
* **Aufwand:** 2–4 Monate

## Phase 6 — v0.7+: Karstos in Firn

* Kernel-Profil gegen echten karst-Code prüfen (ABI, Inline-Assembler, MMIO)
* Erstes Karstos-Modul in Firn (Kandidat: ein Treiber, klein und isoliert)
* Danach schrittweise Ersetzung — kein „großer Neuschrieb"
* **Aufwand:** Jahre, parallel zur Karstos-Entwicklung

## Phase 7 — v1.0: Stabilität

* Sprachstabilitätsversprechen, Rückwärtskompatibilität
* Optimierer (GVN, Inlining, Schleifenoptimierung), optionales LLVM-Backend
  für Anwendungscode als Vergleichsmaßstab
* Paketverwaltung, Dokumentationswerkzeug, Formatierer
* **Zeitpunkt:** frühestens in mehreren Jahren

---

## Woran das Projekt scheitern kann

Offen benannt, damit es nicht überrascht:

1. **Durchhalten.** Der gefährlichste Punkt ist Phase 3/4 — der Reiz ist weg,
   die Arbeit wird zäh (Fehlermeldungen, Randfälle, Regressionen).
2. **Codegen-Qualität.** Eigener Codegen ohne LLVM heißt auf Jahre 2–5x
   langsamerer Code. Für den Kernel egal, für Anwendungen irgendwann nicht.
3. **Selbstbezug.** Ein Compiler, der sich selbst übersetzt, verbirgt Fehler
   hervorragend. Gegenmittel: Fixpunkt-Prüfung und eine Testsuite, die von
   Anfang an ernst genommen wird.
4. **Zwei Baustellen gleichzeitig.** Karstos *und* Firn parallel ist viel. Firn
   darf Karstos nicht ausbremsen — deshalb bleibt Rust im Kernel, bis Firn
   nachweislich besser passt.

---

## Nächster konkreter Schritt

Phase 1 abschließen und `test.sh` grün bekommen. Danach entscheiden, ob zuerst
der Move-Prüfer (Phase 2, Sprachkern) oder das Modulsystem (Phase 3,
Benutzbarkeit) kommt.
