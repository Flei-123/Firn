#!/bin/bash
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FIRNC="${FIRNC:-/root/firn/compiler/target/release/firnc}"
export FIRNLIB="$ROOT/lib"
D="$(mktemp -d)"
chmod 755 "$D"
BIN=""
cleanup() {
  [ -n "$BIN" ] && setpriv --reuid=nobody --regid=nogroup --clear-groups "$BIN/pg_ctl" -D "$D/data" -m immediate stop >/dev/null 2>&1
  rm -rf "$D"
}
trap cleanup EXIT
fail=0
(cd "$D" && apt-get download postgresql-15 >/dev/null 2>&1 && dpkg -x postgresql-15_*.deb x) || { echo "postgres download failed"; exit 1; }
BIN="$D/x/usr/lib/postgresql/15/bin"
chmod -R a+rX "$D"
for t in pg_check pg_fuzz_client setup_roles; do
  "$FIRNC" "$ROOT/tools/pg/$t.fi" -o "$D/$t" || { echo "compile $t failed"; exit 1; }
done
mkdir "$D/data"; chown nobody "$D" "$D/data"
echo secretpw > "$D/pw"
PORT=$(python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
cd /tmp
as_pg() { setpriv --reuid=nobody --regid=nogroup --clear-groups "$@"; }
as_pg "$BIN/initdb" -D "$D/data" -U firn --pwfile="$D/pw" --auth-host=scram-sha-256 --auth-local=scram-sha-256 -E UTF8 --locale=C >/dev/null || { echo initdb failed; exit 1; }
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -keyout "$D/data/server.key" -out "$D/data/server.crt" -days 2 -subj /CN=localhost -addext subjectAltName=DNS:localhost >/dev/null 2>&1
chown nobody "$D/data"/server.*; chmod 600 "$D/data/server.key"
as_pg "$BIN/pg_ctl" -D "$D/data" -l "$D/log" -o "-p $PORT -k $D -c listen_addresses=127.0.0.1 -c ssl=on" -w start >/dev/null || { cat "$D/log"; exit 1; }
echo "$PORT" | "$D/setup_roles" || { echo "role setup failed"; exit 1; }
(echo "host all md5user 127.0.0.1/32 md5"; echo "host all clearuser 127.0.0.1/32 password"; cat "$D/data/pg_hba.conf") > "$D/hba.new"
cp "$D/hba.new" "$D/data/pg_hba.conf"; chown nobody "$D/data/pg_hba.conf"
as_pg "$BIN/pg_ctl" -D "$D/data" reload >/dev/null; sleep 1
printf '%s\nfirn\nsecretpw\npostgres\n' "$PORT" | timeout 300 "$D/pg_check" || fail=1
printf '%s\nfirn\nsecretpw\npostgres\n%s' "$PORT" "$D/data/server.crt" | timeout 300 "$D/pg_check" || fail=1
printf '%s\nmd5user\nmd5pw\npostgres\n' "$PORT" | timeout 300 "$D/pg_check" || fail=1
printf '%s\nclearuser\nclearpw\npostgres\n' "$PORT" | timeout 300 "$D/pg_check" || fail=1
python3 -I "$ROOT/tools/pg/pg_fuzz.py" "$D/pg_fuzz_client" 500 || fail=1
[ $fail = 0 ] && echo "pg all ok" || echo "pg FAILED"
exit $fail
