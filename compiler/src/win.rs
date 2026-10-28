// SPDX-License-Identifier: MPL-2.0
//! **Round WINDOWS — PE/COFF, the Win64 calling convention and the imports.**
//!
//! Everything in this file exists because the second axis of a target
//! (`target.rs`, round ARM-FREESTANDING) got a third value: `Os::Windows`.
//! The instruction set is unchanged — every `mov`, every `imul`, every
//! `cmov` that `codegen_x86.rs` and `regalloc.rs` write for Linux is
//! written for Windows as well, character for character. What changes is
//! everything AROUND the instructions:
//!
//!   1. **The object format.** ELF cannot become a PE image. The COFF port
//!      of the same binutils (`x86_64-w64-mingw32-as` / `-ld`) does it, and
//!      it is used exactly the way `as`/`ld` are used on Linux: as an
//!      assembler and as a linker, never as a compiler.
//!   2. **The binding to the operating system.** There is no `syscall`
//!      instruction on Windows that a program may use; the numbers are not
//!      stable between Windows versions on purpose. Everything goes through
//!      `kernel32.dll` and its relatives, and it gets there through the
//!      **import table of the PE file** — which this file writes ITSELF
//!      (§ `idata_asm`) instead of pulling in an import library. No foreign
//!      object file enters the image.
//!   3. **The calling convention at the outer boundary.** Win64 is not
//!      System V (§ `thunk`).
//!   4. **The stack.** Windows grows a thread stack over guard pages; a
//!      frame bigger than one page has to walk down page by page or the
//!      guard is jumped over and the process dies (§ `chkstk_asm`).
//!
//! ## Why a thunk and not a second calling convention in the code generator
//!
//! Win64 puts the first four integer arguments in `rcx, rdx, r8, r9`,
//! demands **32 octets of shadow space** below the return address that the
//! callee may scribble on, and counts `rsi`, `rdi` and `xmm6`–`xmm15` as
//! callee-saved. System V puts six arguments in `rdi, rsi, rdx, rcx, r8,
//! r9`, has no shadow space, and counts `rsi`/`rdi` as scratch.
//!
//! Firn's **internal** calls do not have to be Win64. Nothing in Windows
//! looks at how one Firn function calls another; SPEC §13 already calls the
//! Firn-to-Firn convention a documented ABI of its own. What MUST be Win64
//! is every call that leaves the program — and every call that comes in
//! (a callback), which this round does not have.
//!
//! So the boundary gets ONE well-defined place: for every imported function
//! the compiler emits a **thunk** that takes its arguments the System V way,
//! rearranges them the Win64 way, allocates the shadow space and jumps
//! through the import address table. The register allocator, the frame
//! layout and both code generators stay untouched — which is exactly why
//! the Linux side cannot get worse.
//!
//! The price is honest and is written down in `docs/ROUND-WINDOWS.md`:
//! four to eight extra `mov`s per Win32 call, and a Firn function cannot be
//! handed to Windows as a callback yet (that would need the mirror image
//! thunk).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

/// Prefix of every symbol this file invents. Same idea as `_F0.`
/// (`modules.rs`): reserved, and no source text can produce it.
pub const PREFIX: &str = "_Fwin.";

/// The generic Win64 call gates behind `std.dynlib` (round FFI). A Firn
/// indirect call is System V, so a pointer from `GetProcAddress` cannot be
/// called directly; these two thunks call ANY Win64 function from an
/// argument array, so no new Win32 call needs a compiler patch (GAPS B16).
pub const DYNCALL: &str = "win64_call";
pub const DYNCALLF: &str = "win64_callf";

/// The stack probe. Called with the frame size in `rax`.
pub const CHKSTK: &str = "_Fwin.chkstk";

/// Prefix of a CALLBACK thunk: the mirror image of `PREFIX`. `_Fwin.X`
/// takes System V arguments and hands them to Win32; `_Fwincb.X` takes
/// Win64 arguments and hands them to Firn.
pub const CBPREFIX: &str = "_Fwincb.";

/// The stub that takes the LINUX system call register convention and hands
/// it to the seam. Everything that emits a `syscall` instruction as HAND
/// WRITTEN assembler (`panic_rt.rs`) writes a `call` to this instead.
pub const SYSSTUB: &str = "_Fwin.syscall";

/// From this frame size on a function has to probe the stack. One page.
pub const PAGE: u64 = 4096;

