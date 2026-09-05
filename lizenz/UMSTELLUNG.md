# Zweig `lizenz` -- was hier liegt und was NICHT passiert ist

Angelegt am 05.09.2026 von der Runde LIZENZ. **Nichts hiervon ist scharf
geschaltet.** Kein SPDX-Kopf wurde geaendert, keine LICENSE-Datei im
Wurzelverzeichnis wurde ersetzt, nichts ist nach `main` gegangen.
Der Zweig enthaelt genau ein Verzeichnis, `lizenz/`, und sonst den
unveraenderten Stand von `main`.

## Die Entscheidung, die umgesetzt werden soll

| Projekt | heute | soll |
|---|---|---|
| **Firn** | MIT (oeffentlich) / lokal GPL-2.0-only + MIT | **MPL-2.0** |
| **Certus** | GPL-2.0-only in 530 SPDX-Zeilen, LICENSE seit 05.09. da | **GPL-3.0-or-later + Namensnennung nach 7(b)** |
| **Osum** | GPL-2.0-only + MIT | **bleibt** |

## Reihenfolge beim Scharfschalten

1. `bash lizenz/spdx-umstellen.sh` -- **Trockenlauf**, Liste lesen.
2. `bash lizenz/spdx-umstellen.sh --pruefen` -- welche Dateien keine
   SPDX-Zeile haben und deshalb ueber `.reuse/dep5` haengen.
3. Erst dann `--echt`, in einem **eigenen Commit**, so wie es am
   27.08.2026 schon einmal gemacht wurde ("SPDX headers: one line per
   source file (SEPARATE COMMIT -- rebase this one on its own)").
4. `lizenz/LICENSE.vorschlag` -> `LICENSE`, `lizenz/NOTICE` -> `NOTICE`,
   bei Certus zusaetzlich `lizenz/ADDITIONAL-TERMS.txt` -> Wurzel.
5. `.reuse/dep5` nachziehen, `LICENSING.md` neu schreiben.
6. README-Absatz einsetzen (`lizenz/README-LIZENZ-ABSATZ.md`).
7. `./test.sh` -- vor allem die Tests, deren Zeile 1 eine Erwartung ist.
8. Erst danach nach `main`, und **nur zusammen mit einem
   Fassungsschnitt** (siehe Bericht, Teil 3): Etikett `v1.0.0` auf den
   letzten MIT-Stand, Etikett `v2.0.0` auf den ersten neuen Stand, und
   ein Absatz in der README, ab welchem Commit was gilt.

## Was noch offen ist, bevor Schritt 8 sinnvoll ist

* **Zweige ohne SPDX-Kopfzeilen.** In Certus sind das u. a. `speed`,
  `mobil`, alle `r35`..`r96`, `b1`..`b6`, `k2`..`k6` --
  in Firn `speed`, `sammeln`, `r35`..`r96`. Wer so einen Zweig nach
  `main` mergt, kippt die Kopfzeilen wieder heraus. Erst mergen, dann
  umstellen -- oder nach jedem Merge das `--pruefen` laufen lassen.
* **Der Certus-Code liegt zweimal.** `firn:main` traegt noch
  `lib/browser`, `lib/css`, `lib/dom`, `lib/font`, `lib/html`, `lib/js`,
  `lib/layout`, `lib/net`, `lib/paint`, `lib/tls` -- dieselben Module,
  die seit 30.08. das eigene Certus-Repo bilden. Unter MPL fuer Firn und
  GPL-3.0 fuer Certus haetten dieselben Dateien in zwei Repos zwei
  Lizenzen. Das muss aufgeloest werden: entweder im Firn-Repo loeschen
  (sie leben jetzt im Certus-Repo) oder dort ausdruecklich auf
  GPL-3.0-or-later setzen.
* **Der DejaVu-Lizenztext** fehlt in Osum und in Firn
  (`tests/data/fonts/FirnSans.ttf`). Siehe `lizenz/OSUM-BEFUND.md`.
