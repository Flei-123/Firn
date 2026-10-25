FNR == NR {
    if ($0 ~ /^[0-9]+:/) {
        n = split($0, parts, /[: ]+/)
        for (i = 2; i <= n; i++) {
            if (parts[i] == "") continue
            base = parts[i]
            spec = ""
            if (index(base, "@") > 0) {
                spec = substr(base, index(base, "@") + 1)
                base = substr(base, 1, index(base, "@") - 1)
                sharded[base] = 1
            }
            key = parts[1] + 0
            if ((base in stage_of) && !(base in sharded)) dup = dup " " base
            if ((key SUBSEP base) in has) dup = dup " " base
            has[key, base] = 1
            if (spec != "") shard[key, base] = spec
            if (!(base in stage_of)) stage_of[base] = key
        }
        if (parts[1] + 0 > last_stage) last_stage = parts[1] + 0
    }
    next
}
/^echo "== [0-9a-z]+\. / {
    id = $0
    sub(/^echo "== /, "", id)
    sub(/\..*/, "", id)
    seen[id] = 1
    if (id in stage_of) {
        for (s = 1; s <= last_stage; s++) {
            if (!((s SUBSEP id) in has)) continue
            item = id
            if ((s SUBSEP id) in shard) item = id "@" shard[s, id]
            if (id != "1" && index(ids[s] " ", " " item " ") == 0) ids[s] = ids[s] " " item
        }
        sel = ((stage SUBSEP id) in has)
    } else {
        if (id != "1" && index(ids[last_stage] " ", " " id " ") == 0) ids[last_stage] = ids[last_stage] " " id
        sel = (stage == last_stage)
    }
    section = id
    if (id == "1") {
        mode = "common"
        if (emit) {
            print "__stage_t0=$SECONDS"
            print "__stage_tick() { echo \"   [elapsed since previous mark: $((SECONDS - __stage_t0))s]\"; __stage_t0=$SECONDS; }"
        }
    }
    else if (list == 1 || sel) mode = "sel"
    else mode = "skip"
    if (mode == "sel" && emit) {
        print "__stage_tick"
        if ((stage SUBSEP id) in shard) {
            split(shard[stage, id], sp, "/")
            print "TEST_SHARD_INDEX=" (sp[1] - 1) "; TEST_SHARD_COUNT=" sp[2]
        }
    }
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
