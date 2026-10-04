# Desktop libraries (round DESKTOP)

What a program that lives on somebody's desktop needs and a window library does not give it: an icon in
the notification area, a message to the user, files dropped on its window, the clipboard, sound, being told
when a folder changes, starting at login, and a second copy that hands its command line to the first. They
come from the OpenPlan/FleiLauncher wish list and from the app kit (`docs/APPKIT.md`).

Everything below has a positive check against something nobody here wrote. Where that was not possible the
table and the "Honest limits" say so.

| piece | module(s) | Linux | Windows | macOS / Android / OrientOS |
|---|---|---|---|---|
| D-Bus client and service | `net.dbus` (`lib/net/dbus.fi`), `net.unix` (`lib/net/unix.fi`) | yes | -- (not a concept there) | -- |
| tray icon + menu | `desktop.tray` | StatusNotifierItem + dbusmenu over `net.dbus` | `Shell_NotifyIconW`, popup menu | stubs |
| notifications | `desktop.notify` | `org.freedesktop.Notifications` over `net.dbus` | tray balloon (a toast on 10/11) | stubs (Android: `window.notify` exists) |
| files dropped on a window | `window.drop_accept/drop_take`, `EV_DROP`, `desktop.filedrop` | XDND v5 (`lib/window/x11.fi`) | `WM_DROPFILES` (`lib/window/win32.fi`) | `false` |
| clipboard | `window.clipboard_*` | X11 selections (existed, LIB-006) | `CF_UNICODETEXT`, `CF_HDROP`, registered formats | Android/OrientOS: `false` |
| audio output | `audio.pcm`, `audio.pulse`, `audio.dev`, `audio.sink`, `audio.player` | PulseAudio native protocol (PipeWire's pulse server too) | `waveOut` | stubs |
| folder watcher | `desktop.watch` | inotify | `ReadDirectoryChangesW` | stubs |
| autostart | `desktop.autostart` | XDG autostart `.desktop` | `HKCU\...\Run` | stubs |
| one copy per user, command line handed over | `appkit.single_instance` (`single_listen/poll/send_args`) | Unix socket | named pipe | stubs in `appkit.platform*` |

One interface per module, one file per platform, picked by the compiler's twin rule (`x.fi` / `x.windows.fi`,
`lib/@android/...`) -- or, for macOS and OrientOS, the `x_macos.fi` / `x_osum.fi` files that are **untested stubs**
(Firn has no macOS target; the stubs answer "not available"). `python3 tools/desktop/platforms.py` compares the
export lists and signatures of all five files of every module and type-checks each one.

## net.dbus -- the base

A D-Bus client written from the specification: the SASL handshake (`EXTERNAL`), the message header with its
field array, marshalling and unmarshalling of every basic type, arrays, structs, dict entries and variants with
the alignment rules, a generic "skip a value by its signature", method call/return/error/signal. It is also a
**service**: own a name, receive calls, reply, send signals. Single threaded and pull style (`bus_recv` with a
timeout); what arrives while a call waits for its reply is queued, not lost and not dispatched re-entrantly.
`net.unix` is the Unix socket part (exact address lengths -- `net.connect_unix` hands the kernel 110 octets,
which breaks abstract names -- listen/accept, `poll`).

It is the base for the Secret Service (Roadmap r204) and for anything else on the session or system bus.

*Checked* (`tools/desktop/dbus_check.py`, built in opt and dev-fast): libdbus (dbus-python) calls a Firn
service with 18 values of every kind and Firn compares each one itself; the Firn service marshals a body of its
own and libdbus decodes it; a body goes back byte for byte; a 1 MiB array both ways; variants in variants;
empty arrays; an error reply with name and text; an unknown method; the queue (a signal that arrives while a
Firn call waits comes out afterwards). *Not supported*: file descriptor passing, big-endian peers, messages over
16 MiB, TCP addresses, `doubles` other than as IEEE bits (`w_f64_bits`; there is no bit cast `f64`<->`u64` yet).

## desktop.tray

```firn
var t: tray.Tray = tray.tray_new("org.example.app", "Example")
tray.tray_set_icon_rgba(&t, 16 as u32, 16 as u32, pixels)      // RGBA, straight alpha; or tray_set_icon_name on Linux
tray.tray_set_tooltip(&t, "Example", "2 new messages")
tray.tray_menu_add(&t, 1, "Open", tray.ITEM_ENABLED)
tray.tray_menu_separator(&t)
tray.tray_menu_add(&t, 2, "Quit", tray.ITEM_ENABLED)
if !tray.tray_show(&t) { /* no tray on this desktop */ }
var ev: tray.TrayEvent = tray.tray_event_zero()
while tray.tray_poll(&t, 1000, &ev) { /* EV_ACTIVATE, EV_SECONDARY, EV_MENU(id), EV_CONTEXT, EV_SCROLL, balloon events */ }
```

