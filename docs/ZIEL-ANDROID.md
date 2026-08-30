# `aarch64-linux-android` — the third target

Round ANDROID. Written for the round that comes after it and needs to put a
Firn library into an app.

Everything in this file was run on the machine it was written on. Where
something was only checked and not executed, it says so.

---

## 1. Why this is a target of its own

Firn had two machines and, since round 80, two targets:

| | `x86_64-linux` | `aarch64-linux` |
|---|---|---|
| code generator | `codegen_x86.rs` | `codegen_a64.rs` |
| entry point | own `_start` | own `_start` |
| C library | none — the program calls the kernel | none |
| linker | `ld` | `aarch64-linux-gnu-ld` |

`aarch64-linux-android` is the **same machine as the second one and a
different target from both**. The instruction set is identical — the whole
code generator, the whole calling convention, every register is shared with
`aarch64-linux`, and `Target::arch()` in `compiler/src/target.rs` is the
line that says so. What differs is everything a LINKER decides:

| question | `aarch64-linux` | `aarch64-linux-android` |
|---|---|---|
| C library | glibc, or none at all | **Bionic**, from the NDK |
| start files | none | `crtbegin_dynamic.o` + `crtend_android.o` (program), `crtbegin_so.o` + `crtend_so.o` (library) |
| loader | `/lib/ld-linux-aarch64.so.1` | **`/system/bin/linker64`** |
| position independent | optional | **required** — Android has refused `ET_EXEC` since Android 5 |
| API level | — | **has to be chosen**; Bionic's set of functions grew with the releases |
| the form an app loads | executable | **shared library** (`.so`) |
| `.note.android.ident` | absent | comes in with the start file |
| read-only data with addresses | `.rodata` | **`.data.rel.ro`** — Android's loader refuses `DT_TEXTREL` |
| symbol `main` | Firn's entry point | **Bionic's**; Firn's becomes `__firn_main` |
| threads | raw `clone(2)` + own thread pointer | **not yet** — the thread pointer is Bionic's |

There is no third code generator and no third calling convention. The round
added 0 instructions to `codegen_a64.rs`'s instruction selection.

---

## 2. What you need

### 2.1 The NDK — and only 17 MB of it

The full NDK is a 633 MB download that unpacks to about 2.6 GB. Firn does
not use any of the toolchain in it: it brings its own code generator and
links with `binutils-aarch64-linux-gnu`. What it uses is the **aarch64
Bionic sysroot** of one API level — the stub libraries and the start files,
about 8 MB per level.

On the machine this round was done on, `sdkmanager` could not be used: the
disk had 3.6 GB free and eleven other builds were running on it, so a 2.6 GB
unpack was not acceptable. The sysroot was fetched out of the zip over HTTP
range requests instead — the zip central directory is read, the wanted
members are located and only those are downloaded and inflated:

```
python3 tools/android/ndk_sysroot.py \
    https://dl.google.com/android/repository/android-ndk-r27c-linux.zip \
    "$HOME/android-sdk/ndk-partial" \
    'sysroot/usr/lib/aarch64-linux-android/(24|26)/' \
    'sysroot/usr/lib/aarch64-linux-android/[^/]+\.(so|a)$'
```

Result: **17 MB on disk instead of 2.6 GB**, and the target works with it in
full. A normally installed NDK
(`sdkmanager 'ndk;27.3.13750724'`, `ANDROID_NDK_HOME=...`) works exactly the
same way — the compiler does not care how the directory got there.

