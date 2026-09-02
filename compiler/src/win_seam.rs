// SPDX-License-Identifier: GPL-2.0-only
//! **Round WINDOWS — the seam that replaces `syscall`.**
//!
//! `syscall(nr, a1..a6)` is built into the language (SPEC §13) and the number
//! is the **x86-64 Linux** one. On Windows there is nothing that a program
//! may put a number into: the system call numbers of `ntdll` change between
//! Windows versions on purpose, and every documented road leads through
//! `kernel32.dll` and its relatives.
//!
//! So on the Windows target `Op::Syscall` no longer becomes a `syscall`
//! instruction but a **call to one function**, `__win_syscall`, which is
//! written in **Firn** and injected into the compilation unit exactly the
//! way `comptime` (round 35) and the test runner (round 94) inject source
//! text: lexed, parsed, appended, type checked like hand written code.
//!
//! Writing it in Firn rather than in emitted assembler is a deliberate
//! choice and it costs nothing: the seam is descriptor tables, UTF-16
//! conversion and an error mapping — data structure work, which is exactly
//! what assembler is worst at. As a bonus the self-hosted compiler
//! (`lib/firnc1/`) will inherit this file unchanged when it catches up.
//!
//! ## The two differences that cannot be abstracted away
//!
//! **1. Paths are UTF-16.** A Linux path is a NUL-terminated octet string;
//! a Windows path is a NUL-terminated `u16` string with a drive letter and
//! back slashes. `__win_u8_to_u16` converts, and turns `/` into `\` on the
//! way, so that a Firn program which writes `"tmp/x.txt"` finds the same
//! file it would find on Linux. What it does NOT do is invent a drive
//! letter: `/etc/passwd` becomes `\etc\passwd` on the current drive, and
//! that is a different file than the one on Linux. Absolute Linux paths do
//! not survive the crossing, and nothing here pretends otherwise.
//!
//! **2. A socket is not a file descriptor.** On Windows a `SOCKET` is its
//! own kind of handle; `ReadFile` on it does not work, `recv` on a file
//! handle does not work, and `CloseHandle` and `closesocket` are two
//! different functions. Linux code says `read(fd)` for both. So the seam
//! keeps a **descriptor table**: `fd` stays the small integer Linux code
//! expects, and the table remembers whether a HANDLE or a SOCKET sits
//! behind it. `0`, `1`, `2` are filled at start-up from `GetStdHandle`.
//!
//! **Errors:** `GetLastError` / `WSAGetLastError` instead of `errno`, mapped
//! onto the negative Linux error numbers the standard library already
//! checks for (`r < 0 && r > -4096`).

/// The imports the seam always needs — registered even if the program
/// itself never says `syscall`, because `_start` has to be able to stop.
pub const BASELINE: &[&str] = &[
    "ExitProcess",
    "GetStdHandle",
    "WriteFile",
    "ReadFile",
    "CloseHandle",
    "CreateFileW",
    "DeleteFileW",
    "GetFileAttributesW",
    "SetFilePointerEx",
    "FlushFileBuffers",
    "VirtualAlloc",
    "VirtualFree",
    "VirtualProtect",
    "GetLastError",
    "QueryPerformanceCounter",
    "QueryPerformanceFrequency",
    "GetSystemTimeAsFileTime",
    "Sleep",
    "SwitchToThread",
    "GetCommandLineW",
    "GetCurrentDirectoryW",
    "GetCurrentProcessId",
    "GetCurrentThreadId",
    "GetCurrentThreadStackLimits",
    "GetCurrentProcess",
    "DuplicateHandle",
    "WSAStartup",
    "WSAGetLastError",
    "socket",
    "connect",
    "send",
    "recv",
    "closesocket",
    "shutdown",
    "bind",
    "listen",
    "accept",
    "setsockopt",
    "getsockname",
    // ROUND CERTUS-WINDOWS: the address carrying forms and `select`.
    "sendto",
    "recvfrom",
    "select",
    "CreateDirectoryW",
    "MoveFileExW",
    "SystemFunction036",
];

/// The Firn name of the dispatcher `Op::Syscall` calls.
pub const SYSCALL_FN: &str = "__win_syscall";
/// The Firn name of the start-up routine `_start` calls first.
pub const INIT_FN: &str = "__win_init";
/// The Firn name of the routine that builds the Linux style start block.
pub const ARGV_FN: &str = "__win_argv";

/// The assembler symbol of `__win_argv` (used by `win::start_asm`).
pub const ARGV_SYM_ASM: &str = "_F0.__win_argv";
/// The assembler symbol of `__win_init`.
pub const INIT_SYM_ASM: &str = "_F0.__win_init";

/// The source text of the seam.
pub fn source() -> String {
    SEAM.to_string()
}

const SEAM: &str = r####"
// ---------------------------------------------------------------------
// Round WINDOWS -- the system seam, injected by the compiler.
// Everything here is ordinary Firn. No import, no std, no allocator:
// this code runs BEFORE the collector and has to answer for `write`.
// ---------------------------------------------------------------------

