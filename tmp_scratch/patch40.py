import re
p = '/root/firn-wt-db/tools/db/run.sh'
s = open(p).read()
a = s.index("run() {")
b = s.index("if [ \"${1:-}\" != \"quick\" ]; then")
new = '''run() {
    local name=$1
    local lines=$2
    shift 2
    echo "-- $name"
    "$@" > "$W/out.log" 2>&1 && ok=0 || ok=$?
    tail -n "$lines" "$W/out.log" | sed 's/^/   /'
    if [ $ok -ne 0 ]; then
        echo "   FAILED: $name"
        rc=1
    fi
}
run "parser" 2 python3 tools/db/check_parse.py "$W/parse_probe"
run "expressions" 3 python3 tools/db/check_expr.py "$W/expr_probe"
run "table B-tree" 1 python3 tools/db/check_bt.py "$W/bt_probe"
run "index B-tree" 1 python3 tools/db/check_idx.py "$W/bt_probe"
run "SELECT" 3 python3 tools/db/check_select.py "$W/sql_probe"
run "DML, DDL, transactions" 8 python3 tools/db/check_dml.py "$W/sql_probe"
run "crashes" 8 python3 tools/db/check_crash.py "$W/sql_probe"
run "locks" 12 python3 tools/db/check_lock.py "$W/sql_probe"
run "damaged files ($HOSTILE)" 4 python3 tools/db/check_hostile.py "$W/sql_probe" "$HOSTILE"
'''
s = s[:a] + new + s[b:]
s = s.replace('''            run "SELECT (Wine)" python3 tools/db/check_select.py "$W/sql_probe.exe" | tail -2
            run "DML, DDL, transactions (Wine)" python3 tools/db/check_dml.py "$W/sql_probe.exe" | tail -8
            run "crash points (Wine)" python3 tools/db/check_crash.py "$W/sql_probe.exe" | tail -6
            run "damaged files (Wine)" python3 tools/db/check_hostile.py "$W/sql_probe.exe" 150 | tail -3''', '''            run "SELECT (Wine)" 2 python3 tools/db/check_select.py "$W/sql_probe.exe"
            run "DML, DDL, transactions (Wine)" 8 python3 tools/db/check_dml.py "$W/sql_probe.exe"
            run "crash points (Wine)" 6 python3 tools/db/check_crash.py "$W/sql_probe.exe"
            run "damaged files (Wine)" 3 python3 tools/db/check_hostile.py "$W/sql_probe.exe" 150''')
open(p, 'w').write(s)
print('ok40')
