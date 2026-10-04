# fUi quickstart -- a window in ten lines

`lib/fui/app.fi` is the high-level layer of fUi. You describe the window --
labels, buttons, text fields, rows and columns -- and hand it to `app.run`.
The platform, the event loop, the font, the theme, focus, keyboard and
display scaling are handled for you. The **same source file** becomes an
X11 window natively and a page in the browser with `--target=wasm32-browser`.

## Hello Window

Python's tkinter:

```python
import tkinter as tk

window = tk.Tk()
window.title("Test Window")
window.geometry("400x250")
label = tk.Label(window, text="Hello, this is a test window!", font=("Arial", 16))
label.pack(pady=20)
button = tk.Button(window, text="Close", command=window.destroy)
button.pack()
window.mainloop()
```

Firn with fUi (`examples/fui/hello_window.fi`):

```firn
import fui.app

fn close(a: *mut app.App) {
    app.quit(a)
}

fn main() -> i32 {
    let a: *mut app.App = app.window("Test Window", 400, 250)
    app.label(a, "Hello, this is a test window!", 16)
    app.button(a, "Close", close)
    return app.run(a)
}
```

Build and run it:

```sh
export FIRNLIB=$PWD/lib
compiler/target/release/firnc -o hello examples/fui/hello_window.fi
./hello                                   # an X11 window ($DISPLAY)

compiler/target/release/firnc --target=wasm32-browser \
    -o demos/webapp/hello_window.wasm examples/fui/hello_window.fi
python3 -m http.server -d demos/webapp 8000
# http://localhost:8000/?wasm=hello_window.wasm&w=400&h=250
```

### How many lines?

Counted as lines that are neither empty nor a comment:

| Version | Lines | Extra files |
|---|---:|---|
| Python / tkinter (the program above) | 9 | -- |
| Firn / `fui.app` (`examples/fui/hello_window.fi`) | **10** (12 with blank lines) | -- |
| Firn / fUi before `fui.app` (`examples/fui/lowlevel/hello_lowlevel.fi`) | 81 | symlink `window/backend.fi -> lib/window/x11.fi` |

Where the lines go: Firn has no closures, so Close gets a named function
(`close`, three lines with its braces) instead of tkinter's
`command=window.destroy`, and the program lives in `fn main` (two lines).
Title and size are one call instead of three.

The low-level version is kept on purpose: it shows what `fui.app` hides
(a font set, a render context, the host that turns window events into
focus and clicks, a stylesheet rule for one font size, the event loop) and
it still works if you need that control.

## Counter

`examples/fui/counter.fi` -- a number and two buttons side by side:

```firn
import fui.app

static mut SHOWN: usize = 0

fn add(a: *mut app.App, d: i64) {
    app.set_number(a, SHOWN, app.number(a, SHOWN) + d)
}

fn plus(a: *mut app.App) {
    add(a, 1)
}

fn minus(a: *mut app.App) {
    add(a, -1)
}

fn main() -> i32 {
    let a: *mut app.App = app.window("Counter", 300, 200)
    SHOWN = app.label(a, "0", 32)
    app.row(a)
    app.button(a, "  -  ", minus)
    app.button(a, "  +  ", plus)
    app.end(a)
    return app.run(a)
}
```

## A form with text fields

`examples/fui/form.fi` -- two text fields, Send and Clear, and an answer
line (it wraps when it is wider than the window):

```firn
fn send(a: *mut app.App) {
    app.set_text(a, OUT, "Thanks, ")
    app.append_text(a, OUT, app.text(a, NAME))
    ...
}

fn main() -> i32 {
    let a: *mut app.App = app.window("Sign up", 420, 300)
    app.label(a, "Sign up for the newsletter", 14)
    NAME = app.entry(a, "Your name")
    MAIL = app.entry(a, "you@example.com")
    app.row(a)
    app.button(a, "Send", send)
    app.button(a, "Clear", clear)
    app.end(a)
    OUT = app.label(a, " ", 0)
    return app.run(a)
}
```

Click or Tab into a field and type; Backspace, Delete, the arrows,
Home/End, Shift+arrows (select), Ctrl+A/C/X/V/Z work (lib/fui/editor.fi).

## Several lines: a text area

`examples/fui/notes.fi` -- `app.textarea(a, hint, rows)` is a field that
holds several lines, wraps at its box and scrolls:

```firn
fn main() -> i32 {
    let a: *mut app.App = app.window("Notes", 420, 360)
    BOX = app.textarea(a, "Notes", 6)
    app.row(a)
    app.button(a, "Count", count)
    app.button(a, "Clear", clear)
    app.end(a)
    INFO = app.label(a, " ", 0)
    return app.run(a)
}
```

Enter is a line break (Ctrl+Enter does nothing here), Up / Down keep their
column, Home / End mean the visual line (Ctrl: the whole text), PageUp /
PageDown move a page, the wheel scrolls. A click puts the caret there, a
drag selects, a double click selects the word, a triple click the paragraph.
`app.text(a, id)` returns the whole text with LF bytes; 4096 bytes at most.
`app.lines_of(a, id)` gives the line layout (`fui.textarea`) for programs
that want the line count or the scroll position.


## A list of rows with symbols

`examples/fui/files.fi` -- `app.list(a)` opens a list, `app.list_item(a, icon,
text, detail, badge, on_pick)` adds a row with a Lucide symbol on the left and
an optional detail and badge on the right; a click chooses the row and runs
its handler. The full API (also for scene-tree programs) is in
`docs/fui-list-rows.md`.


## Beyond `fui.app`: a launcher-style window

