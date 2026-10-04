# Packaging a Firn program

`tools/pack/` makes the files a person installs a Firn program with, on every
platform, **without the platform's own packaging tools**: no NSIS, no `dpkg-deb`,
no `appimagetool`/`mksquashfs`, no `hdiutil`, no ImageMagick. The Firn parts
(installer, self-extract stub, icon renderer) are Firn programs; the container
formats (zip, tar, ar, SquashFS, plist, OPKG) are written by a Python 3 script that
uses only the standard library. Where a foreign tool exists on the build machine
it is used to **check** the result (`dpkg-deb`, `unsquashfs`, the AppImage
runtime, `makensis`, `xorriso`, `opk.py`, Wine), never to make it.

```
bash newapp.sh ...            # a program on the appkit (docs/APPKIT.md)
cd myapp && bash package.sh   # = tools/pack/all.sh: every package into dist/<version>/
```

| platform | what you get | how it is checked here |
|---|---|---|
| Windows | `setup.exe` (installer + uninstaller), the program with its icon, a portable zip, an NSIS script | **under Wine**: silent and window install, shortcuts, registry entry, upgrade, uninstall, cancel; not on a real Windows |
| Linux | `.deb`, `.tar.gz` + `install.sh`, a real **AppImage**, a self-extracting `.run`, the raw program | `dpkg-deb`, `dpkg -i` in a Debian container, `unsquashfs`, the AppImage runtime (`--appimage-extract`, extract-and-run), run |
| macOS | `.app` bundle, zip, `.dmg`, `sign-and-notarize.sh` | structure only (script + `plistlib`, `xorriso`); **nothing run on a Mac, nothing signed**; Firn has no macOS target, so the program must be given |
| Android | signed APK with launcher icons | `tools/android/build.sh` (existing); the APK build is run by `tests` when the SDK/NDK is there |
| OrientOS | `.opk` store package | byte for byte against OrientOS's own `opk.py`; **never run on OrientOS** |
| all | `manifest.json` (SHA-256 + Ed25519 per file), `store-add.sh` | signatures verified, a throw-away store takes the commands |

## 1. The one command

```
bash tools/pack/all.sh APP-DIR VERSION [--platforms linux,windows,mac,android,osum|all]
        [--no-build] [--mac-binary F --mac-arch arm64] [--osum-start F] [--runtime F]
        [--sign-key F] [--notes TEXT] [--channel stabil]
```

`APP-DIR` is a project from `tools/newapp.sh`: it has `VERSION`, `pack.ini`,
`assets/icon.svg` and the three build scripts. `all.sh` builds with them (unless
`--no-build`), draws the icons, runs each platform's writers, writes the manifest.
A platform that fails or cannot run here (no mingw, no SDK, no Mach-O program) is
named in the summary, the others still finish, the exit code is 1 if a listed one
failed. The output (`dist/<version>/`):

```
ID-VERSION-linux-x86_64          the raw program        (store art `bin`)
ID-VERSION-linux-x86_64.deb  .tar.gz
ID-VERSION-x86_64.AppImage       real AppImage          (store art `appimage`)
ID-VERSION-x86_64.run            self-extracting        (same, when no runtime is available)
ID-VERSION-windows-x86_64.exe    program + icon         (store art `exe`: what the updater serves)
ID-VERSION-setup.exe             installer
ID-VERSION-windows-portable.zip
nsis/ID.nsi (+ its setup)        the same install for NSIS
ID-VERSION-macos-arm64.zip .dmg, macos/ID.app, sign-and-notarize.sh
ID-VERSION-android.apk           ID-VERSION-osum.opk
icons/                           png/, ID.ico, ID.icns, hicolor/, android/res/
manifest.json                    store-add.sh
```

`pack.sh` is the front door: `pack.sh stubs` (build the Firn parts into `build/pack/`),
`pack.sh icons IN OUT`, `pack.sh all ...`, and any command of `pack.py`
(`win-installer`, `deb`, `appimage`, ... -- `pack.sh --help`). `pack.ini` (key=value):
`id name vendor summary description url license category icon icon_bg android_id depends`.

## 2. Icons: one SVG or PNG, every size (`tools/pack/icons.fi`, `lib/pack/icons.fi`)

An SVG is drawn once per size with `lib/svg` (sharp everywhere); a PNG is resampled
(premultiplied tent filter: no halo, a flat colour stays flat). Written: `png/ID-<n>.png`
(16 .. 1024), `ID.ico` (16, 24, 32, 48, 64, 128, 256 as **PNG entries**, Vista and later),
`ID.icns` (`icp4..icp6`, `ic07..ic14` as PNG, the @2x forms included), `hicolor/<n>x<n>/apps/ID.png`
(+ `scalable/apps/ID.svg`), Android `mipmap-*dpi/ic_launcher{,_round,_foreground}.png`,
`mipmap-anydpi-v26/ic_launcher.xml`, the background colour (`icon_bg`) and `playstore-512.png`.
Not square: centred on a transparent square. A PNG source below 512 px is enlarged (it says so).

