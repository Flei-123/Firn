# vs_python -- the same task in Python and Firn

One dir per task: `NN_name/task.py` (idiomatic Python, stdlib only) and
`NN_name/task.fi` (shortest Firn, existing `lib/` used). Both must do the same
and print the same thing (see `expect.txt` if output is checked).
Measure: `python3 tools/vspy/measure.py examples/vs_python/NN_name`.
Result table + gap list: `docs/FIRN-VS-PYTHON.md`.
