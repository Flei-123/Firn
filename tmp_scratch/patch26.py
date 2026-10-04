import re
base = '/root/firn-wt-db/tools/db/'
for name in ['check_dml.py', 'check_select.py', 'check_crash.py']:
    s = open(base + name).read()
    if 'RUNNER' not in s:
        s = s.replace("probe = sys.argv[1]\n", "probe = sys.argv[1]\nRUNNER = os.environ.get(\"RUNNER\", \"\").split()\n", 1)
        s = s.replace("args = [probe, db, script]", "args = RUNNER + [probe, db, script]")
        s = s.replace("r = subprocess.run([probe, work, script], capture_output=True)", "r = subprocess.run(RUNNER + [probe, work, script], capture_output=True)")
        s = s.replace("args = [probe, db, sc] + ([str(crash)] if crash else [])", "args = RUNNER + [probe, db, sc] + ([str(crash)] if crash else [])")
        s = s.replace("killed = r.returncode == -9", "killed = r.returncode in (-9, 9)")
        s = s.replace("p = subprocess.Popen([probe, work, sc], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)", "p = subprocess.Popen(RUNNER + [probe, work, sc], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)")
        open(base + name, 'w').write(s)
# the external-kill part is skipped under a runner (the launcher would die, not the program)
s = open(base + 'check_crash.py').read()
s = s.replace('    print("random kill -9 during a stream of transactions:")', '    print("random kill -9 during a stream of transactions:")\n    if RUNNER:\n        print("  (skipped under a runner: killing the launcher is not killing the program)")\n        print("FAILED" if fails else "ALL OK", fails)\n        sys.exit(1 if fails else 0)')
open(base + 'check_crash.py', 'w').write(s)
print('ok26')
