# Archives, file hashes and a keyring in std

Written for the first program that needs all of it, **FleiLauncher** (a
Minecraft launcher in Firn/fUi): it downloads a Java runtime from Adoptium
(`.zip` on Windows, `.tar.gz` on Linux), checks every file against the SHA-1
or SHA-256 of a manifest, writes the downloads so that a crash never leaves
half a file, and keeps the Microsoft/Xbox tokens somewhere better than a JSON
file. Everything is Firn, no foreign library; every module has its own
positive test in `tests/` and is held against programs nobody here wrote by
`tools/stdarchive/run.sh` (section 74 of `test.sh`).

| module | file | what it is |
|---|---|---|
| `std.tar` | `lib/std/tar.fi` | tar reader (ustar, V7, GNU long names, pax), tar writer, `tar_add_tree`, tar.gz, tar.zst, tar.xz, tar.bz2, tar.lz4 (via `lib/compress`, [COMPRESSION.md](COMPRESSION.md)), safe extraction |
| `std.extract` | `lib/std/extract.fi` | `extract_archive(path, dest)`: `.zip`, `.tar.gz`/`.tgz`, `.tar.zst`, `.tar.xz`, `.tar.bz2`, `.tar.lz4`, `.tar`, found by content |
| `std.safefs` | `lib/std/safefs.fi` | the rules and writers both extractors share (names, link targets, no write through a link) |
| `std.hashfile` | `lib/std/hashfile.fi` | streaming file hashes (md5, sha1, sha256, sha512), hex, atomic download target |
| `std.secret` | `lib/std/secret.fi`, `secret_os.fi`, `secret_os.windows.fi` | keyring: Credential Manager on Windows, encrypted file on Linux |
| `std.osinfo` | `lib/std/osinfo.fi`, `osinfo.windows.fi` | `is_windows()`, decided when the program is built |

Small additions elsewhere: `std.fs` got `write_file_mode`, `write_atomic_mode`
(mode set at creation), `chmod`, `readlink`, `open_read`/`read_fd`/`close_fd`,
`create_excl`/`write_fd_all`/`sync_fd`/`temp_name_for`; `std.md5` got a
streaming interface (`md5_new`/`md5_update`/`md5_final`); `std.deflate` got
`gzip_decompress_limited`; `lib/zip` got `zip_entry_mode`,
`zip_entry_is_symlink`, `zip_reader_empty`; the compiler learned `flock` and
`readlinkat` for AArch64 and four Credential Manager imports for Windows.

## Unpacking a Java runtime

```firn
import std.extract
import std.safefs
import std.tar

var o: safefs.ExtractOptions = safefs.extract_options_default()
o.strip = 1                       // "jdk-21.0.4+7-jre/" is dropped: bin/java lands in dest/bin/java
let st: safefs.ExtractStats = extract.extract_archive_opts(path, "/opt/fleilauncher/java", o,
    tar.tar_limits_default()) catch |e| ...
```

The format is found by the content (`PK\3\4`, `1f 8b`, `ustar`), not the file
name. Files get the permission bits of the entry (setuid/setgid/sticky are
never set), so `bin/java` comes out `0755`; a ZIP made on Unix keeps its modes
too. Symbolic links are created (on Windows they need Developer Mode or the
privilege; the seam asks for the unprivileged flag).

## What an extraction refuses

Everything is judged **before the first byte is written**; an archive with one
bad entry changes nothing on disk.

