# Application libraries (OpenPlan LIB-001 … LIB-010)

OpenPlan, the electrical CAD built on Firn, listed the libraries it cannot
do without (`LIBRARY_REQUESTS.md` in the OpenPlan repository). This page
says what exists, where it lives, and what proves it. Every library has a
positive test in `tests/` (run in every build level and on AArch64 by
`test.sh`) and is held against an implementation nobody here wrote by
`tools/libmvp/run.sh` (section 65 of `test.sh`).

| request | module | what it does | held against |
|---|---|---|---|
| LIB-001 | `std.fs` (`lib/std/fs.fi`) | `mkdir_all`, `rename` (atomic), `remove`, `remove_tree` (does not follow links), `symlink`, `stat`/`lstat` (statx, one layout on every machine), `fsync_file`/`fsync_dir`, whole-file read/write/append, `write_atomic` (temp file, fsync, rename, fsync of the directory), path helpers; every failure is an `FsError` | the kernel's own answers (`tests/1910_std_fs.fi`) |
| LIB-002 | `std.time` (`lib/std/time.fi`) | wall and monotonic clock, the proleptic Gregorian calendar (Hinnant's day counts), ISO 8601 out and in, weekday/day of year, UTC offsets from TZif files v1–4 including the POSIX rule in the footer | Python `datetime`/`zoneinfo`: 20,000 instants × 5 zones |
| LIB-003 | `lib/pdf` (`pdf.fi`, `ttfsub.fi`) | PDF 1.7 writer in millimetres from the top left: paths, colours, dashes, clipping, TrueType text as CIDFontType2 with a subset font and ToUnicode, RGBA images with soft mask, internal and URI links, bookmarks, optional content (layers); deterministic output | poppler (`pdfinfo`, `pdffonts`, `pdftotext`, `pdftoppm` pixels), `pypdf` in strict mode, fontTools |
| LIB-004 | `lib/zip` (`zip.fi`) | ZIP reader and writer with store/DEFLATE, CRC-32, ZIP64, UTF-8 names; refuses zip slip, duplicate names, overlapping entries, size bombs (`inflate_limited` in `std.deflate`) | Python `zipfile` and Info-ZIP `unzip`/`zip`, both directions, plus hostile archives |
| LIB-005 | `fuishell.dock`, `fuishell.filechooser` (`lib/fuishell/`), `window.wait_any`, `fuiwin.fenster_step`/`fenster_wait_many` | **done**: the dock model (areas left/right/bottom/center with tabs, splitter and tab dragging, layout saved/loaded as JSON), `Split`, the file chooser model (listing, sort, glob filter, open/save/directory), and several top-level windows in one program. The dock is also drawn (`fuishell.dockpaint`: tab strips, active tab with accent line, ellipsized titles, splitters, the drop hint while a tab is dragged). The file chooser has its face and hands too (`fuishell.chooserview`: path bar with an up button, the scrolling list with directory/file marks and a size column, name field of a save dialog, Cancel/OK; mouse with double click, wheel, keys, typing). **Done** apart from wiring it into OpenPlan itself | `tests/1918`, `tests/1919`, `tests/1920`, `tests/1921` (pixels read back); two windows on an Xvfb driven by python-xlib |
| LIB-006 | `window.clipboard_*` (`lib/window/x11.fi`) | the system clipboard on X11 (CLIPBOARD selection, owner and requestor, TARGETS, any MIME type); `fuiwin.fenster_clip_read/write` for fUi text fields. Windows follows with the Windows target (LIB-016); other backends answer false and fUi keeps its internal clipboard; large contents (up to 64 MiB) go as INCR transfers in both directions | Tk and python-xlib on an Xvfb |
| LIB-007 | `lib/i18n` (`i18n.fi`) | `.opmsg` catalogs, fallback chain, variant selection, arguments, CLDR plural rules, number and date formatting for en, de, fr, it, es, pl, cs, ru; pseudo-localization (en-XA) | ICU 72 (PyICU): 8,885 cases |
| LIB-008 | `lib/regex` (`regex.fi`) | RE2-style regular expressions in linear time (Pike VM): classes, groups, named groups, lazy quantifiers, anchors, flags, replacement with `$1`/`${name}`; refuses backreferences and lookaround | Python `re` on 20,000 random patterns |
| LIB-009 | `lib/print` (`print.fi`) | IPP client over `net.http`: CUPS-Get-Printers/-Default, Get-Printer-Attributes, Print-Job, Get-Job-Attributes, Cancel-Job | CUPS' `ippeveprinter` (a real IPP Everywhere printer) and a stand-in for the CUPS scheduler |
| LIB-010 | `lib/jpeg` (`jpeg.fi`) | baseline and progressive JPEG to RGBA, any subsampling, restart markers, grey/YCbCr/RGB/CMYK, EXIF orientation; libjpeg's islow IDCT, fancy upsampling and colour tables | Pillow (libjpeg-turbo): the same RGBA octets for 46 files |

The compiler learned three more system calls for AArch64 on the way
(`unlinkat`, `symlinkat`, `statx`; `compiler/src/syscalls.rs`), and
`lib/svg/` became MPL-2.0 (LICENSING.md), which OpenPlan (GPL-3.0) needed
to use fUi.
