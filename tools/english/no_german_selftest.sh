#!/usr/bin/env bash
# no_german_selftest.sh -- proof that the shared guard counts, ratchets and honours the allow list.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
D="$(mktemp -d)"; trap 'rm -rf "$D"' EXIT
fail=0
chk() { if [ "$2" = "$3" ]; then echo "  OK    $1"; else echo "  FAIL  $1 (got '$2', want '$3')"; fail=1; fi; }
cd "$D" && git init -q .
mkdir -p src locale
printf '// Das ist ein deutscher Kommentar und wir haben ihn geschrieben\nfn zaehle_zeilen() { return 1; }\nlet s = "Die Datei wurde nicht gefunden";\n' > src/a.js
printf '// english only, nothing to see\nfn count_lines() { return 1; }\nlet s = "file not found";\n' > src/b.js
printf 'let t = "Die Datei wurde nicht gefunden";\n' > locale/de.js
git add -A   # the guard reads `git ls-files`: staged is enough, no commit (and no identity) needed
sum() { python3 "$here/no_german.py" --root "$D" --summary; }
c="$(sum)"
chk "counts German comment, identifier and string; catalog is ignored" "$c" '{"path": 0, "identifier": 1, "comment": 1, "string": 1}'
python3 "$here/no_german.py" --root "$D" --update-baseline 2>/dev/null
python3 "$here/no_german.py" --root "$D" >/dev/null 2>&1; chk "equal to the baseline passes" "$?" 0
printf '// noch ein deutscher Kommentar, das ist nicht gut\n' >> src/b.js; git add -A
python3 "$here/no_german.py" --root "$D" >/dev/null 2>&1; chk "one more German comment fails" "$?" 1
printf '// noch ein deutscher Kommentar, das ist nicht gut english: ok\n' > src/c.js; git add -A
git rm -qf src/b.js
python3 "$here/no_german.py" --root "$D" >/dev/null 2>&1; chk "marker 'english: ok' and a removed file pass" "$?" 0
python3 "$here/no_german.py" --root "$D" --update-baseline 2>/dev/null; chk "lowering the baseline works" "$?" 0
printf '// noch ein deutscher Kommentar, das ist nicht gut\n' > src/d.js; git add -A
python3 "$here/no_german.py" --root "$D" --update-baseline 2>/dev/null; chk "raising the baseline is refused" "$?" 1
printf '{"skip":["src/d.js"]}' > no-german.json; git add -A
python3 "$here/no_german.py" --root "$D" >/dev/null 2>&1; chk "allow list skip passes again" "$?" 0
[ $fail -eq 0 ] && echo "no_german selftest: all ok" || echo "no_german selftest: FAILED"
exit $fail
