# MySQL / MariaDB client (lib/mysql)

Blocking client for the MySQL protocol (text protocol and prepared statements).

## Modules
- `mysql/mysql.fi` connection, handshake, auth, COM_QUERY, COM_STMT_PREPARE/EXECUTE/CLOSE, COM_PING.
- `mysql/mywire.fi` packet helpers (length-encoded ints), `mysql_native_password` and `caching_sha2_password` scrambles.
- `mysql/myrows.fi` result set and parameter list. `mysql/mybin.fi` text and binary row decoders.

## Use
```
import mysql.mysql
import mysql.myrows
var c: mysql.My = mysql.my_new()
mysql.my_connect(&c, addr_ipv4, 3306, "user", "password", "db", mysql.TLS_OFF, "host", 0, 0, 0)
var r: myrows.MyResult = myrows.result_new()
mysql.my_query(&c, "SELECT 1", &r)
var np: usize = 0
let id: i64 = mysql.my_prepare(&c, "INSERT INTO t (a, b) VALUES (?, ?)", &np)
var q: myrows.MyParams = myrows.params_new()
myrows.params_text(&q, ptr, len)
myrows.params_null(&q)
mysql.my_exec_stmt(&c, id as u32, &q, &r)
myrows.result_value(&r, row, col, &p, &n)
```
`result_value` is false for NULL. `my_error` / `my_error_code` hold the server message and error number. Results of prepared statements are decoded to text (ints, strings, DECIMAL, DATE/DATETIME/TIMESTAMP).

## Auth
- `mysql_native_password`, `caching_sha2_password` fast path, AuthSwitchRequest between them.
- `caching_sha2_password` full authentication works over TLS only (cleartext password inside TLS); without TLS it fails with a clear error (no RSA public key exchange).
- TLS: SSLRequest, certificate checked against the given roots and host. TLS 1.3 works against MariaDB (OpenSSL 3): the client answers the CertificateRequest with an empty Certificate and derives the application keys from the transcript up to the server Finished. A failed TLS read reports the error number and the received alert code in `my_error`.

## Not included
- FLOAT/DOUBLE/TIME columns in prepared results (error "malformed row", cast to text in SQL), LOCAL INFILE, multi-statement/multi-result, compression, `sha256_password`, `ed25519`, DNS (pass an IPv4 address).

## Tests
`bash tools/mysql/run.sh` downloads `mariadb-server-core` with apt, unpacks it in a temp dir, starts `mariadbd`, and runs `tools/mysql/mysql_check.fi` plain and with TLS: wrong password (1045), 100k rows, 1 MB parameter, 20 MB result (multi-packet), 100 prepared inserts, binary rows (BIGINT, UNSIGNED, TINYINT, DECIMAL, DATETIME, NULL), SQL error 1064 and recovery. `my_fake.py` checks `caching_sha2_password` (fast path and plugin switch) against an independent Python scramble and fuzzes 300 hostile servers: 0 crashes. MariaDB has no `caching_sha2_password`, so that plugin is only covered by the fake server.
