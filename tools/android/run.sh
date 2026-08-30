#!/usr/bin/env bash
# tools/android/run.sh -- THE CROSS CHECK OF ROUND ANDROID.
#
# `aarch64-linux-android` is the THIRD target, and it is not the second one
# under a different name. Same instruction set, different everything else:
# Bionic instead of glibc, `crtbegin_dynamic.o` instead of nothing,
# `/system/bin/linker64` instead of the kernel, PIE instead of a fixed
# address -- and, for an app, a SHARED LIBRARY instead of an executable.
#
# This script proves that the difference is only in the linking, by running
# the same programs twice and comparing what they DO:
#
#   x86-64   firnc --target=x86_64-linux            -> runs natively
#   android  firnc --target=aarch64-linux-android   -> runs under
#            qemu-aarch64 against a REAL Bionic (an Android system image's
#            /system/bin/linker64 and /system/lib64/libc.so)
#
# Four buckets, and none of them is swept under the carpet:
#
#   SAME           both compiled, both ran, same output and same exit code
#   DIFFERENT      both compiled and they do not agree
#   NOT SUPPORTED  the aarch64 code generator REFUSED the program with a
#                  clear message. Nothing was emitted, nothing was guessed.
#   X86 ALREADY    the x86-64 side does not meet its own expectation from
#                  line 1 -- then there is nothing to compare
#
# Stage two is the one the round exists for: a Firn SHARED LIBRARY
# (`--shared`) is linked, a second Firn program is linked AGAINST it, and
# the real Android loader puts the two together at run time.
#
# What it needs, and what it says instead of failing when it is missing:
#
#   * an NDK (`FIRN_ANDROID_NDK`, or `<sdk>/ndk*/<version>`) -- for the
#     Bionic stub libraries and the start files. Only the aarch64 sysroot is
#     used; the 2.6 GB toolchain is not, because Firn brings its own code
#     generator and links with binutils.
#   * `qemu-aarch64` and `binutils-aarch64-linux-gnu`
#   * a directory with a REAL Bionic for the run
#     (`ANDROID_QEMU_ROOT`, default `$HOME/android-root`), holding
#     `system/bin/linker64` and `system/lib64/{libc,libm,libdl,liblog}.so`.
#     Where those come from is written in docs/ZIEL-ANDROID.md. Without it
#     the artifacts are BUILT and CHECKED but not run, and the report says
#     so instead of claiming a pass.
#
# Usage:
#   tools/android/run.sh            all of tests/*.fi, optimised build
#   tools/android/run.sh --no-opt   the same corpus with the optimiser off
#   AND_FILTER=340 tools/android/run.sh   only matching cases
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"

FIRNC="$ROOT/compiler/target/release/firnc"
QEMU=${QEMU:-qemu-aarch64}
AROOT=${ANDROID_QEMU_ROOT:-$HOME/android-root}
WORK="$ROOT/.android-work"
JOBS=${JOBS:-8}
FLAGS=""
LABEL="dev-fast"
if [ "${1:-}" = "--no-opt" ]; then
    FLAGS="--no-opt"
    LABEL="no-opt"
    shift
fi
FILTER=${AND_FILTER:-}

if [ ! -x "$FIRNC" ]; then
    echo "firnc is missing: $FIRNC (cargo build --release --manifest-path compiler/Cargo.toml)"
    exit 1
fi
# The same house rule the other tools follow: a missing cross toolchain is
# said out loud and skipped, it does not turn a machine that never had it
# installed into a red suite.
for t in "$QEMU" aarch64-linux-gnu-as aarch64-linux-gnu-ld; do
    command -v "$t" >/dev/null 2>&1 || {
        echo "SKIP: $t is missing (apt-get install qemu-user binutils-aarch64-linux-gnu)"
        exit 0
    }
done

rm -rf "$WORK"
mkdir -p "$WORK"

# --- is there an NDK? The COMPILER answers that, with the message that
# names every place it looked -- so the skip here and the error a user gets
# say the same thing.
cat > "$WORK/ndkprobe.fi" <<'EOF'
fn main() -> i32 { return 0 }
EOF
if ! "$FIRNC" --target=aarch64-linux-android -o "$WORK/ndkprobe" \
        "$WORK/ndkprobe.fi" >"$WORK/ndkprobe.err" 2>&1; then
    echo "SKIP: no usable Android NDK --"
    sed 's/^/  /' "$WORK/ndkprobe.err"
    exit 0