/// The Win32 functions this compiler knows: name, DLL, number of arguments.
///
/// The arity is not decoration — the thunk cannot be written without it
/// (arguments five and up travel over the stack, and where they land
/// depends on how many there are). An `extern fn` naming something that is
/// not in this table is refused with that reason rather than mis-called.
const KNOWN: &[(&str, &str, u32)] = &[
    // --- kernel32: process, memory, files, time -----------------------
    ("ExitProcess", "KERNEL32.dll", 1),
    // --- kernel32: THREADS (round WIN-THREADS) -------------------------
    // `CreateThread` for `Op::ThreadSpawn`; `TlsAlloc`/`TlsSetValue` keep the
    // thread block pointer (the Windows twin of `fs:0`, see
    // `thread::self_sequence_windows`); `WaitOnAddress`/`WakeByAddress*` are
    // the futex of the thread runtime (Windows 8 and later, the same floor
    // as `GetCurrentThreadStackLimits` below).
    ("CreateThread", "KERNEL32.dll", 6),
    ("TlsAlloc", "KERNEL32.dll", 0),
    ("TlsSetValue", "KERNEL32.dll", 2),
    ("TlsGetValue", "KERNEL32.dll", 1),
    ("VirtualQuery", "KERNEL32.dll", 3),
    ("WaitOnAddress", "API-MS-WIN-CORE-SYNCH-L1-2-0.dll", 4),
    ("WakeByAddressSingle", "API-MS-WIN-CORE-SYNCH-L1-2-0.dll", 1),
    ("WakeByAddressAll", "API-MS-WIN-CORE-SYNCH-L1-2-0.dll", 1),
    // --- kernel32: JOB OBJECTS (round WIN-THREADS, std.process.kill_tree)
    ("CreateJobObjectW", "KERNEL32.dll", 2),
    ("AssignProcessToJobObject", "KERNEL32.dll", 2),
    ("SetInformationJobObject", "KERNEL32.dll", 4),
    ("TerminateJobObject", "KERNEL32.dll", 2),
    ("ResumeThread", "KERNEL32.dll", 1),
    ("GetStdHandle", "KERNEL32.dll", 1),
    ("WriteFile", "KERNEL32.dll", 5),
    ("ReadFile", "KERNEL32.dll", 5),
    ("CloseHandle", "KERNEL32.dll", 1),
    ("CreateFileW", "KERNEL32.dll", 7),
    ("DeleteFileW", "KERNEL32.dll", 1),
    ("GetFileAttributesW", "KERNEL32.dll", 1),
    ("SetFilePointerEx", "KERNEL32.dll", 4),
    ("FlushFileBuffers", "KERNEL32.dll", 1),
    // lib/db (the embedded database): shorten a file, ask its size, and lock the byte
    // ranges SQLite's own Windows VFS locks (LockFileEx / UnlockFileEx).
    ("SetEndOfFile", "KERNEL32.dll", 1),
    ("GetFileSizeEx", "KERNEL32.dll", 2),
    ("LockFileEx", "KERNEL32.dll", 6),
    ("UnlockFileEx", "KERNEL32.dll", 5),
    ("VirtualAlloc", "KERNEL32.dll", 4),
    ("VirtualFree", "KERNEL32.dll", 3),
    ("VirtualProtect", "KERNEL32.dll", 4),
    ("GetLastError", "KERNEL32.dll", 0),
    ("QueryPerformanceCounter", "KERNEL32.dll", 1),
    ("QueryPerformanceFrequency", "KERNEL32.dll", 1),
    ("GetSystemTimeAsFileTime", "KERNEL32.dll", 1),
    ("FileTimeToSystemTime", "KERNEL32.dll", 2),
    ("SystemTimeToTzSpecificLocalTime", "KERNEL32.dll", 3),
    ("SystemTimeToFileTime", "KERNEL32.dll", 2),
    ("Sleep", "KERNEL32.dll", 1),
    ("SwitchToThread", "KERNEL32.dll", 0),
    ("GetCommandLineW", "KERNEL32.dll", 0),
    ("GetCurrentDirectoryW", "KERNEL32.dll", 2),
    ("GetCurrentProcessId", "KERNEL32.dll", 0),
    ("GetCurrentThreadId", "KERNEL32.dll", 0),
    // The stack bounds of this thread. Windows 8 and later; the
    // collector needs them, because there is no /proc to read.
    ("GetCurrentThreadStackLimits", "KERNEL32.dll", 2),
    ("GetCurrentProcess", "KERNEL32.dll", 0),
    // The process heap, for callers that need small blocks of their own
    // (the waveOut buffer headers of Certus' sound back end).
    ("GetProcessHeap", "KERNEL32.dll", 0),
    ("HeapAlloc", "KERNEL32.dll", 3),
    ("HeapFree", "KERNEL32.dll", 3),
    ("DuplicateHandle", "KERNEL32.dll", 7),
    ("GetEnvironmentStringsW", "KERNEL32.dll", 0),
    // --- ws2_32: the sockets ------------------------------------------
    ("WSAStartup", "WS2_32.dll", 2),
    ("WSAGetLastError", "WS2_32.dll", 0),
    ("socket", "WS2_32.dll", 3),
    ("connect", "WS2_32.dll", 3),
    ("send", "WS2_32.dll", 4),
    ("recv", "WS2_32.dll", 4),
    ("closesocket", "WS2_32.dll", 1),
    ("shutdown", "WS2_32.dll", 2),
    ("bind", "WS2_32.dll", 3),
    ("listen", "WS2_32.dll", 2),
    ("accept", "WS2_32.dll", 3),
    ("setsockopt", "WS2_32.dll", 5),
    ("getsockopt", "WS2_32.dll", 5),
    ("getsockname", "WS2_32.dll", 3),
    ("ioctlsocket", "WS2_32.dll", 3),
    // --- ws2_32: what round CERTUS-WINDOWS had to add ------------------
    // `sendto`/`recvfrom` WITH an address. Round MERGE-WIN bound only the
    // connected form and wrote down that Certus does not need the other
    // one -- which was measured on the wrong tree: `lib/net/udp.fi` asks
    // every DNS question with `sendto(fd, .., addr, alen)`, and a UDP
    // socket that was never `connect`ed answers WSAENOTCONN to `send`.
    ("sendto", "WS2_32.dll", 6),
    ("recvfrom", "WS2_32.dll", 6),
    ("select", "WS2_32.dll", 5),
    // --- kernel32: directories, renaming, and looking a symbol up -----
    ("CreateDirectoryW", "KERNEL32.dll", 2),
    // --- kernel32: listing a directory, file metadata, removing a
    // directory, symbolic links (std.dir / std.fs over the seam) -------
    ("GetFileAttributesExW", "KERNEL32.dll", 3),
    ("FindFirstFileW", "KERNEL32.dll", 2),
    ("FindNextFileW", "KERNEL32.dll", 2),
    ("FindClose", "KERNEL32.dll", 1),
    ("RemoveDirectoryW", "KERNEL32.dll", 1),
    ("CreateSymbolicLinkW", "KERNEL32.dll", 3),
    ("MoveFileExW", "KERNEL32.dll", 3),
    ("SetFileAttributesW", "KERNEL32.dll", 2),
    ("SetFileTime", "KERNEL32.dll", 4),
    ("GetModuleHandleW", "KERNEL32.dll", 1),
    ("GetProcAddress", "KERNEL32.dll", 2),
    // --- std.dynlib (docs/FFI.md): load ANY dll at run time ---------------
    ("LoadLibraryA", "KERNEL32.dll", 1),
    ("LoadLibraryW", "KERNEL32.dll", 1),
    ("FreeLibrary", "KERNEL32.dll", 1),
    // The two generic Win64 call gates (not imports: DLL "" = internal).
    // `win64_call(fn, argv, n) -> rax`, `win64_callf` the same but the
    // result is xmm0's low 64 bits. See `dyncall_asm`.
    (DYNCALL, "", 3),
    (DYNCALLF, "", 3),
    // --- kernel32: starting a helper process and reading its answer ----
    // Round UPDATE-CHECK (Certus). Certus called these three through
    // GetProcAddress + a Firn function pointer -- and a Firn indirect call
    // is System V, not Win64: the arguments landed in rdi/rsi, Windows read
    // rcx/rdx, GetModuleFileNameW returned 0 and "Auf Aktualisierung
    // pruefen" never started certus-update.exe. Bound here, they get the
    // thunk every import gets.
    ("GetModuleFileNameW", "KERNEL32.dll", 3),
    ("CreateProcessW", "KERNEL32.dll", 10),
    ("GetExitCodeProcess", "KERNEL32.dll", 2),
    // --- kernel32 + shell32: std.process / std.shell / std.env --------
    // A Firn function value is called System V, so `GetProcAddress` + `fn`
    // cannot reach these (the comment above, measured). Bound here they
    // get the thunk like every import. Pipes with a handle that is NOT
    // inherited, waiting with a timeout, killing, peeking into a pipe so a
    // read never blocks, and ShellExecute for "open this".
    ("CreatePipe", "KERNEL32.dll", 4),
    ("SetHandleInformation", "KERNEL32.dll", 3),
    ("WaitForSingleObject", "KERNEL32.dll", 2),
    ("TerminateProcess", "KERNEL32.dll", 2),
    ("PeekNamedPipe", "KERNEL32.dll", 6),
    ("ShellExecuteW", "SHELL32.dll", 6),
    // --- user32: the window ------------------------------------------
    // Round MERGE-WIN 5.2 says "any GUI" is not bound and therefore not
    // reachable. This is where that stops being true. Everything here is
    // Win32 straight out of the DLL; no wrapper library, no widget set.
    ("RegisterClassW", "USER32.dll", 1),
    ("CreateWindowExW", "USER32.dll", 12),
    ("DestroyWindow", "USER32.dll", 1),
    ("ShowWindow", "USER32.dll", 2),
    ("UpdateWindow", "USER32.dll", 1),
    ("DefWindowProcW", "USER32.dll", 4),
    ("GetMessageW", "USER32.dll", 4),
    ("PeekMessageW", "USER32.dll", 5),
    ("TranslateMessage", "USER32.dll", 1),
    ("DispatchMessageW", "USER32.dll", 1),
    ("PostQuitMessage", "USER32.dll", 1),
    ("GetClientRect", "USER32.dll", 2),
    ("InvalidateRect", "USER32.dll", 3),
    ("ValidateRect", "USER32.dll", 2),
    ("GetDC", "USER32.dll", 1),
    ("ReleaseDC", "USER32.dll", 2),
    ("IsWindow", "USER32.dll", 1),
    ("SetWindowTextW", "USER32.dll", 2),
    ("LoadCursorW", "USER32.dll", 2),
    ("SetProcessDpiAwarenessContext", "USER32.dll", 1),
    ("SetProcessDPIAware", "USER32.dll", 0),
    ("GetDpiForWindow", "USER32.dll", 1),
    ("MsgWaitForMultipleObjectsEx", "USER32.dll", 5),
    ("AdjustWindowRectEx", "USER32.dll", 4),
    ("GetSystemMetrics", "USER32.dll", 1),
    ("GetKeyState", "USER32.dll", 1),
    // Touch vs mouse (Certus 0.68): the extra info of the last mouse message
    // says whether a finger made it (signature 0xFF515700).
    ("GetMessageExtraInfo", "USER32.dll", 0),
    ("SetWindowLongPtrW", "USER32.dll", 3),
    ("GetWindowLongPtrW", "USER32.dll", 2),
    ("SetCapture", "USER32.dll", 1),
    // Round CERTUS-WIN2: send WM_SETTINGCHANGE to all windows --
    // so that the switch light/dark can be PROVEN in running operation
    // and not only claimed (tools/windows/farbruf.fi).
    ("SendMessageTimeoutW", "USER32.dll", 7),
    // ... and the two with which a window can be enlarged FROM ANOTHER
    // PROCESS -- the way in which the acceptance of this round produces a real WM_SIZE
    // (tools/windows/groesse.fi).
    ("FindWindowW", "USER32.dll", 2),
    ("SetWindowPos", "USER32.dll", 7),
    // Round CERTUS-WIN4: the START SIZE. SPI_GETWORKAREA gives the
    // screen WITHOUT the taskbar; GetWindowRect needs it to place the window
    // in the centre. Without the two lines the linker does not find the
    // symbols -- this list IS the import table.
    ("SystemParametersInfoW", "USER32.dll", 4),
    ("GetWindowRect", "USER32.dll", 2),
    // W45-d: the monitor a window is on (MONITOR_DEFAULTTONEAREST) and its rcMonitor / rcWork, so the safe-area insets use the
    // taskbar of THAT monitor and not SPI_GETWORKAREA of the primary one (lib/window/win32.fi r_monitor_work).
    ("MonitorFromWindow", "USER32.dll", 2),
    ("GetMonitorInfoW", "USER32.dll", 2),
    ("ReleaseCapture", "USER32.dll", 0),
    // --- gdi32: the DIB section and the one blit ----------------------
    ("CreateDIBSection", "GDI32.dll", 6),
    ("CreateCompatibleDC", "GDI32.dll", 1),
    ("SelectObject", "GDI32.dll", 2),
    ("BitBlt", "GDI32.dll", 9),
    ("DeleteObject", "GDI32.dll", 1),
    ("DeleteDC", "GDI32.dll", 1),
    ("SetTextColor", "GDI32.dll", 2),
    ("SetBkColor", "GDI32.dll", 2),
    ("SetBkMode", "GDI32.dll", 2),
    ("TextOutA", "GDI32.dll", 5),
    ("GetStockObject", "GDI32.dll", 1),
    ("GdiFlush", "GDI32.dll", 0),
    // --- iphlpapi: WHOM DO YOU ASK FOR NAMES? (round CERTUS-WIN2) -----
    // Windows has no `/etc/resolv.conf`. The name servers are listed per
    // network card in `IP_ADAPTER_ADDRESSES.FirstDnsServerAddress`;
    // `GetNetworkParams` is the older, simpler way for IPv4.
    ("GetAdaptersAddresses", "IPHLPAPI.DLL", 5),
    ("GetNetworkParams", "IPHLPAPI.DLL", 2),
    // --- kernel32: the environment ------------------------------------
    // `/proc/self/environ` does not exist here; the environment comes as
    // a block of UTF-16 from kernel32.
    ("FreeEnvironmentStringsW", "KERNEL32.dll", 1),
    // --- user32/gdi32: what resizing and the bar need
    ("GetSysColor", "USER32.dll", 1),
    // --- user32: synthetic input (lib/input/sendinput.fi) -------------
    ("SendInput", "USER32.dll", 3),
    ("GetCursorPos", "USER32.dll", 1),
    // --- advapi32: the random source ----------------------------------
    // `SystemFunction036` IS `RtlGenRandom`; that is the name it is
    // exported under, and Microsoft's own header only gives it the other
    // one through a macro.
    ("SystemFunction036", "ADVAPI32.dll", 2),
    // --- crypt32: THE WINDOWS TRUST STORE (round DNS) ------------------
    // `lib/tls/trust.windows.fi` hands the machine's root certificates to
    // the TLS client: `CertOpenSystemStoreW(0, L"ROOT")`, then
    // `CertEnumCertificatesInStore` until it returns NULL; every
    // `CERT_CONTEXT` carries the DER encoding of one certificate.
    ("CertOpenSystemStoreW", "CRYPT32.dll", 2),
    ("CertEnumCertificatesInStore", "CRYPT32.dll", 2),
    ("CertCloseStore", "CRYPT32.dll", 2),
    // --- advapi32: the system setting light/dark -----------------------
    // HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize,
    // value `AppsUseLightTheme`. `RegGetValueW` does opening, reading and
    // closing in one call.
    ("RegGetValueW", "ADVAPI32.dll", 7),
    // --- advapi32: WRITING the registry (lib/pack/winreg.fi) -----------
    // Round PACK: an installer writes HKCU\Software\Microsoft\Windows\
    // CurrentVersion\Uninstall\<id> (the "Apps & features" entry) and its
    // uninstaller removes it again. Bound here because an indirect call
    // through GetProcAddress is System V, not Win64 (see GetModuleFileNameW).
    // RegCreateKeyExW(hkey, sub, 0, class, options, sam, sa, *result, *disp),
    // RegSetValueExW(hkey, name, 0, type, data, bytes), RegCloseKey,
    // RegDeleteTreeW(hkey, sub) (Vista and later; Wine has it),
    // RegOpenKeyExW(hkey, sub, options, sam, *result).
    ("RegCreateKeyExW", "ADVAPI32.dll", 9),
    ("RegSetValueExW", "ADVAPI32.dll", 6),
    ("RegCloseKey", "ADVAPI32.dll", 1),
    ("RegDeleteTreeW", "ADVAPI32.dll", 2),
    ("RegOpenKeyExW", "ADVAPI32.dll", 5),
    // The shell is told that shortcuts / associations changed so that the
    // new icons show without a restart of Explorer.
    ("SHChangeNotify", "SHELL32.dll", 4),
    // --- advapi32: the Credential Manager (std.secret_os) --------------
    // The keyring of Windows: generic credentials, kept per user by the
    // system and protected with the user's logon secret (DPAPI). Round
    // STD-SECRET. They are bound HERE because an indirect call through
    // GetProcAddress is System V, not Win64 (see `GetModuleFileNameW`
    // below): the arguments would land in rdi/rsi and the call would fail.
    // CredWriteW(credential*, flags), CredReadW(target, type, flags,
    // credential**), CredDeleteW(target, type, flags), CredFree(buffer).
    ("CredWriteW", "ADVAPI32.dll", 2),
    ("CredReadW", "ADVAPI32.dll", 4),
    ("CredDeleteW", "ADVAPI32.dll", 3),
    ("CredFree", "ADVAPI32.dll", 1),
    // --- comdlg32 + shell32 + ole32 + user32: THE SYSTEM DIALOGS (std.dialog, round DIALOG) ----
    // The file, colour and font dialogs every Windows program shows, the shell's folder picker and the
    // message box. Bound here because an indirect call through GetProcAddress is System V, not Win64
    // (see `GetModuleFileNameW`). Each takes one pointer to a structure (OPENFILENAMEW, CHOOSECOLORW,
    // CHOOSEFONTW, BROWSEINFOW); CommDlgExtendedError tells a Cancel (0) from a failure.
    ("GetOpenFileNameW", "COMDLG32.dll", 1),
    ("GetSaveFileNameW", "COMDLG32.dll", 1),
    ("ChooseColorW", "COMDLG32.dll", 1),
    ("ChooseFontW", "COMDLG32.dll", 1),
    ("CommDlgExtendedError", "COMDLG32.dll", 0),
    ("SHBrowseForFolderW", "SHELL32.dll", 1),
    ("SHGetPathFromIDListW", "SHELL32.dll", 2),
    ("CoInitializeEx", "OLE32.dll", 2),
    ("CoTaskMemFree", "OLE32.dll", 1),
    ("MessageBoxW", "USER32.dll", 4),
    // ... and what a test needs to press the buttons of a dialog of ANOTHER process: find the dialog
    // window (class #32770), look a control up, send it a message (tools/dialog, tests/2381).
    ("GetDlgItem", "USER32.dll", 2),
    ("SendMessageW", "USER32.dll", 4),
    ("FindWindowExW", "USER32.dll", 4),
    ("GetWindowThreadProcessId", "USER32.dll", 2),
    ("IsWindowVisible", "USER32.dll", 1),
    ("GetWindowTextW", "USER32.dll", 3),
    // --- dwmapi: THE TITLE BAR (round CERTUS C-039) -------------------
    // Justin, 18.09.2026: "the window bar at the top does not fit the
    // style either". Certus painted its shell dark, while Windows
    // drew the light title bar above it.
    //
    // `DwmSetWindowAttribute` colours the bar that the SYSTEM
    // draws -- attribute 20 (DWMWA_USE_IMMERSIVE_DARK_MODE, from
    // Windows 10 1809; 19 on the builds before) and attribute 35
    // (DWMWA_CAPTION_COLOR, from Windows 11) for the exact colour.
    // The system buttons, dragging and Aero Snap remain
    // Windows' business. Four arguments: hwnd, attribute, pointer, length.
    ("DwmSetWindowAttribute", "dwmapi.dll", 4),
    // --- the DESKTOP round (docs/DESKTOP.md): tray icon and menu, balloon
    // notifications, the clipboard, files dropped on a window, waveOut audio,
    // directory change notification, the Run key, a named pipe between two
    // copies of a program. Every one of them is bound HERE because an indirect
    // call through GetProcAddress is System V, not Win64 (see GetModuleFileNameW).
    ("Shell_NotifyIconW", "SHELL32.dll", 2),
    ("DragAcceptFiles", "SHELL32.dll", 2),
    ("DragQueryFileW", "SHELL32.dll", 4),
    ("DragFinish", "SHELL32.dll", 1),
    ("CreatePopupMenu", "USER32.dll", 0),
    ("AppendMenuW", "USER32.dll", 4),
    ("TrackPopupMenu", "USER32.dll", 7),
    ("DestroyMenu", "USER32.dll", 1),
    ("SetForegroundWindow", "USER32.dll", 1),
    ("PostMessageW", "USER32.dll", 4),
    ("RegisterWindowMessageW", "USER32.dll", 1),
    ("CreateIconIndirect", "USER32.dll", 1),
    ("DestroyIcon", "USER32.dll", 1),
    ("LoadIconW", "USER32.dll", 2),
    ("OpenClipboard", "USER32.dll", 1),
    ("CloseClipboard", "USER32.dll", 0),
    ("EmptyClipboard", "USER32.dll", 0),
    ("SetClipboardData", "USER32.dll", 2),
    ("GetClipboardData", "USER32.dll", 1),
    ("IsClipboardFormatAvailable", "USER32.dll", 1),
    ("EnumClipboardFormats", "USER32.dll", 1),
    ("RegisterClipboardFormatW", "USER32.dll", 1),
    ("GetClipboardFormatNameW", "USER32.dll", 3),
    ("CreateBitmap", "GDI32.dll", 5),
    ("GlobalAlloc", "KERNEL32.dll", 2),
    ("GlobalLock", "KERNEL32.dll", 1),
    ("GlobalUnlock", "KERNEL32.dll", 1),
    ("GlobalSize", "KERNEL32.dll", 1),
    ("GlobalFree", "KERNEL32.dll", 1),
    ("CreateEventW", "KERNEL32.dll", 4),
    ("ResetEvent", "KERNEL32.dll", 1),
    ("SetEvent", "KERNEL32.dll", 1),
    ("GetOverlappedResult", "KERNEL32.dll", 4),
    ("CancelIo", "KERNEL32.dll", 1),
    ("ReadDirectoryChangesW", "KERNEL32.dll", 8),
    ("CreateNamedPipeW", "KERNEL32.dll", 8),
    ("ConnectNamedPipe", "KERNEL32.dll", 2),
    ("DisconnectNamedPipe", "KERNEL32.dll", 1),
    ("WaitNamedPipeW", "KERNEL32.dll", 2),
    ("SetNamedPipeHandleState", "KERNEL32.dll", 4),
    ("waveOutOpen", "WINMM.dll", 6),
    ("waveOutClose", "WINMM.dll", 1),
    ("waveOutPrepareHeader", "WINMM.dll", 3),
    ("waveOutUnprepareHeader", "WINMM.dll", 3),
    ("waveOutWrite", "WINMM.dll", 3),
    ("waveOutReset", "WINMM.dll", 1),
    ("waveOutPause", "WINMM.dll", 1),
    ("waveOutRestart", "WINMM.dll", 1),
    ("waveOutGetNumDevs", "WINMM.dll", 0),
    ("waveOutGetPosition", "WINMM.dll", 3),
    ("waveOutSetVolume", "WINMM.dll", 2),
    ("RegQueryValueExW", "ADVAPI32.dll", 6),
    ("RegDeleteValueW", "ADVAPI32.dll", 2),
];

