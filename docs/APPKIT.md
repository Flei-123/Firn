# appkit -- the application kit

A new Firn program should not have to solve updates, settings, logs, crash
reports, a second-instance check and translated texts again. `lib/appkit/` is
that code, and `templates/app/` + `tools/newapp.sh` turn it into a runnable
program in one command. Update source is the **own signed store**
(`https://store.fleitec.com`, the orientstore of this tree), not a
third-party service.

```
bash tools/newapp.sh FleiLauncher fleilauncher --android-id de.fleitec.fleilauncher
cd fleilauncher && bash build.sh && build/fleilauncher
```

| platform | status |
|---|---|
| Linux x86-64 | **tested**: unit tests, `tools/appkit/e2e.sh` (60 checks), the generated program built and started, a dry-run release |
| Windows x86-64 | **tested under Wine** (`E2E_TARGET=windows`, same 60 checks, worker process instead of a thread); **not on a real Windows machine**: FLEI-ONE is online but cannot reach this server and the helper can only write text files, so no executable can be put on it (the same gap as roadmap r211) |
| Android x86-64 | **tested on the emulator** (`tools/appkit/android_check.sh`, API 35): download, hash refusal, hand-over to the PackageInstaller, the question and its answer, replacement, refusal of a foreign key |
| Android arm64 | compiles and links (APK builds, receiver symbol exported); **not run** (no arm64 emulator or phone here) |
| Linux aarch64 | compiles; not run |
| macOS | **untested stub** (`platform_macos.fi`): Firn has no macOS target yet |
| OrientOS | **untested stub** (`platform_osum.fi`): written against the OrientOS tree, never run on it |