extern fn ExitProcess(code: i64);
extern fn GetStdHandle(n: i64) -> i64;
extern fn WriteFile(h: i64, buf: u64, n: i64, done: u64, ov: i64) -> i32;
extern fn ReadFile(h: i64, buf: u64, n: i64, done: u64, ov: i64) -> i32;
extern fn CloseHandle(h: i64) -> i32;
extern fn CreateFileW(p: u64, acc: i64, share: i64, sa: i64, disp: i64, fl: i64, tm: i64) -> i64;
extern fn DeleteFileW(p: u64) -> i32;
extern fn GetFileAttributesW(p: u64) -> i32;
extern fn SetFilePointerEx(h: i64, dist: i64, np: u64, how: i64) -> i32;
extern fn FlushFileBuffers(h: i64) -> i32;
extern fn VirtualAlloc(a: u64, n: u64, t: i64, prot: i64) -> u64;
extern fn VirtualFree(a: u64, n: u64, t: i64) -> i32;
extern fn VirtualProtect(a: u64, n: u64, prot: i64, old: u64) -> i32;
extern fn GetLastError() -> i32;
extern fn QueryPerformanceCounter(p: u64) -> i32;
extern fn QueryPerformanceFrequency(p: u64) -> i32;
extern fn GetSystemTimeAsFileTime(p: u64);
extern fn Sleep(ms: i64);
extern fn SwitchToThread() -> i32;
extern fn GetCommandLineW() -> u64;
extern fn GetCurrentDirectoryW(n: i64, buf: u64) -> i32;
extern fn GetCurrentProcessId() -> i32;
extern fn GetCurrentThreadId() -> i32;
extern fn GetCurrentThreadStackLimits(lo: u64, hi: u64);
extern fn GetCurrentProcess() -> i64;
extern fn DuplicateHandle(sp: i64, sh: i64, tp: i64, th: u64, acc: i64, inh: i64, opt: i64) -> i32;
extern fn WSAStartup(ver: i64, data: u64) -> i32;
extern fn WSAGetLastError() -> i32;
extern fn socket(af: i64, ty: i64, pr: i64) -> i64;
extern fn connect(s: i64, a: u64, n: i64) -> i32;
extern fn send(s: i64, b: u64, n: i64, f: i64) -> i32;
extern fn recv(s: i64, b: u64, n: i64, f: i64) -> i32;
extern fn closesocket(s: i64) -> i32;
extern fn shutdown(s: i64, how: i64) -> i32;
extern fn bind(s: i64, a: u64, n: i64) -> i32;
extern fn listen(s: i64, back: i64) -> i32;
extern fn accept(s: i64, a: u64, n: u64) -> i64;
extern fn setsockopt(s: i64, lvl: i64, opt: i64, v: u64, n: i64) -> i32;
extern fn getsockname(s: i64, a: u64, n: u64) -> i32;
extern fn sendto(s: i64, b: u64, n: i64, f: i64, a: u64, al: i64) -> i32;
extern fn recvfrom(s: i64, b: u64, n: i64, f: i64, a: u64, al: u64) -> i32;
extern fn select(n: i64, rd: u64, wr: u64, ex: u64, tv: u64) -> i32;
extern fn CreateDirectoryW(p: u64, sa: i64) -> i32;
extern fn MoveFileExW(a: u64, b: u64, fl: i64) -> i32;
extern fn SystemFunction036(buf: u64, n: i64) -> i32;

// 0 = free, 1 = file handle, 2 = socket, 3 = a file that only exists here
static mut __win_kind: [i64; 256] = [0; 256]
static mut __win_hnd: [i64; 256] = [0; 256]
// only for kind 3: how far it has been read and how long it is
static mut __win_pos: [i64; 256] = [0; 256]
static mut __win_len: [i64; 256] = [0; 256]
// scratch pages, allocated once at start-up
static mut __win_path: u64 = 0
static mut __win_blk: u64 = 0
static mut __win_tmp: u64 = 0
static mut __win_freq: i64 = 0
static mut __win_wsa: i64 = 0
static mut __win_argp: u64 = 0

// --------------------------------------------------------- raw memory
fn __win_ld8(p: u64, i: i64) -> i64 {
    let q: *mut u8 = ((p as i64) + i) as *mut u8
    return (*q) as i64
}
fn __win_st8(p: u64, i: i64, v: i64) {
    let q: *mut u8 = ((p as i64) + i) as *mut u8
    *q = v as u8
}
fn __win_ld16(p: u64, i: i64) -> i64 {
    let q: *mut u16 = ((p as i64) + i * 2) as *mut u16
    return (*q) as i64
}
fn __win_st16(p: u64, i: i64, v: i64) {
    let q: *mut u16 = ((p as i64) + i * 2) as *mut u16
    *q = v as u16
}
fn __win_ld64(p: u64, i: i64) -> i64 {
    let q: *mut i64 = ((p as i64) + i * 8) as *mut i64
    return *q
}
fn __win_st64(p: u64, i: i64, v: i64) {
    let q: *mut i64 = ((p as i64) + i * 8) as *mut i64
    *q = v
}

// --------------------------------------------------------- error codes
// GetLastError -> the negative Linux number the library already tests for.
fn __win_errno() -> i64 {
    let e: i64 = (GetLastError() as i64) & 65535
    if e == 2 { return 0 - 2 }        // ERROR_FILE_NOT_FOUND -> ENOENT
    if e == 3 { return 0 - 2 }        // ERROR_PATH_NOT_FOUND -> ENOENT
    if e == 5 { return 0 - 13 }       // ERROR_ACCESS_DENIED  -> EACCES
    if e == 8 { return 0 - 12 }       // NOT_ENOUGH_MEMORY    -> ENOMEM
    if e == 15 { return 0 - 2 }       // INVALID_DRIVE
    if e == 32 { return 0 - 13 }      // SHARING_VIOLATION
    if e == 80 { return 0 - 17 }      // FILE_EXISTS          -> EEXIST
    if e == 87 { return 0 - 22 }      // INVALID_PARAMETER    -> EINVAL
    if e == 109 { return 0 - 32 }     // BROKEN_PIPE          -> EPIPE
    if e == 183 { return 0 - 17 }     // ALREADY_EXISTS       -> EEXIST
    if e == 232 { return 0 - 32 }     // NO_DATA              -> EPIPE
    return 0 - 5                      // EIO
}