/// DLL and arity of a known Win32 function.
pub fn known(name: &str) -> Option<(&'static str, u32)> {
    KNOWN.iter().find(|(n, _, _)| *n == name).map(|(_, d, a)| (*d, *a))
}

thread_local! {
    /// Round CERTUS-WIN2: which PE subsystem the image declares.
    /// `console` (the default) gives the program a console -- which is
    /// right for every test program in this tree, because that is where
    /// their output is compared. A WINDOW program wants `windows`:
    /// otherwise double clicking a browser opens a DOS box next to it.
    static SUBSYSTEM: RefCell<String> = RefCell::new(String::from("console"));
    /// Which imports this compilation unit really needs.
    static USED: RefCell<BTreeSet<String>> = RefCell::new(BTreeSet::new());
    /// Functions carrying `#[win_callback]`: internal name -> arity.
    static CBMARK: RefCell<BTreeMap<String, u32>> = RefCell::new(BTreeMap::new());
    /// Callback thunks that really have to be emitted: symbol -> arity.
    static CBUSED: RefCell<BTreeMap<String, u32>> = RefCell::new(BTreeMap::new());
    /// Did any function need a stack probe?
    static PROBED: RefCell<bool> = const { RefCell::new(false) };
    /// Did the hand written runtime need the system call stub?
    static SYSSTUB_USED: RefCell<bool> = const { RefCell::new(false) };
    /// Did a thread start (`Op::ThreadSpawn`) need the thread entry thunk?
    static THREADMAIN_USED: RefCell<bool> = const { RefCell::new(false) };
}

