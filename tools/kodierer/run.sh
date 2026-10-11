#!/usr/bin/env bash
# =====================================================================
# ROUND KODIERER -- THE ACCEPTANCE
#
# For every Firn source in the tree:
#   1. produce assembler text (firnc --emit=asm), in several build stages
#   2. send the same text ONCE through `as`  -> object file A
#      and        ONCE through the own encoder -> object file B
#   3. compare .text, .data, .rodata OCTET BY OCTET,
#      plus the relocations and the global symbols
#
# The text is deleted again after every file -- the whole tree as
# assembler text would be several gigabytes.
#
# Usage:
#   bash tools/kodierer/run.sh              # x86-64, whole tree
#   bash tools/kodierer/run.sh --a64        # ARM64
#   bash tools/kodierer/run.sh --quick      # only one build stage
#   bash tools/kodierer/run.sh --only tests/1613_crypto.fi
# =====================================================================
set -u
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"

ARCH=x86
TFLAG=""
STAGES="dev-fast release-fast release-safe"
ONLY=""
JOBS="${JOBS:-8}"
while [ $# -gt 0 ]; do
    case "$1" in
        --a64) ARCH=a64; TFLAG="--target=aarch64-linux" ;;
        --quick) STAGES="release-fast" ;;
        --only) shift; ONLY="$1" ;;
        *) echo "unknown option $1"; exit 2 ;;
    esac
    shift
done

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi

# The sources: all test cases, the examples, the self-hosted compiler and the
# big programs of the browser.
if [ -n "$ONLY" ]; then
    SOURCES="$ONLY"
else
    SOURCES="$(ls tests/*.fi tests/opt/*.fi examples/*.fi 2>/dev/null)
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

one() { # $1 = source, $2 = stage, $3 = worker number
    local f="$1" lvl="$2" id="$3"
    local s="$W/$id.s"
    if ! "$FIRNC" $TFLAG --emit=asm --opt-level="$lvl" -o "$s" "$f" >/dev/null 2>&1; then
        echo "SKIPPED $f [$lvl]" >> "$W/log.$id"
        rm -f "$s"
        return 0
    fi
    local extra=""
    [ "$ARCH" = a64 ] && extra="--a64"
    python3 tools/kodierer/compare.py $extra "$FIRNC" "$s" >> "$W/log.$id" 2>&1
    rm -f "$s"
}

n=0
for f in $SOURCES; do
    [ -f "$f" ] || continue
    for lvl in $STAGES; do
        one "$f" "$lvl" "$((n % JOBS))" &
        n=$((n + 1))
        if [ $((n % JOBS)) -eq 0 ]; then wait; fi
    done
done
wait

cat "$W"/log.* > "$W/all.log" 2>/dev/null
python3 - "$W/all.log" <<'PY'
import json, sys
total = {"files": 0, "good": 0, "bad": 0, "bytes": 0, "relocs": 0}
skipped = 0
errors = []
buf = []
for line in open(sys.argv[1], errors="replace"):
    line = line.rstrip("\n")
    if line.startswith("SKIPPED"):
        skipped += 1
        continue
    if line.startswith("{") and line.endswith("}"):
        try:
            d = json.loads(line)
        except Exception:
            buf.append(line); continue
        for k in total:
            total[k] += d.get(k, 0)
        buf = []
        continue
    buf.append(line)
    if len(buf) < 4000:
        errors.append(line)
print("=" * 66)
print("ACCEPTANCE OF THE BINARY ENCODER -- own way against GNU as")
print("=" * 66)
if errors:
    print("\n".join(errors[:400]))
    print()
print("translated units      : %d" % total["files"])
print("bit-identical         : %d" % total["good"])
print("deviating             : %d" % total["bad"])
print("skipped               : %d  (source does not build, not the encoder's fault)"
      % skipped)
print("octets compared       : %d" % total["bytes"])
print("relocations compared  : %d" % total["relocs"])
print("=" * 66)
sys.exit(0 if total["bad"] == 0 and total["files"] > 0 else 1)
PY