Linux: the StatusNotifierItem specification (KDE, GNOME's AppIndicator extension, Xfce, Cinnamon, LXQt, MATE,
Budgie): the item `org.kde.StatusNotifierItem-<pid>-1` with `Properties` (`Id`, `Title`, `Status`, `IconName`,
`IconPixmap` as ARGB32, `ToolTip`, `Menu` ...), `Activate`/`SecondaryActivate`/`ContextMenu`/`Scroll`, and the menu
as `com.canonical.dbusmenu` (`GetLayout`, `GetGroupProperties`, `Event`, `EventGroup`, `AboutToShow`), registered with
the `StatusNotifierWatcher`; it registers again by itself when a watcher appears later (`NameOwnerChanged`).
Windows: a hidden window, `Shell_NotifyIconW`, the popup menu through `TrackPopupMenu`, `TaskbarCreated` handled.

*Checked*: Linux against a StatusNotifierWatcher, a property reader and a dbusmenu client written with libdbus
(49 checks: the properties, the ARGB pixels octet by octet, the layout with separator, check mark, disabled item,
revision and `LayoutUpdated`, `Introspect`, every click, the order of events). Windows under Wine with Wine's
`explorer /desktop=...` as the shell: the shell accepts the icon (and refuses it without a shell), and the
**screenshot of the X server shows the picture's red, green and blue in the tray in the picture's column order**;
the messages a shell and a popup menu send are posted to the program's window and become the same events.

## desktop.notify

`notify_open`, `notify_show` (summary, body, icon, actions as `key\nLabel\n...`, urgency, category,
desktop-entry, timeout, `replaces`), `notify_close`, `notify_poll` (EV_ACTION with `notify_event_key`, EV_CLOSED with
the reason), `notify_capabilities`, `notify_server_info`, `notify_once`. Linux: the Desktop Notifications
Specification. Windows: a balloon from a tray icon (Windows 10/11 turn it into a toast in the action center); a
click is the action `default`. *Checked* against a notification server written with libdbus (the arguments the
server received -- app name, icon, UTF-8 body, action pairs, the `urgency` hint as a **byte**, category,
desktop-entry, timeout, replaces id; the signals back in order) and, on Windows, the balloon call, the click and
the timeout through Wine's shell.

## Files dropped on a window

`window.drop_accept(f, true)` once, `EV_DROP` in the event loop, `window.drop_take(f, &buf)` gives the paths
(UTF-8, each followed by a NUL); `desktop.filedrop.filedrop_event(w, kind, callback, ud)` turns that into a
callback per path. X11: **XDND version 5, the target's half** -- `XdndAware`, `XdndEnter` (also the type list in
`XdndTypeList`), `XdndPosition` answered with `XdndStatus`, `XdndLeave`, `XdndDrop` -> `ConvertSelection` of
`text/uri-list` -> `XdndFinished`; local `file://` URIs percent-decoded, remote hosts, other schemes and comments
dropped. Windows: `DragAcceptFiles` + `WM_DROPFILES` + `DragQueryFileW`.

*Checked on X11* (`tools/desktop/drop_check.py`): **GTK 3 as a real drag source**, the mouse driven with xdotool
like a person dragging a file out of a file manager; a python-xlib source written from the specification for the
rest (five types with the list in the property, text only -> refused with the right Status/Finished, a Leave, a
list with nothing local, percent escapes, a NUL, a `%25`). *Windows*: a real `WM_DROPFILES` with a real `HDROP`
(a `GlobalAlloc` block) is posted to the program's own window under Wine; **Wine's own XDND-to-WM_DROPFILES
conversion could not be driven reliably** (GTK's drag over Wine's virtual desktop produced no drop), so that
path is not verified end to end.

## Clipboard on Windows (LIB-006's Windows half)

`text/plain` <-> `CF_UNICODETEXT` (`\n` <-> `\r\n`), `text/uri-list` <-> `CF_HDROP`, `image/png` <-> the registered
format `PNG`, any other type <-> a format registered under that name. *Checked* both directions against GTK 3's
clipboard through Wine's X-selection bridge: UTF-8 text with line breaks both ways, the file list, a private
type's name on the X side, a type nobody offers answers "none".

## Audio

```firn
var s: sink.Sink = sink.sink_open("My App", 44100 as u32, 2 as u32)   // FIRN_AUDIO=file:... / raw:... / null for tests
sink.sink_set_volume(&s, 80 as u32)
sink.sink_write(&s, pcm, frames)            // S16LE interleaved; blocks for the device
sink.sink_drain(&s)
sink.sink_close(&s)

player.play_mp3("My App", bytes, len, 80 as u32)                       // or player_new/player_play_mp3/player_pump for an event loop
```