/// The PE subsystem this image declares (`console` or `windows`).
pub fn subsystem() -> String {
    SUBSYSTEM.with(|s| s.borrow().clone())
}

/// `--win-subsystem=windows` on the command line. Refused for anything
/// else, so that a typo does not silently produce a console program.
pub fn set_subsystem(v: &str) -> Result<(), String> {
    if v != "console" && v != "windows" {
        return Err(format!(
            "unknown --win-subsystem '{}' (console, windows)",
            v
        ));
    }
    SUBSYSTEM.with(|s| *s.borrow_mut() = v.to_string());
    Ok(())
}

/// Only for the module tests, which compile several programs in one process.
pub fn reset() {
    USED.with(|u| u.borrow_mut().clear());
    CBMARK.with(|u| u.borrow_mut().clear());
    CBUSED.with(|u| u.borrow_mut().clear());
    PROBED.with(|p| *p.borrow_mut() = false);
    SYSSTUB_USED.with(|p| *p.borrow_mut() = false);
    THREADMAIN_USED.with(|p| *p.borrow_mut() = false);
}

/// Registers `name` as an import and yields the symbol a System V caller
/// jumps to. `None` = not a Win32 function this compiler knows.
pub fn note(name: &str) -> Option<String> {
    known(name)?;
    USED.with(|u| u.borrow_mut().insert(name.to_string()));
    Some(format!("{}{}", PREFIX, name))
}

/// Registers `name` (the internal FIR name) as `#[win_callback]` with its
/// arity. Called from `extfn::register`, next to its two neighbours.
pub fn mark_callback(name: &str, argc: u32) {
    CBMARK.with(|c| {
        c.borrow_mut().insert(name.to_string(), argc);
    });
}

/// Is this function marked `#[win_callback]`? If so, its arity.
pub fn is_callback(name: &str) -> Option<u32> {
    CBMARK.with(|c| c.borrow().get(name).copied())
}

/// The callback thunk symbol for a function symbol.
pub fn cb_thunk(sym: &str) -> String {
    format!("{}{}", CBPREFIX, sym)
}

/// Records that the callback thunk for `sym` has to be emitted, and yields
/// its symbol. `fnval::records_asm` calls this: the record of a
/// `#[win_callback]` function holds the address of the THUNK, because that
/// address is the only one Windows may ever jump to.
pub fn note_callback(sym: &str, argc: u32) -> String {
    CBUSED.with(|c| {
        c.borrow_mut().insert(sym.to_string(), argc);
    });
    cb_thunk(sym)
}

/// The thunk symbol of an import (without registering it).
pub fn thunk(name: &str) -> String {
    format!("{}{}", PREFIX, name)
}

/// The import address table slot of an import.
fn imp(name: &str) -> String {
    format!("__imp_{}", name)
}

/// Records that a stack probe was emitted.
pub fn note_probe() {
    PROBED.with(|p| *p.borrow_mut() = true);
}

/// The symbol of the thread entry thunk (Win64 -> Firn) that `CreateThread`
/// jumps to; see `thread_main_asm`.
pub const THREAD_MAIN: &str = "_Fwin.thread_main";

/// Records that `Op::ThreadSpawn` needs the thread entry thunk (and the
/// import it ends with).
pub fn note_thread_main() {
    THREADMAIN_USED.with(|p| *p.borrow_mut() = true);
    let _ = note("WakeByAddressAll");
}

/// Is the thread entry thunk needed?
pub fn thread_main_used() -> bool {
    THREADMAIN_USED.with(|p| *p.borrow())
}

/// Records that the hand written runtime needs the system call stub.
pub fn note_sysstub() {
    SYSSTUB_USED.with(|p| *p.borrow_mut() = true);
}

/// Is the system call stub needed?
pub fn sysstub_used() -> bool {
    SYSSTUB_USED.with(|p| *p.borrow())
}

/// **The stub for hand written runtime assembler.**
///
/// `panic_rt.rs` writes its two `write(2, …)` calls and its
/// `exit_group(101)` as instructions, not as Firn — it has to, because it
/// runs with a stack frame it built itself and with the panic arguments in
/// registers of its own choosing. On Windows those `syscall` instructions
/// would be a crash (there is no Linux kernel below), so the same lines
/// call THIS instead, and it forwards to the seam.
///
/// In: `rax` = the canonical (x86-64 Linux) call number, `rdi, rsi, rdx,
/// r10, r8, r9` = the arguments — exactly the register set the instruction
/// itself reads. Out: `rax`, exactly as the instruction leaves it.
fn sysstub_asm() -> String {
    let mut s = String::new();
    s.push_str(&format!("\n.globl {}\n{}:\n", SYSSTUB, SYSSTUB));
    s.push_str("    # the Linux syscall register set -> the seam\n");
    s.push_str("    push rbp\n    mov rbp, rsp\n");
    // THE ALIGNMENT, and it is not decoration. This stub is jumped to out
    // of hand written runtime code whose `rsp` is whatever its own pushes
    // left behind; a Win32 function entered one word off dies inside the
    // first aligned SSE move of some system DLL, far away from the cause.
    // `leave` puts the stack back exactly, so forcing it here is free.
    s.push_str("    and rsp, -16\n");
    s.push_str("    sub rsp, 16\n");
    s.push_str("    mov qword ptr [rsp], r9\n"); // a6
    s.push_str("    mov r9, r8\n");              // a5
    s.push_str("    mov r8, r10\n");             // a4
    s.push_str("    mov rcx, rdx\n");            // a3
    s.push_str("    mov rdx, rsi\n");            // a2
    s.push_str("    mov rsi, rdi\n");            // a1
    s.push_str("    mov rdi, rax\n");            // the number
    s.push_str(&format!(
        "    call {}\n",
        crate::codegen_x86::label(crate::win_seam::SYSCALL_FN)
    ));
    s.push_str("    leave\n    ret\n");
    s
}

/// Was a stack probe emitted anywhere?
pub fn probed() -> bool {
    PROBED.with(|p| *p.borrow())
}

fn align_up(x: u64, a: u64) -> u64 {
    (x + a - 1) / a * a
}

/// **The generic Win64 call gate.**
///
/// System V in: `rdi` = target, `rsi` = argv (at least 4 readable slots),
/// `rdx` = argument count (clamped to 16). Win64 out: slots 0-3 go into
/// `rcx, rdx, r8, r9` AND `xmm0-xmm3` (Win64 numbers integer and float
/// arguments by POSITION, so the callee reads whichever register its
/// prototype names; a `float` is the low 32 bits of its slot), slots 4..
/// onto the stack above 32 octets of shadow space. `float_result` returns
/// `xmm0` (as raw bits) instead of `rax`.
fn dyncall_asm(float_result: bool) -> String {
    let name = if float_result { DYNCALLF } else { DYNCALL };
    let mut s = String::new();
    s.push_str(&format!("\n.globl {}\n{}:\n", thunk(name), thunk(name)));
    s.push_str("    push rbp\n    mov rbp, rsp\n");
    s.push_str("    sub rsp, 128\n"); // 32 shadow + 12 stack args = 16 slots, aligned
    s.push_str("    mov r10, rdi\n    mov r11, rsi\n    mov rax, rdx\n");
    s.push_str("    cmp rax, 16\n    jbe 1f\n    mov rax, 16\n1:\n");
    s.push_str("    mov rcx, 4\n");
    s.push_str("2:\n    cmp rcx, rax\n    jnb 3f\n");
    s.push_str("    mov rdx, qword ptr [r11+rcx*8]\n");
    s.push_str("    mov qword ptr [rsp+rcx*8], rdx\n"); // slot i at rsp+32+8*(i-4)
    s.push_str("    inc rcx\n    jmp 2b\n3:\n");
    s.push_str("    mov rcx, qword ptr [r11]\n    mov rdx, qword ptr [r11+8]\n");
    s.push_str("    mov r8, qword ptr [r11+16]\n    mov r9, qword ptr [r11+24]\n");
    s.push_str("    movq xmm0, rcx\n    movq xmm1, rdx\n    movq xmm2, r8\n    movq xmm3, r9\n");
    s.push_str("    call r10\n");
    if float_result {
        s.push_str("    movq rax, xmm0\n");
    }
    s.push_str("    leave\n    ret\n");
    s
}

/// **One System V → Win64 thunk.**
///
/// On entry the System V arguments sit in `rdi, rsi, rdx, rcx, r8, r9` and,
/// from the seventh on, at `[rbp+16]`, `[rbp+24]`, … On exit to the Win32
/// function they have to sit in `rcx, rdx, r8, r9` and, from the fifth on,
/// at `[rsp+32]`, `[rsp+40]`, … — above 32 octets of shadow space that
/// belongs to the callee.
///
/// The ORDER of the moves is the whole trick: `r8` and `r9` are Win64
/// argument registers 3 and 4 AND at the same time System V argument
/// registers 5 and 6. Writing `mov r8, rdx` before argument five has left
/// `r8` loses it. So the stack arguments go down first, `r8`/`r9` among
/// them, and only then the four register arguments in descending order.
fn thunk_asm(name: &str, argc: u32) -> String {
    let mut s = String::new();
    let n = argc as u64;
    // 32 octets of shadow + one word for every argument from the fifth on.
    let space = align_up(32 + 8 * n.saturating_sub(4), 16);
    s.push_str(&format!("\n.globl {}\n{}:\n", thunk(name), thunk(name)));
    s.push_str(&format!(
        "    # System V -> Win64 for {}({} arguments)\n",
        name, argc
    ));
    s.push_str("    push rbp\n    mov rbp, rsp\n");
    s.push_str(&format!("    sub rsp, {}\n", space));
    // Arguments seven and up: out of the CALLER's frame into the Win64 area.
    let mut i = 6u64;
    while i < n {
        s.push_str(&format!(
            "    mov rax, qword ptr [rbp+{}]\n    mov qword ptr [rsp+{}], rax\n",
            16 + 8 * (i - 6),
            8 * i
        ));
        i += 1;
    }
    // Arguments five and six: FIRST, because r8/r9 are about to be overwritten.
    if n >= 5 {
        s.push_str("    mov qword ptr [rsp+32], r8\n");
    }
    if n >= 6 {
        s.push_str("    mov qword ptr [rsp+40], r9\n");
    }
    // The four register arguments, in the one order that destroys nothing.
    if n >= 4 {
        s.push_str("    mov r9, rcx\n");
    }
    if n >= 3 {
        s.push_str("    mov r8, rdx\n");
    }
    if n >= 2 {
        s.push_str("    mov rdx, rsi\n");
    }
    if n >= 1 {
        s.push_str("    mov rcx, rdi\n");
    }
    s.push_str(&format!("    call [rip + {}]\n", imp(name)));
    // `leave` is `mov rsp, rbp` + `pop rbp` — the shadow space disappears
    // with the frame, whatever the callee did to it.
    s.push_str("    leave\n    ret\n");
    s
}