fn __win_sockerrno() -> i64 {
    let e: i64 = (WSAGetLastError() as i64) & 65535
    if e == 10035 { return 0 - 11 }   // WSAEWOULDBLOCK -> EAGAIN
    if e == 10036 { return 0 - 115 }  // WSAEINPROGRESS -> EINPROGRESS
    if e == 10048 { return 0 - 98 }   // WSAEADDRINUSE  -> EADDRINUSE
    if e == 10053 { return 0 - 104 }  // WSAECONNABORTED
    if e == 10054 { return 0 - 104 }  // WSAECONNRESET  -> ECONNRESET
    if e == 10057 { return 0 - 107 }  // WSAENOTCONN
    if e == 10060 { return 0 - 110 }  // WSAETIMEDOUT
    if e == 10061 { return 0 - 111 }  // WSAECONNREFUSED
    if e == 10065 { return 0 - 113 }  // WSAEHOSTUNREACH
    return 0 - 5
}

// ------------------------------------------------------------ UTF-16
// UTF-8 -> UTF-16, NUL terminated. `/` becomes `\`.
// Returns the number of u16 units written, or -1 if `cap` is too small.
fn __win_u8_to_u16(src: u64, dst: u64, cap: i64) -> i64 {
    var i: i64 = 0
    var o: i64 = 0
    while true {
        let c0: i64 = __win_ld8(src, i)
        if c0 == 0 {
            break
        }
        var cp: i64 = 0
        var len: i64 = 1
        if c0 < 128 {
            cp = c0
        } else if (c0 & 224) == 192 {
            cp = c0 & 31
            len = 2
        } else if (c0 & 240) == 224 {
            cp = c0 & 15
            len = 3
        } else if (c0 & 248) == 240 {
            cp = c0 & 7
            len = 4
        } else {
            cp = 65533
            len = 1
        }
        var k: i64 = 1
        while k < len {
            cp = cp * 64 + (__win_ld8(src, i + k) & 63)
            k = k + 1
        }
        i = i + len
        if cp == 47 {
            cp = 92
        }
        if cp < 65536 {
            if o + 2 > cap {
                return 0 - 1
            }
            __win_st16(dst, o, cp)
            o = o + 1
        } else {
            if o + 3 > cap {
                return 0 - 1
            }
            let v: i64 = cp - 65536
            __win_st16(dst, o, 55296 + v / 1024)
            __win_st16(dst, o + 1, 56320 + v % 1024)
            o = o + 2
        }
    }
    __win_st16(dst, o, 0)
    return o
}

// UTF-16 -> UTF-8, NUL terminated. `n` = number of u16 units (-1 = up to
// the NUL). Returns the number of octets written without the NUL.
fn __win_u16_to_u8(src: u64, n: i64, dst: u64, cap: i64) -> i64 {
    var i: i64 = 0
    var o: i64 = 0
    while true {
        if n >= 0 && i >= n {
            break
        }
        let w: i64 = __win_ld16(src, i)
        if w == 0 && n < 0 {
            break
        }
        var cp: i64 = w
        i = i + 1
        if w >= 55296 && w < 56320 {
            let lo: i64 = __win_ld16(src, i)
            if lo >= 56320 && lo < 57344 {
                cp = 65536 + (w - 55296) * 1024 + (lo - 56320)
                i = i + 1
            }
        }
        if cp < 128 {
            if o + 2 > cap { return o }
            __win_st8(dst, o, cp)
            o = o + 1
        } else if cp < 2048 {
            if o + 3 > cap { return o }
            __win_st8(dst, o, 192 + cp / 64)
            __win_st8(dst, o + 1, 128 + cp % 64)
            o = o + 2
        } else if cp < 65536 {
            if o + 4 > cap { return o }
            __win_st8(dst, o, 224 + cp / 4096)
            __win_st8(dst, o + 1, 128 + (cp / 64) % 64)
            __win_st8(dst, o + 2, 128 + cp % 64)
            o = o + 3
        } else {
            if o + 5 > cap { return o }
            __win_st8(dst, o, 240 + cp / 262144)
            __win_st8(dst, o + 1, 128 + (cp / 4096) % 64)
            __win_st8(dst, o + 2, 128 + (cp / 64) % 64)
            __win_st8(dst, o + 3, 128 + cp % 64)
            o = o + 4
        }
    }
    __win_st8(dst, o, 0)
    return o
}

// ------------------------------------------------------------- /proc
// THE COLLECTOR NEEDS THE STACK BOUNDS, and on Linux it reads them out of
// `/proc/self/maps` (lib/gc/gc.fi, `__gc_stack_bottom_maps`). Windows has
// no `/proc`, and this is the one place where "just refuse it" would be
// the wrong answer: `gc_init` then returns false and every program with a
// `gc class` in it stops working.
//
// Worse, under Wine the refusal does not even happen. Wine maps the drive
// `Z:` onto the host's root directory, so `\proc\self\maps` really opens
// the LINUX file -- and the collector then scans from the Windows stack
// pointer up to the end of a LINUX mapping, walks off the committed part
// of the Windows stack and the process dies with a page fault. That is
// not a theory; it is what round WINDOWS measured before this function
// existed (35 of 46 failing cases).
//
// So the seam answers the file itself, out of `GetCurrentThreadStackLimits`
// -- one line in exactly the shape the collector parses.
fn __win_hexdigit(v: i64) -> i64 {
    if v < 10 {
        return 48 + v
    }
    return 87 + v
}

fn __win_puthex(dst: u64, off: i64, v: i64) -> i64 {
    if v == 0 {
        __win_st8(dst, off, 48)
        return off + 1
    }
    // The highest nibble first: find it, then walk down.
    var sh: i64 = 60
    while sh > 0 {
        if (v / __win_pow16(sh / 4)) % 16 != 0 {
            break
        }
        sh = sh - 4
    }
    var o: i64 = off
    while sh >= 0 {
        __win_st8(dst, o, __win_hexdigit((v / __win_pow16(sh / 4)) % 16))
        o = o + 1
        sh = sh - 4
    }
    return o
}

fn __win_pow16(n: i64) -> i64 {
    var r: i64 = 1
    var i: i64 = 0
    while i < n {
        r = r * 16
        i = i + 1
    }
    return r
}

