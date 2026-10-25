FNR == NR {
    if ($0 ~ /^[0-9]+:/) {
        n = split($0, parts, /[: ]+/)
        for (i = 2; i <= n; i++) {
            if (parts[i] == "") continue
            if (parts[i] in stage_of) dup = dup " " parts[i]
            stage_of[parts[i]] = parts[1] + 0
        }
        if (parts[1] + 0 > last_stage) last_stage = parts[1] + 0
    }
    next
}
/^echo "== [0-9a-z]+\. / {
    id = $0
    sub(/^echo "== /, "", id)
    sub(/\..*/, "", id)
    st = (id in stage_of) ? stage_of[id] : last_stage
    seen[id] = 1
    if (id != "1" && index(ids[st] " ", " " id " ") == 0) ids[st] = ids[st] " " id
    section = id
    if (id == "1") {
        mode = "common"
        if (emit) {
            print "__stage_t0=$SECONDS"
            print "__stage_tick() { echo \"   [elapsed since previous mark: $((SECONDS - __stage_t0))s]\"; __stage_t0=$SECONDS; }"
        }
    }
    else if (list == 1 || st == stage) mode = "sel"
    else mode = "skip"
    if (mode == "sel" && emit) print "__stage_tick"
    next_is_section = 1
}
section == "2" && $0 == "rm -rf \"$WORK\"" { mode = "common" }
/^TOTAL=/ { mode = "common"; if (emit && stage_started) print "__stage_tick" }
{
    if (mode == "sel" || mode == "common" || mode == "pre") {
        if (emit) print
        if (mode == "sel") stage_started = 1
    }
}
BEGIN { mode = "pre"; emit = (list == 1) ? 0 : 1 }
END {
    if (list == 1) {
        for (s = 1; s <= last_stage; s++) print s ":" ids[s]
    }
    for (i in stage_of) if (!(i in seen)) { missing = missing " " i }
    if (dup != "" || missing != "") {
        print "teststages: stages.txt problem; duplicate:" dup " unknown:" missing > "/dev/stderr"
        exit 2
    }
}