`audio.pcm` has the volume (`out = trunc(in * gain / 65536)`, clamped), a WAVE header and CRC-32. `audio.pulse` is a
**PulseAudio native-protocol client** (AUTH with the cookie, client name, playback stream, flow control by the
server's REQUEST grants, DRAIN/FLUSH/CORK, DELETE; protocol version 15 announced). `audio.dev` is the platform
device (`dev.fi` PulseAudio, `dev.windows.fi` waveOut with six buffers, small writes coalesced, `waveOutPause`).
`audio.sink` adds file/raw/null sinks, the volume and a CRC of what went out. `audio.player` decodes MP3 with the
existing `ton.mp3` and feeds the sink; `player_pump` never blocks, `play_mp3` is the loop around it; pause is the
device's pause (no gap), stop flushes.

*Checked* (`tools/desktop/audio_check.py`): the PCM through the player is **byte for byte what the plain decoder
writes** (4 files, mono/stereo/short blocks), CRC-32 equals zlib's, WAVE read back with Python's `wave`, volume
by the written formula (0, 25, 50, 150 %), pause, stop, junk input; the decoder against **ffmpeg** (SNR 100 dB, 1 LSB
after aligning the encoder delay); through a **real pulseaudio** (null sink, `parec` records its monitor): our PCM
comes out **bit for bit**, also at 50 % volume; `pactl` shows the stream with its name, role and format; no stream
is left after stop; no server gives a clean error. Windows (`audio_check_win.py`): Wine's ALSA driver pointed at
ALSA's `file` plugin, so everything waveOut hands over is a file: a generated pattern (3 s, every frame distinct)
arrives **exactly and bit for bit** with blocks of 77, 1152 and 4410 frames; the MP3 through the Windows player
equals the decoder's PCM, at 100/50/150 %, pause without a gap, stop a prefix, no device -> `NOSINK`.

## desktop.watch and desktop.autostart

`watch_add(w, path, recursive)`, `watch_poll(w, ms, &ev)` with CREATED/MODIFIED/DELETED/RENAMED_FROM/RENAMED_TO
(one cookie), ATTRIB, OVERFLOW, ROOT_GONE, `is_dir`, the path. inotify on Linux (a folder that appears under a
recursive watch is watched and scanned so nothing is missed), `ReadDirectoryChangesW` on Windows (overlapped
reads, re-issued before the records are decoded). `tests/2220` is the same program on both (Windows under Wine).
`autostart_set(app_id, name, exe, args)`, `_remove`, `_is_enabled`: Linux writes the XDG autostart entry with the
Desktop Entry specification's quoting; Windows writes the Run value with the C runtime's quoting (and honours the
Task Manager's "disabled" approval). *Checked*: Linux by Python's reading of the specification **and by GLib's
`gio launch`**, which starts the Exec line -- the program it starts receives exactly the arguments (blanks, quotes,
`$`, backticks, `%`, UTF-8, empty arguments, 3000-octet arguments); Windows by Wine's `reg query` and by **MSVCRT's
own command line parser** (a MinGW program prints the arguments `CreateProcess` gave it).

## The command line of a second copy (appkit.single_instance)

After `single_acquire` said `SI_OK`: `single_listen()`; in the event loop `single_poll(ms, &msg)` and
`single_item(&msg, i)` (item 0 = the sender's directory, then the arguments). The second copy: `single_send_args(1)`
-- true means the first copy acknowledged it. Linux: a Unix socket `instance.sock` next to the lock file (an abstract
name when the path is too long), same user only (`SO_PEERCRED`); Windows: a named pipe
(`FILE_FLAG_FIRST_PIPE_INSTANCE`, remote clients refused). It lives in the platform layer (`plat_ipc_*` in all five
`appkit.platform*` files) so nothing is built twice. `tests/2221` runs it as two real processes (arguments with blanks,
UTF-8 and an empty one, 50 000 octets, an oversized message refused, garbage and a silent client, stale socket,
no listener) on Linux and under Wine.

## Honest limits (what was NOT done or NOT verified)

* **No real desktop.** There is no GNOME, KDE or Windows on the build machine: the tray and the notifications are
  checked against implementations of the *other side* of the specifications written with libdbus (and, on Windows,
  Wine's shell), not against the desktops' own code. No XEMBED tray fallback was built (a bare window manager
  without a StatusNotifierWatcher gets `tray_show` = false; it keeps serving and registers when one appears).
* **Windows is under Wine only.** Real Windows cannot be reached from here (the FLEI-ONE helper cannot transfer
  binaries); a kit zip for it is future work. Known Wine differences: a deleted watched folder is not reported
  (`ROOT_GONE` untested on Windows), a folder *moved into* a watched tree is not followed (tests/2220 skips those
  two checks on Windows), the XDND path to `WM_DROPFILES` was not driven; Wine's PulseAudio driver drops audio
  under load (the tests use its ALSA driver into a file instead).
* The balloon is the classic one, not WinRT toasts (no action buttons/images); ALSA directly, WASAPI, AAudio, Core
  Audio, FSEvents, UNUserNotificationCenter, NSStatusItem, LaunchAgents: not implemented (the macOS/Android/OrientOS
  files are stubs and say so). No resampling or gapless playback, no seeking; playback only advances while
  `player_pump` is called (no threads in the libraries).
* Pulse: playback only, S16LE, 1 or 2 channels, no shared memory transport, no TCP servers.