The Windows program gets the icon **inside the exe**: `pack.py pe-icon` adds an `.rsrc`
section with `RT_GROUP_ICON` + `RT_ICON` (Firn's PE files have none). Firn leaves a COFF symbol table after the last
section; it is dropped so the section can follow (a stripped program runs the same).
That patched exe is the one `all.sh` puts into the installer **and** offers the store, so the hash the
updater compares is the hash of what is installed.

## 3. Windows

### 3.1 The installer (`tools/pack/installer/`, `lib/pack/install.fi`)

One Firn program is installer **and** uninstaller. `pack.py win-installer` appends a payload to it:

```
[ installer program ][ payload.zip ][ trailer, 64 octets ]
trailer: "FIRNPAK1" | payload offset u64 | payload length u64 | SHA-256 of the payload | 8 zero octets
payload: the program's files + `.pack/info`  (id= name= version= vendor= exe= icon= url= desktop= launch=)
```

At start it finds the trailer, checks lengths and SHA-256 (a truncated or damaged download never installs half a
program), judges **every name** with `std.safefs` (no `..`, no drive letter, no `CON`/`NUL`, no duplicates, no
links) before it writes a byte, then:

1. writes the files into the install folder, one payload entry per step (a window draws a progress bar between the steps);
   default `%LOCALAPPDATA%\Programs\<name>` -- **no administrator**, and the folder is the user's, so
   appkit's rename-based self-update works there unchanged;
2. removes files an older install of the same program had and this one does not (upgrade in place; the user's own files stay);
3. writes `uninstall.exe` (= the installer without payload and trailer) and `.pack/install.txt`
   (`f <file>`, `d <folder>`, `s <shortcut>` per line -- the uninstaller removes exactly these);
4. writes the Start Menu shortcut and, if ticked, the Desktop shortcut (see 3.2) -- folders from
   `HKCU\...\Explorer\Shell Folders` (OneDrive-redirected desktops included), environment as fallback;
5. writes `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\<id>` (**"Apps & features"**): `DisplayName`,
   `DisplayVersion`, `Publisher`, `InstallLocation`, `DisplayIcon`, `UninstallString` (`"<dir>\uninstall.exe" /uninstall`),
   `QuietUninstallString` (`... /S`), `URLInfoAbout`, `InstallDate`, `EstimatedSize`, `NoModify`, `NoRepair`.
   The registry functions (`RegCreateKeyExW`, `RegSetValueExW`, `RegCloseKey`, `RegDeleteTreeW`, `RegOpenKeyExW`, `SHChangeNotify`)
   are bound in `compiler/src/win.rs`; `lib/pack/winreg.fi` (twin `winreg.windows.fi`) wraps them.

Command line: `/S` (silent), `/D=<folder>` or `--dir`, `--no-desktop --no-startmenu --no-launch --no-registry`,
`--log FILE`, `--extract DIR` (unpack only), `--info`; `uninstall.exe /uninstall [/S]`. Exit 0 done, 2 no/damaged payload, 3 install failed, 4 uninstall failed.

**The window** (`ui.fi`, fUi): folder field, two check boxes, Install / Cancel; progress; done page with "Start <name>";
the uninstaller asks "Remove <name> from this computer?". English and German (the system language).

**Uninstall.** A running program cannot delete itself on Windows. `uninstall.exe /uninstall` copies itself to `%TEMP%` and
starts the copy (`/uninstall-stage2 <dir>`); the copy removes the listed files, the shortcuts and the registry key, then
writes a small batch file that waits (up to two minutes, until the window of stage 1 is closed) and deletes the copy,
`uninstall.exe`, the folder (only if empty) and itself. A folder that still holds a file of the user's stays.

**After an appkit update** the program calls `installed.sync_version(id, version)` (`lib/pack/installed.fi`, the template
does it once the new version is healthy) so "Apps & features" shows the version that runs. It does nothing for a program that was
not installed by this installer.

### 3.2 Shortcuts without COM (`lib/pack/lnk.fi`)

The Shell Link format is documented ([MS-SHLLINK]); the writer makes: header, a `LinkTargetIDList` (My Computer, drive, one item per
path component -- only when the whole path is ASCII), `LinkInfo` (fixed drive, local base path in ANSI **and** UTF-16),
Unicode `StringData` (name, working directory, arguments, icon), the terminal block. Link tracking is off. `lnk_read` reads
it back; `tests/2200` round-trips it, `tools/pack/test/lnkread.py` is a second, independent reader, and **Wine's shell starts the program through
the shortcut with the working directory honoured**. Not seen: Explorer on a real Windows.

### 3.3 The other Windows packages

* **portable zip** `ID-VERSION-windows-portable.zip`: the program, its icon and `portable.txt`. With that file next to the
  program appkit keeps everything in `<program folder>/data/{config,data,state,cache,logs}/<app>` (`plat_dir`; delete the file
  to use the profile). Works on Linux too (next to the AppImage).
* **NSIS** `nsis/ID.nsi`: a 64-bit, per-user script that does the same install (Start Menu, Desktop, the Uninstall key,
  uninstaller); built by `makensis` when it is installed, and run under Wine in the tests. This is the "if you want the standard tool" path.
* **MSI** is not made. The installer above is the MSI-free variant.

### 3.4 fUi windows on Windows (what had to be fixed for the installer's window)

The appkit template's window had never opened on Windows: `lib/@linux/window/backend.fi` (X11) and the font path
`/usr/share/fonts/...` were chosen for the Windows target too, so under Wine it died with "no X server reachable". Now
`target::platform_dir()` is `@windows` for `--target=x86_64-windows`; `lib/@windows/` holds the Win32 backend link
and the (shared) `apphost`, and `lib/plat/sysfont.fi` also tries `C:/Windows/Fonts/segoeui.ttf`, `arial.ttf`, `tahoma.ttf`.
Result under Wine on Xvfb: the template's `--selftest` opens its window and draws 30 frames; the installer's window is drawn and clicked.
Still never seen on a real Windows (DPI, Segoe UI metrics, the title bar).

## 4. Linux

* **`.deb`** (`linux.py`): `ar` with `debian-binary`, `control.tar.gz` (control, md5sums, postinst/postrm that refresh the desktop and
  icon caches when the tools exist) and `data.tar.xz` (or `--compress gz`); root:root, fixed times, so the same inputs give the same bytes.
  `Depends:` from `pack.ini` (the X11 window loads DejaVu Sans: `fonts-dejavu-core | fonts-dejavu`). `.desktop` file + icons in the hicolor theme.
  Architecture from the ELF header (`amd64`, `arm64`).
* **tar.gz**: `bin/`, `share/`, `install.sh [--prefix DIR]` (default `~/.local`, patches `Exec=`, refreshes caches), `uninstall.sh`.
* **AppImage** (type 2): the official runtime (downloaded once to `~/.cache/firn-pack/`, or `--runtime FILE` / `$APPIMAGE_RUNTIME`)
  followed by a **SquashFS 4.0 image written by `packlib/squashfs.py`** (directories, files, symlinks, gzip blocks, no fragments/xattrs;
  read by `unsquashfs`, the kernel and the runtime). The AppDir has `AppRun`, `ID.desktop`, `ID.png`, `.DirIcon`, `usr/bin/ID`,
  `usr/share/...`. Needs FUSE on the target to mount; `--appimage-extract` and `APPIMAGE_EXTRACT_AND_RUN=1` do not.
* **Self-extracting `.run`** (`tools/pack/stub/selfx.fi`): the same trailer and payload format as the Windows installer, around the same AppDir.
  It needs nothing but the kernel: unpacks once to `$XDG_CACHE_HOME/firn-run/<id>-<12 hex of the payload hash>/` (temp name + rename), then
  `execve`s the program with `APPIMAGE=<this file>`, `APPDIR`, `ARGV0` -- so appkit replaces *this file* when it updates
  (`plat_art()` is `appimage`, `plat_exe_path()` is `$APPIMAGE`) and the next start finds the new payload under a new folder.
  `--appimage-extract [DIR]`, `--appimage-info`. A damaged file is refused before anything is unpacked.

## 5. macOS (`packlib/mac.py`) -- structure, not proof

`Name.app/Contents/{Info.plist, PkgInfo, MacOS/<exe>, Resources/<id>.icns}` (`plistlib`; identifier `com.<vendor>.<id>`, minimum system 11.0,
`NSHighResolutionCapable`), a zip that keeps the executable bit, a `.dmg` made by `xorriso` (ISO9660/HFS+ image holding the bundle and an
`/Applications` link -- no `hdiutil`, no compression). `check_app` verifies the shape. The program has to be a **Mach-O** file (magic checked):
Firn has no macOS target yet, so `--allow-any` exists only to test the layout.
`sign-and-notarize.sh` is *generated, never run*: `codesign --options runtime --timestamp --entitlements`, `notarytool submit --wait`,
`stapler staple`, `spctl --assess`, `hdiutil create -format UDZO`. It needs `MACOS_SIGN_IDENTITY` and `NOTARY_PROFILE`.
An unsigned bundle on Apple Silicon starts only after "Open Anyway".

## 6. Android (`tools/pack/android.sh`)

Wraps `tools/android/build.sh` (NDK link, `aapt2`, `zipalign`, `apksigner`): name, package, version code (`major*10000 + minor*100 + patch`)
from `pack.ini` + `VERSION`, the appkit install receiver and permissions, and the launcher icons (`--icon-res`: compiled with `aapt2`,
`android:icon` and `roundIcon` in the manifest; adaptive icon with the `icon_bg` colour). Signing key: `~/.firn/android.keystore` -- **keep it**.

## 7. OrientOS (`packlib/osum.py`)

`.opk` = `OPKG0001`, lengths, SHA-256 over metadata + data, metadata (`name fassung titel info keys`, `braucht=`, `handle=`, `arch=`), and the
deterministic archive (`d`/`f`, mode, name, length, content, sorted by name octets; mode 755 for `start`, 644 otherwise). Contents: `start` (an ELF
for OrientOS -- machine from the ELF header), `INFO`, `symbol` (OSYM, 32x32 BGRA from the icon), `data/...`. Checked byte for byte against
`pkg/opk.py bauen` of the OrientOS tree and read by the store's `opkleser.py`. Never installed on OrientOS. Reserved names (`kernel`, `app`, ...) are refused.

## 8. The manifest and the signature (`packlib/manifest.py`)

`manifest.json` has one entry per artifact: `art`, `platform`, `file`, `size`, `sha256`, `store` (does the catalog take it), and
`signature`: **Ed25519 over the text** `pack1\n<id>\n<version>\n<art>\n<platform>\n<sha256>\n<size>\n` -- a signature cannot be moved to another version, platform or file. The key is a file named by `--sign-key`,
`$PACK_SIGN_KEY` or the store's `$ORIENTSTORE_SCHLUESSEL` (32 raw octets or 64 hex digits); it is read, never copied anywhere, never stored in a project.
The public key is in the manifest; `pack.py verify --manifest F [--key-expected HEX]` checks every signature **and** every file's hash. Without a key the manifest says
`unsigned` and still has the hashes.

`store-add.sh` holds the `store add-app` / `store add` lines for the store artifacts. **It is never run by the tools**: the live store
(store.fleitec.com) is touched only by a person who read it. The tests run it against a throw-away repository.
This is a second layer next to the catalog signature `store add-app` makes -- it also covers files that are in no catalog (installers, packages).

## 9. Tests

| | what |
|---|---|
| `tests/2200_pack_lnk.fi` | the shortcut writer and reader: layout, ID list, non-ASCII path, refusals |
| `tests/2201_pack_icons.fi` | PNG encoder round trip, resampler, `.ico`, `.icns` |
| `tests/2202_pack_payload.fi` | trailer, SHA-256, truncation / damage / lying lengths, `.pack/info` |
| `tests/2203_pack_install.fi` | install, list, upgrade, uninstall, user files, `..` entry, no program, damaged payload |
| `tools/pack/test/checks.py` | every writer against an independent reader (dpkg-deb, `dpkg -i` in a container, unsquashfs, the AppImage runtime, opk.py, makensis, xorriso, own parsers): ~165 checks |
| `tools/pack/test/windows.sh` | under Wine: installer, shortcuts (own reader **and** Wine's shell), registry, upgrade, `sync_version`, uninstall (+ leftovers), the windows (xdotool), NSIS setup, portable zip, the template window |

`test.sh` section 77 runs the unit tests (via the normal list) and `tools/pack/test/run.sh`; `PACK_WINE=1` adds the Wine run.

## 10. Honest list of what is not done / not seen

* **No real Windows**: FLEI-ONE is online but cannot reach this server and the helper writes text only, so no exe can be put on it. Everything
  Windows is Wine. Not seen: Explorer showing the shortcut icon, SmartScreen (the files are **not Authenticode-signed**), Defender, High-DPI, per-machine installs.
* **No macOS** (above), **no OrientOS** run, **no RPM** (roadmap), **no MSI**, no Authenticode signing (needs a certificate -- `signtool` on the exe/setup would be one more step), no Android arm64 run (emulator is x86_64).
* A **real AppImage needs a runtime** from the network once (cached); offline, `all.sh` serves the self-extracting `.run` under the `appimage` art instead and says so.
* The `.dmg` is an ISO/HFS+ image, uncompressed and unverified on macOS.
* The installer is per-user only. It does not close a running copy of the program (the write fails with the file's name instead).
* Roadmap entries cover RPM, Authenticode, an installer language list beyond en/de, delta updates for installers.
