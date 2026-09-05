# testdata/ — fremde Testdaten (kein Fremdcode)

## html5lib-tokenizer/
Offizielle Tokenizer-Testsuite aus https://github.com/html5lib/html5lib-tests
(Commit 224991ec10db04f056a89eed8b0bd8695fd2950e, geholt 13.08.2026).

* **14 `.test`-Dateien, 6.810 Testfaelle** (JSON).
* Das sind **normative Testdaten**, kein ausfuehrbarer Fremdcode — nach
  `DECISION.md` §7.1 (Auslegung B1) ausdruecklich erlaubt.
* Format: `{"tests":[{"description":..,"input":..,"output":[..],"initialStates":[..],
  "lastStartTag":..,"errors":[..]}]}`. Einige Dateien haben `"xmlViolationTests"`.
  `"doubleEscaped": true` bedeutet, dass `input` und `output` zusaetzlich
  \\uXXXX-entschluesselt werden muessen.

Zaehlung reproduzieren:
    python3 -c "import json,glob;print(sum(len(json.load(open(f)).get('tests',json.load(open(f)).get('xmlViolationTests',[]))) for f in glob.glob('testdata/html5lib-tokenizer/*.test')))"
