#!/usr/bin/env bash
# tools/freestanding/run.sh — DER NACHWEIS, DASS `profile kernel` ETWAS BEDEUTET.
#
# Runde 52. Geprueft wird, was man an der erzeugten Datei ABLESEN kann, nicht
# was der Compiler ueber sich selbst behauptet:
#
#   1. `demos/kernel/core.fi` uebersetzt mit BEIDEN Compilern zu einer
#      ELF-Objektdatei (`ET_REL`), nicht zu einer ausfuehrbaren Datei.
#   2. Die Objektdatei hat KEINEN undefinierten Namen — kein libc, kein
#      `_start`, keine Laufzeit. (`nm -u` liefert nichts.)
#   3. Sie enthaelt keinen einzigen `syscall`-Befehl (`objdump -d`).
#   4. Sie laesst sich mit `ld -T demos/kernel/linker.ld` zu einem Abbild
#      binden und in QEMU BOOTEN: die serielle Ausgabe des Kernels erscheint.
#   5. Der Inline-Assembler steht wirklich drin (`in`/`out`, `hlt`, `cli`),
#      der Interrupt-Einsprungpunkt endet mit `iretq` und rettet 14 Register.
#   6. Die volatile-Zusage haelt in ALLEN DREI Baustufen: `asm` und MMIO
#      bleiben stehen und werden nicht zusammengelegt (tests/850-854 pruefen
#      dasselbe zur Laufzeit; hier zaehlen wir die Befehle im Assembler).
#
# Kein Vergleich der Assemblertexte zwischen den Stufen: `firnc0` hat eine
# Registerzuteilung, `lib/firnc1/codegen.fi` nicht. Verglichen wird, was
# gleich sein MUSS — die Symboltabelle und die Freistehendheit.
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
QUELLE=demos/kernel/core.fi
SKRIPT=demos/kernel/linker.ld
# Eigenes Temp-Verzeichnis je Lauf (mehrere Runden laufen parallel).
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

pass=0
fail=0
ok()  { pass=$((pass+1)); printf '  OK    %s\n' "$1"; }
bad() { fail=$((fail+1)); printf '  FAIL  %s\n' "$1"; }

[ -x "$FIRNC" ] || { echo "firnc0 fehlt: $FIRNC"; exit 1; }
# `.firnc1` neu bauen, wenn es fehlt oder eine Quelle juenger ist (die Falle
# aus Runde 35/45/46: ein veraltetes Binary misst den Stand von gestern).
neu=0
[ -x "$FC1" ] || neu=1
if [ -x "$FC1" ]; then
    [ "$FIRNC" -nt "$FC1" ] && neu=1
    while IFS= read -r q; do
        [ "$q" -nt "$FC1" ] && { neu=1; break; }
    done < <(find bin lib -name '*.fi' -not -type l)
fi
[ "$neu" -eq 1 ] && { rm -f "$FC1"; "$FIRNC" bin/firnc1.fi -o "$FC1" || exit 1; }

echo "== 1. Uebersetzen (beide Compiler, Profil aus der Quelle) =="
"$FIRNC" -o "$TMPD/k0.o" "$QUELLE" 2>"$TMPD/e0" \
    && ok "firnc0: $QUELLE -> k0.o" || { bad "firnc0 uebersetzt nicht"; sed 's/^/        /' "$TMPD/e0"; }
"$FC1" "$QUELLE" -o "$TMPD/k1.o" >"$TMPD/e1" 2>&1 \
    && ok "firnc1: $QUELLE -> k1.o" || { bad "firnc1 uebersetzt nicht (rc=$?)"; sed 's/^/        /' "$TMPD/e1"; }

# Gegenprobe: derselbe Quelltext mit `--profile=app` MUSS scheitern —
# `syscall` gibt es dort zwar, aber `#[interrupt]` nicht. Ohne diese Probe
# wuerde ein Compiler, der das Profil ignoriert, hier unbemerkt durchlaufen.
if "$FIRNC" --profile=app -o "$TMPD/app.o" "$QUELLE" >"$TMPD/app.err" 2>&1; then
    bad "Gegenprobe: --profile=app haette scheitern muessen"
else
    grep -q "nur im profil 'kernel'" "$TMPD/app.err" \
        && ok "Gegenprobe: --profile=app wird abgelehnt (#[interrupt])" \
        || { bad "Gegenprobe: falsche Meldung"; sed 's/^/        /' "$TMPD/app.err" | head -4; }
fi