* **Names**: absolute, `..`, `.` inside, empty components, backslash, NUL, over
  4096 octets. On Windows also `< > : " | ? *` and control characters (`:` is
  how an alternate data stream is addressed), components ending in `.` or
  blank, DOS device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`).
* **Link targets**: relative; `..` only at the *start* and at most as often as
  the link is deep. `a/b/l -> ../../x` is fine, `l -> a/../../x` and `l -> ../x`
  from the top are refused.
* **A write through a link**: before an entry is written every directory on its
  way is looked at with `lstat`; a symbolic link or a file where a directory
  belongs is `UnsafePath`. The last component is created with `O_EXCL`; an
  existing entry is `Exists`, or with `overwrite` unlinked first, never written
  through.
* The same name twice (`Duplicate`; case-folded on Windows), a hard link to
  anything but an *earlier file of the archive* (it is written as a copy),
  device nodes and fifos (`UnsafeType`), sparse files (`Unsupported`).
* **Size bombs**: entries, one file, all content, and — for `.gz` — the
  *inflated* size (`TarLimits`; the inflate stops at the limit, it does not
  inflate first and look afterwards).

Why the link rule is enough (sketch). Every directory on the way to an entry is
a real directory (checked), so the *physical* location of a link equals its
*lexical* location. Its target starts with `k <= depth` times `..` — which ends
inside the root — followed by plain names, which only go *down*. Resolution of
a plain name may pass through another link; by induction that link is of the
same form and is entered from a real directory inside the root, so it again
ends inside the root. A chain of such links can therefore never assemble a path
that leaves the directory, and nothing can write *through* one because writing
refuses links on the way. Without the "no `..` after a normal component" rule
this fails: with `x/y -> ..` a target `x/y/..` is lexically `x` but physically
the parent of the root.

## tar

* **Reads** ustar (with `prefix`), V7, GNU `L`/`K` long names, pax `x` records
  (`path`, `linkpath`, `size`, `mtime`), pax `g` skipped, octal and base-256
  sizes. Checksums are verified (both the unsigned and the old signed sum).
* **Writes** ustar; names over 100 octets are split at a `/` (prefix 155 +
  name 100) when that works, else a pax `path` record; link targets over 100
  and sizes over 8 GiB likewise. The archive ends with two zero blocks and is
  padded to 10,240 octets, as GNU tar does.
* `tar_add_tree(w, dir, prefix)` packs a directory (byte-wise sorted, links not
  followed, mtime and mode kept), `targz_compress` makes the `.tar.gz`.

## std.hashfile

```firn
let hex: str = hashfile.hash_file_hex(path, hashfile.HASH_SHA1) catch |e| ...
let ok: bool = hashfile.verify_file_hex(path, hashfile.HASH_SHA256, want) catch |e| ...   // false = differs, BadHex = not a digest