fi
NDKLIB=$("$FIRNC" --target=aarch64-linux-android --print-ndk-lib 2>/dev/null)
echo "  ndk: $NDKLIB"

RUNNABLE=yes
if [ ! -x "$AROOT/system/bin/linker64" ] || [ ! -f "$AROOT/system/lib64/libc.so" ]; then
    RUNNABLE=no
fi

# ---------------------------------------------------------------- one case
one_case() {
    local file="$1" base exp hdr kind
    base=$(basename "$file" .fi)
    hdr=$(head -1 "$file")
    case "$hdr" in
        *expect_out:*)  kind=out;  exp=${hdr#*expect_out: } ;;
        *expect_exit:*) kind=exit; exp=${hdr#*expect_exit: } ;;
        *) echo "X86BAD $file :: no expectation in line 1"; return ;;
    esac

    if ! "$FIRNC" $FLAGS --target=x86_64-linux -o "$WORK/$base.x86" "$file" \
            >"$WORK/$base.x86.err" 2>&1; then
        echo "X86BAD $file :: x86 compilation failed"; return
    fi
    local xout xrc try
    for try in 1 2; do
        xout=$(timeout 60 "$WORK/$base.x86" </dev/null 2>/dev/null); xrc=$?
        if [ "$kind" = out ]; then
            [ "$xrc" -eq 0 ] && [ "$xout" = "$exp" ] && break
        else
            [ "$xrc" = "$exp" ] && break
        fi
        [ "$try" -eq 2 ] && {
            echo "X86BAD $file :: x86 does not meet its own expectation (twice)"
            return
        }
        sleep 2
    done

    # --- android
    if ! "$FIRNC" $FLAGS --target=aarch64-linux-android -o "$WORK/$base.and" \
            "$file" >"$WORK/$base.and.err" 2>&1; then
        local why
        why=$(grep -m1 -E '^error: ' "$WORK/$base.and.err" | cut -c8-160)
        [ -z "$why" ] && why=$(head -1 "$WORK/$base.and.err" | cut -c1-160)
        case "$why" in
            *"not supported on aarch64"*|*"aarch64:"*|*"no meaning on aarch64"*|\
            *"threads are not supported"*|*"does not support the kernel profile"*)
                echo "NOTSUP $file :: $why" ;;
            *)  echo "DIFF $file :: android build failed: $why" ;;
        esac
        return
    fi
    if [ "$RUNNABLE" != yes ]; then
        echo "BUILT $file"; return
    fi
    local aout arc
    aout=$(timeout 120 "$QEMU" -L "$AROOT" "$WORK/$base.and" </dev/null 2>"$WORK/$base.run.err"); arc=$?
    # A second, quieter attempt -- only when the two disagree, so a run that
    # already agrees costs nothing. Several cases run at a time here and a
    # few programs in this corpus measure timing.
    if [ "$xrc" != "$arc" ] || [ "$xout" != "$aout" ]; then
        sleep 2
        aout=$(timeout 120 "$QEMU" -L "$AROOT" "$WORK/$base.and" </dev/null 2>"$WORK/$base.run.err"); arc=$?
    fi
    # A REFUSAL AT RUN TIME is not a difference. `Op::ThreadSpawn` cannot be
    # refused while compiling -- the collector's runtime carries it into
    # every garbage collected program -- so the code generator emits a
    # message and an exit code at the point where a thread would really
    # start (codegen_a64.rs). A program that reaches it says so out loud,
    # and that is the same bucket a compile time refusal lands in.
    if grep -q 'firn: threads are not supported' "$WORK/$base.run.err" 2>/dev/null; then
        echo "NOTSUP $file :: threads (refused at run time, exit $arc)"; return
    fi
    if [ "$xrc" = "$arc" ] && [ "$xout" = "$aout" ]; then
        echo "SAME $file"; return
    fi

    # --- they disagree. WHOSE difference is it?
    #
    # `aarch64-linux-android` and `aarch64-linux` are the same code
    # generator. If the GNU target answers exactly what Android answers,
    # then what was found is a difference between x86-64 and the aarch64
    # port -- it was there before this round, `tools/aarch64/run.sh` owns
    # it, and counting it here as Android's would be a lie. That is only
    # accepted when the gnu side is really BUILT AND RUN in this very run;
    # it is not a list of exceptions.
    if "$FIRNC" $FLAGS --target=aarch64-linux -o "$WORK/$base.gnu" "$file" \
            >"$WORK/$base.gnu.err" 2>&1; then
        local gout grc
        gout=$(timeout 120 "$QEMU" "$WORK/$base.gnu" </dev/null 2>/dev/null); grc=$?
        if [ "$grc" = "$arc" ] && [ "$gout" = "$aout" ]; then
            echo "A64ALREADY $file :: aarch64-linux answers the same (exit $grc)"
            return
        fi
    fi
    if [ "$xrc" != "$arc" ]; then
        echo "DIFF $file :: exit code x86=$xrc android=$arc"; return
    fi
    local x1 a1
    x1=$(printf '%s' "$xout" | head -c 60 | tr '\n' '|')
    a1=$(printf '%s' "$aout" | head -c 60 | tr '\n' '|')
    echo "DIFF $file :: output x86='$x1' android='$a1'"
}
export -f one_case
export FIRNC QEMU WORK FLAGS ROOT FIRNLIB AROOT RUNNABLE

