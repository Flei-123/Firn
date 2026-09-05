#!/usr/bin/env bash
# =====================================================================
# RUNDE KODIERER -- DIE ABNAHME
#
# Fuer jede Firn-Quelle im Baum:
#   1. Assemblertext erzeugen (firnc --emit=asm), in mehreren Baustufen
#   2. denselben Text EINMAL durch `as`  -> Objektdatei A
#      und           EINMAL durch den eigenen Kodierer -> Objektdatei B
#   3. .text, .data, .rodata OKTETT FUER OKTETT vergleichen,
#      dazu die Umsetzungen und die globalen Symbole
#
# Der Text wird nach jeder Datei wieder geloescht -- der ganze Baum in
# Assemblertext waeren mehrere Gigabyte.
#
# Aufruf:
#   bash tools/kodierer/run.sh              # x86-64, ganzer Baum
#   bash tools/kodierer/run.sh --a64        # ARM64
#   bash tools/kodierer/run.sh --schnell    # nur eine Baustufe
#   bash tools/kodierer/run.sh --nur tests/1613_crypto.fi
# =====================================================================
set -u
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"

ARCH=x86
TFLAG=""
STUFEN="dev-fast release-fast release-safe"
NUR=""
JOBS="${JOBS:-8}"
while [ $# -gt 0 ]; do
    case "$1" in
        --a64) ARCH=a64; TFLAG="--target=aarch64-linux" ;;
        --schnell) STUFEN="release-fast" ;;
        --nur) shift; NUR="$1" ;;
        *) echo "unbekannte Option $1"; exit 2 ;;
    esac
    shift
done

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi

# Die Quellen: alle Testfaelle, die Beispiele, der selbstgehostete
# Uebersetzer und die grossen Programme des Browsers.
if [ -n "$NUR" ]; then
    QUELLEN="$NUR"
else
    QUELLEN="$(ls tests/*.fi tests/opt/*.fi examples/*.fi 2>/dev/null)
bin/firnc1.fi
lib/browser/b4_main.fi
lib/browser/parse_main.fi
lib/js/parse_main.fi
lib/css/style_main.fi
lib/layout/layout_main.fi
lib/paint/b3_main.fi"
fi

W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT

eine() { # $1 = quelle, $2 = stufe, $3 = arbeitsnummer
    local f="$1" lvl="$2" id="$3"
    local s="$W/$id.s"
    if ! "$FIRNC" $TFLAG --emit=asm --opt-level="$lvl" -o "$s" "$f" >/dev/null 2>&1; then
        echo "UEBERSPRUNGEN $f [$lvl]" >> "$W/log.$id"
        rm -f "$s"
        return 0
    fi
    local extra=""
    [ "$ARCH" = a64 ] && extra="--a64"
    python3 tools/kodierer/vergleich.py $extra "$FIRNC" "$s" >> "$W/log.$id" 2>&1
    rm -f "$s"
}

n=0
for f in $QUELLEN; do
    [ -f "$f" ] || continue
    for lvl in $STUFEN; do
        eine "$f" "$lvl" "$((n % JOBS))" &
        n=$((n + 1))
        if [ $((n % JOBS)) -eq 0 ]; then wait; fi
    done
done
wait

cat "$W"/log.* > "$W/alle.log" 2>/dev/null
python3 - "$W/alle.log" <<'PY'
import json, sys
gesamt = {"dateien": 0, "gut": 0, "schlecht": 0, "bytes": 0, "relocs": 0}
uebersprungen = 0
fehler = []
puffer = []
for line in open(sys.argv[1], errors="replace"):
    line = line.rstrip("\n")
    if line.startswith("UEBERSPRUNGEN"):
        uebersprungen += 1
        continue
    if line.startswith("{") and line.endswith("}"):
        try:
            d = json.loads(line)
        except Exception:
            puffer.append(line); continue
        for k in gesamt:
            gesamt[k] += d.get(k, 0)
        puffer = []
        continue
    puffer.append(line)
    if len(puffer) < 4000:
        fehler.append(line)
print("=" * 66)
print("ABNAHME DES BINAERKODIERERS -- eigener Weg gegen GNU as")
print("=" * 66)
if fehler:
    print("\n".join(fehler[:400]))
    print()
print("uebersetzte Einheiten : %d" % gesamt["dateien"])
print("bitgleich             : %d" % gesamt["gut"])
print("abweichend            : %d" % gesamt["schlecht"])
print("uebersprungen         : %d  (Quelle baut nicht, nicht dem Kodierer anzulasten)"
      % uebersprungen)
print("verglichene Oktette   : %d" % gesamt["bytes"])
print("verglichene Umsetzungen: %d" % gesamt["relocs"])
print("=" * 66)
sys.exit(0 if gesamt["schlecht"] == 0 and gesamt["dateien"] > 0 else 1)
PY