// Is `p` the NUL terminated text `/proc/self/maps`?
fn __win_is_maps(p: u64) -> i64 {
    var lit: [u8; 16] = "/proc/self/maps\0"
    let q: u64 = (&lit[0]) as u64
    var i: i64 = 0
    while i < 16 {
        if __win_ld8(p, i) != __win_ld8(q, i) {
            return 0
        }
        i = i + 1
    }
    return 1
}

// Does `p` begin with `/proc/`?
fn __win_is_proc(p: u64) -> i64 {
    var lit: [u8; 7] = "/proc/\0"
    let q: u64 = (&lit[0]) as u64
    var i: i64 = 0
    while i < 6 {
        if __win_ld8(p, i) != __win_ld8(q, i) {
            return 0
        }
        i = i + 1
    }
    return 1
}

// "<lo>-<hi> rw-p 00000000 00:00 0 [stack]\n" -- the one line the
// collector is looking for. Returns the length.
fn __win_make_maps(dst: u64) -> i64 {
    let lo: u64 = __win_tmp + 160
    let hi: u64 = __win_tmp + 168
    __win_st64(lo, 0, 0)
    __win_st64(hi, 0, 0)
    GetCurrentThreadStackLimits(lo, hi)
    let a: i64 = __win_ld64(lo, 0)
    let b: i64 = __win_ld64(hi, 0)
    if a == 0 || b == 0 || b <= a {
        return 0
    }
    var o: i64 = __win_puthex(dst, 0, a)
    __win_st8(dst, o, 45)
    o = o + 1
    o = __win_puthex(dst, o, b)
    var tail: [u8; 32] = " rw-p 00000000 00:00 0 [stack]\n\0"
    let t: u64 = (&tail[0]) as u64
    var i: i64 = 0
    while i < 30 {
        __win_st8(dst, o, __win_ld8(t, i))
        o = o + 1
        i = i + 1
    }
    return o
}

// ------------------------------------------------------ descriptor table
fn __win_slot(kind: i64, h: i64) -> i64 {
    var i: i64 = 3
    while i < 256 {
        if __win_kind[i as usize] == 0 {
            __win_kind[i as usize] = kind
            __win_hnd[i as usize] = h
            return i
        }
        i = i + 1
    }
    return 0 - 24
}

fn __win_kind_of(fd: i64) -> i64 {
    if fd < 0 || fd >= 256 {
        return 0
    }
    return __win_kind[fd as usize]
}

fn __win_handle(fd: i64) -> i64 {
    if fd < 0 || fd >= 256 {
        return 0 - 1
    }
    return __win_hnd[fd as usize]
}

// ------------------------------------------------------------- start-up
fn __win_init() {
    // 64 KiB of scratch: path buffer, start block, small helpers.
    let p: u64 = VirtualAlloc(0, 262144, 12288, 4)
    __win_path = p
    __win_blk = p + 65536
    __win_tmp = p + 196608
    __win_kind[0] = 1
    __win_hnd[0] = GetStdHandle(0 - 10)
    __win_kind[1] = 1
    __win_hnd[1] = GetStdHandle(0 - 11)
    __win_kind[2] = 1
    __win_hnd[2] = GetStdHandle(0 - 12)
    QueryPerformanceFrequency(__win_tmp)
    __win_freq = __win_ld64(__win_tmp, 0)
    if __win_freq <= 0 {
        __win_freq = 10000000
    }
}

// The block Linux would have left on the stack:
//   [argc][argv0]..[argvN][0][envp0=0][0]
// `_start` puts its address into the first parameter of `main`.
fn __win_argv() -> u64 {
    let blk: u64 = __win_blk
    let cmd: u64 = GetCommandLineW()
    // The text goes behind the pointer table: 512 words of table, then the
    // octets.
    let text: u64 = blk + 4096
    let n: i64 = __win_u16_to_u8(cmd, 0 - 1, text, 32768)
    // Split the command line the way Windows does for the simple cases:
    // quotes group, spaces separate. A backslash before a quote is NOT
    // special here -- that rule only exists inside quoted stretches in
    // Microsoft's own parser and would silently corrupt paths.
    var argc: i64 = 0
    var i: i64 = 0
    while i < n {
        while i < n && __win_ld8(text, i) == 32 {
            i = i + 1
        }
        if i >= n {
            break
        }
        var q: i64 = 0
        if __win_ld8(text, i) == 34 {
            q = 1
            i = i + 1
        }
        let start: i64 = i
        while i < n {
            let c: i64 = __win_ld8(text, i)
            if q == 1 && c == 34 {
                break
            }
            if q == 0 && c == 32 {
                break
            }
            i = i + 1
        }
        if argc < 500 {
            __win_st64(blk, 1 + argc, (text as i64) + start)
            argc = argc + 1
        }
        // Terminate the word in place; the next one starts behind it.
        if i < n {
            __win_st8(text, i, 0)
            i = i + 1
        }
    }
    __win_st64(blk, 0, argc)
    __win_st64(blk, 1 + argc, 0)
    __win_st64(blk, 2 + argc, 0)
    __win_argp = blk
    return blk
}

// ------------------------------------------------------------ the calls
fn __win_write(fd: i64, buf: u64, n: i64) -> i64 {
    let k: i64 = __win_kind_of(fd)
    if k == 0 || k == 3 {
        return 0 - 9
    }
    if k == 2 {
        let r: i64 = send(__win_handle(fd), buf, n, 0) as i64
        if r < 0 {
            return __win_sockerrno()
        }
        return r
    }
    let done: u64 = __win_tmp + 256
    __win_st64(done, 0, 0)
    let ok: i32 = WriteFile(__win_handle(fd), buf, n, done, 0)
    if ok == 0 {
        return __win_errno()
    }
    return __win_ld64(done, 0) & 4294967295
}

