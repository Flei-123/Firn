# std.dialog and std.toast -- system dialogs and notifications as small libraries

Round DIALOG (roadmap r347). A program asks for a file, a folder, a colour, a font or an answer to a question, or shows
a notification, and gets the same result on every system. It needs no window of its own and no widget library: a command
line tool can call it as well as a fUi program. Same pattern as [CLIPBOARD.md](CLIPBOARD.md): one API file, one operating
system half per target, no crash when a backend is missing -- a status code says so.

```
import std.rt
import std.dialog

var path: rt.Buf = rt.buf_new()
let r: i32 = dialog.open_file("Open a picture", "", "Pictures|*.png;*.jpg|All files|*", &path)
if r == dialog.OK { /* rt.buf_len(&path) octets of UTF-8, "/" as separator */ }
else if r == dialog.CANCELED { /* the user pressed Cancel: nothing to do */ }
else { /* dialog.NO_BACKEND or dialog.FAILED: tell the user, or fall back to a text field */ }
rt.buf_free(&path)

var rgb: u32 = 0x1BD96A
dialog.pick_color("Accent colour", rgb, &rgb)                       // 0xRRGGBB, no alpha
var yes: bool = false
dialog.confirm("Delete?", "Delete 3 files?", &yes)                  // r == OK and yes
```

There are **two levels** (decision of the boss, 09.10.2026). **Level 1** is this clean Firn API (`std.dialog`, `std.toast`).
**Level 2** is a later second stage: OrientOS speaks the Linux desktop interfaces (`org.freedesktop.Notifications`,
`xdg-desktop-portal`) so that foreign GTK/Qt/Firefox programs use our dialogs and toasts without being changed. It is
described at the end (section "Level 2"); the spike is in the second part of this round.

## The API

| call | meaning | result |
|---|---|---|
| `open_file(title, dir, filters, out)` | one existing file | path appended to `out` |
| `open_files(title, dir, filters, out)` | one or more existing files | paths appended to `out`, one per line (`\n`) |
| `save_file(title, dir, name, filters, out)` | a file name to write; the system asks before overwriting | path appended to `out` |
| `pick_folder(title, dir, out)` | an existing directory | path appended to `out` |
| `pick_color(title, initial, out)` | a colour | `*out` = 0xRRGGBB |
| `pick_font(title, family, size, out_family, out)` | a font | family appended to `out_family`, `*out` = `FontChoice{size, bold, italic}` |
| `message(kind, title, text, buttons, answer)` | a message box | `*answer` = `ANS_OK` / `ANS_CANCEL` / `ANS_YES` / `ANS_NO` |
| `confirm(title, text, yes)` | a question with Yes and No | `*yes` |
| `backend_name()` | what a call would use | `"win32"`, `"zenity"`, `"kdialog"`, `"fui"`, `"web"`, `"none"` |

All strings are UTF-8. `dir` and `name` may be empty ("the system's choice"). `kind` is `KIND_INFO`, `KIND_QUESTION`,
`KIND_WARNING` or `KIND_ERROR`; `buttons` is `BTNS_OK`, `BTNS_OK_CANCEL`, `BTNS_YES_NO` or `BTNS_YES_NO_CANCEL`. Anything else
answers `FAILED`.

**Filters** are one string, the same on every system: pairs of a label and a list of patterns, all separated by `|`,
patterns by `;`: `"Pictures|*.png;*.jpg|All files|*"`. An empty string means every file. The first pair is the one that is
selected.

**Status** -- every function returns an `i32`, never crashes and never waits for anything but the user:

| code | name | meaning |
|---|---|---|
| 0 | `OK` | the user chose; the out parameters are filled |
| 1 | `CANCELED` | the user closed the dialog or pressed Cancel; the out parameters are untouched |
| 2 | `NO_BACKEND` | this system has no way to show that dialog (no display, no helper, a browser, Android ...) |
| 3 | `FAILED` | the dialog could not be shown or its answer could not be read |

**The same result everywhere.** Paths use `/` (Windows: `C:/Users/x/a.txt`, which every Windows call of Firn accepts). A
colour is always `0xRRGGBB` (Windows' `COLORREF` is converted, GTK's `rgb(r,g,b)` text is parsed). A font is the family
name, a size in whole points and two flags. A message box answers with our four answers, not with the system's numbers. A
box that was closed without a button answers `ANS_CANCEL` when there is a Cancel button, `ANS_NO` when there is only Yes /
No, else `ANS_OK`.

## The backends

