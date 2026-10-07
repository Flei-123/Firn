# The shared "no German in code" guard

One script for every repo: `tools/english/no_german.py` (Python 3, no dependencies; vocabulary = `morphemes.tsv` next to it).
It checks the files `git ls-files` lists: **paths**, **identifiers**, **comments** and **string literals** (German UI text belongs in a catalog).

## Use in a repo
1. `no-german.json` in the repo root (allow list, all keys optional): `skip` (path prefixes), `catalogs` (extra prefixes where German is allowed; `i18n/ locale/ lang/ catalogs/ de.json texts.fi` always count), `words` (identifier parts that look German but are English), `lines` (regex of ignored lines). A single line can carry the marker `english: ok`.
2. `python3 <firn>/tools/english/no_german.py --root . --update-baseline` writes `no-german.baseline.json` (frozen counts per kind). From then on the CI step **fails when any count rises**; it never allows raising the file (needs `--force`).
3. CI step: `bash "${FIRN:-/root/firn}/tools/english/no_german_ci.sh" .` (prints one `OK`/`FAIL` line, exit code 0 = ok).
4. When a rename round lowers a count, lower the baseline in the same commit (`--update-baseline`; the guard prints the hint).

`--summary` prints the four counts as JSON (for `project_metric`); `--files` lists the worst files; without a baseline the run is strict (0 allowed).
Precision on samples (audit 07.10.2026): strings and comments about 95 %, identifiers about 85 % (false hits: `short_circuit`, `digit_count`, `listen_*` -> `words`).

Repos wired (07.10.2026): Firn (`test.sh` 21b), Certus (`tools/ci.sh`), Osum (`test.sh` 58b, next to its own ratchet), OpenPlan (`tools/ci.sh` 2c, next to `tools/english.py`), FirnChat (`tests/run.sh`), FreeViewer (`ci.sh`), OrientStore (`tests/check_english.sh`, first step of `tests/lauf.sh`), Daidalos (`tools/run_tests.sh`), FleiLauncher (`npm run check`), Cockpit (`npm test` via `pretest`), JARVIS (`npm run check`). LogicLab keeps its own `check:english`.