echo "== 2. Es ist eine OBJEKTdatei, und sie ist freistehend =="
for s in 0 1; do
    f="$TMPD/k$s.o"
    [ -f "$f" ] || { bad "firnc$s: keine Ausgabedatei"; continue; }
    typ=$(readelf -h "$f" | awk -F: '/^  Type:/ {print $2}' | awk '{print $1}')
    [ "$typ" = "REL" ] && ok "firnc$s: ELF-Typ REL (verschiebbare Objektdatei)" \
                       || bad "firnc$s: ELF-Typ '$typ', erwartet REL"
    undef=$(nm -u "$f" 2>/dev/null | sed '/^$/d')
    [ -z "$undef" ] && ok "firnc$s: KEIN undefiniertes Symbol" \
                    || { bad "firnc$s: undefinierte Symbole"; echo "$undef" | sed 's/^/        /'; }
    # Jedes definierte Symbol gehoert dem Programm selbst (Praefix _F0./_F1.).
    fremd=$(nm --defined-only "$f" | awk '{print $3}' | grep -vE "^_F[01]\." || true)
    [ -z "$fremd" ] && ok "firnc$s: alle definierten Symbole sind eigene" \
                    || { bad "firnc$s: fremde Symbole"; echo "$fremd" | sed 's/^/        /'; }
    if objdump -d "$f" | grep -qE '^\s+[0-9a-f]+:.*\bsyscall\b'; then
        bad "firnc$s: die Objektdatei enthaelt einen syscall"
    else
        ok "firnc$s: kein syscall im Maschinencode"
    fi
done

echo "== 3. Gegen das Linkerskript binden (kein libc, keine crt-Dateien) =="
as --64 -o "$TMPD/start.o" demos/kernel/start.s 2>"$TMPD/as.err" \
    && ok "start.s assembliert (Multiboot-Kopf, Langer Modus)" \
    || { bad "start.s"; sed 's/^/        /' "$TMPD/as.err" | head -5; }
for s in 0 1; do
    f="$TMPD/k$s.o"
    [ -f "$f" ] || continue
    if ld -n -T "$SKRIPT" --defsym=KERN_START="_F$s.core_start" \
          -o "$TMPD/k$s.elf" "$TMPD/start.o" "$f" 2>"$TMPD/ld$s.err"; then
        ein=$(readelf -h "$TMPD/k$s.elf" | awk -F: '/Entry point/ {print $2}' | tr -d ' ')
        ok "firnc$s: gelinkt, Einsprung $ein"
        # Der einzige nicht aus Firn stammende Code ist `start.s`.
        undef=$(nm -u "$TMPD/k$s.elf" 2>/dev/null | sed '/^$/d')
        [ -z "$undef" ] && ok "firnc$s: das gebundene Abbild hat kein offenes Symbol" \
                        || { bad "firnc$s: offene Symbole im Abbild"; echo "$undef" | sed 's/^/        /'; }
    else
        bad "firnc$s: ld schlug fehl"; sed 's/^/        /' "$TMPD/ld$s.err" | head -5
    fi
done

echo "== 3b. In QEMU booten (der eigentliche Beweis) =="
if command -v qemu-system-x86_64 >/dev/null 2>&1; then
    for s in 0 1; do
        [ -f "$TMPD/k$s.elf" ] || continue
        # QEMUs Multiboot-Lader nimmt nur ELF32; alle Adressen liegen unter
        # 4 GiB, deshalb genuegt eine Umschrift des Kopfes.
        objcopy -O elf32-i386 "$TMPD/k$s.elf" "$TMPD/k$s.mb" 2>/dev/null
        timeout 20 qemu-system-x86_64 -kernel "$TMPD/k$s.mb" -serial stdio \
            -display none -no-reboot > "$TMPD/q$s.txt" 2>&1
        if grep -q "FIRN: profile kernel ist" "$TMPD/q$s.txt" \
           && grep -q "freestanding." "$TMPD/q$s.txt"; then
            ok "firnc$s: gebootet, serielle Ausgabe erschienen"
        else
            bad "firnc$s: keine serielle Ausgabe aus QEMU"
            sed 's/^/        /' "$TMPD/q$s.txt" | head -6
        fi
    done
else
    echo "  (uebersprungen: qemu-system-x86_64 ist nicht vorhanden)"
fi

echo "== 4. Inline-Assembler und Interrupt-ABI stehen wirklich im Code =="
for s in 0 1; do
    f="$TMPD/k$s.o"
    [ -f "$f" ] || continue
    objdump -d "$f" > "$TMPD/d$s.txt"
    for befehl in cli hlt "out    %al,(%dx)" "in     (%dx),%al" iretq; do
        if grep -qF "$befehl" "$TMPD/d$s.txt"; then
            ok "firnc$s: '$befehl' im Maschinencode"
        else
            bad "firnc$s: '$befehl' fehlt"
        fi
    done
    # `#[interrupt]`: 14 push + 14 pop im Einsprungpunkt.
    n=$(awk '/<_F'"$s"'\.timer_ih>:/,/iretq/' "$TMPD/d$s.txt" | grep -cE '\bpush\b')
    [ "$n" -eq 15 ] && ok "firnc$s: timer_ih rettet 14 Register + rbp" \
                    || bad "firnc$s: timer_ih hat $n push, erwartet 15"