fn __win_read(fd: i64, buf: u64, n: i64) -> i64 {
    let k: i64 = __win_kind_of(fd)
    if k == 0 {
        return 0 - 9
    }
    if k == 3 {
        let src: u64 = __win_handle(fd) as u64
        var left: i64 = __win_len[fd as usize] - __win_pos[fd as usize]
        if left > n {
            left = n
        }
        if left <= 0 {
            return 0
        }
        var i: i64 = 0
        while i < left {
            __win_st8(buf, i, __win_ld8(src, __win_pos[fd as usize] + i))
            i = i + 1
        }
        __win_pos[fd as usize] = __win_pos[fd as usize] + left
        return left
    }
    if k == 2 {
        let r: i64 = recv(__win_handle(fd), buf, n, 0) as i64
        if r < 0 {
            return __win_sockerrno()
        }
        return r
    }
    let done: u64 = __win_tmp + 264
    __win_st64(done, 0, 0)
    let ok: i32 = ReadFile(__win_handle(fd), buf, n, done, 0)
    if ok == 0 {
        // A pipe whose writer is gone reads as end of file on Linux.
        let e: i64 = (GetLastError() as i64) & 65535
        if e == 109 || e == 38 {
            return 0
        }
        return __win_errno()
    }
    return __win_ld64(done, 0) & 4294967295
}

fn __win_open(path: u64, flags: i64, mode: i64) -> i64 {
    // `/proc` does not exist here. `/proc/self/maps` is answered out of
    // the thread's own stack bounds; every other name under `/proc` is a
    // clean ENOENT rather than whatever Wine's drive Z: would find.
    if __win_is_proc(path) == 1 {
        if __win_is_maps(path) == 0 {
            return 0 - 2
        }
        let buf: u64 = __win_tmp + 1024
        let n: i64 = __win_make_maps(buf)
        if n == 0 {
            return 0 - 2
        }
        let fd: i64 = __win_slot(3, buf as i64)
        if fd < 0 {
            return fd
        }
        __win_pos[fd as usize] = 0
        __win_len[fd as usize] = n
        return fd
    }
    let w: u64 = __win_path
    if __win_u8_to_u16(path, w, 16384) < 0 {
        return 0 - 36
    }
    let acc_r: i64 = 2147483648
    let acc_w: i64 = 1073741824
    var acc: i64 = acc_r
    let lo: i64 = flags & 3
    if lo == 1 {
        acc = acc_w
    }
    if lo == 2 {
        acc = acc_r + acc_w
    }
    // O_CREAT 64, O_TRUNC 512, O_APPEND 1024, O_EXCL 128
    var disp: i64 = 3
    if (flags & 64) != 0 && (flags & 512) != 0 {
        disp = 2
    } else if (flags & 64) != 0 && (flags & 128) != 0 {
        disp = 1
    } else if (flags & 64) != 0 {
        disp = 4
    } else if (flags & 512) != 0 {
        disp = 5
    }
    let h: i64 = CreateFileW(w, acc, 7, 0, disp, 128, 0)
    if h == 0 - 1 {
        return __win_errno()
    }
    if (flags & 1024) != 0 {
        SetFilePointerEx(h, 0, 0, 2)
    }
    let fd: i64 = __win_slot(1, h)
    if fd < 0 {
        CloseHandle(h)
        return fd
    }
    return fd
}

fn __win_close(fd: i64) -> i64 {
    let k: i64 = __win_kind_of(fd)
    if k == 0 {
        return 0 - 9
    }
    if fd >= 3 {
        __win_kind[fd as usize] = 0
    }
    if k == 3 {
        return 0
    }
    if k == 2 {
        closesocket(__win_handle(fd))
        return 0
    }
    if CloseHandle(__win_handle(fd)) == 0 {
        return __win_errno()
    }
    return 0
}

fn __win_prot(p: i64) -> i64 {
    if p == 0 { return 1 }
    if p == 1 { return 2 }
    if p == 3 { return 4 }
    if p == 5 { return 32 }
    if p == 7 { return 64 }
    if p == 4 { return 16 }
    return 4
}

fn __win_clock(id: i64, ts: u64) -> i64 {
    if id == 0 {
        // CLOCK_REALTIME: FILETIME is 100 ns units since 1601.
        GetSystemTimeAsFileTime(__win_tmp + 128)
        let ft: i64 = __win_ld64(__win_tmp + 128, 0)
        let u: i64 = ft - 116444736000000000
        __win_st64(ts, 0, u / 10000000)
        __win_st64(ts, 1, (u % 10000000) * 100)
        return 0
    }
    QueryPerformanceCounter(__win_tmp + 136)
    let c: i64 = __win_ld64(__win_tmp + 136, 0)
    let f: i64 = __win_freq
    __win_st64(ts, 0, c / f)
    __win_st64(ts, 1, (c % f) * 1000000000 / f)
    return 0
}

fn __win_wsa_up() {
    if __win_wsa == 0 {
        WSAStartup(514, __win_tmp + 512)
        __win_wsa = 1
    }
}

fn __win_getcwd(buf: u64, size: i64) -> i64 {
    let n: i32 = GetCurrentDirectoryW(8192, __win_path)
    if n == 0 {
        return __win_errno()
    }
    let m: i64 = __win_u16_to_u8(__win_path, n as i64, buf, size)
    // Linux hands back a path with forward slashes; keep it recognizable.
    var i: i64 = 0
    while i < m {
        if __win_ld8(buf, i) == 92 {
            __win_st8(buf, i, 47)
        }
        i = i + 1
    }
    return m + 1
}

