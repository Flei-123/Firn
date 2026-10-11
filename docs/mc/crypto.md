# mc_crypto -- login cryptography (`lib/mc/mc_crypto.fi`)

Import: `import mc.mc_crypto`. Everything works on `u64` addresses + lengths. Result codes are `i32`
(`CR_OK = 0`); `error_name(code)` gives a short name.

## API
| Function | Purpose |
|---|---|
| `rsa_key_from_spki(der, n, *RsaKey) -> i32` | strict X.509 SubjectPublicKeyInfo (rsaEncryption) -> key copy. |
| `rsa_key_bytes / rsa_key_bits(*RsaKey)` | modulus size (ciphertext size), 0 for an empty key |
| `rsa_encrypt_pkcs1(*RsaKey, msg, mlen, out, outcap, *outlen) -> i32` | RSAES-PKCS1-v1_5, block type 2, >= 8 random NON-ZERO padding octets, fresh each call |
| `shared_secret_new(out16) -> bool` | 16 octets from getrandom (/dev/urandom fallback) |
| `cfb8_new(secret16) / cfb8_new_soft(secret16) -> Cfb8Stream` | AES-128-CFB8, key = IV = secret; `_soft` forces the scalar path |
| `cfb8_encrypt / cfb8_decrypt(*Cfb8Stream, buf, n)` | IN PLACE, any chunk size, state persists; one stream per direction |
| `cfb8_wipe(*Cfb8Stream)` | zero key schedule + register |

## Errors
`CR_BAD_DER` (malformed, truncated, trailing octets, negative integers, bit string with unused bits),
`CR_NOT_RSA` (EC, Ed25519, RSA-PSS... valid SPKI, other OID), `CR_BAD_KEY` (modulus < 512 or > 4096 bit,
even modulus, e < 3 / even / > 8 octets), `CR_MSG_TOO_LONG` (> modulus - 11), `CR_OUT_SMALL`,
`CR_RANDOM`, `CR_ARG`. A refused key leaves an empty key behind (`nlen = 0`).

## Design notes
* Encrypt only (the client never decrypts). `rsa_public_op` of `std.crypto.rsa` does the modular exponentiation.
* CFB8 decrypt has its own AES-NI path: the keystream of octet i depends only on ciphertext octets i-16..i-1,
  which are known, so 8 octets are pipelined through `aesenc` (segments of 4096). Encrypt is inherently serial.
  Without AES-NI both directions fall back to the scalar code of `std.crypto.aes` (about 0.8 MiB/s).
* `cfb8_*` return nothing; a wrong key size is impossible (`secret` is always 16 octets).

## Measured (AMD EPYC 7571, shared busy server, `xcheck bench`, release-fast)
| Operation | Rate |
|---|---|
| CFB8 decrypt, AES-NI | ~48-58 MiB/s |
| CFB8 encrypt, AES-NI | ~16-22 MiB/s |
| CFB8 scalar (either direction) | ~0.8 MiB/s |
| RSA-1024 encrypt (e = 65537) | ~1500 /s |
| RSA-2048 encrypt | ~430 /s |

## Tests
* `tests/2670` CFB8 (known answer, chunking, NI vs scalar for 54 lengths incl. segment edges, 5000-octet hash),
  `2671` RSA (SPKI, 6 message sizes x20 opened with the private exponent, 20 hand made bad/odd keys),
  `2672` fuzz (all prefixes, all octet values, 24000 random mutations, trailing octets, absurd lengths).
* `tools/mc/crypto/run.sh`: node `aes-128-cfb8` byte identical (128 chunked runs, both directions, both paths),
  SPKI fields == node JWK for 512..4096 bit keys, ciphertexts decrypted by node (`privateDecrypt`, needs
  `--security-revert=CVE-2023-46809` in node 18.20) and by python `cryptography`, raw block shape (`00 02 PS 00 M`),
  EC/Ed25519/RSA-PSS keys refused, counter-check (tampered outputs must fail).