LIST="$WORK/list.txt"
ls tests/*.fi > "$LIST"
if [ -n "$FILTER" ]; then
    grep "$FILTER" "$LIST" > "$LIST.f" && mv "$LIST.f" "$LIST"
fi
TOTAL=$(wc -l < "$LIST")

if [ "$RUNNABLE" = yes ]; then
    echo "  bionic: $AROOT (run under $QEMU)"
else
    echo "  bionic: MISSING at $AROOT -- the artifacts are built and checked,"
    echo "          but NOT RUN. See docs/ZIEL-ANDROID.md."
fi
echo "== android cross check ($LABEL, $TOTAL cases, $JOBS at a time) =="
xargs -a "$LIST" -P "$JOBS" -I{} bash -c 'one_case "$@"' _ {} | sort > "$WORK/raw.$LABEL.txt"

SAME=$(grep -c '^SAME ' "$WORK/raw.$LABEL.txt")
BUILT=$(grep -c '^BUILT ' "$WORK/raw.$LABEL.txt")
NOTSUP=$(grep -c '^NOTSUP ' "$WORK/raw.$LABEL.txt")
X86BAD=$(grep -c '^X86BAD ' "$WORK/raw.$LABEL.txt")
A64BAD=$(grep -c '^A64ALREADY ' "$WORK/raw.$LABEL.txt")
DIFF=$(grep -c '^DIFF ' "$WORK/raw.$LABEL.txt")

echo
echo "  SAME            $SAME"
[ "$BUILT" -gt 0 ] && echo "  BUILT (not run) $BUILT"
echo "  NOT SUPPORTED   $NOTSUP"
echo "  X86 ALREADY     $X86BAD"
echo "  AARCH64 ALREADY $A64BAD"
echo "  DIFFERENT       $DIFF"
if [ "$A64BAD" -gt 0 ]; then
    grep '^A64ALREADY ' "$WORK/raw.$LABEL.txt" | head -10 | sed 's/^/  /'
fi
if [ "$DIFF" -gt 0 ]; then
    echo
    grep '^DIFF ' "$WORK/raw.$LABEL.txt" | head -40 | sed 's/^/  /'
fi

# ----------------------------------------------------- stage two: the .so
# The form an Android app really loads. A Firn library, a Firn program
# linked against it, and Bionic's own loader putting the two together.
echo
echo "== the shared library =="
SO_OK=0
SO_TOTAL=3
mkdir -p "$WORK/so"
cat > "$WORK/so/lib.fi" <<'EOF'
#[export_c]
fn firn_sum_squares(n: i64) -> i64 {
    var i: i64 = 1
    var s: i64 = 0
    while i <= n {
        s = s + i * i
        i = i + 1
    }
    return s
}
#[export_c]
fn firn_gcd(a: i64, b: i64) -> i64 {
    var x: i64 = a
    var y: i64 = b
    while y != 0 {
        let t: i64 = y
        y = x % y
        x = t
    }
    return x
}
EOF
cat > "$WORK/so/user.fi" <<'EOF'
extern fn firn_sum_squares(n: i64) -> i64;
extern fn firn_gcd(a: i64, b: i64) -> i64;
fn main() -> i32 {
    let a: i64 = firn_sum_squares(21)
    let b: i64 = firn_gcd(1071, 462)
    if a != 3311 {
        return 1
    }
    if b != 21 {
        return 2
    }
    return 0
}
EOF

if "$FIRNC" --target=aarch64-linux-android --shared -o "$WORK/so/libfirnprobe.so" \
        "$WORK/so/lib.fi" >"$WORK/so/lib.err" 2>&1; then
    echo "  1/3 the library links                        ok"
    SO_OK=$((SO_OK + 1))
else
    echo "  1/3 the library links                        FAILED"
    sed 's/^/      /' "$WORK/so/lib.err"
fi

# What it has to BE, checked and not believed: an aarch64 shared object for
# Android, position independent, against Bionic, with the exported symbols
# typed as functions and with NO text relocation -- Android's loader has
# refused DT_TEXTREL since Android 6.
if [ -f "$WORK/so/libfirnprobe.so" ]; then
    H=$(readelf -hd "$WORK/so/libfirnprobe.so" 2>/dev/null)
    D=$(readelf --dyn-syms "$WORK/so/libfirnprobe.so" 2>/dev/null)
    ok=yes
    echo "$H" | grep -q 'Machine:.*AArch64'        || { ok=no; echo "      not aarch64"; }
    echo "$H" | grep -q 'Type:.*DYN'               || { ok=no; echo "      not a shared object"; }
    echo "$H" | grep -q 'NEEDED.*libc\.so'         || { ok=no; echo "      does not need Bionic's libc.so"; }
    echo "$H" | grep -q 'SONAME.*libfirnprobe\.so' || { ok=no; echo "      no SONAME"; }
    echo "$H" | grep -q 'TEXTREL'                  && { ok=no; echo "      has DT_TEXTREL"; }
    echo "$H" | grep -qi 'ld-linux'                && { ok=no; echo "      depends on the glibc loader"; }
    echo "$D" | grep -q 'FUNC.*firn_sum_squares'   || { ok=no; echo "      firn_sum_squares is not exported as a function"; }
    echo "$D" | grep -q 'FUNC.*firn_gcd'           || { ok=no; echo "      firn_gcd is not exported as a function"; }
    if [ "$ok" = yes ]; then
        echo "  2/3 it is a valid Android aarch64 .so        ok"
        SO_OK=$((SO_OK + 1))
    else
        echo "  2/3 it is a valid Android aarch64 .so        FAILED"
    fi
else
    echo "  2/3 it is a valid Android aarch64 .so        FAILED (no file)"
fi

# ...and the real loader puts it together with a program.
if [ -n "$NDKLIB" ] && [ -d "$NDKLIB" ] \
   && "$FIRNC" --target=aarch64-linux-android -c -o "$WORK/so/user.o" \
        "$WORK/so/user.fi" >"$WORK/so/user.err" 2>&1 \
   && aarch64-linux-gnu-ld -o "$WORK/so/user" -pie \
        --dynamic-linker /system/bin/linker64 -z relro -z now \
        -z max-page-size=4096 --eh-frame-hdr --no-undefined \
        -rpath '$ORIGIN' -L "$NDKLIB" \
        "$NDKLIB/crtbegin_dynamic.o" "$WORK/so/user.o" \
        "$WORK/so/libfirnprobe.so" -lc -lm -ldl -llog \
        "$NDKLIB/crtend_android.o" >"$WORK/so/link.err" 2>&1; then
    if [ "$RUNNABLE" = yes ]; then
        (cd "$WORK/so" && LD_LIBRARY_PATH="$WORK/so" timeout 120 "$QEMU" -L "$AROOT" ./user >/dev/null 2>&1)
        rc=$?
        if [ "$rc" -eq 0 ]; then
            echo "  3/3 Bionic's loader resolves it at run time  ok"
            SO_OK=$((SO_OK + 1))
        else
            echo "  3/3 Bionic's loader resolves it at run time  FAILED (exit $rc)"
        fi
    else
        echo "  3/3 Bionic's loader resolves it at run time  SKIPPED (no bionic)"
        SO_TOTAL=2
    fi
else
    echo "  3/3 Bionic's loader resolves it at run time  FAILED (link)"
    sed 's/^/      /' "$WORK/so/link.err" 2>/dev/null | head -5
fi

echo
echo "  shared library: $SO_OK of $SO_TOTAL"
if [ "$DIFF" -gt 0 ] || [ "$SO_OK" -lt "$SO_TOTAL" ]; then
    exit 1
fi
exit 0