// `dup`/`dup2`. Linux hands out a second name for the same open file; the
// Windows equivalent is a second HANDLE for the same object, which is what
// `DuplicateHandle` makes. `into` < 0 means "the lowest free slot" (that is
// `dup`), otherwise it is `dup2` and the slot is closed first.
//
// Only file handles. A SOCKET would need `WSADuplicateSocket` and a second
// `socket` call in the target, which is a different thing and is refused
// rather than faked.
fn __win_dup(fd: i64, into: i64) -> i64 {
    if __win_kind_of(fd) != 1 {
        return 0 - 9
    }
    if into == fd {
        return fd
    }
    let me: i64 = GetCurrentProcess()
    let out: u64 = __win_tmp + 176
    __win_st64(out, 0, 0)
    // DUPLICATE_SAME_ACCESS = 2
    if DuplicateHandle(me, __win_handle(fd), me, out, 0, 0, 2) == 0 {
        return __win_errno()
    }
    let h: i64 = __win_ld64(out, 0)
    if into < 0 {
        return __win_slot(1, h)
    }
    if into >= 256 {
        return 0 - 9
    }
    if __win_kind[into as usize] != 0 {
        __win_close(into)
    }
    __win_kind[into as usize] = 1
    __win_hnd[into as usize] = h
    return into
}

// ------------------------------------------------- the address family
// ROUND CERTUS-WINDOWS. A `sockaddr_in` is the same twelve octets on both
// systems, and AF_INET is 2 on both -- that is why round MERGE-WIN got
// away without ever looking. AF_INET6 is 10 on Linux and 23 on Windows,
// and `lib/net/dns.fi` opens exactly that socket to find out whether the
// machine has a v6 route. Passing 10 through makes Winsock answer
// WSAEAFNOSUPPORT, which is not wrong here (the answer "no v6" is
// correct), but it would be wrong the moment anything really speaks v6.
// So the number is translated in both directions, in one place.
fn __win_af_to_win(af: i64) -> i64 {
    if af == 10 { return 23 }
    return af
}

// A sockaddr on its way OUT: if it is v6, its family octets have to say 23.
// Everything else is copied unchanged. The copy goes into scratch, because
// the caller's buffer belongs to the caller.
fn __win_sa_out(a: u64, n: i64) -> u64 {
    if n <= 0 { return a }
    let fam: i64 = __win_ld16(a, 0)
    if fam != 10 { return a }
    let d: u64 = __win_tmp + 1024
    var i: i64 = 0
    while i < n {
        __win_st8(d, i, __win_ld8(a, i))
        i = i + 1
    }
    __win_st16(d, 0, 23)
    return d
}

// A sockaddr on its way IN: Windows says 23, the library expects 10.
fn __win_sa_in(a: u64, n: i64) {
    if n <= 0 { return }
    if __win_ld16(a, 0) == 23 {
        __win_st16(a, 0, 10)
    }
}

// ------------------------------------------------------------- poll
// `poll(2)` over `select`. Linux `struct pollfd` is {i32 fd, i16 events,
// i16 revents} = 8 octets; a Windows `fd_set` is {u32 count, pad,
// SOCKET[64]}. Only sockets can be waited for -- a file handle is always
// ready and is answered that way, which is what `poll` on a regular file
// does on Linux too.
fn __win_fdset_clear(p: u64) {
    __win_st64(p, 0, 0)
}

fn __win_fdset_add(p: u64, s: i64) {
    let c: i64 = __win_ld64(p, 0)
    if c >= 64 { return }
    __win_st64(p, 1 + c, s)
    __win_st64(p, 0, c + 1)
}

fn __win_fdset_has(p: u64, s: i64) -> bool {
    let c: i64 = __win_ld64(p, 0)
    var i: i64 = 0
    while i < c {
        if __win_ld64(p, 1 + i) == s { return true }
        i = i + 1
    }
    return false
}

fn __win_poll(fds: u64, nfds: i64, ms: i64) -> i64 {
    if nfds <= 0 {
        if ms > 0 { Sleep(ms) }
        return 0
    }
    let rd: u64 = __win_tmp + 2048
    let wr: u64 = __win_tmp + 3072
    let ex: u64 = __win_tmp + 4096
    __win_fdset_clear(rd)
    __win_fdset_clear(wr)
    __win_fdset_clear(ex)
    var ready: i64 = 0
    var socks: i64 = 0
    var i: i64 = 0
    while i < nfds {
        let off: i64 = i * 8
        let q: *mut i32 = ((fds as i64) + off) as *mut i32
        let fd: i64 = (*q) as i64
        let ev: i64 = __win_ld16(fds, off / 2 + 2)
        let rp: *mut u16 = ((fds as i64) + off + 6) as *mut u16
        *rp = 0 as u16
        if fd >= 0 {
            if __win_kind_of(fd) == 2 {
                let h: i64 = __win_handle(fd)
                if (ev & 1) != 0 { __win_fdset_add(rd, h) }
                if (ev & 4) != 0 { __win_fdset_add(wr, h) }
                __win_fdset_add(ex, h)
                socks = socks + 1
            } else {
                if __win_kind_of(fd) != 0 {
                    // a file or the console: always ready
                    *rp = (ev & 5) as u16
                    ready = ready + 1
                }
            }
        }
        i = i + 1
    }
    if socks == 0 {
        if ready == 0 {
            if ms > 0 { Sleep(ms) }
        }
        return ready
    }
    if ready > 0 {
        // something is ready already -- ask without waiting
        let tv0: u64 = __win_tmp + 5120
        __win_st64(tv0, 0, 0)
        __win_st64(tv0, 1, 0)
        select(0, rd, wr, ex, tv0)
    } else {
        var tvp: u64 = 0
        if ms >= 0 {
            let tv: u64 = __win_tmp + 5120
            __win_st64(tv, 0, ms / 1000)
            __win_st64(tv, 1, (ms % 1000) * 1000)
            tvp = tv
        }
        let r: i32 = select(0, rd, wr, ex, tvp)
        if (r as i64) < 0 {
            return __win_sockerrno()
        }
    }
    i = 0
    while i < nfds {
        let off: i64 = i * 8
        let q: *mut i32 = ((fds as i64) + off) as *mut i32
        let fd: i64 = (*q) as i64
        let rp: *mut u16 = ((fds as i64) + off + 6) as *mut u16
        if fd >= 0 {
            if __win_kind_of(fd) == 2 {
                let h: i64 = __win_handle(fd)
                var re: i64 = 0
                if __win_fdset_has(rd, h) { re = re | 1 }
                if __win_fdset_has(wr, h) { re = re | 4 }
                if __win_fdset_has(ex, h) { re = re | 8 }
                if re != 0 {
                    *rp = re as u16
                    ready = ready + 1
                }
            }
        }
        i = i + 1
    }
    return ready
}

