# lib/db -- an embedded SQL database that is a SQLite file

`lib/db/` is a database engine written in Firn (no libc, no SQLite). It reads
and writes **real SQLite 3 files**: SQLite opens what this library wrote, this
library opens what SQLite wrote, and a process killed in the middle of a commit
leaves a journal that either one rolls back. Written for the FleiLauncher
(search cache, play history); useful for any Firn program that wants tables,
indexes and transactions in one file.

```firn
import db.dbcore as dc
import db.sqlite as sq

let d: *mut dc.Db = sq.db_open("cache.db", sq.OPEN_CREATE) catch | e | (0 as *mut dc.Db)
sq.db_exec(d, "CREATE TABLE IF NOT EXISTS t(id INTEGER PRIMARY KEY, name TEXT)") catch false
let st: *mut dc.Stmt = sq.db_prepare(d, "INSERT INTO t(name) VALUES (?1)") catch | e | (0 as *mut dc.Stmt)
sq.stmt_bind_text(st, 1, "x") catch false
sq.stmt_step(st) catch false            // true = a row, false = done
sq.stmt_finalize(st)
sq.db_close(d)
```

Two programs show it in use, both self-checking: `examples/db_modrinth_cache.fi`
(search answers with a time to live, UPSERT, LIKE search over mods, an injection
attempt that stays text) and `examples/db_game_history.fi` (instances, sessions,
events: joins, GROUP BY, `strftime`, a session recorded in one transaction,
rollback, a run that never ended).

## The decision: (a) the SQLite file format, not (b) an own key-value store

| | (a) SQLite-compatible | (b) own transactional KV/document store |
|---|---|---|
| work | large: pager, journal, B-trees, record format, SQL parser, planner, expressions | small: a log-structured or B-tree store, an API |
| proof | **a second implementation nobody here wrote**: Python's `sqlite3` reads and writes the same files; every claim can be checked bit for bit, in both directions | only our own tests |
| tools | every SQLite tool opens the file (`sqlite3` CLI, DB Browser, Python, backups) | none |
| risk | SQL semantics are huge (affinity, collation, NULL rules, dates) | none |
| fit | a launcher's data is relational (instances -> sessions -> events, mods -> versions) | fits a cache, not a history with queries |

(a) was chosen because it is the only choice with an **independent judge**:
the format has a reference implementation, and the interesting bugs (a page
that SQLite's `integrity_check` calls corrupt, a comparison that sorts
differently) are found by it, not by us. It cost about 20,000 lines. The
price is a **subset** of SQL (below) and speed (below); the format itself is
complete for what is supported.

## What is supported

SQL (the parser says `Unsupported` with the word for anything else):

- `SELECT` with `DISTINCT`, `WHERE`, `GROUP BY`, `HAVING`, `ORDER BY` (`NULLS FIRST/LAST`),
  `LIMIT/OFFSET`, `UNION [ALL]`/`INTERSECT`/`EXCEPT`, sub-selects (scalar, `IN`, `EXISTS`,
  correlated, in `FROM`), joins: comma, `[INNER|CROSS|LEFT] JOIN ... ON/USING` (up to 8 tables).
- `INSERT` (`VALUES` lists, `SELECT`, `DEFAULT VALUES`, `OR REPLACE/IGNORE/...`, `ON CONFLICT ... DO NOTHING / DO UPDATE`), `REPLACE`, `UPDATE`, `DELETE`.
- `CREATE TABLE` (column and table constraints: `PRIMARY KEY`, `UNIQUE`, `NOT NULL`, `DEFAULT`, `CHECK`, `REFERENCES` parsed), `CREATE [UNIQUE] INDEX`, `DROP TABLE/INDEX`, `ALTER TABLE ... ADD COLUMN`, `AUTOINCREMENT`.
- `BEGIN [DEFERRED|IMMEDIATE|EXCLUSIVE]`, `COMMIT`, `ROLLBACK`; a failed statement inside a transaction is undone alone (statement savepoint).
- `PRAGMA` user_version, application_id, schema_version, page_size, page_count, freelist_count, table_info, index_list, index_info, integrity_check, synchronous, busy_timeout, journal_mode (reads `delete`), foreign_keys (accepted, not enforced), encoding.
- Values `INTEGER`, `REAL`, `TEXT`, `BLOB`, `NULL`, SQLite's type affinity and comparison rules, collations BINARY/NOCASE/RTRIM (in expressions and `ORDER BY`), parameters `?`, `?N`, `:name`, `@name`, `$name`.
- About 45 functions: `abs, length, substr, instr, replace, trim/ltrim/rtrim, upper, lower, like, glob, coalesce, ifnull, nullif, iif, min, max, typeof, round, printf/format, hex, unhex, quote, char, unicode, zeroblob, randomblob, random, changes, last_insert_rowid, sqlite_version`, `CAST`, aggregates `count, sum, total, avg, min, max, group_concat`, and the **date and time functions** (`date, time, datetime, julianday, strftime, unixepoch`, modifiers, `CURRENT_TIMESTAMP`). `upper/lower` fold ASCII only, like SQLite without ICU.

