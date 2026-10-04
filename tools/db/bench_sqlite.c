/* SPDX-License-Identifier: MPL-2.0
 * tools/db/bench_sqlite.c -- the same work as tools/db/bench.fi, through SQLite's C API
 * (prepared statements, bound parameters), for the comparison table in docs/DB.md.
 *
 *   gcc -O2 tools/db/bench_sqlite.c /usr/lib/x86_64-linux-gnu/libsqlite3.so.0 -o bench_sqlite
 *   ./bench_sqlite <dir> [rows]
 *
 * (no sqlite3.h is needed: the few prototypes are declared here)
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

typedef struct sqlite3 sqlite3;
typedef struct sqlite3_stmt sqlite3_stmt;
extern int sqlite3_open(const char *, sqlite3 **);
extern int sqlite3_close(sqlite3 *);
extern int sqlite3_exec(sqlite3 *, const char *, void *, void *, char **);
extern int sqlite3_prepare_v2(sqlite3 *, const char *, int, sqlite3_stmt **, const char **);
extern int sqlite3_step(sqlite3_stmt *);
extern int sqlite3_reset(sqlite3_stmt *);
extern int sqlite3_finalize(sqlite3_stmt *);
extern int sqlite3_bind_int64(sqlite3_stmt *, int, long long);
extern int sqlite3_bind_double(sqlite3_stmt *, int, double);
extern int sqlite3_bind_text(sqlite3_stmt *, int, const char *, int, void (*)(void *));
extern long long sqlite3_column_int64(sqlite3_stmt *, int);
extern const char *sqlite3_libversion(void);

static long long now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long long)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}
static void report(const char *name, long long t0) { printf("%s %lld\n", name, now_ms() - t0); fflush(stdout); }
static sqlite3 *db;
static void ex(const char *sql) {
    char *err = 0;
    if (sqlite3_exec(db, sql, 0, 0, &err)) { fprintf(stderr, "ERROR %s: %s\n", sql, err ? err : "?"); }
}
static sqlite3_stmt *prep(const char *sql) {
    sqlite3_stmt *st = 0;
    if (sqlite3_prepare_v2(db, sql, -1, &st, 0)) { fprintf(stderr, "prepare failed: %s\n", sql); exit(1); }
    return st;
}
static long long scalar(const char *sql) {
    sqlite3_stmt *st = prep(sql);
    long long v = -1;
    if (sqlite3_step(st) == 100) v = sqlite3_column_int64(st, 0);
    sqlite3_finalize(st);
    return v;
}

int main(int argc, char **argv) {
    if (argc < 2) return 2;
    long long n = 100000, big = 1000000;
    if (argc > 2) { n = atoll(argv[2]); big = n * 10; }
    char path[4096];
    snprintf(path, sizeof path, "%s/bench.db", argv[1]);
    if (sqlite3_open(path, &db)) return 1;
    ex("CREATE TABLE b(id INTEGER PRIMARY KEY, a INTEGER, name TEXT, v REAL)");
    ex("CREATE INDEX b_a ON b(a)");
    long long t0 = now_ms();
    ex("BEGIN");
    sqlite3_stmt *ins = prep("INSERT INTO b(a, name, v) VALUES (?, ?, ?)");
    for (long long i = 0; i < n; i++) {
        sqlite3_bind_int64(ins, 1, (i * 7919) % 10007);
        sqlite3_bind_text(ins, 2, "name number", -1, 0);
        sqlite3_bind_double(ins, 3, (double)i * 0.5);
        sqlite3_step(ins);
        sqlite3_reset(ins);
    }
    sqlite3_finalize(ins);
    ex("COMMIT");
    report("insert_100k_one_txn", t0);
    ex("PRAGMA synchronous = OFF");
    ex("CREATE TABLE c(id INTEGER PRIMARY KEY, a INTEGER)");
    t0 = now_ms();
    sqlite3_stmt *ins2 = prep("INSERT INTO c(a) VALUES (?)");
    for (long long k = 0; k < n / 10; k++) { sqlite3_bind_int64(ins2, 1, k); sqlite3_step(ins2); sqlite3_reset(ins2); }
    sqlite3_finalize(ins2);
    report("insert_10k_own_txn_nosync", t0);
    ex("PRAGMA synchronous = FULL");
    t0 = now_ms();
    sqlite3_stmt *ins3 = prep("INSERT INTO c(a) VALUES (?)");
    for (long long k = 0; k < 200; k++) { sqlite3_bind_int64(ins3, 1, k); sqlite3_step(ins3); sqlite3_reset(ins3); }
    sqlite3_finalize(ins3);
    report("insert_200_own_txn_fsync", t0);
    ex("CREATE TABLE s(id INTEGER PRIMARY KEY, a INTEGER, b INTEGER, t TEXT)");
    t0 = now_ms();
    ex("BEGIN");
    sqlite3_stmt *ins4 = prep("INSERT INTO s(a, b, t) VALUES (?, ?, ?)");
    for (long long j = 0; j < big; j++) {
        sqlite3_bind_int64(ins4, 1, j % 1000);
        sqlite3_bind_int64(ins4, 2, (j * 31) % 977);
        sqlite3_bind_text(ins4, 3, "row", -1, 0);
        sqlite3_step(ins4);
        sqlite3_reset(ins4);
    }
    sqlite3_finalize(ins4);
    ex("COMMIT");
    report("insert_1M_one_txn", t0);
    t0 = now_ms();
    long long c1 = scalar("SELECT count(*) FROM s WHERE a = 7 AND b > 100");
    report("scan_1M_count_where", t0);
    t0 = now_ms();
    long long c2 = scalar("SELECT sum(a * b) FROM s");
    report("scan_1M_sum_expr", t0);
    t0 = now_ms();
    long long c3 = scalar("SELECT count(*) FROM s WHERE t = 'row' AND id % 3 = 0");
    report("scan_1M_text_filter", t0);
    t0 = now_ms();
    sqlite3_stmt *sel = prep("SELECT id, a, b FROM s WHERE a < 500");
    long long acc = 0;
    while (sqlite3_step(sel) == 100) acc += sqlite3_column_int64(sel, 1);
    sqlite3_finalize(sel);
    report("scan_1M_rows_through_api", t0);
    ex("CREATE INDEX s_a ON s(a)");
    t0 = now_ms();
    ex("CREATE INDEX s_b ON s(b)");
    report("create_index_1M", t0);
    t0 = now_ms();
    sqlite3_stmt *look = prep("SELECT b FROM s WHERE id = ?");
    long long sum = 0;
    for (long long q = 0; q < n; q++) {
        sqlite3_bind_int64(look, 1, 1 + (q * 7919) % big);
        if (sqlite3_step(look) == 100) sum += sqlite3_column_int64(look, 0);
        sqlite3_reset(look);
    }
    sqlite3_finalize(look);
    report("lookup_100k_by_rowid", t0);
    t0 = now_ms();
    sqlite3_stmt *look2 = prep("SELECT count(*) FROM s WHERE a = ?");
    for (long long q = 0; q < 2000; q++) {
        sqlite3_bind_int64(look2, 1, q % 1000);
        if (sqlite3_step(look2) == 100) sum += sqlite3_column_int64(look2, 0);
        sqlite3_reset(look2);
    }
    sqlite3_finalize(look2);
    report("lookup_2k_by_index_1000_rows_each", t0);
    t0 = now_ms();
    long long c4 = scalar("SELECT count(*) FROM (SELECT b, id FROM s ORDER BY b, id LIMIT 100000)");
    report("order_by_100k_of_1M", t0);
    t0 = now_ms();
    long long c5 = scalar("SELECT count(*) FROM (SELECT a, count(*), avg(b) FROM s GROUP BY a)");
    report("group_by_1M", t0);
    t0 = now_ms();
    ex("UPDATE b SET v = v + 1, a = a + 1 WHERE id % 2 = 0");
    report("update_50k_with_index", t0);
    t0 = now_ms();
    ex("DELETE FROM b WHERE id % 3 = 0");
    report("delete_33k_with_index", t0);
    long long pages = scalar("PRAGMA page_count");
    printf("pages %lld\n", pages);
    printf("check %lld%lld%lld%lld%lld\n", c1, c2, c3, c4, c5);
    sqlite3_close(db);
    (void)acc; (void)sum;
    return 0;
}