/// **One Win64 -> System V thunk: the mirror image of `thunk_asm`.**
///
/// Round MERGE-WIN 5.3 named this as the ONE thing missing for a window:
/// "A Firn function cannot be handed to Windows as a function pointer. The
/// thunk exists in one direction only." This is the other direction.
///
/// A window procedure is a callback. Windows enters it with `rcx, rdx, r8,
/// r9` and 32 octets of shadow space, and it expects `rbx`, `rbp`, `rsi`,
/// `rdi`, `r12`-`r15` and **`xmm6`-`xmm15`** to come back unchanged. Firn
/// compiles to System V, where `rsi`/`rdi` are argument registers and every
/// `xmm` is scratch -- so a Firn function entered directly by `user32`
/// would quietly destroy ten SSE registers of its caller.
///
/// So: save what Win64 calls callee-saved and System V does not, move the
/// arguments across, call, restore.
///
/// The alignment, and it is the whole reason this is written out rather
/// than hand waved: Windows enters with `rsp = 8 (mod 16)`. `push rbp`
/// makes it 0, the two further pushes make it 0 again, and the 160 octets
/// of `xmm` room keep it 0 -- which is exactly what System V wants at a
/// `call`.
fn cb_thunk_asm(sym: &str, argc: u32) -> String {
    win64_thunk_asm(&cb_thunk(sym), sym, &vec![b'i'; argc as usize])
}

/// A parameter of an exported function as the Win64 entry thunk sees it (`extfn::export_thunk_info`).
#[derive(Clone, Debug, PartialEq)]
pub enum WinParam {
    /// integer / pointer / bool, and a struct over 16 octets (a pointer to a copy on both sides)
    Int,
    F32,
    F64,
    /// Win64 passes a pointer to 16 octets in an integer slot; System V wants the vector in an `xmm`
    V128,
    /// a struct of at most 16 octets. Win64: by value in the slot (1, 2, 4, 8 octets, ALWAYS an integer slot, also for
    /// floats) else a pointer to a copy. Firn: eightbyte words, `sse[k]` = word `k` travels in an `xmm`.
    Agg { size: u32, sse: Vec<bool> },
}

/// The result of an exported function as the Win64 entry thunk sees it.
#[derive(Clone, Debug, PartialEq)]
pub enum WinRet {
    /// scalar or nothing: `rax` / `xmm0` are the same in both conventions
    Plain,
    /// a struct of at most 8 octets: Firn answers in `rax` (`sse`: `xmm0`); Win64 in `rax` for 1, 2, 4, 8 octets, else
    /// through a hidden pointer in the first slot
    Word { size: u32, sse: bool },
    /// a struct over 8 octets: hidden pointer in the first integer slot / register, handed back in `rax` (both sides)
    Sret,
}

/// Win64 passes a struct of exactly these sizes in the slot itself; every other size as a pointer to a copy.
fn win_by_value(size: u32) -> bool {
    matches!(size, 1 | 2 | 4 | 8)
}

/// Where one System V argument comes from. Every source is MEMORY (the parked slot, or the copy a pointer slot names).
enum ArgSrc {
    Slot(usize),
    V128(usize),
    F32(usize),
    F64(usize),
    /// word `off/8` of an aggregate: `ptr` = slot holding the pointer to the copy (`None`: the value is the slot itself),
    /// `rem` = octets of this word (1..=8)
    Word { ptr: Option<usize>, slot: usize, off: u32, rem: u32 },
}

/// `rax` = the `rem` octets at `[base+off]`, zero extended (`r11` is scratch).
fn load_bits_rax(base: &str, off: usize, rem: u32) -> String {
    let m = |o: usize| format!("[{}+{}]", base, off + o);
    match rem {
        1 => format!("    movzx eax, byte ptr {}\n", m(0)),
        2 => format!("    movzx eax, word ptr {}\n", m(0)),
        3 => format!(
            "    movzx eax, word ptr {}\n    movzx r11d, byte ptr {}\n    shl r11, 16\n    or rax, r11\n",
            m(0),
            m(2)
        ),
        4 => format!("    mov eax, dword ptr {}\n", m(0)),
        5 => format!(
            "    mov eax, dword ptr {}\n    movzx r11d, byte ptr {}\n    shl r11, 32\n    or rax, r11\n",
            m(0),
            m(4)
        ),
        6 => format!(
            "    mov eax, dword ptr {}\n    movzx r11d, word ptr {}\n    shl r11, 32\n    or rax, r11\n",
            m(0),
            m(4)
        ),
        7 => format!(
            "    mov eax, dword ptr {}\n    movzx r11d, word ptr {}\n    shl r11, 32\n    or rax, r11\n    movzx r11d, byte ptr {}\n    shl r11, 48\n    or rax, r11\n",
            m(0),
            m(4),
            m(6)
        ),
        _ => format!("    mov rax, qword ptr {}\n", m(0)),
    }
}

/// Win64 entry `t` -> System V function `sym` (see `cb_thunk_asm`); scalar parameters only.
fn win64_thunk_asm(t: &str, sym: &str, kinds: &[u8]) -> String {
    let params: Vec<WinParam> = kinds
        .iter()
        .map(|k| match k {
            b'f' => WinParam::F32,
            b'd' => WinParam::F64,
            b'v' => WinParam::V128,
            _ => WinParam::Int,
        })
        .collect();
    win64_thunk_asm_ex(t, sym, &params, &WinRet::Plain)
}

