#!/usr/bin/env bash
# tools/pakete/run.sh — das Paket- und Projektsystem (Runde 48).
#
# Geprueft wird DREIERLEI, an echten Projekten auf der Platte:
#
#   1. Der Bau-Treiber `--paket` uebersetzt ein Projekt anhand seines
#      Manifests, das Ergebnis LAEUFT und gibt das Erwartete aus.
#   2. Jede Fehlerlage (privates Modul, fremdes Paket, Zyklus, kaputtes
#      Manifest, Namenskonflikt) wird erkannt — Exit-Code 2 und eine
#      Meldung, die den Grund nennt.
#   3. `firnc0` (Rust) und `firnc1` (Firn) verhalten sich GLEICH: jeder Fall
#      laeuft durch BEIDE Uebersetzer, und ihre Meldungen werden Oktett fuer
#      Oktett verglichen. Ein Paketsystem, das nur in einem der beiden
#      Uebersetzer stimmt, waere keins.
#
# Eigenes `mktemp -d` je Lauf: auf dieser Maschine laufen mehrere Runden
# gleichzeitig, feste /tmp-Namen wuerden sich gegenseitig ueberschreiben.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)

FIRNC="$ROOT/compiler/target/release/firnc"
FC1="$ROOT/.firnc1"
export FIRNLIB="$ROOT/lib"

if [ ! -x "$FIRNC" ]; then
    echo "firnc0 fehlt: $FIRNC"
    exit 1
fi

# LEKTION aus den Runden 35/45/46: nie ein Binary wiederverwenden, nur weil
# es existiert. Ist `firnc0` oder eine Quelle juenger, wird `.firnc1` neu
# gebaut — sonst misst dieser Lauf einen Compiler, den es nicht mehr gibt.
neu_bauen=0
[ -x "$FC1" ] || neu_bauen=1
if [ -x "$FC1" ]; then
    [ "$FIRNC" -nt "$FC1" ] && neu_bauen=1
    while IFS= read -r q; do
        [ "$q" -nt "$FC1" ] && { neu_bauen=1; break; }
    done < <(find bin lib -name '*.fi' -not -type l)
fi
if [ "$neu_bauen" -eq 1 ]; then
    "$FIRNC" bin/firnc1.fi -o "$FC1" || { echo "firnc1 laesst sich nicht bauen"; exit 1; }
fi

WORK=$(mktemp -d "${TMPDIR:-/tmp}/firn-pakete.XXXXXXXX")
trap 'rm -rf "$WORK"' EXIT

OK=0
BAD=0
fall() { printf "  %-58s" "$1"; }
gut()  { OK=$((OK + 1)); echo "ok"; }
weh()  { BAD=$((BAD + 1)); echo "FEHLER"; for z in "$@"; do printf '      %s\n' "$z"; done; }

# Beide Uebersetzer auf denselben Fall loslassen.
beide() {
    local kennung="$1"; shift
    "$FIRNC" "$@" > "$WORK/$kennung.0.out" 2> "$WORK/$kennung.0.err"
    echo $? > "$WORK/$kennung.0.rc"
    "$FC1"   "$@" > "$WORK/$kennung.1.out" 2> "$WORK/$kennung.1.err"
    echo $? > "$WORK/$kennung.1.rc"
}

# Erwartung an einen FEHLERFALL: beide melden Exit 2, dieselbe Meldung,
# und der Text enthaelt das gesuchte Stichwort.
erwarte_fehler() {
    local kennung="$1" stichwort="$2"
    local rc0 rc1
    rc0=$(cat "$WORK/$kennung.0.rc")
    rc1=$(cat "$WORK/$kennung.1.rc")
    if [ "$rc0" != "2" ]; then
        weh "firnc0 gab Exit $rc0, erwartet 2" "$(head -2 "$WORK/$kennung.0.err")"
        return
    fi
    if [ "$rc1" != "2" ]; then
        weh "firnc1 gab Exit $rc1, erwartet 2" "$(head -2 "$WORK/$kennung.1.err")"
        return
    fi
    if ! grep -qF "$stichwort" "$WORK/$kennung.0.err"; then
        weh "Meldung ohne '$stichwort'" "$(head -2 "$WORK/$kennung.0.err")"
        return
    fi
    if ! cmp -s "$WORK/$kennung.0.err" "$WORK/$kennung.1.err"; then
        weh "firnc0 und firnc1 melden Verschiedenes" \
            "0: $(head -1 "$WORK/$kennung.0.err")" \
            "1: $(head -1 "$WORK/$kennung.1.err")"
        return
    fi
    gut
}

# Eine eigene Kopie des Beispielprojekts je Fall.
kopie() {
    rm -rf "$WORK/$1"
    cp -r beispiele/pakete "$WORK/$1"
    echo "$WORK/$1"
}

