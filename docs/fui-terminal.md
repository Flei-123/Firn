# fUi terminal widget (M10)

Goal: a terminal emulator widget for fUi apps. Same source for X11 now, OrientOS tty and web later.

## Layers

| Layer | Path | UI-free | Job |
|---|---|---|---|
| width | `lib/term/termwidth.fi` | yes | column width of a code point (0, 1, 2) |
| parser | `lib/term/vtparse.fi` | yes | bytes -> actions (UTF-8, ESC, CSI, OSC, DCS skipped) |
| grid | `lib/term/termgrid.fi` | yes | cells, cursor, scroll region, scrollback ring, resize |
| SGR | `lib/term/termsgr.fi` | yes | colours 16 / 256 / truecolor, bold, italic, underline, inverse |
| CSI | `lib/term/termcsi.fi` | yes | cursor, erase, insert/delete, scroll, modes, reports |
| ESC / OSC | `lib/term/termesc.fi` | yes | C0, ESC, charset, save/restore, title |
| terminal | `lib/term/term.fi` | yes | parser + main/alt grid + modes + reply buffer, `feed` |
| selection | `lib/term/termsel.fi` | yes | selection over absolute lines, text extraction |
| input | `lib/term/terminput.fi` | yes | keys, mouse (X10, SGR 1006), bracketed paste -> bytes |
| backend | `lib/term/termback.fi` | yes | backend interface (read, write, resize, alive, close) |
| pty | `lib/term/pty.fi` (+ `lib/@web`, `lib/@windows` stubs) | no | Linux pty by raw syscalls |
| loop | `lib/term/termloop.fi` | yes | scripted backend for tests and web without a shell |
| widget | `lib/fui/termwstate.fi` (state, pump, fit), `termpaint.fi`, `terminteract.fi`, `termwidget.fi` (`tw_attach`), `termax.fi` (a11y) | no | paint, keys, pointer, wheel, selection, clipboard, a11y |
| pty link | `lib/term/termpty.fi` | no | `Backend` over `pty.fi` |

## Parser (vtparse)

- Pull style, no callbacks: `feed(parser, byte)` returns an action kind and leaves the data in the parser struct.
- Kinds: PRINT (cp), EXEC (C0 byte), CSI (marker, params, intermediate, final), ESC (intermediate, final), OSC (string), NONE.
- Params: up to 16, `-1` = default, `:` recorded as sub-parameter separator (needed for `38:2::r:g:b`).
- UTF-8 decoder inside, invalid sequences give U+FFFD. DCS / APC / PM / SOS strings are swallowed.
- OSC ends with BEL or ST; capped at 512 octets, rest dropped.

## Grid

- Cell = 4 x u32: code point, fg, bg, flags. Colour word: `0` default, `0x01000000 | idx` indexed, `0x02000000 | rgb` truecolor.
- Flags: bold, italic, underline, inverse, dim, strike, wide, wide tail, wrapped (line continues).
- Storage: ring of rows (`rows + scrollback`). A full-screen scroll only moves the ring head; partial regions copy rows.
- Alt screen: second grid without scrollback (DECSET 1049 / 1047 / 47), cursor saved and restored.
- Scroll region DECSTBM, origin mode, autowrap with deferred wrap (xterm pending-wrap), tabs, insert mode, charset G0 line drawing.
- Resize: columns truncated / padded per row, rows shrink by pushing top lines into scrollback, grow by pulling them back. No reflow (documented limit).
- Dirty tracking: a generation counter; the widget repaints when it changes.

## Supported sequences (xterm subset)

C0: BEL BS HT LF VT FF CR SO SI. ESC: `7 8 D E M c H =` `>` `(0` `(B`.
CSI: `@ A B C D E F G H I J K L M P S T X Z ` ` d e f g h l m n r s u` plus `?` modes 1 (app cursor), 6, 7, 25, 47, 1000, 1002, 1003, 1006, 1047, 1048, 1049, 2004, and `c` (DA), `n` (DSR 5 / 6), `t` (window report 18).
OSC: 0 / 1 / 2 title. Everything else is ignored, never an error.
Replies (DA, DSR) go to a reply buffer that the widget writes back to the backend.

## Selection and copy

- Anchors are `(absolute line, column)`; absolute line 0 = oldest scrollback line, so scrolling does not move a selection.
- Mouse: press + drag selects, double click selects a word, triple click a line; Shift forces selection while an app has mouse reporting on.
- Copy: Ctrl+Shift+C and Ctrl+Insert not needed; text through `fui.services.clip_copy`. Paste: Ctrl+Shift+V -> `clip_request`, answer arrives as EV_CLIP, written with bracketed paste markers when mode 2004 is on.
- Trailing blanks per line are trimmed; wrapped lines join without a newline.

