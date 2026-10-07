# tools/testkit -- shared test helpers (shell and Python)

Replaces the ~240 private copies of `ok()`/`bad()` and the ~58 copies of a PPM reader found by the
cross-project audit (07.10.2026). Repos use it by path: `FIRN` (default `/root/firn`).

Shell (`testkit.sh`, source it): `ok`, `bad`, `check "label" cmd...`, `check_out "label" want cmd...`,
`tk_tmpdir` (scratch dir in `$D`, removed on exit), `tk_summary [name]`. Counters `pass` and `fail`; the printed lines are
`  OK    label` / `  FAIL  label`, byte for byte the old copies (section runners grep for them).

    . "${FIRN:-/root/firn}/tools/testkit/testkit.sh"

Python (`testkit.py`):

    sys.path.insert(0, os.path.join(os.environ.get("FIRN", "/root/firn"), "tools/testkit"))
    from testkit import Kit, read_ppm, parse_ppm, parse_ppm_np, ppm_diff, ppm_crop, ppm_to_png

`adopt.py FILE.sh ...` switches scripts that carry the exact old pair of definitions (counters `pass`/`fail`) to the kit
(`--dry-run` shows which); scripts with another form are reported and left alone (Certus `CHECKS`/`FAILS`, OpenPlan
`check(ok, what)` are not covered yet: 100+ files, see the roadmap).

`selftest.sh` runs in Firn's `test.sh` section 21c.