Details per platform and the list of what is missing: [section 7](#7-platform-layer).

## 1. What is in it

| module (`lib/appkit/`) | what it does |
|---|---|
| `appinfo` | who the program is: id, name, vendor, version, build, channel, store address and the store's Ed25519 key (the compiled-in trust anchor) |
| `update` | the updater: check, download, verify, replace, confirm, roll back; background worker; the status the UI reads |
| `catalog` | the store's `entry.json` and catalog: signature, freshness, hashes, choosing the build of this platform, comparing versions |
| `fetch` | one streamed HTTP(S) GET with SHA-256 on the way, progress and cancel (names through `net.dns`, https through `tls.trust`) |
| `version` | semantic versions (`1.2.3`, `1.2.3-beta.1`) and their order |
| `config` | settings in one small JSON file, atomic save |
| `log` | a log file with levels and rotation |
| `crash` | crash reports written by the next start (run marker + log tail + stderr) |
| `single_instance` | one copy per user, a lock the kernel drops when the process dies |
| `texts` | translated texts on `lib/i18n` (`.opmsg`), English and German built in |
| `platform*.fi` | the one platform interface (section 7) |
| `util` | octet strings, hex, argument lists |
| `std.crypto.ed25519` (`lib/std/crypto/ed25519.fi`) | Ed25519 verify/sign (RFC 8032), tested with the RFC vectors and a real store entry |

The code is documented in the files' own headers; this page is the map.

## 2. The program

`appinfo_init` once, at the top of `main`:

```firn
appinfo.appinfo_init(appinfo.AppSpec {
    id: "fleilauncher", name: "FleiLauncher", vendor: "FleiTec",
    version: "0.5.5", build: 0, channel: "stabil",
    store_url: "https://store.fleitec.com/",
    store_key: "33f41a31...",            // 64 hex digits, compiled in
    android_id: "de.fleitec.fleilauncher" })
```

Then (the template's `main.fi` does exactly this):

```
plat_init(start) -> appinfo_init -> update_new/update_init -> update_boot
 -> log_open -> crash_begin -> single_acquire -> texts -> the window
 -> crash_end, single_release
```

* `update_boot` is the start-up bookkeeping of the update: it runs the worker
  mode (`--appkit-worker`, the program started again as a worker, which never
  returns), counts the failed starts of a freshly updated version (after two
  it puts the old program back itself), notes a start after a rollback and
  removes the backup once the update is confirmed. (The lock of the old copy
  is waited for by `single_acquire`, which the template calls with a few
  seconds when it was started by an update.)
* a program that runs the update in a **thread** defines the dispatcher the
  language wants in the program itself:
  `fn __thread_work(kind: u64, arg: u64) -> u64 { return update.thread_dispatch(kind, arg) }`
* the UI calls `update_start(u, OP_CHECK, RUN_AUTO)` (check) or
  `update_start(u, OP_FETCH, RUN_AUTO)` (check and download) and then
  `update_poll(u)` every frame; `update_status(u)` has the phase, progress,
  version, notes and an error code (`err_text`, `status_error`).
  `update_cancel` stops a download. `update_apply` replaces the program.
* `update_mark_healthy(u)` is how the **new** program says it came up. Call it
  once the window works (the template does it after the first frames).

`APPKIT_HOME=/some/dir` relocates every directory appkit uses
(`<dir>/{config,data,state,cache,logs}/<app>`): for tests and portable
installs. (Wine overrides `APPDATA`, so the Windows test cannot use that.)

## 3. The store

The client reads the real store format
(`orientstore/docs/KATALOG-FORMAT.md`, catalog format 1) -- no parallel world:

| file | what the client does with it |
|---|---|
| `entry.json` + `entry.json.sig` | the entry point (TUF-light): signature, then timestamp, expiry, revision, then the catalog's hash and size |
| `index.json` | the catalog (`pakete`): parsed only after the entry vouches for it |
| `speicher/<xx>/<sha256>.<ext>` | the file; fetched by the path in the build, checked against the signed SHA-256 and size |
| `oeffentlich.key` | **not used**: the key is compiled into the program |

Order of checks (the format's, in this order): signature of `entry.json`
(Ed25519, 64 raw octets) -> its timestamp is not older than the last one seen
-> not expired (`laeuft_ab`, the 14-day freeze protection) -> its revision is
not lower than the last one seen -> the catalog's SHA-256 and size -> parse.
A store that has no `entry.json` yet is read the old way (`index.json` +
`.sig`) and only while no entry was ever seen. Everything it accepted is
remembered in the state file (`seen.revision`, `seen.timestamp`,
`entry.seen`).

### The extension (additive; format stays 1)

The old store knew `apk` and `opk`. A desktop program is neither, so
`orientstore` got `store add-app` and these additions (old clients skip
packages of a kind they do not know, nothing existing changes):

| `art` | what | `plattformen` words |
|---|---|---|
| `exe` | Windows program | `windows-x86_64` |
| `bin` | Linux program | `linux-x86_64`, `linux-aarch64` |
| `appimage` | Linux AppImage | same |
| `macos-app` | a zip of a bundle | `macos-arm64`, `macos-x86_64` |
| `apk`, `opk` | as before | `android-arm64`, `android-x86_64`, `android-arm`, `android-x86`; `osum-x86_64` |

* the package key is `<art>:<name>` (`exe:fleilauncher`); `apk` keeps its
  reverse-DNS name, `opk` its `opk:<name>`;
* a build says what it is: `fassung` (semver), `maschine`, `plattformen`,
  `groesse`, `sha256`, `datei`, `aenderungen`; a package may set
  `mindestFassung` (the floor: clients never install below it);
* channels are pointers (`kanaele`): `stabil@windows-x86_64` is looked up
  first and `stabil` second, so one package can serve two machines;
* `fassungen` mirrors the newest builds for old clients.

`store add-app --art exe --id fleilauncher --fassung 0.5.6 --ziel windows-x86_64
--name FleiLauncher --aenderungen "..." build/fleilauncher.exe` publishes one;
`store add app.apk` (as always) an Android build. The tool lives in the
orientstore repository (`werkzeug/store`, tests `tests/t30_apps.py`).

**Nothing in appkit or its tests touches the live store.** Every test talks to
a local one (`http://127.0.0.1:<port>/`, `http://10.0.2.2:<port>/` from the
Android emulator). `release.sh` of a generated program publishes into a
throw-away store unless `--store-repo DIR` is given, and then asks for a typed
YES (`--yes` skips the question).

## 4. The update flow

```
check     entry + signature -> catalog -> the build this channel points at
          for this platform -> is it newer than me?
fetch     download to <exe>.new (next to the program), SHA-256 + size from
          the SIGNED catalog + file magic -> READY
apply     keep the old file as <exe>.old, rename the new one over the program
          (one rename), start it with --appkit-restarted
confirm   the new program calls update_mark_healthy()
rollback  no confirmation within update.healthy_s seconds (default 45) or the
          new process died: the old file goes back and starts again
```

**What it decides** (`catalog.decide`): the installed program is known by the
SHA-256 of its own file (the store has it as a `sha256`), or, where the file
cannot be read (Android) or the build id is not known, by the compiled-in
version only.

| situation | answer |
|---|---|
| the running file is the offered one | up to date |
| offered version is higher | update |
| equal version, the running build is a **known older** build of the store | update |
| equal version, the running build is unknown (a developer's own) | up to date -- never overwritten |
| offered version is lower | up to date; a **rollback** by the publisher only when the store moved the channel back, the running build is known and newer, and the setting `update.follow_rollback` is on |
| offered version below the package's `mindestFassung` | error, never installed |

**What is checked on the way** -- a file whose SHA-256 differs from the signed
catalog is deleted and never run; so is one of the wrong size or without the
platform's magic (MZ / ELF / `PK`). A build that rolled back once is
remembered (`bad.0` .. `bad.3` in the state) and not offered again by itself.

**Where the work runs** (`RUN_AUTO` picks):

| | |
|---|---|
| a thread (`thread_start`) | Linux x86-64 (and AArch64 where the compiler has threads) |
| a worker **process** (`--appkit-worker check|fetch`: the program again; progress in `update-status.txt` with a beat counter -- 30 s without a beat means the worker died --, cancel by a file) | Windows (Firn has no threads there) |
| right here (`RUN_BLOCKING`; the progress call-back is called from the same thread) | Android, and tools/tests |

The UI side is the same for all three: `update_start`, `update_poll` each
frame, read `update_status`.

**Settings** (the program's `settings.json`; keys the updater reads):
`update.auto` (check by itself, default on), `update.interval_h` (hours
between automatic checks, default 24), `update.channel`,
`update.follow_rollback` (default off), `update.healthy_s` (default 45),
`update.store` (another store address, e.g. a test store). The template adds
`update.auto_download` (download what the automatic check finds, default off).

**State** (`state.json`): `seen.revision`, `seen.timestamp`, `entry.seen`,
`check.last`, `pending.*` (the update in flight: `state` = applied | confirmed |
rolledback, `from`, `to`, `sha`, `started`, `tries`), `rollback.reason`,
`bad.0..3` (the builds that failed here).

**Channels.** A program follows the channel it was built with
(`CHANNEL=beta bash build.sh`) or the one in its settings. Hidden/encrypted
channels of the store format are not implemented in the client.

## 5. Modules in short

* `fetch`: GET only, http and https (TLS 1.3 chain checked against the system
  roots plus the embedded Let's Encrypt roots, name checked), redirects (5,
  never https -> http), `Content-Length`/chunked/read-to-close, a short body
  is an error, no resume, no proxy, no IPv6. A body goes to memory or to a
  file through SHA-256. Names: dotted IPv4, `localhost`, else `net.dns`.
* `config`: 64 entries, keys <= 47, strings <= 191 octets, dotted names; save
  is temp + rename. Nested values are not preserved.
* `log`: one `write(2)` per line (atomic with `O_APPEND`), rotation to
  `.log.1 .. .log.<keep>`, levels, a module-wide singleton so any module can
  log; before `log_open` every call is a no-op.
* `crash`: Firn has no portable way to run code while dying, so the **next**
  start makes the report from the run marker (a file held locked while the
  process lives, emptied by `crash_end`), the log tail and the redirected
  stderr (a Firn panic prints its text there). Five reports are kept. Nothing
  is sent anywhere; the program decides what to do with the path
  (`crash_pending`).
* `single_instance`: `single_acquire(wait_ms)`; a program that has just
  updated itself waits a few seconds for the old copy.
* `texts`: `.opmsg` catalogs, lookup selected language -> English -> the key;
  appkit's own pages come in English and German
  (`lib/appkit/locale/*.opmsg`); the system language comes from
  `plat_locale`.

## 6. The template

`tools/newapp.sh <Name> <app-id> [--dir D] [--vendor V] [--store-url U]
[--store-key HEX | --store-key-file F] [--android-id ID] [--version V]` writes
`templates/app/` with the placeholders filled in:

| file | what |
|---|---|
| `src/main.fi` | the start sequence of section 2 |
| `src/appspec.fi` | name, id, vendor, store address and key (the key from `--store-key`, `$FIRN_STORE_KEY`, `/srv/store/oeffentlich.key`, or -- trust on first use, shown with its fingerprint -- the store's `oeffentlich.key`) |
| `src/ui.fi` | a window on `fui.kit`: sidebar with Home, Settings, Updates and About, dark with a green accent, a banner and a toast when an update is ready, the "restart and install" button; a window narrower than 560 logical pixels (a phone upright) folds the sidebar to its icons and gives the banner two rows |
| `src/locale/en.opmsg`, `de.opmsg` | the program's own texts |
| `build.sh`, `build-windows.sh`, `build-android.sh` | the three builds; `VERSION` is the only place the version lives; `CHANNEL=` and `STORE=` pick the channel and store |
| `release.sh` | build -> `store add-app` / `store add` -> verify; `--platforms linux,windows,android`, `--channel`, `--notes`, `--min-version`, `--store-repo`, `--publish-to` |

The store's **private key never lives in a project**: the store tool reads it
from `$ORIENTSTORE_SCHLUESSEL` or from the store directory it publishes into.
`tools/appkit/newapp_test.sh` generates a program, builds it (Linux, and the
Android APK when the Android build tools are there), starts it under Xvfb
(`--selftest`: 30 frames and a clean exit) and runs a dry-run release.

## 7. Platform layer

Every module that touches the system calls the `plat_*` functions and nothing
else. The contract is documented in `lib/appkit/platform.fi`; the same names
with the same signatures exist in:

| file | platform | how it is chosen |
|---|---|---|
| `lib/appkit/platform.fi` | Linux (and any POSIX) | default |
| `lib/appkit/platform.windows.fi` | Windows | module twin rule for `--target=x86_64-windows` |
| `lib/@android/appkit/platform.fi` | Android | platform-directory rule for the Android targets |
| `lib/appkit/platform_macos.fi` | macOS | **untested**; Firn has no macOS target |
| `lib/appkit/platform_osum.fi` | OrientOS | **untested**; written against the OrientOS tree |

`tools/appkit/platforms.py` checks that all five export the same names with
the same signatures and that each type-checks.

| | Linux | Windows | Android | macOS | OrientOS |
|---|---|---|---|---|---|
| replace the program | rename over it (a running binary may be replaced); backup is a copy | rename the old one away, the new one in; share-mode lock | **PackageInstaller** (below) | bundle swap: not supported yet | `opk` install |
| settings | `$XDG_CONFIG_HOME` / `~/.config/<app>` | `%APPDATA%\<vendor>\<app>` | `<files>/config/<app>` | `~/Library/Application Support/<app>` | `/data/...` (assumed) |
| state, logs | `~/.local/state/<app>` | `%LOCALAPPDATA%\<vendor>\<app>` | `<files>/{state,logs}/<app>` | | |
| single instance | POSIX record lock | file with share mode 0 | POSIX record lock | | |
| second process | `fork`/`exec` (std.process) | `CreateProcessW`, detached | not possible | | |
| threads | yes | no | **no** (see below) | no | no |
| language | `LC_ALL`/`LANG` | the registry (`HKCU\Control Panel\International`, `LocaleName`) | `Locale.getDefault()` through JNI | | |
| package kind / id | `bin` or `appimage` / `linux-<arch>` | `exe` / `windows-x86_64` | `apk` / `android-<abi>` | `macos-app` | `opk` / `osum-x86_64` |

### Android

* **No self-replacement.** A program is an APK. `plat_apply` streams the
  downloaded file into a `PackageInstaller` session and commits it; the
  system answers through a broadcast to `org.firn.FirnInstall`, a class
  **Firn writes into classes.dex** (`tools/appkit/installdex_main.fi`, no
  Java, no javac): `onReceive` is native (`Java_org_firn_FirnInstall_onReceive`
  in the platform file). When Android needs the user it sends a "pending user
  action" intent; the receiver starts it. Anything else (success, failure)
  goes to logcat (tag `appkit`) and `<files>/install-status.txt`
  (`status=<n>` and Android's message).
* **The manifest needs** `INTERNET`, `REQUEST_INSTALL_PACKAGES`,
  `UPDATE_PACKAGES_WITHOUT_USER_ACTION` and
  `<receiver android:name="org.firn.FirnInstall" android:exported="false"/>`;
  `templates/app/build-android.sh` does all of it.
* **What the user sees** (measured on API 35): the first update asks for the
  switch "install unknown apps" for this app; after that an app that updates
  itself is updated silently (`USER_ACTION_NOT_REQUIRED`, API 31+), but Android
  throttles silent updates per app and then asks once more -- the receiver
  starts that question, the user answers, the update goes through. An APK signed
  with another key is refused by Android (`status=5
  INSTALL_FAILED_UPDATE_INCOMPATIBLE`): **keep the signing key**
  (`~/.firn/android.keystore`, see `tools/android/build.sh`).
* **After the install** the system replaces the app and ends the process; it
  does not start it again, the user opens the app. (The receiver of a
  successful install usually runs in a fresh process without a window, so
  it writes no status file; the new version knows it is new by its version.)
* **No rollback.** The system owns the replacement; there is no backup file
  (`plat_rollback` answers "unsupported").
* **No threads.** `thread_start` re-seats the thread pointer (`arch_prctl` /
  `tpidr_el0`), which on Android belongs to bionic -- JNI, errno and the C
  library would be blind afterwards. `plat_threads` is false and there is no
  second process either (`plat_worker_process` false), so the check and the
  download run on the app thread; the progress call-back runs there too. A
  program with a big download should keep input flowing from that call-back;
  a service in another process would be the real answer (see the gaps).
* The directories live in the app's files directory (the host's data path,
  `lib/android/activity.fi`); the host's start block has no environment, so
  `APPKIT_HOME` does nothing there.

### Gaps (what is missing, per platform)

* **macOS**: the platform file has never been compiled for a macOS target (there
  is none); bundle replacement (`macos-app` zip) is "unsupported"; code
  signing / notarisation are not touched.
* **OrientOS**: written against the OrientOS tree (start block layout, `/data`
  directories, `opk` installation), never run on it.
* **Android arm64**: builds, never run. **Android in general**: no rollback
  (OS-owned), no download off the app thread, a program that also uses
  `lib/plat/android/push.fi` / `pick.fi` needs one `classes.dex` with all three
  classes (`servicedex_main.fi` and `installdex_main.fi` write one each).
* **Windows**: tested under Wine only (see the table at the top); no threads
  in Firn, so a worker process is used; no Authenticode check of the new file
  (the store's hash and signature are the trust). The renames of an update are
  retried for a few seconds, and so is the re-hash of the downloaded file,
  because a virus scanner (Defender) can hold a fresh file for a moment --
  written from what Windows is known to do, never seen here.
* **Linux aarch64**: compiles; the AArch64 syscall table has what appkit needs
  (`readlinkat` is used instead of `readlink`), never run.
* **All**: hidden/encrypted store channels, HTTP range/resume, proxies, IPv6
  and delta (`diffs`) downloads are not implemented; a download restarts from
  zero.

## 8. Tests

| test | what |
|---|---|
| `tests/2050_ed25519.fi` | the RFC 8032 vectors and a real store entry |
| `tests/2051_appkit_version.fi` | semver parse/compare |
| `tests/2052_appkit_config.fi` | settings: types, limits, atomic save, hand-edited files |
| `tests/2053_appkit_log.fi` | levels, rotation, the line format |
| `tests/2054_appkit_crash.fi` | the marker, the report, the five newest |
| `tests/2055_appkit_texts.fi` | catalogs, fallback, plurals |
| `tests/2056_appkit_catalog.fi` | the entry/catalog checks, every package kind, channel lookup, `decide` |
| `tools/appkit/platforms.py` | the five platform files agree and type-check |
| `tools/appkit/e2e.sh` | 60 checks against a local store: check (thread/process/blocking), progress, wrong hash, wrong signature, wrong key, changed catalog, expired entry, older catalog, the old catalog-only way, channels, the floor, chunked/redirect/cut-off/slow, cancel, the real replacement, confirmation, **rollback** after a crash and after a hang; `E2E_TARGET=windows` runs it under Wine |
| `tools/appkit/android_check.sh` | on the emulator: platform and JNI locale, check, hash and signature refusals, the PackageInstaller hand-over, the user's no (status 3) and yes, the next update, a foreign key (status 5) |
| `tools/appkit/newapp_test.sh` | generate, build (Linux, and the Android APK), run under Xvfb, dry-run release |
| `tools/appkit/winkit.sh` + `tools/appkit/winkit/run.py` | **a kit for another machine**: a zip with the test programs, three signed catalogs and a Python script that plays the store and runs the same story (33 checks); see below |

**A real Windows PC.** The end-to-end run needs bash, this tree and the store
tool, and this server cannot put files on a Windows machine. So
`bash tools/appkit/winkit.sh windows` makes `build/appkit-windows-kit.zip`
(about 2.4 MB): the programs built for Windows, a store made on the spot
(throw-away key, three signed catalogs, good for 14 days) and `run.py`. On the
PC: unzip, `py run.py` (Python 3 is all it needs; it plays the store on
127.0.0.1 itself and keeps every program's files in a temporary folder).
Each check prints `ok` or `FAIL`, the exit code says it all. The same kit was
run here under Wine (`KIT_RUNNER=wine python3 run.py`) and, built for Linux,
on Linux (`winkit.sh linux`): 33 checks each. Windows Defender may need the
folder allowed (unsigned test programs).

`test.sh` section 76 runs the platform check, the end-to-end run and the
generator test (the Windows run with `APPKIT_E2E_WINDOWS=1`, the Android run
with `APPKIT_E2E_ANDROID=1`). Heavy runs go through `/root/jarvis/bin/heavy`.

## 9. Things that bite when extending it

* generic function names are **global**, not per module: give a generic
  helper a module-specific name (`upd_size_of`, `plat_size_of`);
* `size_of[T]()` cannot name a module-local type directly -- wrap it in a
  generic helper of the same module;
* string literals are single-line; long texts go in a file and
  `__include_str("file")`;
* a module called `i18n` clashes with `lib/i18n`'s module name: appkit's is
  `texts`;
* `#[no_gc]` is transitive: code that runs on Android's main thread (the
  receiver) may call only `#[no_gc]` functions;
* the Windows target has no threads and an empty `envp`; the platform file
  reads the environment through `GetEnvironmentStringsW`.
