#!/bin/bash
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
FIRNC="${FIRNC:-/root/firn/compiler/target/release/firnc}"
export FIRNLIB="$ROOT/lib"
D="$(TMPDIR=/var/tmp mktemp -d)"
PIDFILE="$D/my.pid"
cleanup() {
  [ -s "$PIDFILE" ] && kill "$(cat "$PIDFILE")" 2>/dev/null && sleep 2
  rm -rf "$D"
}
trap cleanup EXIT
fail=0
(cd "$D" && apt-get download mariadb-server-core mariadb-client-core >/dev/null 2>&1 && for f in *.deb; do dpkg -x "$f" x; done) || { echo "mariadb download failed"; exit 1; }
X="$D/x/usr"
for t in mysql_check my_fake_client; do
  "$FIRNC" "$ROOT/tools/mysql/$t.fi" -o "$D/$t" || { echo "compile $t failed"; exit 1; }
done
mkdir "$D/data"
"$X/bin/mariadb-install-db" --no-defaults --basedir="$X" --datadir="$D/data" --user=root --auth-root-authentication-method=normal --skip-test-db >/dev/null 2>&1 || { echo install-db failed; exit 1; }
cat > "$D/init.sql" <<SQL
CREATE DATABASE IF NOT EXISTS test;
CREATE USER 'firn'@'127.0.0.1' IDENTIFIED VIA mysql_native_password USING PASSWORD('secretpw');
GRANT ALL ON *.* TO 'firn'@'127.0.0.1';
SQL
openssl req -x509 -newkey rsa:2048 -nodes -keyout "$D/srv.key" -out "$D/srv.crt" -days 2 -subj /CN=localhost -addext subjectAltName=DNS:localhost >/dev/null 2>&1
PORT=$(python3 -I -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')
setsid nohup "$X/sbin/mariadbd" --no-defaults --basedir="$X" --datadir="$D/data" --socket="$D/my.sock" --port="$PORT" --bind-address=127.0.0.1 --user=root --max-allowed-packet=64M --init-file="$D/init.sql" --ssl-cert="$D/srv.crt" --ssl-key="$D/srv.key" --pid-file="$PIDFILE" --log-error="$D/err.log" >/dev/null 2>&1 &
for i in $(seq 60); do python3 -I -c "import socket; socket.create_connection(('127.0.0.1',$PORT),1)" 2>/dev/null && break; sleep 0.5; done
sleep 1
printf '%s\nfirn\nsecretpw\ntest\n' "$PORT" | timeout 300 "$D/mysql_check" || fail=1
printf '%s\nfirn\nsecretpw\ntest\n%s' "$PORT" "$D/srv.crt" | timeout 300 "$D/mysql_check" || fail=1
python3 -I "$ROOT/tools/mysql/my_fake.py" sha2 "$D/my_fake_client" 3 || fail=1
python3 -I "$ROOT/tools/mysql/my_fake.py" switch "$D/my_fake_client" 3 || fail=1
python3 -I "$ROOT/tools/mysql/my_fake.py" fuzz "$D/my_fake_client" 300 || fail=1
[ $fail = 0 ] && echo "mysql all ok" || echo "mysql FAILED"
exit $fail