// --------------------------------------------------------- the dispatcher
// The number is the canonical (x86-64 Linux) one, exactly as it is written
// in the source. Everything that has no Windows equivalent answers -38
// (ENOSYS) -- visibly wrong rather than quietly wrong.
fn __win_syscall(nr: i64, a1: i64, a2: i64, a3: i64, a4: i64, a5: i64, a6: i64) -> i64 {
    if nr == 1 {
        return __win_write(a1, a2 as u64, a3)
    }
    if nr == 0 {
        return __win_read(a1, a2 as u64, a3)
    }
    if nr == 9 {
        // mmap: anonymous private only.
        let p: u64 = VirtualAlloc(0, a2 as u64, 12288, __win_prot(a3))
        if p == 0 {
            return 0 - 12
        }
        return p as i64
    }
    if nr == 11 {
        // munmap: whole reservations go back, a piece of one is decommitted.
        if VirtualFree(a1 as u64, 0, 32768) == 0 {
            VirtualFree(a1 as u64, a2 as u64, 16384)
        }
        return 0
    }
    if nr == 10 {
        if VirtualProtect(a1 as u64, a2 as u64, __win_prot(a3), __win_tmp + 144) == 0 {
            return __win_errno()
        }
        return 0
    }
    if nr == 3 {
        return __win_close(a1)
    }
    if nr == 2 {
        return __win_open(a1 as u64, a2, a3)
    }
    if nr == 257 {
        // openat(AT_FDCWD, path, flags, mode)
        return __win_open(a2 as u64, a3, a4)
    }
    if nr == 228 {
        return __win_clock(a1, a2 as u64)
    }
    if nr == 60 || nr == 231 {
        ExitProcess(a1 & 255)
        return 0
    }
    if nr == 8 {
        // lseek(fd, off, whence)
        if __win_kind_of(a1) != 1 {
            return 0 - 9
        }
        if SetFilePointerEx(__win_handle(a1), a2, __win_tmp + 152, a3) == 0 {
            return __win_errno()
        }
        return __win_ld64(__win_tmp + 152, 0)
    }
    if nr == 24 {
        SwitchToThread()
        return 0
    }
    if nr == 35 {
        // nanosleep(ts, rem)
        let s: i64 = __win_ld64(a1 as u64, 0)
        let ns: i64 = __win_ld64(a1 as u64, 1)
        Sleep(s * 1000 + ns / 1000000)
        return 0
    }
    if nr == 39 {
        return (GetCurrentProcessId() as i64) & 4294967295
    }
    if nr == 186 {
        return (GetCurrentThreadId() as i64) & 4294967295
    }
    if nr == 318 {
        // getrandom(buf, len, flags) -- SystemFunction036 IS RtlGenRandom.
        if (SystemFunction036(a1 as u64, a2) & 255) == 0 {
            return 0 - 5
        }
        return a2
    }
    if nr == 21 {
        // access(path, mode)
        if __win_u8_to_u16(a1 as u64, __win_path, 16384) < 0 {
            return 0 - 36
        }
        if GetFileAttributesW(__win_path) == 0 - 1 {
            return 0 - 2
        }
        return 0
    }
    if nr == 87 {
        // unlink(path)
        if __win_u8_to_u16(a1 as u64, __win_path, 16384) < 0 {
            return 0 - 36
        }
        if DeleteFileW(__win_path) == 0 {
            return __win_errno()
        }
        return 0
    }
    if nr == 79 {
        return __win_getcwd(a1 as u64, a2)
    }
    if nr == 74 {
        // fsync
        if __win_kind_of(a1) != 1 {
            return 0 - 9
        }
        FlushFileBuffers(__win_handle(a1))
        return 0
    }
    if nr == 41 {
        // socket(domain, type, protocol)
        __win_wsa_up()
        let s: i64 = socket(__win_af_to_win(a1), a2 & 255, a3)
        if s == 0 - 1 {
            return __win_sockerrno()
        }
        return __win_slot(2, s)
    }
    if nr == 42 {
        if __win_kind_of(a1) != 2 {
            return 0 - 88
        }
        if connect(__win_handle(a1), __win_sa_out(a2 as u64, a3), a3) != 0 {
            return __win_sockerrno()
        }
        return 0
    }
    if nr == 44 {
        // sendto(fd, buf, len, flags, addr, alen).
        // Without an address this is the connected form and goes through
        // the same path `write` takes. WITH one it is the form round
        // MERGE-WIN did not bind, and it is the form every DNS question
        // in `lib/net/udp.fi` uses.
        if a5 == 0 || a6 <= 0 {
            return __win_write(a1, a2 as u64, a3)
        }
        if __win_kind_of(a1) != 2 {
            return 0 - 88
        }
        let sa: u64 = __win_sa_out(a5 as u64, a6)
        let r: i64 = sendto(__win_handle(a1), a2 as u64, a3, a4, sa, a6) as i64
        if r < 0 {
            return __win_sockerrno()
        }
        return r
    }
    if nr == 45 {
        // recvfrom(fd, buf, len, flags, addr, alenp)
        if a5 == 0 || a6 == 0 {
            return __win_read(a1, a2 as u64, a3)
        }
        if __win_kind_of(a1) != 2 {
            return 0 - 88
        }
        let r: i64 = recvfrom(__win_handle(a1), a2 as u64, a3, a4,
            a5 as u64, a6 as u64) as i64
        if r < 0 {
            return __win_sockerrno()
        }
        let lp: *mut i32 = a6 as *mut i32
        __win_sa_in(a5 as u64, (*lp) as i64)
        return r
    }
    if nr == 7 {
        return __win_poll(a1 as u64, a2, a3)
    }
    if nr == 83 {
        // mkdir(path, mode) -- the mode has no Windows equivalent.
        if __win_u8_to_u16(a1 as u64, __win_path, 16384) < 0 {
            return 0 - 36
        }
        if CreateDirectoryW(__win_path, 0) == 0 {
            return __win_errno()
        }
        return 0
    }
    if nr == 82 {
        // rename(old, new). MOVEFILE_REPLACE_EXISTING = 1, which is what
        // rename(2) does and what CreateDirectoryW's neighbour does not.
        if __win_u8_to_u16(a1 as u64, __win_path, 8192) < 0 {
            return 0 - 36
        }
        if __win_u8_to_u16(a2 as u64, __win_path + 16384, 8192) < 0 {
            return 0 - 36
        }
        if MoveFileExW(__win_path, __win_path + 16384, 1) == 0 {
            return __win_errno()
        }
        return 0
    }
    if nr == 48 {
        if __win_kind_of(a1) != 2 {
            return 0 - 88
        }
        if shutdown(__win_handle(a1), a2) != 0 {
            return __win_sockerrno()
        }
        return 0
    }
    if nr == 49 {
        if __win_kind_of(a1) != 2 {
            return 0 - 88
        }
        if bind(__win_handle(a1), __win_sa_out(a2 as u64, a3), a3) != 0 {
            return __win_sockerrno()
        }
        return 0
    }
    if nr == 50 {
        if __win_kind_of(a1) != 2 {
            return 0 - 88
        }
        if listen(__win_handle(a1), a2) != 0 {
            return __win_sockerrno()
        }
        return 0
    }
    if nr == 43 || nr == 288 {
        // accept / accept4
        if __win_kind_of(a1) != 2 {
            return 0 - 88
        }
        let s: i64 = accept(__win_handle(a1), a2 as u64, a3 as u64)
        if s == 0 - 1 {
            return __win_sockerrno()
        }
        return __win_slot(2, s)
    }
    if nr == 32 {
        return __win_dup(a1, 0 - 1)
    }
    if nr == 33 {
        return __win_dup(a1, a2)
    }
    if nr == 51 {
        if __win_kind_of(a1) != 2 {
            return 0 - 88
        }
        if getsockname(__win_handle(a1), a2 as u64, a3 as u64) != 0 {
            return __win_sockerrno()
        }
        return 0
    }
    if nr == 54 {
        // setsockopt: SOL_SOCKET is 1 on Linux and 65535 on Windows,
        // SO_REUSEADDR 2 there and 4 here.
        if __win_kind_of(a1) != 2 {
            return 0 - 88
        }
        var lvl: i64 = a2
        var opt: i64 = a3
        var val: u64 = a4 as u64
        var len: i64 = a5
        if lvl == 1 {
            lvl = 65535
            if opt == 2 { opt = 4 }
            if opt == 9 { opt = 8 }
            // SO_RCVTIMEO / SO_SNDTIMEO are the one option where the VALUE
            // differs and not just its number: Linux takes a `struct
            // timeval` of two words, Windows takes a single DWORD of
            // milliseconds. Passing the timeval through would set a
            // timeout of whatever the seconds field happens to be in
            // milliseconds -- silently wrong, which is the worst kind.
            if opt == 20 || opt == 21 {
                let sec: i64 = __win_ld64(val, 0)
                let usec: i64 = __win_ld64(val, 1)
                let ms: u64 = __win_tmp + 184
                __win_st64(ms, 0, 0)
                let q: *mut u32 = ms as *mut u32
                *q = (sec * 1000 + usec / 1000) as u32
                val = ms
                len = 4
                if opt == 20 { opt = 4102 }
                if opt == 21 { opt = 4101 }
            }
        }
        if setsockopt(__win_handle(a1), lvl, opt, val, len) != 0 {
            return __win_sockerrno()
        }
        return 0
    }
    if nr == 158 {
        // arch_prctl(ARCH_SET_FS): Windows keeps its own thread block.
        return 0
    }
    return 0 - 38
}
"####;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seam_declares_every_baseline_import() {
        let s = source();
        for n in BASELINE {
            assert!(
                s.contains(&format!("extern fn {}(", n)) || s.contains(&format!("extern fn {}()", n)),
                "the seam never declares {}",
                n
            );
        }
    }

    #[test]
    fn the_dispatcher_and_the_start_up_are_there_under_their_names() {
        let s = source();
        assert!(s.contains(&format!("fn {}(", SYSCALL_FN)));
        assert!(s.contains(&format!("fn {}(", INIT_FN)));
        assert!(s.contains(&format!("fn {}(", ARGV_FN)));
    }

    /// Nothing here may use the built-in `syscall` — that is the whole
    /// point, and it would be an endless loop besides. The dispatcher is
    /// CALLED `__win_syscall`, so the name alone is not the test: what is
    /// looked for is the built-in, which is `syscall(` with no identifier
    /// character in front of it.
    #[test]
    fn the_seam_makes_no_system_call_itself() {
        let s = source();
        for (i, _) in s.match_indices("syscall(") {
            let before = s[..i].chars().next_back().unwrap_or(' ');
            assert!(
                before == '_' || before.is_alphanumeric(),
                "the seam uses the built-in syscall at offset {}",
                i
            );
        }
    }

    /// The seam runs before the collector and must not need it.
    #[test]
    fn the_seam_uses_no_import_and_no_text_type() {
        let s = source();
        assert!(!s.contains("\nimport "));
        assert!(!s.contains(": str"));
    }
}
