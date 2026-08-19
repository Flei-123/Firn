#!/usr/bin/env bash
# Erzeugt die Rc-Testprogramme aus der EINEN Implementierung
# (tests/modules/rc.fi, Symlink lib/rc/rc.fi) und den Testrumpfen in
# lib/rc/teile/.
#
# Warum ueberhaupt zusammenkopieren statt 'import modules.rc'?
# Stufe 0 loest generische Vorlagen nicht ueber Modulgrenzen auf: weder
# 'rc.Zaehlverweis[T]' in Typstellung noch 'rc.rc_neu[T](..)' im Aufruf
# lassen sich uebersetzen ("erwartet '=' nach dem namen ..." bzw. "nur
# direkte funktionsnamen koennen aufgerufen werden"). Genau dieselbe Loesung
# benutzt bereits lib/str (tools/strlib/expand.py): die Bibliothek steht
# einmal im Baum und wird woertlich in die Testprogramme eingesetzt.
# Ausfuehren:  bash lib/rc/erzeuge_tests.sh
set -euo pipefail
cd "$(dirname "$0")/../.."
MOD=tests/modules/rc.fi

erzeuge() {                 # $1 = Rumpf, $2 = Zieldatei
    local rumpf="$1" ziel="$2"
    {
        head -1 "$rumpf"
        echo "// ERZEUGT von lib/rc/erzeuge_tests.sh: Zeile 1 und der Rumpf aus"
        echo "// $rumpf, dazwischen tests/modules/rc.fi WOERTLICH."
        cat "$MOD"
        tail -n +2 "$rumpf"
    } > "$ziel"
}

for r in lib/rc/teile/5*.fi; do
    erzeuge "$r" "tests/$(basename "$r")"
    echo "  tests/$(basename "$r")"
done
for r in lib/rc/teile/neg_*.fi; do
    n=$(basename "$r"); n=${n#neg_}
    erzeuge "$r" "tests/neg/$n"
    echo "  tests/neg/$n"
done

# Runde 47: dieselbe Mechanik fuer `Arc[T]` (lib/rc/arc.fi).
MOD=lib/rc/arc.fi
for r in lib/rc/teile/83*.fi; do
    erzeuge "$r" "tests/$(basename "$r")"
    echo "  tests/$(basename "$r")"
done
for r in lib/rc/teile/negarc_*.fi; do
    n=$(basename "$r"); n=${n#negarc_}
    erzeuge "$r" "tests/neg/$n"
    echo "  tests/neg/$n"
done
