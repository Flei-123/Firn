# std.clipboard -- the system clipboard for programs without a window

`lib/std/clipboard.fi` (round CLIPBOARD, roadmap r345). A command line tool that wants to put a line on the
clipboard or read it should not have to open a window. It does not: the library keeps ONE hidden 1x1 window
(created at the first call, never shown) and goes through the clipboard code that the window layer already has
(`window.clipboard_set` / `clipboard_get`, [DESKTOP.md](DESKTOP.md)).

```
import std.rt
import std.clipboard

clipboard.copy_text("héllo")                   // true = it is on the clipboard
var out: rt.Buf = rt.buf_new()
if clipboard.paste_text(&out) { /* UTF-8 octets, "\n" line ends, appended to out */ }
rt.buf_free(&out)
clipboard.has_text()   clipboard.clear()
clipboard.copy_png(p, n)   clipboard.paste_png(&out)   clipboard.has_png()     // a PNG file's octets
clipboard.serve(3000)  // X11 only: stay alive for 3 s and answer pasters
clipboard.backend_name()   // "native" | "wl-copy" | "xclip" | "xsel" | "web" | "none"
```

Every function answers `false` when it cannot do the job -- no display, no clipboard in this environment, no PNG
there, data too large. None crashes and none waits longer than a few seconds.

| system | how | notes |
|---|---|---|
| Windows | window layer (`win32.fi`): `CF_UNICODETEXT`, registered format `PNG` | data is copied into the system at once and outlives the program |
| Linux, X11 | window layer (`x11.fi`): the CLIPBOARD selection through the hidden window; up to 64 MiB, INCR above 200,000 octets | the owner must stay alive (below) |
| Linux, helpers | `wl-copy`/`wl-paste` (Wayland), `xclip`, `xsel` | tried FIRST for a copy (they fork into the background and keep the data) |
| browser | `lib/@web/std/clipboard.fi` on `plat.webclip` | write inside a user gesture; reading is asynchronous: `paste_text_request()` then `web.WE_CLIP` |
| Android | `lib/@android/std/clipboard.fi` | `false`; an Android program with a window uses `window.clipboard_*` |

## The Linux choice

X11 has no clipboard storage. The program that copies owns the selection and has to be alive and answering when
somebody pastes. A tool that copies and exits would leave nothing. So `copy_*` hands the data first to an installed
helper (Wayland: `wl-copy`; X11: `xclip`, then `xsel`), and owns the selection itself only when there is none (or the
helper refused). Pasting asks the selection directly -- any owner works -- and uses the helpers only if that fails
(on Wayland `wl-paste` goes first). An empty text is never worth a helper.

`FIRN_CLIPBOARD` forces a choice: `native` (only the window layer), `wl-copy`, `xclip`, `xsel` (only that helper) or
`none`. Without a `DISPLAY` and a `WAYLAND_DISPLAY` nothing is attempted at all (`backend_name()` is `none`) -- the
library never connects to a display it was not told about.

When the window layer owns the data: a program that exits loses it. End with `clipboard.serve(ms)` to answer pasters
for a while, or install a helper.

## What the details are

* Text is UTF-8 with `\n` on every system (Windows converts to UTF-16 and `\r\n`). An empty string is a valid text.
* `copy_*` replaces everything this program offered before: a text copy removes an earlier PNG.
* `paste_*` APPENDS to the buffer and leaves it untouched when it answers `false`.
* `copy_png` takes the octets of a PNG file and refuses anything without the PNG signature; `paste_png` returns exactly
  the file (up to and including `IEND`), without the padding Windows may add.
* `has_text` pastes and drops the text: a full transfer for a large clipboard.

## What is tested, and how

| what | test | status |
|---|---|---|
| backend choice, exact helper arguments, UTF-8, empty, 300 KB, PNG, replace, clear, the fall-through when a display is unreachable, nothing installed | `tests/2370_std_clipboard_helpers.fi` (stand-in scripts for `xclip`, `xsel`, `wl-copy`, `wl-paste`; the program starts itself with a controlled environment) | Linux; not on Windows (no `sh`) |
| round trips through the window layer: empty, UTF-8 1-4 octets, 100 KB, 1 MiB, replace, append, PNG, has/clear | `tests/2371_std_clipboard_native.fi` (starts its own Xvfb; on Windows it runs in place; SKIPs on stderr without Xvfb or a display) | Linux, Wine |
| a second program: GTK 3 reads and writes the clipboard, text, 1.5 MiB (INCR), PNG pixels, `clear` against a foreign owner | `tools/desktop/clipboard_check.py` (section 83 of `test.sh`, steps 9b / 15b of `tools/desktop/run.sh`) | Linux native; Windows build under Wine (Wine's clipboard bridge to the X selections) |
| the Linux, Android and browser files agree and build for five targets | `tools/desktop/clipboard_platforms.py` | build only |

## HONEST

* B1 of the header of `lib/std/clipboard.fi`: without a helper, X11 data lives only while the program runs and answers.
* The helper paths are verified against stand-in scripts, not against the real `wl-clipboard`, `xclip` and `xsel`
  (none was installed on the test machine). The flags used are the documented ones; empty-input behaviour of
  `wl-copy` and `xclip` is not verified, which is why an empty text goes to the window layer first.
* `xclip` cannot clear: `clear()` then needs the window layer (it takes the selection with an empty text and gives it
  up); with `FIRN_CLIPBOARD=xclip` forced, `clear()` is `false`. `xsel` knows no PNG.
* The Windows build was run under Wine only (never on a real Windows machine); Wine without a display cannot make the
  window and the test skips. Wayland itself (`wl-copy`) was not run.
* The browser file is compiled for `wasm32-browser` only; the host side (`lib/plat/webclip.fi`, `demos/webdemo/firn.js`)
  has its own browser test. `paste_*` cannot work synchronously there.
* Found and fixed on the way, in `lib/window/x11.fi`: `fetch_selection` read a reply record that did not exist when the
  owner did not answer (a null read, a crash after a clear + copy in one program), and `clipboard_clear` left its own
  `SelectionClear` notice in the stream, which dropped the next copy's data.
* Not done: more than one image type, `text/html`, a file list (`window.clipboard_set` knows `text/uri-list`), watching
  the clipboard for changes.