/// Win64 entry `t` -> System V function `sym`: scalars, `v128` and structs by value, struct results.
fn win64_thunk_asm_ex(t: &str, sym: &str, params: &[WinParam], ret: &WinRet) -> String {
    let argc = params.len();
    // A result that comes back through a hidden pointer takes Win64 slot 0 (rcx).
    let hidden = match ret {
        WinRet::Sret => true,
        WinRet::Word { size, .. } => !win_by_value(*size),
        WinRet::Plain => false,
    };
    let shift = hidden as usize;
    let mut s = String::new();
    s.push_str(&format!("\n.globl {}\n{}:\n", t, t));
    s.push_str(&format!(
        "    # Win64 -> System V for {}({} arguments)\n",
        sym, argc
    ));
    s.push_str("    push rbp\n    mov rbp, rsp\n");
    s.push_str("    push rsi\n    push rdi\n");
    s.push_str("    sub rsp, 160\n");
    for k in 0..10u32 {
        s.push_str(&format!("    movaps [rsp+{}], xmm{}\n", k * 16, k + 6));
    }
    // Win64 numbers argument slots by POSITION: slot i is rcx/rdx/r8/r9 (integer class) or
    // xmm0-3 (float class) for i < 4, else the caller's stack at [rbp+16+8*i] (return address
    // at [rbp+8], the 32-octet shadow space above it). Park the four register slots in their
    // own shadow space first: after that every argument sits in memory at [rbp+16+8*i] and
    // the System V placement below can load them in any order without clobbering a source.
    for i in 0..(argc + shift).min(4) {
        let home = 16 + 8 * i;
        let win_int = ["rcx", "rdx", "r8", "r9"];
        let k = if i < shift { &WinParam::Int } else { &params[i - shift] };
        match k {
            WinParam::F32 => s.push_str(&format!("    movss dword ptr [rbp+{}], xmm{}\n", home, i)),
            WinParam::F64 => s.push_str(&format!("    movsd qword ptr [rbp+{}], xmm{}\n", home, i)),
            _ => s.push_str(&format!("    mov qword ptr [rbp+{}], {}\n", home, win_int[i])),
        }
    }
    // The System V arguments in order of writing, each with its class (`true` = SSE).
    let mut items: Vec<(bool, ArgSrc)> = Vec::new();
    if matches!(ret, WinRet::Sret) {
        items.push((false, ArgSrc::Slot(16))); // hidden result pointer = Firn's first integer argument
    }
    for (j, k) in params.iter().enumerate() {
        let src = 16 + 8 * (j + shift);
        match k {
            WinParam::F32 => items.push((true, ArgSrc::F32(src))),
            WinParam::F64 => items.push((true, ArgSrc::F64(src))),
            // `v128`: Win64 passed a pointer to 16 octets (an integer slot, parked above); System V
            // wants the vector itself in the next xmm (extfn.rs rejects the overflow case).
            WinParam::V128 => items.push((true, ArgSrc::V128(src))),
            WinParam::Int => items.push((false, ArgSrc::Slot(src))),
            WinParam::Agg { size, sse } => {
                for (w, is_sse) in sse.iter().enumerate() {
                    let off = 8 * w as u32;
                    let rem = (*size - off).min(8);
                    let ptr = if win_by_value(*size) { None } else { Some(src) };
                    items.push((*is_sse, ArgSrc::Word { ptr, slot: src, off, rem }));
                }
            }
        }
    }
    // System V placement (what Firn code expects, `codegen_x86::place_args`): integers in
    // rdi rsi rdx rcx r8 r9, floats in xmm0-7, whatever finds no register of its class on the
    // stack in order of writing.
    let sysv_int = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];
    let (mut ni, mut nx) = (0usize, 0usize);
    let mut stack_args: Vec<&ArgSrc> = Vec::new();
    let mut loads: Vec<String> = Vec::new();
    // aggregate word -> rax (zero extended); `rax`, `r10`, `r11` are free in both conventions
    let word_rax = |src: &ArgSrc| -> String {
        match src {
            ArgSrc::Word { ptr: Some(p), off, rem, .. } => format!(
                "    mov r10, qword ptr [rbp+{}]\n{}",
                p,
                load_bits_rax("r10", *off as usize, *rem)
            ),
            ArgSrc::Word { ptr: None, slot, rem, .. } => load_bits_rax("rbp", *slot, *rem),
            ArgSrc::Slot(a) => format!("    mov rax, qword ptr [rbp+{}]\n", a),
            _ => String::new(),
        }
    };
    for (sse, src) in &items {
        if *sse && nx < 8 {
            match src {
                ArgSrc::F32(a) => loads.push(format!("    movss xmm{}, dword ptr [rbp+{}]\n", nx, a)),
                ArgSrc::F64(a) => loads.push(format!("    movsd xmm{}, qword ptr [rbp+{}]\n", nx, a)),
                ArgSrc::V128(a) => loads.push(format!(
                    "    mov rax, qword ptr [rbp+{}]\n    movups xmm{}, xmmword ptr [rax]\n",
                    a, nx
                )),
                w => loads.push(format!("{}    movq xmm{}, rax\n", word_rax(w), nx)),
            }
            nx += 1;
        } else if !*sse && ni < 6 {
            match src {
                ArgSrc::Slot(a) => loads.push(format!("    mov {}, qword ptr [rbp+{}]\n", sysv_int[ni], a)),
                w => loads.push(format!("{}    mov {}, rax\n", word_rax(w), sysv_int[ni])),
            }
            ni += 1;
        } else {
            stack_args.push(src);
        }
    }
    // The stack block keeps the 16-octet alignment the call needs. A `v128` takes 16 octets at a 16 octet
    // boundary (r570, same rule as `codegen_x86::stack_offsets`); Win64 hands it over as a pointer.
    let mut stack_offs: Vec<usize> = Vec::new();
    let mut stack_end = 0usize;
    for src in &stack_args {
        if matches!(src, ArgSrc::V128(_)) {
            stack_end = (stack_end + 15) / 16 * 16;
            stack_offs.push(stack_end);
            stack_end += 16;
        } else {
            stack_offs.push(stack_end);
            stack_end += 8;
        }
    }
    let stack_room = (stack_end + 15) / 16 * 16;
    if stack_room > 0 {
        s.push_str(&format!("    sub rsp, {}\n", stack_room));
        for (j, src) in stack_args.iter().enumerate() {
            let at = stack_offs[j];
            match src {
                ArgSrc::F32(a) => s.push_str(&format!(
                    "    mov eax, dword ptr [rbp+{}]\n    mov qword ptr [rsp+{}], rax\n",
                    a, at
                )),
                ArgSrc::V128(a) => s.push_str(&format!(
                    "    mov rax, qword ptr [rbp+{}]\n    movups xmm8, xmmword ptr [rax]\n    movups xmmword ptr [rsp+{}], xmm8\n",
                    a, at
                )),
                ArgSrc::F64(a) | ArgSrc::Slot(a) => s.push_str(&format!(
                    "    mov rax, qword ptr [rbp+{}]\n    mov qword ptr [rsp+{}], rax\n",
                    a, at
                )),
                w => s.push_str(&format!("{}    mov qword ptr [rsp+{}], rax\n", word_rax(w), at)),
            }
        }
    }
    for l in &loads {
        s.push_str(l);
    }
    s.push_str(&format!("    call {}\n", sym));
    if stack_room > 0 {
        s.push_str(&format!("    add rsp, {}\n", stack_room));
    }
    // A float result is in xmm0 in both conventions, an integer one in rax. A struct of at most 8 octets
    // answers in rax or xmm0 (Firn) and must end up in rax (Win64, sizes 1/2/4/8) or behind the hidden pointer.
    if let WinRet::Word { size, sse } = ret {
        if *sse {
            s.push_str("    movq rax, xmm0\n");
        }
        if !win_by_value(*size) {
            s.push_str("    mov r10, qword ptr [rbp+16]\n");
            match size {
                3 => s.push_str("    mov word ptr [r10], ax\n    shr eax, 16\n    mov byte ptr [r10+2], al\n"),
                5 => s.push_str("    mov dword ptr [r10], eax\n    shr rax, 32\n    mov byte ptr [r10+4], al\n"),
                6 => s.push_str("    mov dword ptr [r10], eax\n    shr rax, 32\n    mov word ptr [r10+4], ax\n"),
                _ => s.push_str(
                    "    mov dword ptr [r10], eax\n    shr rax, 32\n    mov word ptr [r10+4], ax\n    shr eax, 16\n    mov byte ptr [r10+6], al\n",
                ),
            }
            s.push_str("    mov rax, r10\n");
        }
    }
    for k in 0..10u32 {
        s.push_str(&format!("    movaps xmm{}, [rsp+{}]\n", k + 6, k * 16));
    }
    s.push_str("    add rsp, 160\n");
    s.push_str("    pop rdi\n    pop rsi\n");
    s.push_str("    leave\n    ret\n");
    s
}

/// **The entry of a Firn thread on Windows.**
///
/// `CreateThread` jumps here with the Win64 convention: `rcx` = the thread
/// block (the `arg` of `Op::ThreadSpawn`, see `thread::spawn_sequence_windows`).
/// What the Linux `clone` sequence does in its child -- fetch the argument,
/// call `__thread_entry` -- is written out here, wrapped in the same
/// callee-saved bookkeeping every `#[win_callback]` thunk has (`rsi`, `rdi`
/// and `xmm6`-`xmm15` are callee-saved in Win64 and scratch in Firn).
///
/// Three things on top of a plain callback:
///   1. the stack bottom the collector scans up to is the TEB's `StackBase`
///      (`gs:[8]`) -- the thread runs on the stack Windows gave it, not on
///      the block the creator mapped (that one stays unused, see
///      docs/WINDOWS_THREADS.md);
///   2. after `__thread_entry` returns, the thread id word is zeroed and
///      woken -- exactly what the kernel does for `CLONE_CHILD_CLEARTID` on
///      Linux, and what `thread_wait` sleeps on;
///   3. the thread simply returns, which is `ExitThread`.
fn thread_main_asm() -> String {
    let mut s = String::new();
    s.push_str(&format!("\n.globl {}\n{}:\n", THREAD_MAIN, THREAD_MAIN));
    s.push_str("    # Win64 thread start -> __thread_entry(tcb)\n");
    s.push_str("    push rbp\n    mov rbp, rsp\n");
    s.push_str("    push rbx\n    push rsi\n    push rdi\n");
    s.push_str("    sub rsp, 168\n");
    for k in 0..10u32 {
        s.push_str(&format!("    movaps [rsp+{}], xmm{}\n", k * 16, k + 6));
    }
    s.push_str("    mov rbx, rcx\n");
    s.push_str("    mov rax, qword ptr gs:[8]\n");
    s.push_str(&format!("    mov qword ptr [rbx+{}], rax\n", crate::thread::WIN_TCB_BOTTOM));
    s.push_str("    mov rdi, rbx\n");
    s.push_str(&format!("    call {}\n", crate::codegen_x86::label(crate::thread::ENTRY)));
    s.push_str(&format!("    mov dword ptr [rbx+{}], 0\n", crate::thread::WIN_TCB_TID));
    s.push_str(&format!("    lea rdi, [rbx+{}]\n", crate::thread::WIN_TCB_TID));
    s.push_str(&format!("    call {}\n", thunk("WakeByAddressAll")));
    for k in 0..10u32 {
        s.push_str(&format!("    movaps xmm{}, [rsp+{}]\n", k + 6, k * 16));
    }
    s.push_str("    xor eax, eax\n");
    s.push_str("    add rsp, 168\n");
    s.push_str("    pop rdi\n    pop rsi\n    pop rbx\n");
    s.push_str("    leave\n    ret\n");
    s
}

/// **The stack probe.**
///
/// A Windows thread stack is reserved but not committed. At its lower end
/// sits a PAGE_GUARD page; touching it makes the kernel commit one more
/// page and move the guard down. A function that subtracts 40 KiB from
/// `rsp` and then writes at the bottom of its frame jumps OVER the guard,
/// hits reserved but uncommitted memory, and the process dies with
/// `STATUS_ACCESS_VIOLATION` — not at the write that was wrong, but at some
/// innocent looking local variable.
///
/// So every frame from one page on walks down page by page and touches each
/// one. `rax` holds the number of octets about to be subtracted; nothing
/// but `rax` and the flags is destroyed, because the prologue that calls
/// this still has the incoming arguments in the argument registers.
fn chkstk_asm() -> String {
    let mut s = String::new();
    s.push_str(&format!("\n.globl {}\n{}:\n", CHKSTK, CHKSTK));
    s.push_str("    # Windows guard page: touch every page of the coming frame.\n");
    s.push_str("    push rcx\n");
    s.push_str("    mov rcx, rsp\n");
    s.push_str(".Lfw_probe:\n");
    s.push_str(&format!("    cmp rax, {}\n", PAGE));
    s.push_str("    jbe .Lfw_last\n");
    s.push_str(&format!("    sub rcx, {}\n", PAGE));
    s.push_str("    mov byte ptr [rcx], 0\n");
    s.push_str(&format!("    sub rax, {}\n", PAGE));
    s.push_str("    jmp .Lfw_probe\n");
    s.push_str(".Lfw_last:\n");
    s.push_str("    sub rcx, rax\n");
    s.push_str("    mov byte ptr [rcx], 0\n");
    s.push_str("    pop rcx\n");
    s.push_str("    ret\n");
    s
}