`fui.app` builds a window out of labels, buttons and fields. For a program whose
whole window is custom -- a sidebar, tabs, a grid of cards, toasts, a dialog
with a focus trap, a Markdown page -- there is the immediate-mode kit
(`lib/fui/kit.fi`, `docs/fui-kit.md`) and the Markdown view
(`lib/fui/markdownview.fi`, `docs/MARKDOWN.md`); `examples/fui/launcher_kit.fi`
is a complete window using all of it, on the same `fui.apphost` (native and
browser).

## The API

| Call | What it does |
|---|---|
| `app.window(title, w, h) -> *mut App` | a window of `w` x `h` points (pixels at 96 dpi) |
| `app.label(a, text, size) -> id` | a line of text; `size` in points like tkinter (16 pt = 21 px at 96 dpi), 0 = theme default |
| `app.button(a, text, on_click) -> id` | a push button; `on_click: fn(*mut app.App)` |
| `app.entry(a, hint) -> id` | a one-line text field with a dimmed hint while empty |
| `app.textarea(a, hint, rows) -> id` | a text area: several lines, wrapping, scrolling, 4096 bytes |
| `app.row(a)` / `app.column(a)` / `app.end(a)` | open / close a nested container |
| `app.spacer(a, w, h) -> id` | empty room |
| `app.set_text(a, id, text)` / `app.append_text(a, id, text)` | change a label, button or field |
| `app.text(a, id) -> str` / `app.number(a, id) -> i64` | read it back |
| `app.set_number(a, id, n)` | a whole number as text |
| `app.focus(a) -> id` | who has the keyboard (`app.NONE` = nobody) |
| `app.set_dark(a, dark)` | force light/dark instead of the system's choice |
| `app.set_data(a, p)` / `app.data(a)` | your own pointer for the callbacks |
| `app.quit(a)` | close the window (in the browser: the page goes blank) |
| `app.run(a) -> i32` | show it and run; returns the exit code for `main` |

The window is a centred column (tkinter's `pack()` default). Every add goes
into the innermost open row/column.

Keyboard, as in any desktop toolkit: Tab / Shift+Tab move the focus, Enter
and the space bar press the focused button. Escape does **not** close the
window.

## Touch and gestures

`app.on(a, node, kinds, handler)` lets a handler hear what happens on a
node or below it. Every pointer has an id, a type (mouse, touch, pen) and
`primary`; a browser's `pointerId` and every Android finger arrive as such.
Taps, double taps, long presses, pans (with fling) and pinches (scale and
turn) are told apart by an arena -- exactly one wins:

```
fn heard(ud: u64, e: *mut event.Ev) -> u32 {
    if event.ev_kind(e) == event.EV_PINCH_MOVE {
        ... event.ev_scale(e), event.ev_angle(e) ...
    }
    return event.H_GO
}
...
app.on(a, app.root(a), event.EV_PINCH_ALL | event.ev_bit(event.EV_TAP), heard)
```

`examples/fui/touchpad.fi` is a complete one (`--log` prints every event).

## The accessibility audit

`FUI_AUDIT=1 ./program` opens no window; it runs `lib/fui/audit.fi` on the
tree the program built and exits with 0 (every operable node named, no key
twice under one parent, no secret in the export) or 1. A text field is named
by its hint. `sh tools/fui/audit.sh` runs every program of `examples/fui/`.

## How the platform is chosen

`lib/fui/app.fi` imports `fui.apphost`. There is no `lib/fui/apphost.fi`:
the compiler looks into a **platform directory** of the library first
(`compiler/src/target.rs`, `platform_dir`):

| Build | Platform directory | Host |
|---|---|---|
| native Linux (default) | `lib/@linux/` | `lib/@linux/fui/apphost.fi` -- an X11 window through `lib/plat/fuiwin.fi` |
| `--target=wasm32-browser` | `lib/@web/` | `lib/@web/fui/apphost.fi` -- a `<canvas>` through `lib/plat/web.fi` |
| `--target=*-android` | `lib/@android/` | `lib/@android/fui/apphost.fi` -- the Linux host again, through `lib/window/android.fi` (build with `tools/android/build.sh`; the on-screen keyboard types into the focused text field, pause / resume / rotation / screen off are checked on an emulator: `tools/android/keyboard_check.sh`, `tools/android/lifecycle_check.sh`) |

`lib/@linux/window/backend.fi` links `lib/window/x11.fi`, so any native
program that imports `window.window` gets X11 without a symlink of its own.
A `window/backend.fi` next to the program still wins (search step 2).

## Limits today

* 128 nodes per window, 8 text fields, 256 octets per text (a text area: 4096).
* One window per program (several windows: `lib/plat/fuiwin.fi`
  `window_step` / `window_wait_many`).
* A click into a text field puts the caret at the click; a drag selects. (Shift+click
  extends nothing yet: the host hook carries no modifier keys.)
* A text area counts against the same 8 text-field slots; it has no tabs and no
  bidirectional cursor movement (lines are painted bidi, moved logically).
* No Windows host yet (fUi roadmap). The Android one has been run on an emulator only, not on a real phone.
* `firnc1` (the self-hosted compiler) knows the platform directory but
  cannot build fUi programs yet -- use `firnc`.

## Proof

* `tools/fui/app_main.fi` (section 18l of `tools/fui/run.sh`): the
  examples driven without a window -- layout and centring, clicks, Tab,
  space, typing, Backspace, Send/Clear, wrapping, scale 2.
* `bash tools/wasm/appdemo.sh`: the four modules in headless Chromium (the
  touch pad with several real fingers: `tools/wasm/touchcheck.py`);
  the first picture of each is the native picture **pixel for pixel**, and
  the pages are operated with the browser's own mouse and key events.