Programming interface (`lib/db/sqlite.fi`): `db_open(path, flags)`, `db_close`,
`db_exec`, `db_prepare`, `stmt_step/reset/finalize`, `stmt_bind_null/int/real/text/blob`,
`stmt_clear_bindings`, `stmt_param_count`, `stmt_column_count/name/type/int/real/text/blob/is_null`,
`db_last_insert_rowid`, `db_changes`, `db_total_changes`, `db_begin/commit/rollback`,
`db_set_busy_timeout`, `db_set_synchronous`, `db_set_cache_pages`, `db_query_int/text/all`,
`last_error_message`. Every function answers `DbError!T` (`Busy, Locked, Io, Corrupt, NotADb,
Unsupported, Full, ReadOnly, TooBig, NoMemory, Misuse, Constraint, Syntax, NoSuchTable,
NoSuchColumn, Exists, Range, Sql`), and `last_error_message()` has SQLite's wording
("UNIQUE constraint failed: t.id", "no such table: t").

## What is not supported (honest list)

- **No WAL.** A file in WAL mode answers `Unsupported`; the journal is the rollback journal (`journal_mode = DELETE`, the SQLite default). Readers and a writer exclude each other; they do not run side by side.
- **UTF-8 text only.** A UTF-16 file answers `Unsupported`.
- **Auto-vacuum files are read-only here**; every write answers `Unsupported`.
- Not parsed: `WITH`/recursive queries, window functions, `RETURNING`, `RIGHT/FULL/NATURAL JOIN`, parenthesized joins, `UPDATE ... FROM`, `UPDATE/DELETE ... ORDER BY/LIMIT`, generated columns, `WITHOUT ROWID`, `CREATE VIEW/TRIGGER/VIRTUAL TABLE`, partial and expression indexes, `ATTACH`, `VACUUM`, `ALTER TABLE ... RENAME / DROP COLUMN`, `CREATE TABLE ... AS SELECT`, `MATCH/REGEXP`.
- A file that already **has views** is fine (tables work; a query on the view says "no such table"). A file that **has triggers** can be read, but every write answers `Unsupported` (a trigger that does not fire would corrupt the meaning).
- Indexes and `UNIQUE`/`PRIMARY KEY` on a column with `COLLATE NOCASE/RTRIM`: `Unsupported`.
- Foreign keys are parsed and stored, not enforced.
- **One connection per file per process on POSIX** (the OS drops all `flock`-style locks of a file when one descriptor closes; open it once and share the handle). Other processes, and SQLite itself, are fine.
- Limits: 2,000 columns per table, 16 indexes per table, 32 columns per index, 8 tables per join, 16 aggregates per query, 16 arguments per function, a statement of at most 1,000,000 bytes, `zeroblob`/`randomblob` of at most 10^9 bytes, 2^30 - 1 pages (`DbError::Full` beyond), a B-tree depth of 20 (a deeper file is `Corrupt`), page sizes 512 to 65,536.
- **Speed**: far slower than C SQLite (table below). It is a cache and a history store, not a data warehouse.
- **Real Windows is untested** (see below); the Windows build runs under Wine.

## Safety