/// **The import table, written by hand.**
///
/// A PE file binds to a DLL through five parallel tables, and every real
/// tool chain hides them inside an "import library" (`libkernel32.a`) whose
/// object files then end up in the image. Firn's rule that no foreign code
/// runs inside the program is easier to keep than to explain here: the five
/// tables are pure DATA, they are twenty lines of assembler per DLL, and
/// the linker's own PE script already collects `.idata$2` … `.idata$7` in
/// the right order and terminates the directory. So the compiler writes
/// them, and `-lkernel32` never appears on the command line.
///
///   * `.idata$2` — one 20 octet descriptor per DLL (lookup table, time
///     stamp, forwarder chain, name, address table).
///   * `.idata$4` — the import LOOKUP table: what the loader reads.
///   * `.idata$5` — the import ADDRESS table: what the loader overwrites
///     with the real addresses, and what our thunks call through.
///   * `.idata$6` — hint/name pairs, two octet aligned.
///   * `.idata$7` — the DLL names.
fn idata_asm(used: &[String]) -> String {
    let mut dlls: Vec<&'static str> = Vec::new();
    for u in used {
        if let Some((d, _)) = known(u) {
            if !dlls.contains(&d) {
                dlls.push(d);
            }
        }
    }
    let mut s = String::new();
    s.push_str("\n# ---- the import table of this program, written by the compiler ----\n");
    s.push_str(".section .idata$2\n");
    for (k, d) in dlls.iter().enumerate() {
        s.push_str(&format!("    .rva .Lfw_ilt{}\n", k));
        s.push_str("    .long 0\n    .long 0\n");
        s.push_str(&format!("    .rva .Lfw_dll{}\n", k));
        s.push_str(&format!("    .rva .Lfw_iat{}\n", k));
        let _ = d;
    }
    // Lookup tables.
    s.push_str(".section .idata$4\n");
    for (k, d) in dlls.iter().enumerate() {
        s.push_str(&format!(".Lfw_ilt{}:\n", k));
        for u in used {
            if known(u).map(|(dd, _)| dd) == Some(*d) {
                s.push_str(&format!("    .rva .Lfw_n_{}\n    .long 0\n", u));
            }
        }
        s.push_str("    .quad 0\n");
    }
    // Address tables — the slots the loader fills in.
    s.push_str(".section .idata$5\n");
    for (k, d) in dlls.iter().enumerate() {
        s.push_str(&format!(".Lfw_iat{}:\n", k));
        for u in used {
            if known(u).map(|(dd, _)| dd) == Some(*d) {
                s.push_str(&format!("{}:\n    .rva .Lfw_n_{}\n    .long 0\n", imp(u), u));
            }
        }
        s.push_str("    .quad 0\n");
    }
    // Hint/name pairs.
    s.push_str(".section .idata$6\n");
    for u in used {
        s.push_str(&format!(
            ".Lfw_n_{}:\n    .short 0\n    .asciz \"{}\"\n    .balign 2\n",
            u, u
        ));
    }
    // DLL names.
    s.push_str(".section .idata$7\n");
    for (k, d) in dlls.iter().enumerate() {
        s.push_str(&format!(".Lfw_dll{}:\n    .asciz \"{}\"\n", k, d));
    }
    s.push_str(".text\n");
    s
}

/// **The entry point.**
///
/// On Linux `_start` gets the initial stack pointer and hands it to `main`
/// as the first argument, so that a program can reach `argc`/`argv`
/// (`docs/SELF_HOSTING.md` §2). Windows hands over nothing at all: the
/// command line is a single UTF-16 string behind `GetCommandLineW`.
///
/// So the seam (`win_seam.rs`, written in Firn) builds the same block
/// Linux would have put on the stack — `[argc][argv0]…[0][envp…][0]` — and
/// gives back its address; `main` cannot tell the difference.
///
/// `and rsp, -16` first: the alignment `_start` is entered with is
/// documented, but a program that comes out one word off dies inside the
/// first `movaps` of some system DLL, and that is a very expensive hour.
pub fn start_asm(gc_init: Option<&str>, seam_init: &str, argv_sym: &str, main_sym: &str) -> String {
    let mut s = String::new();
    s.push_str(".globl _start\n_start:\n");
    s.push_str("    and rsp, -16\n");
    s.push_str("    sub rsp, 32\n");
    // The seam first: it opens the standard handles, and everything after
    // it — the collector included — may want to report an error.
    s.push_str(&format!("    call {}\n", seam_init));
    if let Some(g) = gc_init {
        s.push_str(&format!("    call {}\n", g));
    }
    s.push_str(&format!("    call {}\n", argv_sym));
    s.push_str("    mov rdi, rax\n");
    s.push_str(&format!("    call {}\n", main_sym));
    s.push_str("    mov ecx, eax\n");
    s.push_str(&format!("    call [rip + {}]\n", imp("ExitProcess")));
    s.push_str("    hlt\n");
    s
}

/// **DLL entry** (`--shared --target=x86_64-windows`). Win64: `ecx` = hinstance, `edx` = reason.
/// On `DLL_PROCESS_ATTACH` (1) only the seam init runs (standard handles); the collector and the
/// thread block start lazily on the first export call. Always returns TRUE.
pub fn dll_entry_asm(seam_init: &str) -> String {
    let mut s = String::new();
    s.push_str(&format!(".globl {}\n{}:\n", DLL_ENTRY, DLL_ENTRY));
    s.push_str("    cmp edx, 1\n    jne .L__dll_done\n");
    s.push_str("    push rbp\n    mov rbp, rsp\n    push rsi\n    push rdi\n    sub rsp, 160\n");
    for k in 0..10u32 {
        s.push_str(&format!("    movaps [rsp+{}], xmm{}\n", k * 16, k + 6));
    }
    s.push_str(&format!("    call {}\n", seam_init));
    for k in 0..10u32 {
        s.push_str(&format!("    movaps xmm{}, [rsp+{}]\n", k + 6, k * 16));
    }
    s.push_str("    add rsp, 160\n    pop rdi\n    pop rsi\n    pop rbp\n");
    s.push_str(".L__dll_done:\n    mov eax, 1\n    ret\n");
    s
}

/// The linker entry symbol of a DLL.
pub const DLL_ENTRY: &str = "__firn_dll_entry";

/// Everything the Windows target adds at the END of the assembly file:
/// the thunks, the stack probe and the import table.
pub fn runtime_asm() -> String {
    let used: Vec<String> = USED.with(|u| u.borrow().iter().cloned().collect());
    let mut s = String::new();
    let any_cb = CBUSED.with(|c| !c.borrow().is_empty());
    let any_export = crate::target::shared() && !crate::extfn::export_names().is_empty();
    if used.is_empty() && !probed() && !sysstub_used() && !any_cb && !thread_main_used() && !any_export {
        return s;
    }
    s.push_str("\n# ==== round WINDOWS: the boundary to Win32 ====\n");
    s.push_str(".text\n");
    for u in &used {
        let (_, argc) = known(u).expect("registered import is known");
        if u == DYNCALL || u == DYNCALLF {
            s.push_str(&dyncall_asm(u == DYNCALLF));
        } else {
            s.push_str(&thunk_asm(u, argc));
        }
    }
    let cbs: Vec<(String, u32)> =
        CBUSED.with(|c| c.borrow().iter().map(|(k, v)| (k.clone(), *v)).collect());
    for (sym, argc) in &cbs {
        s.push_str(&cb_thunk_asm(sym, *argc));
    }
    if crate::target::shared() {
        for th in crate::extfn::export_thunk_info() {
            let target = crate::codegen_x86::label(&th.internal);
            s.push_str(&win64_thunk_asm_ex(&th.c_name, &target, &th.params, &th.ret));
        }
    }
    if probed() {
        s.push_str(&chkstk_asm());
    }
    if thread_main_used() {
        s.push_str(&thread_main_asm());
    }
    if sysstub_used() {
        s.push_str(&sysstub_asm());
    }
    // The call gates are code, not imports: they get no descriptor.
    let imports: Vec<String> = used
        .iter()
        .filter(|u| u.as_str() != DYNCALL && u.as_str() != DYNCALLF)
        .cloned()
        .collect();
    if !imports.is_empty() {
        s.push_str(&idata_asm(&imports));
    }
    s
}

