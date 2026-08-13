# ABNAHME.md — Ist Firn bereit für die Browser-Engine?

**Maßgeblich:** `../karstos-browser/FIRN-ANFORDERUNGEN.md` §13
**Stand dieser Datei:** 2026-08-13 · **Gesamtergebnis: 0 von 6 Punkten**

Diese Datei ist die einzige Stelle, an der der *Umsetzungsstand* steht. `SPEC.md`
beschreibt das Ziel, `README.md` den Compiler, **hier** wird abgehakt.

## Regeln für das Abhaken

1. Ein Punkt wird **nur** grün, wenn er **gemessen** wurde. Eine Zahl ohne
   Messung ist kein Ergebnis.
2. Die Messung muss **reproduzierbar** sein: Befehl, Datum, Rechner, Ergebnis.
3. **Teilerfolge werden als Zahl geführt**, nicht als „fast fertig".
4. Ein Punkt darf **zurückgestuft** werden, wenn eine Regression auftritt.
5. Wer abhakt, trägt Befehl und Ausgabe ein — nicht nur ein Häkchen.

Legende: `[ ]` offen · `[~]` teilweise, mit Zahl · `[x]` bestanden und gemessen

---

## Die sechs Punkte

### 1. `[ ]` Firn compiliert Firn, reproduzierbar in drei Bootstrap-Stufen

| | |
|---|---|
| **Anforderung** | `L1` · `SPEC.md` §11 |
| **Kriterium** | `firnc1` (in Firn) übersetzt `firnc2`; `firnc2` übersetzt sich selbst zu `firnc2'`; `firnc2` und `firnc2'` sind **bit-identisch** |
| **Messbefehl** | `./bootstrap.sh --verify-fixpoint` (existiert noch nicht) |
| **Stand** | Stufe 0 (`firnc0`, in Rust) läuft. Stufen 1–3 nicht begonnen |
| **Blockiert durch** | Modulsystem, `comptime`, Standardbibliothek → ROADMAP Phase 3/4 |
| **Aufwand laut PLAN-FIRN** | Teil von F0.1 |

---

### 2. `[ ]` Speichermodell entschieden **und prototypisch belegt**

| | |
|---|---|
| **Anforderung** | `S1`–`S3`, `S7` · `TODO-FIRN.md` 0.9 · `SPEC.md` §3 |
| **Kriterium** | DOM-Prototyp mit Eltern-/Kind-Zyklen **und** Listener-Zyklen läuft **24 h** ohne Speicherwachstum |
| **Messbefehl** | `./bench/dom_soak.sh --hours 24` (existiert noch nicht); Messgröße RSS über die Zeit, Toleranz: kein monotoner Anstieg nach der Aufwärmphase |
| **Stand Entscheidung** | **`[x]` getroffen und begründet** — Opt-in-Tracing-GC in drei Stufen, `SPEC.md` §3.2/§3.5. Alternativen (Arena+Indizes, Refcount+Weak) mit Begründung verworfen |
| **Stand Beleg** | **`[ ]` nicht belegt.** Kein GC implementiert, kein Prototyp, keine Messung |
| **Teilpunkte** | `S1` deterministisch als Standard: entworfen, in Stufe 0 nur Rohzeiger · `S2` GC-Heap: nur entworfen · `S3` schwache Verweise: nur entworfen · `S4` Finalisierer: nur entworfen · `S5` inkrementell: geplant v0.5 · `S6` Pausenzeiten messbar: geplant · `S7` `Rc`/`Arc`: nur entworfen |
| **Aufwand laut TODO-FIRN** | 0.1 = 2 PM (Entscheidung + Prototyp), 0.9 = 2 PM (Dauerlauf) |
| **Risiko** | Konservatives Stack-Scanning schließt einen kompaktierenden Sammler aus → Fragmentierung im Dauerlauf ist das eigentliche Risiko dieses Punktes |

---

### 3. `[ ]` HTML5-Tokenizer: **100 % `html5lib-tests/tokenizer/` UND ≤ 2× Referenz**

| | |
|---|---|
| **Anforderung** | `TODO-FIRN.md` 0.8 · `P9` · der härteste der sechs Punkte |
| **Kriterium A** | **100 %** der offiziellen `html5lib-tests/tokenizer/`-Fälle bestehen — exakte Zahl, keine Schätzung |
| **Kriterium B** | **≤ 2×** langsamer als eine Referenzimplementierung (z. B. `html5ever`), gemessen auf demselben Rechner, mit demselben Eingabekorpus |
| **Messbefehl** | `./bench/tokenizer.sh` (existiert noch nicht) — gibt bestandene Fälle und Durchsatz in MB/s aus |
| **Stand** | **`[ ]`** Kein Tokenizer, keine Zeichenketten in der Sprache, kein `match`, keine Registerzuteilung |
| **Blockiert durch** | ROADMAP Phase 2 vollständig |
| **Aufwand laut TODO-FIRN** | 3 PM |
| **Warum es hier klemmt** | Kriterium B hängt fast vollständig an `P3` (Registerzuteilung). Stufe 0 legt **jeden** Wert auf den Stack — damit ist ein Faktor 2 unerreichbar. Das ist bekannt, gemessen wird trotzdem, sobald der Tokenizer läuft |