| system | how | file |
|---|---|---|
| Windows | `comdlg32` (`GetOpenFileNameW`, `GetSaveFileNameW`, `ChooseColorW`, `ChooseFontW`), `SHBrowseForFolderW` (shell32, with a callback that selects the start folder), `MessageBoxW` (user32); bound in the compiler's import table (`compiler/src/win.rs`) | `lib/std/dialogos.windows.fi` |
| Linux | the helpers of the desktop: `zenity` (GTK), `kdialog` (KDE); `kdialog` first in a KDE session, else `zenity`; without a display nothing is tried | `lib/std/dialogos.fi` |
| Linux, OrientOS | no helper: the **dialog service** `dialogd` (below), a fUi program, started per request | `tools/dialogd/` (spike) |
| browser | `alert` / `confirm` for `message` / `confirm` (synchronous); colour and files are asynchronous requests (`lib/plat/webdialog.fi`) and answer `NO_BACKEND` through `std.dialog` | `lib/@web/std/dialogos.fi` |
| Android | the system pickers need an Activity result; not in this round: `NO_BACKEND` | `lib/@android/std/dialogos.fi` |

`FIRN_DIALOG` forces a choice on Linux: `zenity`, `kdialog`, `service`, `none`; `FIRN_DIALOG_SERVICE` names the service
program (default `dialogd` on `PATH`).

### Why not IFileDialog on Windows

`IFileOpenDialog` (the Vista dialog, with a real "pick folders" mode) is a COM object: every call is an indirect call
through a vtable, and a Firn indirect call is System V, not Win64 (see `lib/std/secret_os.windows.fi`). Only `extern fn`
imports get the thunk. So the classic functions are used -- they are the dialogs Windows shows to every program, only
without the newest places pane and without multi-select of folders. The folder picker is the shell's
`SHBrowseForFolderW` with the new style flag.

### Browser

A page cannot ask for a path. `message` and `confirm` are the page's `alert` / `confirm` (Yes / No / Cancel does not exist
there and answers `NO_BACKEND`). A colour and files are requests inside a user gesture that answer later as the event
`web.WE_DIALOG` (`lib/plat/webdialog.fi`):

```
webdialog.web_dialog_color("Accent", 0x1BD96A)     // later: WE_DIALOG q = "color", p/n = "rrggbb", status 1 (0 = cancelled)
webdialog.web_dialog_file(".png,.jpg", true)       // later: one WE_DIALOG per file: q = its name, p/n = its CONTENT,
                                                   //        id = its number; then q = "", status 2 (the end; 0 = cancelled)
```

## Notifications: std.toast

```
import std.toast

let id: u32 = toast.show("Download finished", "mod.jar, 4.2 MiB")       // 0 = nobody showed it

var o: toast.Options = toast.options()
o.icon = "emblem-downloads"   o.urgency = toast.URGENCY_CRITICAL   o.timeout_ms = 7000
o.actions = "open\nShow file\n"                                          // key, label, key, label ...
let id2: u32 = toast.send("Download finished", "mod.jar", &o)

fn clicked(ud: u64, id: u32, key: str) { ... }                           // a named function: Firn has no closures
toast.dispatch(5000, clicked, 0 as u64)                                  // waits up to 5 s for ONE click or close
```

The module is called `std.toast`, not `std.notify`: Firn's module names are flat, and a module `notify` would collide
with `desktop.notify` that it is built on. It is a small layer over `desktop.notify` (`lib/desktop/notify.fi`) that keeps
one connection per program and offers events as `poll` or as a callback (`dispatch`):

| system | how | file |
|---|---|---|
| Linux | `org.freedesktop.Notifications` over D-Bus (`net.dbus`): title, body, icon, actions, urgency, category, timeout, replacing in place, `ActionInvoked`, `NotificationClosed`, capabilities | `lib/desktop/notify.fi` |
| Windows | a balloon of a notification area icon, which Windows 10/11 shows as a toast: title, body, click (= action `default`). Not WinRT toasts: those need COM and WinRT activation | `lib/desktop/notify.windows.fi` |
| browser | the Notification API; permission is asked at the first call inside a gesture (that call answers 0); click and close come as `WE_DIALOG` and `toast.from_web_event` | `lib/@web/std/toast.fi` |
| Android | `desktop.notify`'s stub (0); a program with a window uses `window.notify` (FIRN r64, through the Push service) | `lib/@android/desktop/notify.fi` |
| OrientOS | the kernel's system bus call `NOTIPOST` (rax 1960, rdi 19): the task bar shows the text as a toast, keeps it in its list and counts it in "Meldg" | `lib/desktop/notify_osum.fi` |

## What fUi has and what this adds

