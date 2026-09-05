# tests/data/tls-chains -- real certificate chains, and where they come from

Six chains, harvested once with

    openssl s_client -connect <host>:443 -servername <host> -showcerts

on 2026-08-26 and stored exactly as they came off the wire (the leaf
first, then whatever the server sent with it). Nothing was reordered and
nothing was removed -- including the self-signed root that two of the six
servers send although RFC 8446 4.4.2 says it may be omitted.

The hosts were not chosen for being easy. They cover what a browser really
meets:

| host | leaf key | chain | why it is here |
|---|---|---|---|
| `example.com` | P-256 | 4, all ECDSA | a **P-384** intermediate and a P-384 root. This is the chain that showed P-256 alone is not enough. |
| `www.google.com` | P-256 | 3 | the most common shape on the web today |
| `www.rust-lang.org` | RSA | 3 | RSA leaf under an RSA intermediate |
| `en.wikipedia.org` | P-256 | 4 | a wildcard `*.wikipedia.org` in the subjectAltName |
| `www.cloudflare.com` | P-256 | 3 | |
| `github.com` | P-256 | 3 | |

`tools/tlsb5/cert_check.py` checks each of them three times: once as it
stands (must verify against the machine's own `/etc/ssl/certs`), once
under a **wrong name** with a `.invalid` suffix (must be refused as
`NAME`), and once at a **time past their notAfter** (must be refused as
`EXPIRED`).

The wrong name has a *suffix* and not a prefix on purpose. The first
version of the check used `not-en.wikipedia.org`, which the verifier
accepted -- correctly, because `*.wikipedia.org` really does match it.
That was a bug in the test and not in the verifier, and it is written down
here so that nobody re-introduces it.

These certificates will expire. When they do, the three real-chain rows
turn into failures with the reason `EXPIRED`, which is the right thing for
them to do: harvest them again with the command above.