## Input encoding

- Keys: arrows / Home / End honour application cursor mode (`ESC O x` vs `ESC [ x`), modifiers use `ESC [ 1 ; m x`. Backspace = DEL (0x7f), Enter = CR, Tab, Shift+Tab = `ESC [ Z`, Alt+char = ESC prefix, Ctrl+letter = C0.
- Mouse: modes 1000 / 1002 / 1003 with SGR (1006) encoding, X10 fallback for columns < 223. Wheel without mouse mode scrolls the scrollback; on the alt screen it sends arrow keys (xterm `alternateScroll`).
- Paste: bracketed (`ESC [ 200 ~ ... ESC [ 201 ~`) when mode 2004 is on; control octets other than TAB / LF / CR are removed.

## Backends

`termback.Backend` = fd-less handle with `read`, `write`, `resize`, `alive`, `close` function fields.

| Target | Backend | Decision |
|---|---|---|
| Linux native (X11) | `pty.fi`: `/dev/ptmx`, `TIOCSPTLCK`, `TIOCGPTN`, fork, `setsid`, `TIOCSCTTY`, execve; `TIOCSWINSZ` on resize, SIGHUP on close | done in v1 |
| Tests | `termloop.fi`: scripted bytes in, recorded keys out | done in v1 |
| Web | no local process possible. Decision: **backend = remote shell over WebSocket, behind an app-provided URL**; none configured -> the widget runs on `termloop` (read-only demo text, input echoed locally). The WebSocket backend needs async I/O in the web host (r-item) | documented, not in v1 |
| Windows | ConPTY (`CreatePseudoConsole`), stub returns `NO_BACKEND` | later |
| OrientOS | kernel tty API (`term.fi` today) as a third backend, the widget stays | later, after r41 |

The pty child is `setsid` + controlling tty + `TERM=xterm-256color`, `COLORTERM=truecolor`; size set before exec. Reads are non-blocking; the app pumps through `app.set_pump` (fd readable wakes the window loop).

## Widget (`lib/fui/termwidget.fi`)

- Cell metrics from the mono face (`app.use_mono_face`, slot `face.MONO_SLOT`): width = advance of `M`, height = line height. Grid size = floor(area / cell) and follows resize (backend resize + `term_resize`).
- Paint: per row, runs of equal attributes in one `painter.text`; backgrounds filled per run; cursor (block, hollow when unfocused), selection overlay, scrollback offset indicator; inverse and dim resolved in the widget. Palette: theme-aware 16 colours, xterm cube for 16..255.
- Plug into `fui.app` through `app.custom` + `node_set_draw` + `set_keys` + `set_wheel` + `set_motion` + `set_extra_wake` (clipboard answers) + `set_pump` (pty fd, only when the backend has one; set the backend BEFORE `tw_attach`); a node provides the focus area and the a11y role.
- a11y: new role `ROLE_TERMINAL` = 36, `ROLE_COUNT` 37 (AT-SPI `ATSPI_ROLE_TERMINAL` 60, UIA Document, no web mapping yet); the node text is the visible screen text.

## Tests

| Test | What |
|---|---|
| `tools/fui/term_vt_main.fi` | parser + grid + modes with fed sequences, asserts cells |
| `tools/fui/term_pty_main.fi` | real pty: `echo` through `/bin/sh`, input encoding round trip, exit status, resize |
| `tools/fui/term_widget_main.fi` | widget logic on the scripted backend: keys, replies, selection, copy / paste, wheel, fit, a11y role |
| `examples/fui_apps/terminal` | demo app: pty shell on X11; `--shot` / `--script` use the scripted backend (`term_sample.fi`); golden `tools/fui/golden_term/terminal_dark.png` (run.sh 18p9b) |

## Limits (honest)

- No reflow on resize, no sixel / images, no IME composition, no ligatures, no bidi (cells are LTR).
- Wide characters use a static width table, not the full Unicode East Asian Width.
- Fonts: fixed cell grid, mono face only; emoji use the fallback face if present.
- Scrollback default 2000 lines per terminal (cap 100000).

## Widget limits (v1)

- The host forwards only arrows, Home / End, PgUp / PgDn, Delete, Backspace, Enter, Tab, Esc, F2, F5 as key events: Insert and other F-keys do not reach the terminal yet.
- The pointer hook carries no modifiers: Shift-forces-selection is not possible; with mouse reporting on, selection is off.
- No cursor blink, output while scrolled back keeps the view anchored, wide / box-drawing glyphs depend on the fallback face, underline / strike lines are drawn but faint in the golden.
- Web: `termloop` only (pty stub), WebSocket backend not built.