---

### 4. `[ ]` Testrunner mit maschinenlesbarer Ausgabe **und** Debugger mit Quellzeilen

| | |
|---|---|
| **Anforderung** | `W2`, `W3` · `TODO-FIRN.md` 0.3, 0.4 |
| **Kriterium A** | Testrunner gibt Ergebnisse als JSON aus, CI-tauglich, Quoten automatisch als Zahl |
| **Kriterium B** | Debugger zeigt Quellzeilen, Variablen, Haltepunkte — Abnahme: **ein echter Fehler wurde damit gefunden** |
| **Messbefehl** | `firn test --format=json` · `gdb ./programm` zeigt `.fi`-Zeilen |
| **Stand A** | **`[~]` teilweise.** `test.sh` läuft real (166/166 bestanden, Stand 13.08.2026) und meldet klar PASS/FAIL — aber als Text, **nicht** maschinenlesbar |
| **Stand B** | **`[ ]`** keine DWARF-Informationen; `gdb` zeigt nur Assembler |
| **Aufwand laut TODO-FIRN** | 0.3 = 1 PM, 0.4 = 3 PM |

---

### 5. `[ ]` Paketverwaltung baut reproduzierbar auf zwei Rechnern

| | |
|---|---|
| **Anforderung** | `W1` · `TODO-FIRN.md` 0.5 |
| **Kriterium** | Zwei verschiedene Rechner erzeugen aus demselben Quelltextstand ein **bit-identisches** Artefakt |
| **Messbefehl** | `firn build --locked` auf zwei Rechnern, danach `sha256sum` vergleichen |
| **Stand** | **`[ ]`** Es gibt kein Modulsystem und keine Paketverwaltung. `firnc0` übersetzt genau eine Datei |
| **Aufwand laut TODO-FIRN** | 2 PM |

---

### 6. `[ ]` Kompilierzeit-Codegenerierung erzeugt eine Unicode-Tabelle aus der UCD

| | |
|---|---|
| **Anforderung** | `G1`–`G4` · `TODO-FIRN.md` 0.6 · `SPEC.md` §6.4 |
| **Kriterium** | Ein Bauskript liest die Unicode Character Database und erzeugt daraus eine Firn-Tabelle; Größe der erzeugten Tabelle dokumentiert |
| **Messbefehl** | `firn build` erzeugt `generated/unicode_tables.fi`, Größe wird ausgegeben |
| **Stand** | **`[ ]`** Kein Bauskript-Mechanismus, kein `comptime`, kein Modulsystem |
| **Aufwand laut TODO-FIRN** | 2 PM |

---

## Zusammenfassung

| # | Punkt | Stand |
|---|---|---|
| 1 | Selbst-Hosting in drei Stufen | `[ ]` |
| 2 | Speichermodell entschieden **und belegt** | `[~]` Entscheidung getroffen, Beleg fehlt |
| 3 | Tokenizer 100 % html5lib **und** ≤ 2× | `[ ]` |
| 4 | Testrunner **und** Debugger | `[~]` Testsuite läuft (166/166), aber nicht maschinenlesbar; kein Debugger |
| 5 | Paketverwaltung reproduzierbar | `[ ]` |
| 6 | Kompilierzeit-Codegen erzeugt UCD-Tabelle | `[ ]` |

**Gesamt: 0 von 6 bestanden, 2 angefangen.**

**Was das bedeutet:** `VORBEDINGUNGEN.md` §5.3 lässt Block 1 des Browser-Projekts
erst starten, wenn **alle sechs** Punkte grün sind. Firn ist davon weit entfernt
— das ist der erwartete Stand nach einem Stufe-0-Prototyp und kein Rückschlag.
`PLAN-FIRN.md` veranschlagt für Phase F0 insgesamt **27 Personenmonate**.

**Die zwei Punkte, die den Plan kippen können**, sind 2 und 3. Sie sollten so
früh wie möglich gemessen werden — auch unfertig, auch mit schlechtem Ergebnis.
Ein schlechter Messwert in Monat 6 ist wertvoll; derselbe Messwert in Jahr 3 ist
eine Katastrophe.