- **Crash safety.** The commit order is the journal's: old pages to the journal, sync, the record count in the journal header, sync, `EXCLUSIVE`, the changed pages into the file, sync, delete the journal (this is the moment of the commit), sync the directory. `tools/db/check_crash.py` kills the process (SIGKILL, in the middle of a page write when the event is a write) at **every** commit event, and from outside (`kill -9` at random moments of a long run); after each kill **SQLite and lib/db** open the file, roll the hot journal back, and the invariants hold (money conserved, log counts, `integrity_check` ok).
- **Hostile files.** `tools/db/check_hostile.py` damages valid SQLite files (flipped bytes, pages overwritten with random data or with other pages -- cycles, header fields set to extremes, cell pointers pointed anywhere, truncated and extended files, page numbers out of range) and runs reads **and writes** on them: no signal, no arithmetic trap, no hang, no big memory. The answer is an error or an answer, never a crash. Every page read is bounds-checked; cell offsets, overflow chains (cycle detection), freelists and tree depth are limited.
- **Size limits** as above; memory is bounded by the page cache (`db_set_cache_pages`, default 1,000 pages) plus per-statement arenas.
- **One writer.** Locks are the byte ranges SQLite's unix VFS uses (`SHARED`, `RESERVED`, `PENDING`, `EXCLUSIVE` at `0x40000000`): lib/db and SQLite exclude each other in both directions (`tools/db/check_lock.py`: SQLite and lib/db processes on one file, writers racing, no lost update, no deadlock). Windows uses `LockFileEx`/`UnlockFileEx` (`lib/db/vfsos.windows.fi`; four imports added to `compiler/src/win.rs`: `LockFileEx`, `UnlockFileEx`, `SetEndOfFile`, `SetFilePointerEx`). `db_set_busy_timeout` retries a busy lock (default 2,000 ms).

## Benchmarks

Measured on this server (20 cores, load about 12 from other work, so absolute numbers are noisy;
the **ratio** is what matters). Both columns do the same work through prepared statements: Firn
`release-fast` build of `tools/db/bench.fi`, and `tools/db/bench_sqlite.c` against the system
`libsqlite3` (3.40.1, `gcc -O2`). Milliseconds, one run.

| work | lib/db | SQLite | ratio |
|---|---:|---:|---:|
| 100,000 inserts, one transaction (3 columns, one index) | 10,758 | 311 | 35 x |
| 1,000,000 inserts, one transaction (no index) | 81,841 | 1,528 | 54 x |
| 10,000 inserts, each its own transaction, no fsync | 3,650 | 1,480 | 2.5 x |
| 200 inserts, each its own transaction, with fsync | 1,281 | 794 | 1.6 x |
| scan of 1,000,000 rows: `count(*) WHERE a = 7 AND b > 100` | 1,202 | 66 | 18 x |
| scan of 1,000,000 rows: `sum(a * b)` | 1,902 | 160 | 12 x |
| scan of 1,000,000 rows: text compare and `id % 3` | 2,688 | 171 | 16 x |
| 500,000 rows through `stmt_step` | 1,466 | 176 | 8 x |
| `CREATE INDEX` on 1,000,000 rows (two indexes, the second timed) | 33,597 | 562 | 60 x |
| 100,000 lookups by rowid | 2,594 | 1,625 | 1.6 x |
| 2,000 lookups through an index, 1,000 rows each | 9,740 | 176 | 55 x |
| `ORDER BY b, id LIMIT 100000` of 1,000,000 rows | 9,236 | 13 | (SQLite walks an index; lib/db sorts) |
| `GROUP BY` of 1,000,000 rows | 7,561 | 2,484 | 3 x |
| `UPDATE` of 50,000 rows with an index | 2,646 | 212 | 12 x |
| `DELETE` of 33,000 rows with an index | 1,399 | 115 | 12 x |
| file size of the insert test | 11,448 pages | 10,708 pages | 1.07 x |

Reading it honestly:

- **Where it is close**: commits (fsync dominates), single lookups by rowid, grouping.
- **Where it is far**: bulk inserts and index maintenance (35 to 60 x), and index range reads. A profile (callgrind) of the insert shows no single hot spot: B-tree cell parsing, record comparison, varint and byte helpers, each a function call because the compiler does not inline across modules. About 90,000 instructions per insert.
- The `ORDER BY` row compares unlike things: SQLite reads the index `s_b` in order; lib/db's planner does not use an index to avoid a sort.
- Typical use stays comfortable: a launcher's cache of a few thousand rows is a few hundred milliseconds even when it is rebuilt, and an interactive query touches few pages. The results are identical to SQLite's (`check` line of the benchmark: the same aggregates); the files have the same size within 7 %.
- Open items for speed are in the roadmap: inlining of the B-tree primitives, a cached cursor path for sequential inserts, a planner that uses an index for `ORDER BY`.

## Tests

| test | what | result |
|---|---|---|
| `tests/2160_db_btree.fi` | pager and B-trees: tables and indexes, overflow chains, balancing, freelist, cache spills, statement savepoints | in every build level and on AArch64 |
| `tests/2161_db_sql.fi` | SQL end to end: DDL, DML, joins, aggregates, sub-selects, upsert, transactions | same |
| `tests/2162_db_expr.fi` | values, affinity, comparison, functions, dates | same |
| `tests/2164_db_readsqlite.fi` | `tests/data/db/sample.db` (made by SQLite: page size 512, overflow, indexes, `user_version` 42) read by lib/db | same |
| `tests/2165_db_api.fi` | the programming interface: binding of every type, reset and reuse, errors, read-only, reopen | same |
| `examples/db_modrinth_cache.fi`, `examples/db_game_history.fi` | the two applications, 20 and 17 self-checks | same |
| `tools/db/run.sh` (section 77 of `test.sh`) | held against Python's `sqlite3` (SQLite 3.40.1): `check_parse.py` (parser accepts and refuses like SQLite), `check_expr.py` (constant expressions bit for bit, including dates), `check_bt.py`, `check_idx.py` (trees edited by lib/db are read by SQLite), `check_select.py` (106 queries), `check_dml.py` (statements, 1,500 random statements, files read both ways), `check_crash.py`, `check_lock.py`, `check_hostile.py`; then the same programs built for `x86_64-windows` and run under Wine | see `docs/APPLICATION_LIBS.md` and the log of the last run |

Reproduce: `bash tools/db/run.sh` (`quick` for fewer hostile files and no Windows build); the
benchmark: `firnc --opt-level=release-fast -o bench tools/db/bench.fi && ./bench <dir>` and
`gcc -O2 tools/db/bench_sqlite.c libsqlite3.so.0 -o bench_sqlite && ./bench_sqlite <dir>`.

## Windows

`vfsos.windows.fi` is picked by the module twin mechanism (`import db.vfsos` on
`--target=x86_64-windows`): same names, `CreateFileW`/`ReadFile`/`WriteFile`/`FlushFileBuffers`/
`LockFileEx`. The whole SQL suite, the crash points and the hostile files run on it under Wine.
**Real Windows has not run it** (the development machine cannot reach one). Things Wine cannot
show: the behaviour of a virus scanner holding a fresh file, `LockFileEx` semantics between two
real processes of different users, the effect of `FlushFileBuffers` on a real disk. A test kit
(`tools/db/winkit.sh`) packs the probe programs and the checks for a real machine.

## Where things are

| file | what |
|---|---|
| `lib/db/vfsos.fi`, `vfsos.windows.fi` | open/read/write/sync/truncate/locks |
| `lib/db/pager.fi` | cache, locks, journal, commit, freelist |
| `lib/db/btree.fi`, `record.fi` | B-trees (table and index), the record format |
| `lib/db/sqllex.fi`, `sqlparse.fi`, `sqlast.fi` | text to tree |
| `lib/db/expr.fi`, `value.fi`, `datetime.fi` | expressions, affinity and comparison, dates |
| `lib/db/plan.fi`, `select.fi`, `rowset.fi` | access paths, joins, sort and group |
| `lib/db/dml.fi`, `ddl.fi`, `schema.fi`, `dbcore.fi` | writes, schema, the connection |
| `lib/db/sqlite.fi` | the public interface |
| `tools/db/` | the checks against SQLite, the benchmark, the profiler script |