**Which NDK**: `r27c` (`27.2.12479018` in `sdkmanager`'s numbering) — the
newest **stable** one; everything called `rc1`/`rc2` was skipped.

### 2.2 Where the compiler looks for it

In this order, first hit wins (`compiler/src/android.rs`):

1. `FIRN_ANDROID_NDK`
2. `ANDROID_NDK_HOME`
3. `ANDROID_NDK_ROOT`
4. `<sdk>/ndk/<newest version>` for `ANDROID_SDK_ROOT`, `ANDROID_HOME`, `$HOME/android-sdk`
5. `<sdk>/ndk-partial/<newest version>`, `<sdk>/ndk-bundle`

A directory counts as an NDK when
`toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android`
is under it.

`FIRN_ANDROID_NDK` is an **order, not a hint**: if it points somewhere that
is not an NDK, the compilation stops there instead of quietly taking a
different one.

```
$ firnc --target=aarch64-linux-android --print-ndk-lib
/root/android-sdk/ndk-partial/android-ndk-r27c/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/24

$ FIRN_ANDROID_NDK=/nope firnc --target=aarch64-linux-android --print-ndk-lib
error: FIRN_ANDROID_NDK points at '/nope', and that is no NDK
note: an NDK is a directory that contains
       'toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android'
note: unset FIRN_ANDROID_NDK to search the usual places

$ firnc --target=aarch64-linux-android --android-api=31 --print-ndk-lib
error: android api level 31 is not in the NDK at '/root/android-sdk/ndk-partial/android-ndk-r27c'
note: available levels: 24, 26
note: choose one with --android-api=<n>
```

### 2.3 The assembler and the linker

`binutils-aarch64-linux-gnu` — the same two programs the `aarch64-linux`
target uses.

That is not a shortcut. An object file for `aarch64-linux-android` and one
for `aarch64-linux-gnu` are the same ELF relocatable; the difference between
the targets is made when LINKING. `aarch64-linux-gnu-ld` produces a correct
Android artifact when it is given Bionic's start files, Bionic's stub
libraries and `/system/bin/linker64` — which is what `android.rs` does. The
NDK's own `ld.lld` would work as well and is not needed.

---

## 3. The switches

```
--target=aarch64-linux-android    (also: aarch64-android, arm64-v8a, android-arm64)
--android-api=<n>                 default 24 (Android 7.0)
--shared                          a shared library (.so) instead of a program
--print-ndk-lib                   print the sysroot directory and end
```

`--shared` is refused on the other two targets, with a message.

### The API level

`libc.so` in an NDK contains no code. It is a **stub**: a shared object with
exactly the symbol table that release of Android offered. Linking against
level 24 is the promise "this artifact uses nothing Android 7.0 did not
have". 24 is the default because it is the oldest level current NDKs carry
and every Play Store device satisfies it; 26 is present too.

---

## 4. Building

### A program

```
firnc --target=aarch64-linux-android -o hello hello.fi
```

produces:

```
Type:            DYN (Position-Independent Executable file)
Machine:         AArch64
.interp          /system/bin/linker64
NEEDED           libc.so, libm.so, libdl.so, liblog.so
FLAGS_1          NOW PIE
.note.android.ident   Android, NDK r27c, API 24
```

The linker command (`compiler/src/android.rs`):

```
aarch64-linux-gnu-ld -o <out> -pie --dynamic-linker /system/bin/linker64 \
    -z relro -z now -z max-page-size=4096 --eh-frame-hdr --no-undefined \
    -L <sysroot>/24 <sysroot>/24/crtbegin_dynamic.o <obj> \
    -lc -lm -ldl -llog <sysroot>/24/crtend_android.o
```

### A shared library — the form an app loads

```
firnc --target=aarch64-linux-android --shared firnmath.fi
# -> libfirnmath.so   (that is the name System.loadLibrary("firnmath") looks for)
```

Without `-o` the name is derived: `x.fi` becomes `libx.so`.

The difference in the command is three arguments — `-shared`, `-soname`, and
`crtbegin_so.o`/`crtend_so.o` instead of the program's start files. There is
no `-pie` and no `--dynamic-linker`: a library has no entry point and the
app's process already has a loader.

### What is exported

Every Firn function is emitted `.globl` with default visibility, so it lands
in `.dynsym`. The name it carries there is the question:

```firn
#[export_c]
fn firn_sum_squares(n: i64) -> i64 { ... }
```

`#[export_c]` gives the function its **bare Firn name** as the linker
symbol — that is the name `dlsym`, a JNI declaration or another `.so` has to
use. Without it the symbol keeps the internal naming scheme
(`modules::symbol`), which is stable but not meant to be written down by
hand.

On this target every emitted function also carries `.type <name>, %function`,
so the entry is `STT_FUNC` and not `NOTYPE`. That line is written **only**
on Android, because the other two targets are compared octet for octet
against their predecessor (`tools/repro`).

### An object file

```
firnc --target=aarch64-linux-android -c -o x.o x.fi
```

Assembled, not linked — for a build that wants to hand the object to
`ld`/`clang` itself, together with an app's own code.

---

## 5. What had to change inside, and why

Five things, and none of them is in the instruction selection.

### 5.1 `main` belongs to Bionic

On the Linux targets Firn's entry point IS the symbol `main`, and Firn's own
`_start` calls it with ONE argument: the pointer to the initial stack block
`[argc][argv...][0][envp...]`, out of which `fn main(start: u64)` reads the
command line.

On Android there is no own `_start`. `crtbegin_dynamic.o` brings it, hands
the stack block to `__libc_init`, which sets Bionic up and then calls a
`main` with the **C signature** `(int argc, char** argv, char** envp)`.

So on this target `modules::symbol` gives Firn's entry point the name
`__firn_main`, and the symbol `main` becomes four instructions
(`codegen_a64::emit_android_entry`):

```asm
main:
    stp x29, x30, [sp, #-32]!
    mov x29, sp
    str x19, [sp, #16]
    sub x19, x1, #8          // argv - 8 IS the stack block: argc sits one
    bl  <gc init>            // word below argv[0], in the same block the
    mov x0, x19              // kernel wrote.
    bl  __firn_main
    ...
    ret
```

The conversion is exact, not an approximation: `argv` points at the first
element of the argument vector inside the very block the kernel wrote, and
`argc` is the word below it. A Firn program reads its command line on
Android out of the same place as on Linux.

### 5.2 Read-only data with addresses may not stay in `.rodata`

This is the one that cost the most and is worth remembering.

Firn emits several tables into `.rodata`: the jump tables of a `switch`, the
collector's type table, the interface method tables, the function records,
the panic messages, the immutable `static`s. Some of them contain ADDRESSES
(`.quad .Lsomething`).

In a fixed-address executable the linker simply writes the address down. In
a position independent one it cannot: the loader has to add the load address
at start-up. If the word sits in a read-only section that is a TEXT
RELOCATION — and Android's loader does not warn about `DT_TEXTREL`, it
refuses the file:

```
CANNOT LINK EXECUTABLE: text relocations (DT_TEXTREL) found in 64-bit ELF file
```

**105 of 309 cases died of this** before `target::rodata_section()` existed
— every program with a garbage collected value or a `switch` over more than
a handful of keys. On Android those blocks now go to `.data.rel.ro`:
writable while the loader relocates, read-only afterwards (`-z relro`). The
promise of an immutable `static` is kept, and the other two targets keep the
`.rodata` they always had.

### 5.3 `__cpu_features()`

On `aarch64-linux` it walks the auxiliary vector, whose address `_start`
kept. There is no own `_start` on Android — but Bionic HAS the libc function
that the round-91 comment calls "the usual answer": `getauxval(3)`, since
API 18. Three instructions instead of the walk; the bit mapping is shared.

### 5.4 The thread pointer belongs to Bionic

Firn keeps its thread control block in the thread pointer:
`arch_prctl(ARCH_SET_FS, p)` on x86-64, `msr tpidr_el0, p` on aarch64. Every
garbage collected program writes it once at start-up and the collector reads
it back with `__thread_tcb` (`lib/gc/gc.fi`).

On Android that register is Bionic's. `tpidr_el0` points at Bionic's
`pthread_internal_t`; `errno`, the stack protector cookie
`__stack_chk_guard` and every thread local of the C library are read through
it. A `.so` runs inside somebody else's thread, so taking that register away
would take an app down.

On Android the block is therefore kept in **a word of its own**
(`__firn_android_tcb`, eight octets in `.bss`) and Bionic's register is
never touched. That is exact as long as there is one thread — and there is
one thread, see the next point.

### 5.5 Threads: refused, and refused out loud

A Firn thread is a raw `clone(2)` whose child installs a thread pointer of
its own. On Android the child dies in the first Bionic code it touches;
measured, before the refusal existed: `tests/860`, `861`, `862`, `1600` all
took signal 11.

The refusal **cannot** be made while compiling, and that is worth saying:
`Op::ThreadSpawn` is in the module of every garbage collected program,
because the collector's runtime carries `thread_start` whether the program
calls it or not. Refusing the instruction would refuse two thirds of the
corpus for code none of it runs.

So the refusal stands where a thread would really start — at run time,
loudly:

```
firn: threads are not supported on aarch64-linux-android
(the thread pointer belongs to Bionic) -- see docs/ZIEL-ANDROID.md
```

and the process ends with exit code 70. Never a thread that half exists.

The way out is not a trick with the register: it is `pthread_create` through
`extern fn`. A round of its own.

---

## 6. Running it

### 6.1 The emulator on this server: no

`/root/android-sdk/system-images` holds exactly one image:
`android-30;default;x86_64`. That is an **x86-64** Android — it cannot run an
aarch64 artifact, and `arm64-v8a` images are 300 MB to 800 MB each. Booting
a full ARM64 Android under `qemu-system-aarch64` without KVM (the host is
x86-64, so KVM cannot help an ARM guest) takes tens of minutes per boot.

So the emulator was **not** used, and no claim in this file rests on it.

### 6.2 What was used: qemu-aarch64 against a real Bionic

`qemu-aarch64` (user mode) runs an aarch64 binary on this x86-64 host and
resolves the ELF interpreter under a prefix. Given a directory that looks
like an Android root, the **real Android loader and the real Bionic** put the
program together — the same `linker64`, the same `libc.so` a device has.

Getting that directory is 300 MB and needs no emulator:

```
# 1. the smallest arm64 image with the API level we link against
curl -LO https://dl.google.com/android/repository/sys-img/android/arm64-v8a-24_r09.zip

# 2. only system.img out of it (2.5 GB apparent, ~658 MB on disk: it is
#    mostly zeros and python writes it sparse), then delete the zip
python3 - <<'EOF'
import zipfile, os
z = zipfile.ZipFile('arm64-v8a-24_r09.zip'); n = 'arm64-v8a/system.img'
size = z.getinfo(n).file_size; BS = 1 << 20
with z.open(n) as f, open('system.img', 'wb') as o:
    while True:
        b = f.read(BS)
        if not b: break
        if b == b'\0' * len(b): o.seek(len(b), 1)
        else: o.write(b)
    os.ftruncate(o.fileno(), size)
EOF

# 3. six files out of the ext4 image -- no mount needed, no root needed
mkdir -p ~/android-root/system/bin ~/android-root/system/lib64
for f in libc.so libm.so libdl.so liblog.so libstdc++.so libc++.so; do
    debugfs -R "dump /lib64/$f ~/android-root/system/lib64/$f" system.img
done
debugfs -R "dump /bin/linker64 ~/android-root/system/bin/linker64" system.img
chmod +x ~/android-root/system/bin/linker64
rm system.img          # 3 MB kept out of 300 MB downloaded
```

Then:

```
$ qemu-aarch64 -L ~/android-root ./hello
```

`tools/android/run.sh` reads `ANDROID_QEMU_ROOT` (default `$HOME/android-root`)
and says clearly when it is not there — it then builds and checks the
artifacts but does not claim they ran.

**One warning appears and is harmless:**

```
WARNING: linker: ./hello: unsupported flags DT_FLAGS_1=0x8000001
```

`0x8000001` is `DF_1_NOW | DF_1_PIE`. GNU ld sets `DF_1_PIE` for `-pie`;
Android 7's loader does not know that bit yet and says so. Newer loaders do.
It is a warning, not a refusal, and the program runs.

### 6.3 On a device

`adb push` the artifact and run it. Not done here — there is no device on
this server, and this file does not pretend otherwise.

---

## 7. What was measured

Compiler build: `cargo build --release`, 24 s. Module tests: **275 passed,
0 failed** — 262 before the round, so the round brought 13 (`cargo test
--release` on the commit before it and on this one).

### The corpus, `tools/android/run.sh`

Every case of `tests/*.fi` compiled twice — natively for x86-64, and for
`aarch64-linux-android`, run under `qemu-aarch64` against the real Bionic —
and the two compared, output character for character and exit code:

```
== android cross check (dev-fast, 309 cases, 6 at a time) ==
  SAME            295
  NOT SUPPORTED     9    4 x inline assembler (x86 text, refused while
                         compiling) + 5 x threads (refused at run time, 5.5)
  X86 ALREADY       4    the x86 side does not meet its own expectation,
                         so there is nothing to compare
  AARCH64 ALREADY   1    tests/1284_std_io_text_owner.fi -- aarch64-linux
                         answers EXACTLY the same (exit 20). The difference
                         is between x86-64 and the aarch64 port and was
                         there before this round; the script proves it by
                         building and running the gnu target in the same
                         run, it is not an exception list.
  DIFFERENT         0
```

Before `target::rodata_section()` (5.2) that same run said `SAME 191,
DIFFERENT 109` — 105 of them the loader refusing `DT_TEXTREL`. The number is
in here because a test that cannot go red proves nothing, and this one did.

### The shared library

```
== the shared library ==
  1/3 the library links                        ok
  2/3 it is a valid Android aarch64 .so        ok
      (AArch64, DYN, NEEDED libc.so, SONAME, no DT_TEXTREL, no glibc,
       exported symbols are STT_FUNC)
  3/3 Bionic's loader resolves it at run time  ok
```

Stage 3 is the point of the round: a Firn `.so`, a second Firn program
linked against it, and Android's own `linker64` putting the two together
under `qemu-aarch64`. The program calls `firn_sum_squares(21)` and
`firn_gcd(1071, 462)` **in the library** and checks that it gets 3311 and
21.

### Sizes and times

The same source (`probe.fi`: arithmetic, branches, loops, calls, array
access, `write(2)`), through all three targets:

| target / form | size | build |
|---|---|---|
| `x86_64-linux` executable | 6,648 octets | 20 ms |
| `aarch64-linux` executable | 7,920 octets | 26 ms |
| `aarch64-linux-android` executable (PIE, Bionic) | 16,464 octets | 23 ms |
| `aarch64-linux-android --shared` | 16,096 octets | 21 ms |

The Android artifacts are about twice the size of the gnu one, and that is
the dynamic linking, not the code: `.dynsym`, `.dynstr`, `.hash`,
`.gnu.hash`, `.rela.dyn`, `.interp`, `.note.android.ident` and nine program
headers instead of two. A library with two exported functions
(`tools/android/run.sh`, stage 2) is **7,800 octets**.

Disk: the NDK sysroot is **17 MB** (instead of 2.6 GB), the Bionic for the
run is **3 MB** kept out of a 306 MB download that was deleted again. Free
space on this server before the round 3.6 GB, after it 3.9 GB — the number
moves on its own, eleven other builds share the disk, but nothing this round
left behind is bigger than the 20 MB above.

### Nothing that was green moved

`tools/android/unchanged.sh` compiles every case of the corpus with the
compiler from BEFORE the round and with the one from after, for
`x86_64-linux` and `aarch64-linux`, and compares the **emitted assembly
octet for octet**. Not "still passes" — did not move a character:

```
== the two old targets, before and after round ANDROID ==
  identical assembly   614
  changed assembly       0
  changed acceptance     0
```

That is the strongest form of the "nothing that is green may change"
auflage, and it is the reason `.type ... %function` and `.data.rel.ro` are
written on the Android target only.

---

## 8. Does `firnc1` come along? — No.

Measured, not guessed:

```
$ grep -c 'aarch64\|Aarch64\|a64\|arm64' lib/firnc1/*.fi
0
$ grep -c 'target' lib/firnc1/*.fi | grep -v ':0'
(only local variables called `target`, no target selection)
$ head -1 lib/firnc1/codegen.fi
// lib/firnc1/codegen.fi -- FROM THE FIR TO x86-64, WRITTEN IN FIRN.
```

**`firnc1` produces x86-64 and nothing else.** It has no `--target`, no
second code generator, and no notion that there could be another machine.
Everything on this page is `firnc0` (the Rust compiler).

That is not a defect of this round — it has been true since round 80, for
`aarch64-linux` just as much. It is a construction site of its own, and this
is what it would cost, honestly estimated and not started:

| piece | in `firnc0` | in `firnc1` today | estimate |
|---|---|---|---|
| the machine choice | `target.rs`, 276 lines | does not exist | ~150 lines |
| the aarch64 code generator | `codegen_a64.rs`, 1,716 lines | — | ~2,000 lines (firnc1 has no register allocation, so its x86 generator is 2,997 lines for what `codegen_x86.rs` does in 1,645; the aarch64 one would swell the same way) |
| the aarch64 SIMD | `simd_a64.rs`, 781 lines | firnc1 has no SIMD at all | 0 (not needed for the fixpoint) |
| the checked arithmetic trampoline | `panic_rt_a64.rs`, 635 lines | ~ | ~600 lines |
| the system call table | `syscalls.rs` | hard-coded x86 numbers | ~150 lines |
| Android on top of that | `android.rs`, 473 lines | — | ~300 lines |

**Roughly 3,000 to 3,500 lines of Firn**, plus the cross check that the two
compilers agree on the new machine the way `tools/fir_compare.sh` makes them
agree on the old one. It is a round, not an afternoon — and it is only worth
doing when the fixpoint is supposed to hold on aarch64 as well, i.e. when
Firn is meant to compile itself ON a phone rather than FOR one.

For an app that is not needed: the artifact is cross compiled on a build
machine, which is how every Android NDK library is built.

---

## 9. For the round that comes next (MOBIL, `/root/certus`, branch `mobil`)

What is ready to build on:

* `firnc --target=aarch64-linux-android --shared -o libX.so X.fi` produces a
  loadable `.so`. Put it in `app/src/main/jniLibs/arm64-v8a/` and
  `System.loadLibrary("X")` finds it.
* `#[export_c]` fixes the exported name. That is the name a `native`
  declaration or a `dlsym` has to use.
* `firnc --target=aarch64-linux-android --print-ndk-lib` gives the sysroot
  path a Gradle/CMake step needs to link something of its own against.
* `--android-api=<n>` has to match the app's `minSdkVersion`. Default 24.
* A program (not a library) also works and is `adb push`-able.

What is NOT ready, and would be a lie to assume:

* **JNI.** There is no `JNIEnv`, no `jstring`, no `JNI_OnLoad`. A Firn
  function is a plain C function; the glue between it and Java has to be
  written (or generated) by that round. The easiest bridge is a C shim, or
  Firn `extern fn` declarations against the JNI function table.
* **Threads.** See 5.5. Anything the app calls into Firn must be called from
  a thread the app already has — that works, Firn does not touch the thread
  pointer on this target — but Firn cannot start one of its own.
* **`libc++`.** Not linked and not needed; Firn has no C++.
* **armeabi-v7a / x86_64 Android.** Only `arm64-v8a` exists. 32 bit ARM
  would need a third code generator; `x86_64-linux-android` would be a
  fourth target and is mostly a copy of `android.rs` with another sysroot
  directory (the code generator is already there).
* **A device test.** Everything here ran under `qemu-aarch64` against a real
  Bionic. That is the loader and the C library of a device, but it is not a
  device.