echo "== Paket- und Projektsystem (Runde 48) =="

# --- 1/2: das Beispielprojekt im Repo, mit BEIDEN Uebersetzern ------------

for c in 0 1; do
    if [ "$c" = 0 ]; then CC="$FIRNC"; NAME="firnc0"; else CC="$FC1"; NAME="firnc1"; fi
    fall "$NAME baut beispiele/pakete/anwendung"
    if ! "$CC" --paket beispiele/pakete/anwendung -o "$WORK/anw$c" \
            > "$WORK/bau$c.log" 2>&1; then
        weh "Uebersetzen schlug fehl" "$(head -4 "$WORK/bau$c.log")"
    else
        aus=$("$WORK/anw$c"); rc=$?
        if [ "$rc" -ne 0 ]; then
            weh "Programm endete mit Exit $rc"
        elif [ "$aus" != "12 14 3" ]; then
            weh "Ausgabe '$aus', erwartet '12 14 3'"
        else
            gut
        fi
    fi
done

# --- 3: `--paket` ohne `-o` legt das Binary unter dem Paketnamen ab -------

fall "--paket ohne -o benennt nach dem Manifest"
P=$(kopie f_name_aus)
if "$FIRNC" --paket "$P/anwendung" >/dev/null 2>&1 \
   && [ -x "$P/anwendung/anwendung" ] \
   && [ "$("$P/anwendung/anwendung")" = "12 14 3" ]; then
    gut
else
    weh "kein lauffaehiges '$P/anwendung/anwendung'"
fi

# --- 4: --paket-info, zeichengleich in beiden Uebersetzern ---------------

fall "--paket-info ist in beiden Uebersetzern gleich"
beide info --paket-info beispiele/pakete/anwendung
if [ "$(cat "$WORK/info.0.rc")" != 0 ] || [ "$(cat "$WORK/info.1.rc")" != 0 ]; then
    weh "Exit-Codes $(cat "$WORK/info.0.rc")/$(cat "$WORK/info.1.rc")"
elif ! cmp -s "$WORK/info.0.out" "$WORK/info.1.out"; then
    weh "Berichte unterscheiden sich" "$(diff "$WORK/info.0.out" "$WORK/info.1.out" | head -4)"
elif ! grep -q '^brauche geo beispiele/pakete/geo$' "$WORK/info.0.out"; then
    weh "Abhaengigkeit fehlt im Bericht" "$(head -8 "$WORK/info.0.out")"
else
    gut
fi

# --- 5: privates Modul einer Abhaengigkeit -------------------------------

fall "privates Modul einer Abhaengigkeit wird abgelehnt"
P=$(kopie f_privat)
sed -i 's/^import geo.punkt$/import geo.innen/' "$P/anwendung/src/main.fi"
beide privat --paket "$P/anwendung" -o "$WORK/f_privat.bin"
erwarte_fehler privat "ist in paket 'geo' nicht oeffentlich"

# --- 6: ein Paket, das nicht als Abhaengigkeit eingetragen ist -----------
#
# `geheim` liegt IM Quellbaum von `app` und ist ein eigenes Paket; `app`
# traegt es nicht ein. Der Import findet die Datei ueber den Weg (1), die
# Sichtbarkeitspruefung muss trotzdem greifen.

fall "Paket ohne 'brauche' wird abgelehnt"
mkdir -p "$WORK/f_fremd/app/src/geheim" "$WORK/f_fremd/h"
cat > "$WORK/f_fremd/app/firn.paket" <<'EOF'
paket app
version 0.1.0
start src/main.fi
quelle src
brauche h ../h
EOF
cat > "$WORK/f_fremd/app/src/main.fi" <<'EOF'
import geheim.geheim

fn main() -> i32 {
    return geheim.wert()
}
EOF
cat > "$WORK/f_fremd/app/src/geheim/firn.paket" <<'EOF'
paket geheim
version 0.1.0
EOF
cat > "$WORK/f_fremd/app/src/geheim/geheim.fi" <<'EOF'
export { wert }
fn wert() -> i32 { return 5 }
EOF
cat > "$WORK/f_fremd/h/firn.paket" <<'EOF'
paket h
version 0.1.0
brauche geheim ../app/src/geheim
EOF
cat > "$WORK/f_fremd/h/h.fi" <<'EOF'
export { hilf }
fn hilf() -> i32 { return 1 }
EOF
beide fremd --paket "$WORK/f_fremd/app" -o "$WORK/f_fremd.bin"
erwarte_fehler fremd "paket 'geheim' ist keine abhaengigkeit von paket 'app'"

# --- 7: Paketzyklus ------------------------------------------------------