done

echo "== 5. volatile haelt in allen drei Baustufen =="
# `cli` und `hlt` stehen in `core_start`, das niemand ruft — sie koennen also
# auch durch Einbetten nicht mehr werden. Genau einmal, in jeder Baustufe.
for stufe in "" "--no-opt" "--opt-level=dev-fast"; do
    name=${stufe:---release-fast}
    "$FIRNC" $stufe --emit=asm -o "$TMPD/k.s" "$QUELLE" 2>/dev/null || { bad "asm-Ausgabe $name"; continue; }
    c=$(grep -cE '^\s+cli$' "$TMPD/k.s")
    h=$(grep -cE '^\s+hlt$' "$TMPD/k.s")
    o=$(grep -cE '^\s+out dx, al$' "$TMPD/k.s")
    i=$(grep -cE '^\s+in al, dx$' "$TMPD/k.s")
    if [ "$c" = 1 ] && [ "$h" = 1 ] && [ "$o" -ge 1 ] && [ "$i" -ge 1 ]; then
        ok "$name: cli=1 hlt=1 (exakt), out=$o in=$i (>=1; --release-fast bettet out8/in8 ein)"
    else
        bad "$name: cli=$c hlt=$h out=$o in=$i"
    fi
done

# Die SCHARFE Zaehlung: tools/freestanding/volatile.fi hat alles in EINER
# Funktion und ruft nichts — Einbetten kann die Zahlen nicht verschieben.
# Gezaehlt wird in der FIR NACH dem Optimierer: dort steht, was er stehen
# gelassen hat. Drei woertlich gleiche `asm("pause")` muessen drei bleiben
# (kein CSE), zwei MMIO-Lasten auf dieselbe Adresse zwei (keine
# Zusammenlegung), und das `rdtsc` mit unbenutztem Ergebnis darf nicht
# verschwinden (die Falle aus Runde 40).
VOL=tools/freestanding/volatile.fi
for stufe in "" "--no-opt" "--opt-level=dev-fast"; do
    name=${stufe:---release-fast}
    "$FIRNC" $stufe --emit=fir "$VOL" > "$TMPD/v.fir" 2>/dev/null || { bad "volatile.fi $name"; continue; }
    a1=$(grep -cF 'asm.void "pause"' "$TMPD/v.fir")
    a2=$(grep -cF 'asm.u64 "rdtsc"' "$TMPD/v.fir")
    l=$(grep -cF 'mmio_load.u32' "$TMPD/v.fir")
    st=$(grep -cF 'mmio_store.u32' "$TMPD/v.fir")
    if [ "$a1" = 3 ] && [ "$a2" = 1 ] && [ "$l" = 2 ] && [ "$st" = 1 ]; then
        ok "volatile.fi $name: pause=3 rdtsc=1 mmio_load=2 mmio_store=1 (exakt)"
    else
        bad "volatile.fi $name: pause=$a1 (3) rdtsc=$a2 (1) load=$l (2) store=$st (1)"
    fi
    "$FIRNC" $stufe -o "$TMPD/v0" "$VOL" 2>/dev/null && "$TMPD/v0" >/dev/null 2>&1
    [ "$?" = 6 ] && ok "volatile.fi $name: laeuft, liefert 6" || bad "volatile.fi $name: falscher Rueckgabewert"
done
# Und dasselbe durch den Compiler in Firn: dort gibt es keinen Optimierer,
# also zaehlt der Assemblertext — und danach das Verhalten.
rm -f "$TMPD/v1" "$TMPD/v1.s" "$TMPD/v1.o"
if "$FC1" "$VOL" -o "$TMPD/v1" >/dev/null 2>&1; then
    c=$(grep -cE '^\s+pause$' "$TMPD/v1.s")
    r=$(grep -cE '^\s+rdtsc$' "$TMPD/v1.s")
    [ "$c" = 3 ] && [ "$r" = 1 ] && ok "volatile.fi firnc1: pause=3 rdtsc=1 (exakt)" \
                                || bad "volatile.fi firnc1: pause=$c (3) rdtsc=$r (1)"
    "$TMPD/v1" >/dev/null 2>&1
    [ "$?" = 6 ] && ok "volatile.fi firnc1: laeuft, liefert 6" || bad "volatile.fi firnc1: falscher Rueckgabewert"
else
    bad "volatile.fi: firnc1 uebersetzt nicht"
fi

echo
echo "FREISTEHEND: $pass bestanden, $fail fehlgeschlagen"
[ "$fail" -eq 0 ] || exit 1
exit 0