/// Every import the seam needs, registered up front. `_start` calls
/// `ExitProcess` through the address table directly, and a program that
/// never mentions a Win32 function still has to be able to stop.
pub fn note_baseline() {
    for n in crate::win_seam::BASELINE {
        let _ = note(n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_call_gate_loads_registers_and_stack_slots() {
        let a = dyncall_asm(false);
        assert!(a.contains("mov r9, qword ptr [r11+24]"), "{}", a);
        assert!(a.contains("movq xmm3, r9"), "{}", a);
        assert!(a.contains("mov qword ptr [rsp+rcx*8], rdx"), "{}", a);
        assert!(!a.contains("movq rax, xmm0"), "{}", a);
        assert!(dyncall_asm(true).contains("movq rax, xmm0"));
    }

    #[test]
    fn the_call_gates_are_no_imports() {
        reset();
        assert_eq!(note("win64_call"), Some("_Fwin.win64_call".to_string()));
        assert_eq!(note("LoadLibraryA"), Some("_Fwin.LoadLibraryA".to_string()));
        let asm = runtime_asm();
        assert!(asm.contains("_Fwin.win64_call:"), "{}", asm);
        assert!(!asm.contains("__imp_win64_call"), "{}", asm);
    }

    #[test]
    fn only_known_functions_become_imports() {
        reset();
        assert_eq!(note("WriteFile"), Some("_Fwin.WriteFile".to_string()));
        assert_eq!(note("PostQuantumDance"), None);
    }

    /// The order of the moves is the bug this thunk exists to avoid.
    #[test]
    fn five_and_six_go_down_before_r8_is_overwritten() {
        let a = thunk_asm("CreateFileW", 7);
        let five = a.find("mov qword ptr [rsp+32], r8").unwrap();
        let overwrite = a.find("mov r8, rdx").unwrap();
        assert!(five < overwrite, "argument five is lost:\n{}", a);
        let six = a.find("mov qword ptr [rsp+40], r9").unwrap();
        let ow9 = a.find("mov r9, rcx").unwrap();
        assert!(six < ow9, "argument six is lost:\n{}", a);
        // The seventh argument comes out of the caller's frame.
        assert!(a.contains("mov rax, qword ptr [rbp+16]"), "{}", a);
        assert!(a.contains("mov qword ptr [rsp+48], rax"), "{}", a);
    }

    #[test]
    fn the_shadow_space_is_there_and_the_stack_stays_aligned() {
        // 32 octets shadow, nothing else, rounded to 16.
        assert!(thunk_asm("CloseHandle", 1).contains("sub rsp, 32"));
        assert!(thunk_asm("VirtualAlloc", 4).contains("sub rsp, 32"));
        // 32 + 1 word = 40 -> 48.
        assert!(thunk_asm("WriteFile", 5).contains("sub rsp, 48"));
        // 32 + 3 words = 56 -> 64.
        assert!(thunk_asm("CreateFileW", 7).contains("sub rsp, 64"));
    }

    #[test]
    fn a_thunk_with_no_arguments_moves_nothing() {
        let a = thunk_asm("GetLastError", 0);
        assert!(!a.contains("mov rcx, rdi"), "{}", a);
        assert!(a.contains("call [rip + __imp_GetLastError]"), "{}", a);
    }

    #[test]
    fn the_import_table_has_one_descriptor_per_dll() {
        let used = vec!["WriteFile".to_string(), "socket".to_string()];
        let a = idata_asm(&used);
        assert_eq!(a.matches(".rva .Lfw_dll").count(), 2, "{}", a);
        assert!(a.contains(".asciz \"KERNEL32.dll\""), "{}", a);
        assert!(a.contains(".asciz \"WS2_32.dll\""), "{}", a);
        // Every table ends in its null entry.
        assert_eq!(a.matches("    .quad 0\n").count(), 4, "{}", a);
        assert!(a.contains("__imp_WriteFile:"), "{}", a);
    }

    #[test]
    fn the_probe_walks_page_by_page() {
        let a = chkstk_asm();
        assert!(a.contains("sub rcx, 4096"), "{}", a);
        assert!(a.contains("mov byte ptr [rcx], 0"), "{}", a);
        // and touches the remainder as well
        assert!(a.contains("sub rcx, rax"), "{}", a);
    }

    /// The thread entry thunk: Win64 in, `__thread_entry` called with the
    /// thread block, the stack bottom taken from the TEB, the TID word zeroed
    /// and woken at the end, and the callee-saved registers kept.
    #[test]
    fn the_thread_entry_thunk_does_what_clone_did_for_the_child() {
        reset();
        let a = thread_main_asm();
        assert!(a.contains("mov rbx, rcx"), "{}", a);
        assert!(a.contains("mov rax, qword ptr gs:[8]"), "{}", a);
        assert!(a.contains("mov qword ptr [rbx+40], rax"), "{}", a);
        assert!(a.contains("mov rdi, rbx"), "{}", a);
        let call = a.find("__thread_entry").expect("calls the entry");
        let zero = a.find("mov dword ptr [rbx+16], 0").expect("zeroes the TID word");
        let wake = a.find("WakeByAddressAll").expect("wakes it");
        assert!(call < zero && zero < wake, "{}", a);
        // rsi/rdi/xmm6-15 are callee-saved in Win64 and scratch in Firn.
        assert!(a.contains("push rsi") && a.contains("pop rdi"), "{}", a);
        assert!(a.contains("movaps [rsp+144], xmm15"), "{}", a);
        // entered with rsp = 8 mod 16: 1 + 3 pushes and 168 octets leave it 16-aligned
        assert!(a.contains("sub rsp, 168"), "{}", a);
    }

    #[test]
    fn the_thread_imports_are_known() {
        for n in ["CreateThread", "TlsAlloc", "TlsSetValue", "WaitOnAddress",
            "WakeByAddressSingle", "WakeByAddressAll", "CreateJobObjectW",
            "AssignProcessToJobObject", "SetInformationJobObject", "TerminateJobObject",
            "ResumeThread"] {
            assert!(known(n).is_some(), "{} is not in the import table", n);
        }
        assert_eq!(known("CreateThread"), Some(("KERNEL32.dll", 6)));
        assert_eq!(known("WaitOnAddress").map(|k| k.1), Some(4));
    }

    #[test]
    fn the_export_thunk_places_floats_and_stack_arguments_for_system_v() {
        // (i32, f64, i64, f32, then 9 integers): Win64 slots 0..3 are parked in the shadow space,
        // the f64 (slot 1) goes to xmm0, the f32 (slot 3) to xmm1, slots 0 and 2 to rdi / rsi.
        let mut kinds = vec![b'i', b'd', b'i', b'f'];
        kinds.extend(std::iter::repeat(b'i').take(9));
        let a = win64_thunk_asm("t", "sym", &kinds);
        assert!(a.contains("mov qword ptr [rbp+16], rcx"), "{}", a);
        assert!(a.contains("movsd qword ptr [rbp+24], xmm1"), "{}", a);
        assert!(a.contains("mov qword ptr [rbp+32], r8"), "{}", a);
        assert!(a.contains("movss dword ptr [rbp+40], xmm3"), "{}", a);
        assert!(a.contains("movsd xmm0, qword ptr [rbp+24]"), "{}", a);
        assert!(a.contains("movss xmm1, dword ptr [rbp+40]"), "{}", a);
        assert!(a.contains("mov rdi, qword ptr [rbp+16]"), "{}", a);
        assert!(a.contains("mov rsi, qword ptr [rbp+32]"), "{}", a);
        // 11 integers in total, 6 in registers: 5 go to the stack -> 40 octets, 48 with alignment
        assert!(a.contains("sub rsp, 48"), "{}", a);
        assert!(a.contains("add rsp, 48"), "{}", a);
        // the last stack word is argument position 12: [rbp+16+8*12]
        assert!(a.contains("mov rax, qword ptr [rbp+112]"), "{}", a);
    }

    #[test]
    fn a_v128_argument_is_loaded_through_its_hidden_pointer() {
        // (i64, v128, v128, f64): slots 1 and 2 are pointers (rdx, r8), the vectors land in xmm0 / xmm1, the f64 in xmm2.
        let a = win64_thunk_asm("t", "sym", &[b'i', b'v', b'v', b'd']);
        assert!(a.contains("mov qword ptr [rbp+24], rdx"), "{}", a);
        assert!(a.contains("mov qword ptr [rbp+32], r8"), "{}", a);
        assert!(a.contains("mov rax, qword ptr [rbp+24]\n    movups xmm0, xmmword ptr [rax]"), "{}", a);
        assert!(a.contains("mov rax, qword ptr [rbp+32]\n    movups xmm1, xmmword ptr [rax]"), "{}", a);
        assert!(a.contains("movsd xmm2, qword ptr [rbp+40]"), "{}", a);
    }

    #[test]
    fn a_struct_argument_is_loaded_by_value_or_through_its_pointer() {
        // (S8 by value, S12 by pointer, F4 {f32} by value): slots 0..2; Firn: S8 -> rdi, S12 -> rsi + rdx (words 4/8 octets),
        // F4 -> xmm0 (SSE word)
        let params = [
            WinParam::Agg { size: 8, sse: vec![false] },
            WinParam::Agg { size: 12, sse: vec![false, false] },
            WinParam::Agg { size: 4, sse: vec![true] },
        ];
        let a = win64_thunk_asm_ex("t", "sym", &params, &WinRet::Plain);
        assert!(a.contains("mov rax, qword ptr [rbp+16]\n    mov rdi, rax"), "{}", a);
        assert!(a.contains("mov r10, qword ptr [rbp+24]\n    mov rax, qword ptr [r10+0]\n    mov rsi, rax"), "{}", a);
        assert!(a.contains("mov r10, qword ptr [rbp+24]\n    mov eax, dword ptr [r10+8]\n    mov rdx, rax"), "{}", a);
        assert!(a.contains("mov eax, dword ptr [rbp+32]\n    movq xmm0, rax"), "{}", a);
    }

    #[test]
    fn a_hidden_result_pointer_shifts_every_slot() {
        // S16 result (Sret): slot 0 = result pointer -> Firn rdi; the i64 is slot 1 (rdx) -> rsi; rax comes back from Firn
        let a = win64_thunk_asm_ex("t", "sym", &[WinParam::Int], &WinRet::Sret);
        assert!(a.contains("mov qword ptr [rbp+24], rdx"), "{}", a);
        assert!(a.contains("mov rdi, qword ptr [rbp+16]"), "{}", a);
        assert!(a.contains("mov rsi, qword ptr [rbp+24]"), "{}", a);
    }

    #[test]
    fn a_small_struct_result_goes_behind_the_hidden_pointer_for_odd_sizes() {
        let a = win64_thunk_asm_ex("t", "sym", &[], &WinRet::Word { size: 3, sse: false });
        assert!(a.contains("mov word ptr [r10], ax"), "{}", a);
        assert!(a.contains("mov byte ptr [r10+2], al"), "{}", a);
        assert!(a.contains("mov rax, r10"), "{}", a);
        // size 8 with floats: Firn answers in xmm0, Win64 wants rax
        let b = win64_thunk_asm_ex("t", "sym", &[], &WinRet::Word { size: 8, sse: true });
        assert!(b.contains("movq rax, xmm0"), "{}", b);
        assert!(!b.contains("r10"), "{}", b);
    }

    #[test]
    fn a_thunk_with_register_only_arguments_needs_no_stack_block() {
        let a = win64_thunk_asm("t", "sym", &[b'd', b'd', b'i']);
        assert!(!a.contains("sub rsp, 16\n"), "{}", a);
        assert!(a.contains("movsd xmm0, qword ptr [rbp+16]"), "{}", a);
        assert!(a.contains("movsd xmm1, qword ptr [rbp+24]"), "{}", a);
        assert!(a.contains("mov rdi, qword ptr [rbp+32]"), "{}", a);
    }
}