fall "Paketzyklus wird gemeldet"
P=$(kopie f_zyklus)
printf 'brauche anwendung ../anwendung\n' >> "$P/geo/firn.paket"
beide zyklus --paket "$P/anwendung" -o "$WORK/f_zyklus.bin"
erwarte_fehler zyklus "paketzyklus: anwendung -> geo -> anwendung"

# --- 8: Abhaengigkeit ohne Manifest --------------------------------------

fall "Abhaengigkeit ohne Manifest wird gemeldet"
P=$(kopie f_kein_manifest)
rm -f "$P/geo/firn.paket"
beide keinman --paket "$P/anwendung" -o "$WORK/f_km.bin"
erwarte_fehler keinman "abhaengigkeit 'geo' hat kein manifest"

# --- 9: Abhaengigkeit zeigt auf ein anders benanntes Paket ---------------

fall "falscher Paketname in der Abhaengigkeit"
P=$(kopie f_name)
sed -i 's/^paket        geo$/paket        geometrie/' "$P/geo/firn.paket"
beide falschname --paket "$P/anwendung" -o "$WORK/f_name.bin"
erwarte_fehler falschname "abhaengigkeit 'geo' zeigt auf paket 'geometrie'"

# --- 10: kaputte Versionsangabe ------------------------------------------

fall "ungueltige Version im Manifest"
P=$(kopie f_version)
sed -i 's/^version      0.2.0$/version      0.2/' "$P/geo/firn.paket"
beide version --paket "$P/anwendung" -o "$WORK/f_ver.bin"
erwarte_fehler version "ungueltige version '0.2' (erwartet zahl.zahl.zahl)"

# --- 11: unbekannter Schluessel ------------------------------------------

fall "unbekannter Schluessel im Manifest"
P=$(kopie f_schluessel)
sed -i 's/^oeffentlich  geo punkt$/oeffentlih  geo punkt/' "$P/geo/firn.paket"
beide schluessel --paket "$P/anwendung" -o "$WORK/f_sch.bin"
erwarte_fehler schluessel "unbekannter schluessel 'oeffentlih'"

# --- 12: fehlende Pflichtzeile -------------------------------------------

fall "Manifest ohne 'paket'-Zeile"
P=$(kopie f_ohne_paket)
sed -i 's/^paket    anwendung$//' "$P/anwendung/firn.paket"
beide ohnepaket --paket "$P/anwendung" -o "$WORK/f_op.bin"
erwarte_fehler ohnepaket "das manifest braucht eine zeile 'paket <name>'"

# --- 13: Namenskonflikt zweier Module ------------------------------------

fall "zwei Module gleichen Namens werden gemeldet"
P=$(kopie f_konflikt)
cat > "$P/geo/src/hilfe.fi" <<'EOF'
export { wert }
fn wert() -> i32 { return 7 }
EOF
sed -i 's/^oeffentlich  geo punkt$/oeffentlich  geo punkt hilfe/' "$P/geo/firn.paket"
cat > "$P/anwendung/src/main.fi" <<'EOF'
import hilfe
import geo.hilfe

fn main() -> i32 {
    hilfe.trenner()
    return 0
}
EOF
beide konflikt --paket "$P/anwendung" -o "$WORK/f_konf.bin"
erwarte_fehler konflikt "namenskonflikt: modul 'hilfe' kommt aus zwei dateien"

# --- 14: `--paket` auf eine Bibliothek ohne Einstiegspunkt ---------------

fall "Bibliothek ohne 'start' laesst sich nicht bauen"
beide biblio --paket beispiele/pakete/geo -o "$WORK/f_bib.bin"
erwarte_fehler biblio "das manifest hat keinen einstiegspunkt"

# --- 15: `--paket` auf ein Verzeichnis ohne Manifest ---------------------

fall "Verzeichnis ohne Manifest wird gemeldet"
mkdir -p "$WORK/leer"
beide leer --paket "$WORK/leer" -o "$WORK/f_leer.bin"
erwarte_fehler leer "kein manifest in"

# --- 16: privates Modul IM eigenen Paket bleibt erlaubt ------------------

fall "innerhalb eines Pakets gibt es keine Schranke"
P=$(kopie f_intern)
cat > "$P/anwendung/src/main.fi" <<'EOF'
import geo

