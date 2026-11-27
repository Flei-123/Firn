# Firn for VS Code

Language support for Firn (`.fi`): live errors, completion, hover, go to definition, rename, format on save,
run with F5, tasks with problem matcher, snippets.

Needs `firnc` (language server) and `firn` (run) on `PATH` or set in the settings `firn.compilerPath` / `firn.path`.
Install guide: `docs/IDE.md`. Package: `bash package.sh OUT_DIR` (gives `vscode-firn-<version>.vsix`). Tests: `node --test test/`.
MPL-2.0.
