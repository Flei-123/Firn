# TLS 1.2 in the TLS client (round TLS12)

`lib/tls/tls.fi` was a TLS 1.3 client. A large part of the web still answers
TLS 1.2 first or only (`login.live.com` over some paths, `badssl.com`'s
tls-v1-2 host, old appliances, a lot of enterprise gear), and a launcher that
logs in against Microsoft has to reach it. The client now speaks both.

## What was added

* **One ClientHello for both versions.** `supported_versions` lists 1.3 and
  1.2, the cipher suites are the two 1.3 ones plus six 1.2 ECDHE suites, and
  the extensions a 1.2 server needs are there (`ec_point_formats`,
  `extended_master_secret`, `renegotiation_info`; `signature_algorithms`
  already carried the RSA PKCS#1 schemes). The server chooses. The key shares
  of the 1.3 offer are reused as the ECDHE keys of a 1.2 handshake.
* **Suites** (RFC 5289, RFC 7905): ECDHE-ECDSA and ECDHE-RSA with
  AES-128-GCM-SHA256 (`c02b`, `c02f`), AES-256-GCM-SHA384 (`c02c`, `c030`) and
  ChaCha20-Poly1305-SHA256 (`cca9`, `cca8`). Groups X25519 and secp256r1.
* **Non-blocking mode** (round ASYNC's state machine, `tls_handshake_step`): the
  TLS 1.2 handshake is a state machine as well (`handshake12_step`, steps 5 and
  6; the flags and the certificates live in the `Tls` struct), so `WouldBlock`
  in the middle of any record loses nothing; `read_record12` follows the same
  rules (N3, N4).
* **Handshake** (`handshake12_step` / `send_flight12` in `tls.fi`): ServerHello, Certificate,
  ServerKeyExchange (signature over both randoms and the share, RSA PKCS#1,
  RSA-PSS or ECDSA), optional CertificateRequest (answered with an empty
  Certificate), ServerHelloDone; then ClientKeyExchange, ChangeCipherSpec,
  Finished; the server's ChangeCipherSpec and Finished are checked in constant
  time.
* **Key derivation** (`lib/tls/prf12.fi`): the TLS 1.2 PRF on HMAC-SHA256 or
  HMAC-SHA384 (the latter is new, `hmac_sha384`), the master secret (extended
  per RFC 7627 when the server echoes it), the key block, `verify_data`.
* **Record layer** (`read_record12`, `write_record12`): the content type in
  the header, AAD = sequence number + type + version + length, GCM with a 4
  octet salt and an 8 octet explicit nonce, ChaCha20-Poly1305 with the IV xor
  sequence number; encryption starts at each direction's ChangeCipherSpec.
* **AES-256** (`aes.fi` `aes256_new`, the key schedule with the extra SubWord;
  `accel.fi` `aes256_ni_encrypt_block`, 14 rounds on AES-NI; `gcm.fi` takes a
  32 octet key). Block encryption only: GCM needs nothing else.
* **API**: `tls_version(t)` (772, 771), `tls_set_max_version(t, 771)` (offer
  1.2 only), `tls_ems(t)`, `tls_require_ems(t, true)`; `TlsError::Downgrade`;
  `net.http`: `client_tls_version`, `client_set_tls_max`; `tls_main` prints
  `VERSION`.

## Version negotiation, and why there is no fallback

The negotiation is inside the one handshake, as in every browser today. After a
failed handshake the client does **not** try again with an older version: that
"fallback dance" is the lever of the POODLE-type downgrade attacks, and
`net.http` neither needs nor does it.

**Downgrade protection** (RFC 8446 4.1.3): a TLS 1.3 server that nevertheless
negotiates 1.2 ends the last eight octets of its ServerHello random with
`DOWNGRD\x01` (or `\x00` for 1.1 and below). A client that offered 1.3 and
sees that refuses with `Downgrade`. When the client offered 1.2 only
(`tls_set_max_version(t, 771)`) the sentinel is legitimate (the server was
asked for 1.2) and is not checked. What a 1.2 server signs (the randoms are in
the ServerKeyExchange signature) and the Finished MACs cover the rest.

## Sessions

As in the 1.3 path: **no resumption** (session id and tickets are not offered,
a ServerHello that echoes our random session id is refused as a resumption we
never asked for), no renegotiation (a HelloRequest after the handshake is
answered with a `no_renegotiation` warning alert; one in the middle of the
handshake is left out of the transcript and ignored).

## Proof

* `tests/2102_tls12_prf.fi`: HMAC-SHA384 (RFC 4231), the PRF with SHA-256
  (the IETF working group's vector) and SHA-384 (an independent implementation
  of RFC 5246 5), prefixes, labels.
* `tests/2103_aes256_gcm.fi`: FIPS 197 C.3 (hardware and scalar path), eight
  GCM cases against Python's `cryptography` including every flipped-bit refusal.
* `tools/tls/tls12_check.py` (part of `tools/tls/run.sh`):
  * **A/B** `openssl s_server`: every suite x both groups x the signature
    schemes (RSA PKCS#1 SHA-256/384, RSA-PSS SHA-256/384, ECDSA SHA-256/384);
    a 1.3 server still gets 1.3, a 1.2-only client gets 1.2 from it.
  * **C** Python `ssl`, TLS 1.2 only, 512 KiB of known bytes, five suites:
    octet for octet.
  * **D** counter-checks with a man in the middle and raw hostile servers:
    flipped bits in the ServerKeyExchange (RSA and ECDSA), in its parameters,
    in the Finished, in application data; the downgrade sentinel (both forms,
    and the "not checked when 1.3 was not offered" case); TLS 1.1 version, an
    unoffered suite, a 1.3 suite without `supported_versions`, a compression
    method, an echoed session id, a cut handshake, garbage, a 65535-octet
    record; expired / wrong-name / empty-store certificates.
  * **F** fuzz: 160 server flights with one random byte replaced; every run has
    to end with a verdict (no signal, no time-out).
  * **G** non-blocking: a proxy hands the server's octets over one to seven at a
    time; `tools/tls/tls12_nb_main.fi` (a `poll` loop around
    `tls_handshake_step` / `tls_read`) must still fetch the whole page, over
    TLS 1.2 and TLS 1.3 (the async tests of round ASYNC cover 1.3 only).
  * **E** real hosts when there is a route: `login.live.com`,
    `tls-v1-2.badssl.com:1012` (the body equals curl's), the refusals
    expired / wrong.host / self-signed / untrusted-root with the right
    verdict, and the TLS 1.0 / 1.1 hosts.
* Windows: `tls_main` built for `x86_64-windows` runs the same handshake under
  Wine (checked once by hand against `openssl s_server`; the DNS tool's
  Windows run of the https fetch does the rest).

## HONEST

* **X1 No static RSA, DHE, CBC, SHA-1 MAC, RC4, 3DES; no TLS 1.0/1.1.** Servers
  that offer nothing else are refused (`Suite`/`Version`/an alert).
* **X2 secp384r1 is not offered as a group**, so a TLS 1.2 server whose
  certificate has a P-384 key (it needs the curve in the client's list, RFC 8422)
  answers `handshake_failure`; the same server works over TLS 1.3. P-384 ECDH
  would need a constant-time P-384 ladder, which does not exist here
  (`p256.fi` is P-256 only; `ecdsa.fi`'s multiplication is for public data).
* **X3 The extended master secret is optional by default.** A server without it
  (`tls-v1-2.badssl.com`, OpenSSL before 1.1.0) is accepted; the attack the
  extension closes needs renegotiation or resumption and this client does
  neither. `tls_require_ems(t, true)` makes it strict.
* **X4 No client certificates** (T3): the empty Certificate answer.
* **X5 AES is not constant time on a machine without AES-NI** (aes.fi A3); on
  AES-NI hardware it is. ChaCha20-Poly1305 is the suite to prefer there, and it
  is offered before AES-256 (after AES-128 which the hardware path speeds up).
* **X6 The Windows trust store, big chains and OCSP** are as in the 1.3 path;
  nothing about revocation is checked in either version.
* **X7 The `Tls` struct grew** (about 400 octets of 1.2 state); code that
  copies one by value pays that.
* **X8 The ChangeCipherSpec of the TLS 1.3 compatibility mode moved**: it is
  now sent right after the ServerHello (before our encrypted flight, RFC 8446
  D.4 allows both places) instead of right after the ClientHello, because a TLS
  1.2 server treats a ChangeCipherSpec in the middle of its first flight as a
  protocol error.
