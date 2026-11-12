# PostgreSQL client (lib/pg)

Blocking wire-protocol v3 client. Text format only.

## Modules
- `pg/pg.fi` connection, auth, simple + extended queries, prepared statements, results.
- `pg/pgwire.fi` message builders and big-endian helpers.
- `pg/pgscram.fi` SCRAM-SHA-256 (RFC 5802/7677) with PBKDF2-HMAC-SHA256; server signature is verified.

## Use
```
import pg.pg
var c: pg.Pg = pg.pg_new()
pg.pg_connect(&c, addr_ipv4, 5432, "user", "password", "db", pg.TLS_REQUIRE, "host", roots_ptr, roots_len, now)
var r: pgrows.PgResult = pgrows.result_new()
pg.pg_query(&c, "SELECT 1", &r)
var q: pgrows.PgParams = pgrows.params_new()
pgrows.params_text(&q, ptr, len)
pgrows.params_null(&q)
pg.pg_query_params(&c, "INSERT INTO t VALUES ($1, $2)", &q, &r)
pg.pg_prepare(&c, "name", sql)
pg.pg_exec_prepared(&c, "name", &q, &r)
pg.pg_deallocate(&c, "name")
pgrows.result_value(&r, row, col, &p, &n)
```
`result_value` is false for SQL NULL. `pg_error` has "SEVERITY: message", `pg_error_code` the 5-char SQLSTATE. `pg_txn_status` is `I`, `T` or `E`. TLS (SSLRequest) checks the certificate against `roots` and `host`. Auth: SCRAM-SHA-256, md5, cleartext.

## Not included
- COPY, LISTEN/NOTIFY delivery, binary formats, cancel requests, SCRAM-SHA-256-PLUS (channel binding), GSS/SSPI, client certificates, DNS (pass an IPv4 address, see `net/dns.fi`).
- A multi-statement simple query returns the last result set only.

## Tests
`bash tools/pg/run.sh` downloads `postgresql-15` with apt, unpacks it in a temp dir, starts it as `nobody`, then runs `tools/pg/pg_check.fi` with SCRAM (plain and TLS), md5 and cleartext users: 100k-row result, 1 MB parameter, 100 prepared executions, SQLSTATE errors and recovery, transaction status. Then a fuzz run of 500 hostile servers (`pg_fuzz.py`): 0 crashes. Everything lives in `mktemp -d` and is removed.
