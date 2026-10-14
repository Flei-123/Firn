# Linguist PR draft: Firn (NOT submitted)

Nothing here was sent to GitHub. Checked 2026-10-13 against
`github-linguist/linguist` CONTRIBUTING.md ("Adding a language") and
`vendor/licenses/config.yml` (main branch).

## Contents
- `languages.yml.entry` -- entry to insert alphabetically into `lib/linguist/languages.yml` (no `language_id`).
- `firn.tmLanguage.json` -- grammar (copy of `editors/vscode-firn/syntaxes/`; MPL-2.0).
- `samples/` -- `csv.fi`, `glob.fi`, `stats.fi` (real library code from `lib/std/`, 770-940 lines each, MPL-2.0, no hello-world).

## Linguist requirements vs. Firn
| Requirement | Linguist says | Firn today |
|---|---|---|
| Usage | >= 2,000 `.fi` files indexed on GitHub in the last year, excl. forks, for an extension that occurs several times per repo; reasonable spread over unique `user/repo` | **NOT MET.** `Flei-123/Firn` alone has ~2,100 tracked `.fi` files, but that is ONE repo (spread requirement) and the repo is not yet indexed as public Firn usage. Needs many independent repos. |
| Grammar licence | must be on allow-list: apache-2.0, bsd-2/3, cc0, isc, `mit`, mpl-2.0, ncsa, permissive, unlicense, wtfpl, zlib | met: MPL-2.0 |
| Grammar added via | `script/add-grammar <repo-URL>` (needs Docker, the grammar must live in its own public repo) | open: grammar is still inside the Firn repo; a standalone repo (e.g. `Flei-123/firn-tmLanguage`) is needed |
| Samples | real-world code, no hello-world/tutorial; licence covering them stated in PR | prepared (3 files), licence MPL-2.0 |
| `.fi` clash | if extension is used by another language: 2 samples each + heuristic | not verified for `.fi` in current languages.yml (grep found no entry); re-check at submission |
| language_id | omit, then run `script/update-ids` | n/a |
| PR body | template + GitHub Code Search links proving in-the-wild usage + sample licence | cannot be produced: Code Search needs login and shows a single repo |
| Color / ace_mode / tm_scope | CONTRIBUTING gives no rule; `tm_scope: source.firn` matches grammar scopeName; `ace_mode: text` is the neutral fallback | chosen, `#4f8fba` is arbitrary |

## Verdict
Firn does **not** meet the usage threshold yet (2,000 files over many repos).
Submitting now would be rejected. Revisit once external Firn repos exist
(Certus, OrientOS, FirnChat, third parties) and are public.

## Submission steps (later)
1. Publish grammar as own public repo with LICENSE.
2. Fork linguist, add entry, `script/add-grammar <url>`, copy samples to `samples/Firn/`, `script/update-ids`.
3. Open PR with Code Search links.