var d: hashfile.Download = hashfile.download_begin(dest, hashfile.HASH_SHA256, 420 as u32) catch |e| ...
hashfile.download_write(&d, chunk_ptr, chunk_len) catch |e| ...          // as data arrives
hashfile.download_commit(&d, want_hex) catch |e| ...                     // verify, fsync, rename, fsync dir
```

`download_begin` creates `<dest>.tmp-<pid>-<n>` next to the destination
(`O_EXCL`); `download_commit` compares the digest, flushes, renames over `dest`
and flushes the directory. Any failure removes the temporary file and leaves
`dest` alone: it is the old file or the new verified one, never a half one.
Memory use does not depend on the file size (64 KiB chunks).

## std.secret

```firn
secret.secret_set("fleilauncher", "microsoft-refresh", token) catch |e| ...
let t: str = secret.secret_get("fleilauncher", "microsoft-refresh") catch |e| ...   // NotFound if none
secret.secret_delete("fleilauncher", "microsoft-refresh") catch |e| ...             // false = was not there
```

* **Windows**: the Credential Manager (`CredWriteW`/`CredReadW`/`CredDeleteW`),
  `CRED_TYPE_GENERIC`, `CRED_PERSIST_LOCAL_MACHINE`. One credential holds 2560
  octets at most (checked on a real Windows 10/11 machine: 2560 is accepted,
  2561 is refused), so a value is split into chunks of 2000 octets
  (`service/key`, `service/key\x1f1`, ...), up to 64 KiB; the first credential is
  written last, so a crash leaves the old value or the new one.
* **Linux**: an encrypted file `$XDG_DATA_HOME/firn/secrets.v1` (else
  `$HOME/.local/share/firn/secrets.v1`), mode 0600 in a 0700 directory;
  ChaCha20-Poly1305, key = HKDF-SHA256(machine id + user id, random file salt),
  fresh nonce on every save, atomic replace, `flock` between processes.
* **Explicit variant** (`vault_open_password`): the same file with Argon2id over
  a password (64 MiB, 3 passes) — the only variant that protects against someone
  who has the file *and* runs as the user.

What is protected and what is not (the long form is in the head of
`lib/std/secret.fi`):

| | Windows | Linux file (machine) | Linux file (password) |
|---|---|---|---|
| another user of the machine | yes | yes (0600) | yes |
| the file copied elsewhere / a leaked backup | n/a (not a file) | **yes** (key needs this machine) | yes |
| a process of the same user | **no** (it can call `CredReadW`) | **no** (it can derive the key) | yes |
| root / malware on the machine | no | no | only until the password is typed |
| the secret in memory | not wiped by the GC (`secret_get` returns a GC string; `secret_get_into` gives a buffer to wipe) | same | same |

The Secret Service of GNOME/KWallet is D-Bus and out of scope.

## Windows notes

* `secret_os.windows.fi` needs `CredWriteW`, `CredReadW`, `CredDeleteW`,
  `CredFree`; they are in the import table of `compiler/src/win.rs` because an
  indirect call through `GetProcAddress` is System V, not Win64 — the arguments
  would land in the wrong registers.
* File modes do not exist on Windows: the extraction ignores them there
  (`chmod` answers an error that is ignored). The tests skip those assertions
  on Windows.
* Run under Wine, file names with non-ASCII characters need a UTF-8 locale
  (`LC_ALL=C.UTF-8`, set by `tools/windows/run.sh`).

## How it was proven

* `tests/2070_std_hashfile.fi`, `2071_std_tar.fi`, `2072_std_extract.fi`,
  `2073_std_secret.fi`: every build level, AArch64 under qemu, Windows under
  Wine. Fixtures from GNU tar 1.34, Info-ZIP and Python in `tests/data/archives/`
  (`tools/stdarchive/gen_fixtures.py`); the password vault there is made by
  `cryptography` + PyNaCl (a known-answer file).
* `tools/stdarchive/run.sh` (182 checks): GNU tar (gnu, ustar, pax, posix) and
  Python `tarfile` (USTAR, GNU, PAX) → Firn and Firn → GNU tar/`tarfile`, on
  trees with names over 100 and 255 octets, Unicode, links, a 5 MiB file;
  fourteen hostile archives that must be refused with the right reason and
  leave the directory empty; damaged archives; Info-ZIP and `zipfile`;
  `hashlib` for 17 sizes × 4 algorithms; the vault decrypted by Python
  (HKDF + ChaCha20-Poly1305, Argon2id from libsodium) and a Python-made vault
  read by Firn; eight processes writing one vault at once.
* `tools/windows/wincred.sh`: the Credential Manager roundtrip of the real
  `.exe` under Wine (18 checks). On a real Windows machine (FLEI-ONE) the
  structure layout (`CREDENTIALW` = 80 octets) and the limits (2560 accepted,
  2561 refused, `ERROR_NOT_FOUND` 1168) were checked with a PowerShell
  P/Invoke probe making the same calls.

## Honest limits

* Whole archives live in memory (the DEFLATE decoder has no streaming mode):
  a 300 MiB JDK needs about that much, plus the compressed file.
* Not preserved: owners, modification times, extended attributes, ACLs; sparse
  files are refused; directory modes are applied as `mode | 0700` only.
* A name twice is refused although GNU tar would let the last one win.
* Extraction is not safe against another process that changes the *target
  directory* while it runs (replacing a directory by a link between our `lstat`
  and our `open`) — the target must not be writable by an attacker.
* The Linux vault is not a Secret Service; read the table above before putting
  anything in it that you would not put in `~/.config/*.json` with mode 0600.
* No rollback protection: an old copy of the vault file is a valid vault file.
