#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/input/run.sh -- lib/input against a REAL Linux kernel.
#
# A container (like the build machine) has no /dev/uinput, and driving
# the input of a desktop that somebody is using is not a test. So the
# proof runs in a throwaway VM: QEMU boots the kernel of THIS machine
# (/boot/vmlinuz-*), the initramfs holds exactly three files --
# tools/input/vm_init.fi as /init (static, no libc), evdev.ko and
# uinput.ko -- and the VM powers itself off after printing the verdict.
#
#   bash tools/input/run.sh            -> "INPUT OK n / n"
#   KERNEL=/path/vmlinuz MODULES=/lib/modules/<ver> bash tools/input/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
KERNEL="${KERNEL:-$(ls /boot/vmlinuz-* 2>/dev/null | head -1)}"
VER="${KERNEL##*/vmlinuz-}"
MODULES="${MODULES:-/usr/lib/modules/$VER}"
if [ ! -r "$KERNEL" ] || [ ! -d "$MODULES" ] || ! command -v qemu-system-x86_64 >/dev/null; then
    echo "SKIP: no kernel/modules/qemu (KERNEL=$KERNEL MODULES=$MODULES)"
    exit 0
fi
"$FIRNC" -o "$W/init" tools/input/vm_init.fi 2> "$W/build.log" || { cat "$W/build.log"; exit 1; }
mkdir -p "$W/root"
cp "$W/init" "$W/root/init"
for m in kernel/drivers/input/evdev.ko kernel/drivers/input/misc/uinput.ko; do
    base=$(basename "$m")
    if [ -f "$MODULES/$m.xz" ]; then
        xz -dc "$MODULES/$m.xz" > "$W/root/$base"
    elif [ -f "$MODULES/$m.zst" ]; then
        zstd -dcq "$MODULES/$m.zst" > "$W/root/$base"
    elif [ -f "$MODULES/$m" ]; then
        cp "$MODULES/$m" "$W/root/$base"
    fi
done
python3 - "$W/root" "$W/init.cpio" <<'EOF'
import os, sys
root, out = sys.argv[1], sys.argv[2]
ino = [1000]
def entry(name, mode, data=b"", major=0, minor=0):
    ino[0] += 1
    name_b = name.encode() + b"\0"
    hdr = "070701%08x%08x%08x%08x%08x%08x%08x%08x%08x%08x%08x%08x%08x" % (
        ino[0], mode, 0, 0, 1, 0, len(data), 0, 0, major, minor, len(name_b), 0)
    b = hdr.encode() + name_b
    b += b"\0" * ((4 - len(b) % 4) % 4)
    b += data
    b += b"\0" * ((4 - len(b) % 4) % 4)
    return b
blob = entry("dev", 0o40755) + entry("dev/console", 0o20600, major=5, minor=1)
for f in ["init", "evdev.ko", "uinput.ko"]:
    p = os.path.join(root, f)
    if os.path.exists(p):
        blob += entry(f, 0o100755, open(p, "rb").read())
blob += entry("TRAILER!!!", 0)
open(out, "wb").write(blob)
EOF
ACCEL=""
[ -w /dev/kvm ] && ACCEL="-enable-kvm"
timeout 120 qemu-system-x86_64 $ACCEL -m 256 -kernel "$KERNEL" -initrd "$W/init.cpio" \
    -append "console=ttyS0 rdinit=/init panic=-1 quiet loglevel=3" \
    -nographic -no-reboot -serial stdio -monitor none > "$W/console.log" 2>&1
grep -aE "vm_init|FAIL|INPUT|events compared|first difference|panic" "$W/console.log" | tr -d "\r" | sed "s/^.*vm_init/vm_init/"
grep -q '^INPUT OK' <(tr -d '\r' < "$W/console.log")
