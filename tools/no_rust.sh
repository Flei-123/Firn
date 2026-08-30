#!/usr/bin/env bash
# tools/no_rust.sh -- THE PROOF THAT RUST IS NO LONGER NEEDED.
#
# The claim in README.md is easy to make and hard to keep: "Rust is only a
# bootstrap archive". This script tries to break it.
#
# It builds a Firn compiler from `bootstrap/` alone -- in an environment in
# which `cargo` and `rustc` DO NOT EXIST:
#
#   * `PATH` is set to a directory of its own that contains only the
#     programs the bootstrap really needs (`as`, `ld`, `gzip`, `sh`,
#     `cmp`, ...), each one a symbolic link. `cargo` and `rustc` are not
#     among them, and the check below proves it.
#   * `CARGO_HOME`/`RUSTUP_HOME` are pointed at an empty directory, so a
#     `~/.cargo/bin` cannot creep back in.
#   * `compiler/target/release/firnc` -- the Rust compiler binary -- is
#     made unreadable for the duration of the run (`chmod 000`), so that
#     even an accidental hard-coded path would fail loudly.
#
# Then, in that environment: `sh bootstrap/build.sh`, and afterwards the
# built compiler has to translate and run a real program.
#
# Usage:  bash tools/no_rust.sh
set -uo pipefail
cd "$(dirname "$0")/.."
ROOT=$(pwd)

SANDBOX=$(mktemp -d)
BIN="$SANDBOX/bin"
mkdir -p "$BIN" "$SANDBOX/empty"
# Exactly the programs `bootstrap/build.sh` is allowed to use.
for p in sh dash bash as ld gzip zcat cmp cp rm mkdir printf sha256sum dirname pwd cat uname sed; do
    q=$(command -v "$p" 2>/dev/null) || continue
    ln -sf "$q" "$BIN/$p"
done

FIRNC0=compiler/target/release/firnc
restore() {
    [ -e "$FIRNC0" ] && chmod 755 "$FIRNC0" 2>/dev/null
    rm -rf "$SANDBOX"
}
trap restore EXIT
[ -e "$FIRNC0" ] && chmod 000 "$FIRNC0"

echo "--- the environment"
env -i PATH="$BIN" HOME="$SANDBOX/empty" CARGO_HOME="$SANDBOX/empty" \
    RUSTUP_HOME="$SANDBOX/empty" "$BIN/sh" -c '
        for p in cargo rustc rustup cc gcc clang make python3; do
            if command -v "$p" >/dev/null 2>&1; then
                printf "    STILL THERE: %s\n" "$p"
                exit 1
            fi
        done
        printf "    no cargo, no rustc, no rustup, no C compiler, no make, no python\n"
    ' || { echo "the sandbox is not clean"; exit 1; }

echo "--- bootstrap out of bootstrap/ alone"
t0=$(date +%s)
env -i PATH="$BIN" HOME="$SANDBOX/empty" CARGO_HOME="$SANDBOX/empty" \
    RUSTUP_HOME="$SANDBOX/empty" "$BIN/sh" "$ROOT/bootstrap/build.sh" \
    "$SANDBOX/firnc1"
rc=$?
t1=$(date +%s)
if [ "$rc" -ne 0 ]; then
    echo "BOOTSTRAP FAILED (rc=$rc)"
    exit 1
fi
echo "    $((t1 - t0)) s"

echo "--- the compiler built that way translates a program"
prog=$SANDBOX/hello.fi
cat > "$prog" <<'EOF'
static MSG: [u8; 13] = "hello, world\n"
fn main() -> i32 {
    syscall(1, 1, (&MSG[0]) as u64, 13)
    return 0
}
EOF
env -i PATH="$BIN" HOME="$SANDBOX/empty" FIRNLIB="$ROOT/lib" \
    "$SANDBOX/firnc1" "$prog" -o "$SANDBOX/hello" || {
    echo "the program was not translated"; exit 1; }
out=$("$SANDBOX/hello")
if [ "$out" != "hello, world" ]; then
    echo "wrong output: [$out]"
    exit 1
fi
echo "    output: $out"

echo
echo "WITHOUT RUST: a Firn compiler out of bootstrap/, and it works."
exit 0