`lib/fui/wave3.fi` has the PICTURES of a dialog: `draw_dialog` (frame, title, buttons), `draw_filedialog` (frame + list +
name field) and `draw_colorpicker` (saturation/value field + hue strip, `hsv_rgb`). They draw; none of them holds a state or
reads an event, and there is no service. `dialogd` is built on `fui.app` (labels, buttons, a text field, a list) and adds
what was missing: the state, the keyboard and mouse handling, and the request/answer protocol. If fUi grows interactive
dialog widgets, `dialogd` moves to them -- agreed with the fUi project as its roadmap item r177.

## The dialog service (`dialogd`)

One program that shows the dialogs of the system with the theme of the system. On Linux it is the fallback when neither
`zenity` nor `kdialog` is installed; on OrientOS it is meant to be THE dialog for every program and for Certus.

The protocol is lines of text on a pipe (`lib/std/dialogproto.fi`). One request is one line, fields separated by TAB, with
the escapes `\t` `\n` `\r` `\\` inside a field; the answer is one line:

```
open_file  <title>  <dir>  <filters>                          ->  ok <path> | cancel | failed
open_files <title>  <dir>  <filters>                          ->  ok <path1>\n<path2> | cancel | failed
save_file  <title>  <dir>  <name>  <filters>                  ->  ok <path> | cancel | failed
pick_folder <title> <dir>                                     ->  ok <path> | cancel | failed
pick_color <title>  <rrggbb>                                  ->  ok <rrggbb> | cancel | failed
pick_font  <title>  <family>  <size>                          ->  ok <family> <size> <bold 0|1> <italic 0|1> | cancel | failed
message    <kind> <title> <text> <buttons>                    ->  ok <ans_ok|ans_cancel|ans_yes|ans_no> | cancel | failed
```

`std.dialog` starts `dialogd --stdio` for one request, writes the line to its standard input and reads the answer from its
standard output. What `dialogd` does today: every message box; the file dialogs as a path field with the start directory
listed under it (a click on a row copies its path; wrong paths are refused; saving over an existing file asks twice);
the colour dialog as a hex field with a swatch and a Preview button. It has **no font chooser** (`failed`), no navigation
inside the file dialog, no real colour picker (fUi's `draw_colorpicker` is the planned body). On OrientOS the same lines
travel over the action bus as the actions `dialog.open_file`, `dialog.save_file`, `dialog.pick_folder`, `dialog.pick_color`,
`dialog.pick_font`, `dialog.message` (level `write`, OrientOS `docs/ACTION-BUS.md`); a resident service answers many
requests in order. That wiring is OrientOS-side work (roadmap there), not done here.

## Level 2: OrientOS speaks the Linux desktop interfaces

Goal: a foreign GTK/Qt/Firefox program that sends `org.freedesktop.Notifications.Notify` or asks the file chooser portal
`org.freedesktop.portal.FileChooser` gets OrientOS' own toast and dialog, with no change to the program. A **service on
OrientOS** answers those D-Bus names and hands the work to the Level 1 pieces (the toast of the task bar, `dialogd`).

What exists and what is missing (looked at 09.10.2026):

| piece | state |
|---|---|
| a D-Bus **client** in Firn | `net.dbus` (tested against libdbus, every type both ways) |
| a D-Bus **server side** (own a name, answer calls, send signals) | `net.dbus` has `bus_request_name`, `bus_pump`, `bus_reply`, `bus_signal`; `desktop.tray` is such a server |
| a D-Bus **bus daemon / broker on OrientOS** | **missing**. OrientOS has none on purpose (ACTION-BUS.md section 1 "Why not D-Bus"; SYSTEMBUS.md: the bus is in the kernel). Foreign programs find their bus through `DBUS_SESSION_BUS_ADDRESS` on a unix socket; `kernel/unixsock.fi` exists (the Wayland round), so a small broker (`Hello`, `RequestName`, routing, match rules, `EXTERNAL`/anonymous auth) is possible and is the first thing to build |
| the toast on OrientOS | `NOTIPOST` of the system bus, shown by the task bar (`barpop`), counted in "Meldg" |
| the dialog on OrientOS | not there; `dialogd` is its first body |
| Wayland (foreign GUI programs) | the Wayland round of the OrientOS department (`wayd`, `docs/RUNDE-WAYLAND.md`, roadmap r498-r503): `wl_data_device`, seat, formats, GPU. It does not cover D-Bus or portals; nothing here duplicates it |

The service (`orient-portald`, spike in `tools/portald`, built, tested, 32 checks) owns `org.freedesktop.Notifications`
(`Notify`, `CloseNotification`, `GetCapabilities`, `GetServerInformation`, the signal `NotificationClosed`) and
`org.freedesktop.portal.Desktop`: `FileChooser.OpenFile` / `SaveFile` (a Request object and the signal `Request.Response(u, a{sv})`
with `uris`; the options `multiple`, `directory`, `current_folder`, `current_name`, `handle_token` and the first filter's glob
patterns are honoured) and `Settings.Read` / `ReadOne` / `ReadAll` for `org.freedesktop.appearance` (`color-scheme`,
`accent-color`); `Introspect` for both objects, because gdbus and dbus-python ask for the signatures before they marshal. A
notification goes to a toast sink (a file on Linux; `notify_osum.fi`'s `NOTIPOST` on OrientOS), a file chooser request goes to
`std.dialog`, i.e. to whatever dialog the machine has (on OrientOS: `dialogd`). Tested on a private bus against dbus-python
(libdbus), `gdbus` (GLib) and `dbus-send`, with a stand-in dialog service whose request lines are compared exactly. Not done:
`SaveFiles`, `Screenshot` / `PickColor`, properties, several requests at once (the daemon is busy while a dialog is open), actions
and clicks back to the sender (`ActionInvoked`), and the broker on OrientOS (above) -- without it a foreign program there finds no
bus.

**Not built, only an idea (roadmap):** compatibility with the Windows API in Wine's way (so that a Windows program's
`GetOpenFileNameW` would show our dialog).

## Honest limits

- **B1** Windows is tested under Wine (the real `comdlg32` / `shell32` / `user32` of Wine 8, driven by a second program
  that presses the buttons), not on a real Windows machine. The code calls the documented entry points with the
  documented structures.
- **B2** Linux: `zenity` and `kdialog` are not installed on the build machine (`zenity` drags in webkit, about 500 MB). The
  command lines are written from their manuals and tested against stand-in scripts that record the arguments and answer
  with canned output (test 2380); the real dialogs of the Linux path are `dialogd`'s (checked on a real X server, a real
  window, real mouse and key events). A different zenity version may print colours or fonts in another form: the parser
  takes `rgb()`, `rgba()`, `#rrggbb` (also 16 bits per channel) and Pango descriptions.
