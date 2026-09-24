# LICENSING.md -- one licence for the whole repository

Decision of 5 September 2026. Justin (Flei-123) is the sole author of Firn,
so the change needed nobody else's agreement.

**Everything in this repository is MPL-2.0** (`LICENSE`): the compiler
(`compiler/`, `lib/firnc1/`, `bin/`), the runtime and standard library
(`lib/`), the tools and the tests. There is no directory-by-directory split
any more.

## History

* Until 27 August 2026 the repository was MIT (`LICENSES/MIT.old.txt`, kept
  verbatim).
* From 27 August to 5 September 2026 it was split: GPL-2.0-only for the
  compiler and the browser engine, MIT for runtime and standard library.
  That state is archived, with its full history, in the private repository
  `FirnOld`. The browser engine now lives in its own repository (Certus).
* Since 5 September 2026: MPL-2.0 for everything, published as a fresh
  history in `Flei-123/Firn`.
* 23 September 2026: the window layer `lib/window/` (`window.fi`, `x11.fi`,
  `win32.fi`, `android.fi`, `osum.fi`) came over from Certus
  (`lib/window/`, Certus commit `1f1ec17bdb68`, last changed in
  `cbbc9d8c0475`), where it was GPL-2.0-only. `git log -- lib/window/` in
  Certus shows 22 commits by Justin and one by JARVIS, his own tool, and no
  other author; `x11.fi` speaks the X11 protocol itself, without Xlib or
  xcb. As sole author Justin released these files for Firn under MPL-2.0 on
  23 September 2026 -- the same step as the licence change of 5 September.
  The first commit of `lib/window/` is the unchanged copy (only the SPDX
  line and a note of origin differ); every later change is its own commit.
  Since MPL-2.0 is GPL-compatible, Certus can import the layer from Firn
  instead of keeping a copy of its own.

* 24 September 2026: `lib/fui/lucide.fi` carries path data of the Lucide
  icon set (ISC; the icons Lucide took from Feather are MIT). It is the one
  file under a licence of its own; its header says so, the texts stand in
  `LICENSES/Lucide-ISC.txt`, and `THIRD_PARTY.md` lists it. Both licences
  are permissive and ask only for the notice to travel with the data.

## Why MPL-2.0

A language is only useful if programs written in it may be closed. The MPL
is file-based: your program files are yours under any licence; the Firn
files stay open, and changes to them must be published when passed on.
That gives users the freedom of MIT for their own code and gives the
language the protection MIT never had. MPL-2.0 is compatible with the GPL,
so Firn may be used inside GPL programs such as Osum.

## Third-party material

See `THIRD_PARTY.md`. Every source file carries an `SPDX-License-Identifier:`
line, which is the authoritative answer for that file.
