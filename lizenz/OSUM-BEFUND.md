# Osum -- Befund der Runde LIZENZ (05.09.2026)

**Entscheidung: Osum bleibt, wie es ist.** GPL-2.0-only fuer den Kernel und
die 70 Ring-3-Programme, MIT fuer die 18 Dateien, die ein Ring-3-Programm
dazulinkt. Das ist genau das Modell von Linux (GPLv2-only) plus einer
permissiven Userspace-Schicht, und es erfuellt Justins Wunsch fuer den
Kernel vollstaendig.

Es gibt hier trotzdem **drei offene Punkte**, alle drei aelter als diese
Runde und keiner davon durch die Entscheidung erledigt:

## 1. Auf GitHub steht noch MIT (der wichtigste Punkt)

`origin/main` ist `3389fbd` vom 27.08.2026 und traegt **nur** eine
LICENSE-Datei, und darin steht der MIT-Text. Der Lizenzwechsel-Commit
`b5654e1` ("Round INVENTORY: THIRD_PARTY.md, the licence change, and the
SPDX headers") ist auf **zehn Nebenzweigen** gepusht
(`origin/a11y`, `customres`, `fsrobust`, `hwnet`, `init`, `kvmfix`,
`look`, `multiuser`, `paint`, `sshd`), aber **nicht auf `main`**.

Wer heute auf github.com/Flei-123/Osum schaut, sieht MIT.
GitHub zeigt den Standardzweig. Der Wechsel ist damit oeffentlich
faktisch nicht vollzogen.

**Zu tun:** `main` pushen. Ein Klick, und der wichtigste Teil der Arbeit
vom 27.08. wird sichtbar.

## 2. Der DejaVu-Lizenztext fehlt im Baum

`assets/osum-sans.ttf` (18.676 Oktett), `assets/osum-mono.ttf`
(14.604 Oktett) und die Bitmap-Tabelle in `kernel/font.fi` (130 Zeilen,
1.520 Oktett) sind aus **DejaVu Sans** bzw. **DejaVu Sans Mono**
herausgeschnitten (`tools/ttf/schnitt.py`). Das steht offen und korrekt
in `THIRD_PARTY.md` Abschnitt 4 und in `LICENSING.md` 5.2 -- aber der
**Lizenztext der Bitstream-Vera-/DejaVu-Lizenz liegt nicht im Repo**,
und aus den `name`-Tabellen der TTFs ist der Copyright-Eintrag heraus.

Die Bitstream-Vera-Lizenz verlangt genau das Gegenteil: der
Urhebervermerk und der Lizenztext muessen mit den Font-Dateien
mitgeliefert werden.

**Zu tun (eine halbe Stunde):** `assets/FONT-LICENSE.txt` mit dem
Bitstream-Vera-/DejaVu-Text anlegen und in `LICENSE`, `NOTICE` und
`THIRD_PARTY.md` darauf verweisen. Das ist der einzige Punkt in allen
drei Repos, der eine echte Lizenzverletzung im AUSGELIEFERTEN System
ist -- die Fonts laufen auf der Maschine mit.

## 3. Wenn Firn auf MPL-2.0 geht

Osum bindet Firn ueber `vendor/firn/COMMIT` ein und benutzt daraus
`lib/net/{wire,tcp,stack}.fi` im Kernel (`vendor/net/PROVENANCE.md`).
Der Kernel ist GPL-2.0-only.

Das geht: MPL-2.0 Abschnitt 1.12 zaehlt "the GNU General Public License,
Version 2.0 ... or any later versions" ausdruecklich als **Secondary
License**, und Abschnitt 3.3 erlaubt, die Covered Software in einem
"Larger Work" zusaetzlich unter dieser Secondary License zu verteilen.
Voraussetzung ist, dass Firn **nicht** als "Incompatible With Secondary
Licenses" gekennzeichnet wird -- also dass MPL Exhibit B im Firn-Repo
NICHT gesetzt wird.

**Zu tun:** in `vendor/net/PROVENANCE.md` und in `THIRD_PARTY.md` einen
Satz nachtragen, dass der eingebundene Firn-Stand MPL-2.0 ist und ueber
MPL 3.3 als GPL-2.0 weitergegeben wird. Und: Exhibit B im Firn-Repo
niemals setzen.