fn main() -> i32 {
    // geo.umfang rechnet ueber geo.innen — ein Modul, das NICHT oeffentlich
    // ist. Innerhalb des Pakets 'geo' ist das erlaubt.
    return geo.umfang(geo.rechteck_neu(0, 0, 3, 4))
}
EOF
"$FIRNC" --paket "$P/anwendung" -o "$WORK/f_intern0.bin" > "$WORK/f_intern.log" 2>&1
rc0=$?
"$WORK/f_intern0.bin" >/dev/null 2>&1; lauf0=$?
"$FC1" --paket "$P/anwendung" -o "$WORK/f_intern1.bin" >> "$WORK/f_intern.log" 2>&1
rc1=$?
"$WORK/f_intern1.bin" >/dev/null 2>&1; lauf1=$?
if [ "$rc0" -eq 0 ] && [ "$rc1" -eq 0 ] && [ "$lauf0" -eq 14 ] && [ "$lauf1" -eq 14 ]; then
    gut
else
    weh "Exit $rc0/$rc1, Lauf $lauf0/$lauf1 (erwartet 0/0 und 14/14)" \
        "$(head -4 "$WORK/f_intern.log")"
fi

# --- 17: ohne Manifest aendert sich NICHTS -------------------------------

fall "ohne Manifest bleibt die Aufloesung von Runde 47"
"$FIRNC" tests/110_module.fi -o "$WORK/alt0" >/dev/null 2>&1 && "$WORK/alt0"
a0=$?
"$FC1" tests/110_module.fi -o "$WORK/alt1" >/dev/null 2>&1 && "$WORK/alt1"
a1=$?
if [ "$a0" -eq 60 ] && [ "$a1" -eq 60 ]; then
    gut
else
    weh "Exit $a0/$a1, erwartet 60/60"
fi

# --- 18: Suchreihenfolge — die eigene Quelle gewinnt ---------------------

fall "Projektquelle gewinnt vor gleichnamigem Modul der Abhaengigkeit"
P=$(kopie f_vorrang)
cat > "$P/anwendung/src/zahl.fi" <<'EOF'
export { wert }
fn wert() -> i32 { return 1 }
EOF
cat > "$P/text/src/zahl.fi" <<'EOF'
export { wert }
fn wert() -> i32 { return 2 }
EOF
cat > "$P/anwendung/src/main.fi" <<'EOF'
import zahl

fn main() -> i32 {
    return zahl.wert()
}
EOF
"$FIRNC" --paket "$P/anwendung" -o "$WORK/f_vor0.bin" >/dev/null 2>&1 && "$WORK/f_vor0.bin"
v0=$?
"$FC1" --paket "$P/anwendung" -o "$WORK/f_vor1.bin" >/dev/null 2>&1 && "$WORK/f_vor1.bin"
v1=$?
if [ "$v0" -eq 1 ] && [ "$v1" -eq 1 ]; then
    gut
else
    weh "Exit $v0/$v1, erwartet 1/1 (die eigene Quelle)"
fi

# --- 19: Manifest wird von der Quelldatei aus nach OBEN gefunden ---------

fall "Manifest wird auch ohne --paket nach oben gefunden"
P=$(kopie f_aufwaerts)
"$FIRNC" "$P/anwendung/src/main.fi" -o "$WORK/f_auf0.bin" >/dev/null 2>&1
r0=$?
"$FC1" "$P/anwendung/src/main.fi" -o "$WORK/f_auf1.bin" >/dev/null 2>&1
r1=$?
if [ "$r0" -eq 0 ] && [ "$r1" -eq 0 ] \
   && [ "$("$WORK/f_auf0.bin")" = "12 14 3" ] \
   && [ "$("$WORK/f_auf1.bin")" = "12 14 3" ]; then
    gut
else
    weh "Exit $r0/$r1 oder falsche Ausgabe"
fi

# --- 20: mehrere Quellverzeichnisse in einem Paket -----------------------

fall "zweites 'quelle'-Verzeichnis wird durchsucht"
P=$(kopie f_zweitquelle)
mkdir -p "$P/anwendung/extra"
printf 'quelle   extra\n' >> "$P/anwendung/firn.paket"
cat > "$P/anwendung/extra/zusatz.fi" <<'EOF'
export { drei }
fn drei() -> i32 { return 3 }
EOF
cat > "$P/anwendung/src/main.fi" <<'EOF'
import zusatz

fn main() -> i32 {
    return zusatz.drei()
}
EOF
"$FIRNC" --paket "$P/anwendung" -o "$WORK/f_zq0.bin" >/dev/null 2>&1 && "$WORK/f_zq0.bin"
z0=$?
"$FC1" --paket "$P/anwendung" -o "$WORK/f_zq1.bin" >/dev/null 2>&1 && "$WORK/f_zq1.bin"
z1=$?
if [ "$z0" -eq 3 ] && [ "$z1" -eq 3 ]; then
    gut
else
    weh "Exit $z0/$z1, erwartet 3/3"
fi

echo
echo "PAKETE: $OK bestanden, $BAD fehlgeschlagen"
[ "$BAD" -eq 0 ] || exit 1
exit 0