- **B3** `zenity --question` cannot tell "No" from "closed with Escape" (both exit 1): `BTNS_YES_NO` answers `ANS_NO` for both.
  `BTNS_YES_NO_CANCEL` uses an extra button, so Cancel is a real answer there. `pick_font` does not pass the start font to
  zenity.
- **B4** `kdialog` has no font chooser: `pick_font` answers `NO_BACKEND` on a KDE session without `zenity`.
- **B5** The title of the colour and the font dialog is not set on Windows (comdlg32 has no field for it).
- **B6** In the browser a file dialog gives no path; Yes / No / Cancel does not exist. Headless Chromium reports
  `Notification.permission = "denied"` whatever is granted, so the test replaces the `Notification` class with a
  recording stand-in; the colour and file inputs and `alert` / `confirm` are the real ones.
- **B7** A dialog is modal and blocks the calling thread until the user answers; there is no time limit. Not thread safe.
- **B8** The xdg-desktop-portal file chooser is not used by Level 1 on Linux (the helpers and `dialogd` are); roadmap.
- **B10** Wine's folder dialog does not reliably select the start folder that `pick_folder` passes (`BFFM_SETSELECTIONW` from the
  callback; it landed on `/`, `/tmp` or another folder in trials), so the test checks only the shape of the answer. The call is
  the documented one; Microsoft's dialog selects it.
- **B9** `desktop.notify` on OrientOS (`notify_osum.fi`) is type-checked, not run on OrientOS (needs the test guest).

## What was really run

| part | how | result |
|---|---|---|
| zenity / kdialog / service command lines and answers, backend choice | test 2380: stand-in scripts, 8 environments | real run (Linux), a mutation made it fail |
| the fUi dialogs | `tools/dialog/dialog_check.py`: Xvfb, xdotool, screenshots, WM_DELETE_WINDOW | real run, 30 checks |
| Windows dialogs | `tools/dialog/dialog_check_win.py`: Wine 8 (real comdlg32 / shell32 / user32) + a pilot program that presses the buttons | real run, 19 checks (not on Microsoft's Windows) |
| browser | `tools/dialog/check_webdialog.cjs`: Chromium (Playwright) | real run, 17 checks |
| toast on Linux | `tools/dialog/toast_check.py`: a notification server written with libdbus | real run |
| orient-portald | `tools/portald/portald_check.py`: dbus-daemon + dbus-python, gdbus, dbus-send | real run, 32 checks |
| Android, OrientOS | `tools/dialog/platforms.py`, `tools/desktop/platforms.py`: type-check and build only | compiled only |
