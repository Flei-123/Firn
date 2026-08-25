# Round B1 — the clamp: tree construction, DOM, computed style

Before this round the browser parts of Firn were big and **not connected**.
The HTML tokeniser produced tokens, the CSS cascade had no elements to point
at, and `lib/dom/` was a GC experiment, not a DOM. This round is the clamp:
one path from bytes of HTML to a tree with a computed style on every
element.

Nothing here is layout, painting or fonts. That is B2 and B3.

## 1. The number

The tree construction runs against the **official html5lib
tree-construction suite**, complete, unfiltered.

```
                             passed      of      quota
before this round (main)      1323      1936     68.34 %
after  this round             1837      1936     94.89 %
```

The suite is 62 `.dat` files with 1936 cases and lies in
`tests/data/html5lib/` (see `PROVENANCE.md` there). It is no longer kept in
the `html5lib/html5lib-tests` repository — with the commit
`224991ec10` ("Tree construction tests have moved to WPT", 26 June 2026)
the directory was deleted there and the README points at
`web-platform-tests/wpt : html/syntax/parsing/resources/*.dat`. That is
where these files come from.

Rules of the run, and they are the point:

* every case counts, `#document-fragment` cases included,
* nothing is skipped and nothing is filtered,
* a `#KAPUTT` note from the binary is a failure,
* the whole tree is compared, not a section of it,
* the same quota has to come out of all three build stages
  (`opt`, `--no-opt`, `dev-fast`).

Per file after the round (only the files that are not at 100 %):

```
processing-instructions.dat    40 / 124        84 missing
webkit02.dat                   45 /  49         4 missing
tests1.dat                    109 / 112         3 missing
scripted_*.dat                  0 /   6         6 missing
adoption02.dat                  2 /   3         1 missing
html5test-com.dat              29 /  30         1 missing
                             ----------------------------
                             1837 / 1936    99 missing
```

## 2. What the tree construction can do now

Round 54 had the insertion modes, the stack of open elements, the list of
active formatting elements with the adoption agency algorithm, implicit
closing and foster parenting. It named three gaps openly
(`tools/html/gaps/README.md`): foreign content, `<template>` and fragment
parsing. **All three are closed.**

**Foreign content** (`lib/browser/foreign.fi`, `foreign_data.fi`,
`lib/browser/tree.fi`)

* the tree construction dispatcher of WHATWG 13.2.6: adjusted current node,
  MathML text integration points (`mi mo mn ms mtext`), HTML integration
  points (`foreignObject desc title`, `annotation-xml` with an `encoding`
  of `text/html` or `application/xhtml+xml`),
* the four correction tables of 13.2.6.5: 37 SVG element names
  (`clippath` → `clipPath`), 58 SVG attribute names
  (`attributename` → `attributeName`), the MathML `definitionurl` and the
  11 foreign attributes (`xlink:href` → prefix `xlink`, local name `href`,
  namespace XLink),
* the breakout tags, including the end tags `br` and `p`,
* `<![CDATA[` inside foreign content. The tokeniser knows no tree and makes
  a bogus comment out of it; the tree construction computes the position of
  the CDATA content and restarts the tokeniser there in its CDATA state.
  The position is COMPUTED (token end minus data length minus 2 or 3,
  depending on whether a `>` closed it), not guessed.

Round 54's own gap file `tools/html/gaps/known_gaps.dat` goes from
**0 of 10** to **9 of 10** closed. The tenth is not a parser gap: it is a
fragment case, and the driver of round 54 (`lib/browser/parse_main.fi`) has
no field for the context element in its job format. One expectation in that
file was corrected while doing it (see the README there).

**`<template>`** — the 23rd insertion mode, the stack of template insertion
modes, the content fragment (`Elem.content`), the redirection of the
insertion point into the template contents, the template rule of foster
parenting, and the end-of-file path through "in template".

**Fragment parsing** — WHATWG 13.2.5 with a context element: the `html`
root, the context element for the adjusted current node and for
`reset the insertion mode appropriately`, the tokeniser start state that
belongs to the context (`title`/`textarea` → RCDATA, `style`/`xmp`/… →
RAWTEXT, `script` → script data, `plaintext` → PLAINTEXT), and the fragment
case of "after body". A fragment has NO last start tag, so no end tag is an
"appropriate end tag" — that is why `<!-- inside </script> -->` in a
`script` context stays text.

Beyond the three named gaps:

* **the relaxed select parser.** WHATWG dropped the insertion modes
  "in select" and "in select in table". `select` now takes arbitrary
  content, `select` joined the scope list and the end tag group, and
  `select`, `option`, `optgroup`, `hr` and `input` got their new rules in
  "in body". One case of round 54 (`tools/html/cases/06_selection_frames.dat`)
  was rewritten to the new rule, with a note in the file.
* **the scripting flag.** It executes nothing; it decides whether
  `noscript` holds raw text or markup. The `.dat` format carries it per
  case (`#script-on`), the runner passes it on.
* **the complete quirks lists** (`lib/browser/quirks_data.fi`): the 55
  public identifier prefixes, the three exact public identifiers, the IBM
  system identifier and the two prefixes whose answer depends on whether a
  system identifier is present. Round 54 had four of them and said so.

## 3. What the DOM can do

The node classes stay in `lib/browser/node.fi` — `Node`, `Elem`, `Attr`,
`Text`, `Comment`, `Doctype`, `Document`, `Fragment`, with parent and child
links in both directions, attributes and namespaces (HTML, SVG, MathML,
XLink, XML, XMLNS). They stay there because the tree construction builds
them, and a DOM whose node type is not the one the parser produces would be
a second DOM. Two things were added to them in this round: `Attr.prefix`
(an attribute has three name parts, and `xmlns` lies in the XMLNS namespace
WITHOUT a prefix) and `Elem.content` (the template contents).

What was missing was the **access**, and that is the new `lib/dom/api.fi`:

| what | how |
| --- | --- |
| walking | `dom_next`, `dom_next_elem`, `dom_first_elem`, `dom_elem_count`, `dom_elem_at`, `dom_child_elem_count`, `dom_child_elem_at` |
| finding | `dom_by_id`, `dom_by_tag`, `dom_by_class` |
| selectors | `dom_query`, `dom_query_all` — over the matcher of `lib/css/sel.fi` |
| text | `dom_text_content` |
| markup | `dom_inner_html`, `dom_outer_html` (reading, the HTML fragment serialisation algorithm with the escaping rules, the void elements and the raw text elements) |
| attributes | `dom_attr_get`, `dom_attr_has`, `dom_attr_set_text`, `dom_attr_remove` |

`lib/dom/dom.fi` stays untouched next to it: that is the GC proof of round
53 (cyclic object graphs), a different thing with a different job.

The collections are **snapshots**, not live `HTMLCollection`s. A live
collection would have to hang on the node and be invalidated on every
change; that belongs to a DOM events round, and `querySelectorAll` returns
a snapshot anyway.

A `template` element behaves as a browser does: its content is a tree of
its own, so `getElementById` and `getElementsByTagName` do NOT find
anything inside it, `textContent` is empty and `innerHTML` gives the
content back. `tools/domb1/cases/dom_access.dom` pins that down.

## 4. The style tree

`lib/dom/style.fi` is the piece that hangs the CSS onto the tree. Three
things were there and unconnected: `lib/css/tok.fi`/`cv.fi` read a
stylesheet, `sel.fi` matches a selector against the DOM, `cascade.fi`
resolves origin, specificity, order and inheritance for ONE element. What
was missing:

1. **The default stylesheet.** `lib/dom/ua.css`, built in through the
   generated `lib/dom/ua_data.fi`. Without it every element is
   `display: inline` without margins and the computed style is worth
   nothing.
2. **The `<style>` elements of the document itself**, in document order, as
   origin AUTHOR — that is where "order of appearance" comes from.
3. **The walk in document order**, so the computed style of the parent is
   always finished before its children: inheritance and `em` need it, and
   one pass is enough.

The `style=` attribute is taken along (`cascade_elem_inline`).

One change went into `lib/css/sel.fi`: a type selector without a namespace
prefix matches in EVERY namespace (selectors-4 5.1), not only in HTML.
Round 60 restricted it to HTML, which was invisible as long as the tree
carried no SVG. Since this round it does, and `svg { display: block }` out
of the default stylesheet has to reach the element. Outside HTML the
comparison is case sensitive. The cross-check of the CSS round against
cssselect2 on 14 real pages (840 match sets) stays at 840 / 840.

`tools/domb1/cases/style_tree.dom` proves, case by case:

* the default stylesheet reaches every element (`head` is `none`, `div` is
  `block`, `li` is `list-item`, `body` has `margin: 8px`),
* inheritance: colour goes down through three levels, the box properties do
  not,
* specificity: `#i` beats `p.c` beats `.c` beats `p`,
* equal specificity: the later rule wins,
* the origins: default stylesheet < user < author,
* `!important` turns user and author round,
* the `style` attribute beats every selector, and an `!important` of the
  author beats the `style` attribute,
* computed values: `em` refers to the own font size, a percentage of
  `font-size` to the one of the parent (`20px` → `150%` = `30px` →
  `2em` = `60px`),
* `inherit` and `initial`,
* the cascade reaches elements of a foreign namespace.

## 5. What is missing, named openly

**Processing instructions — 88 of the 99 missing cases.**
`processing-instructions.dat` (84), three cases in `tests1.dat` and one in
`html5test-com.dat`. WHATWG accepted `<?target data?>` as a
`ProcessingInstruction` node in the DOM in June 2026 (whatwg/html PR
`#12118`); before that it was a bogus comment, which is what this parser
still makes of it. The change belongs in the TOKENISER, and there it
collides head on with the suite of section 9: the html5lib tokeniser tests
in `testdata/html5lib-tokenizer/` are frozen at the old behaviour and
contain 38 cases with `<?` that expect a comment token. Doing it right
means a tokeniser flag (PI mode on for the tree construction, off for the
conformance run), a token kind of its own with target and data, a node
class and the serialisation. That is a round of its own and it was not
started here — a half implementation that reinterprets the bogus comment
would pass most of the cases and be wrong on `<?t d > ?>`, where the PI
ends at `?>` and the bogus comment at the first `>`.

**Scripting — 6 cases.** `scripted_adoption01.dat`, `scripted_ark.dat`,
`scripted_foster01.dat`, `scripted_webkit01.dat` need a parser that
EXECUTES `document.write` while parsing. Firn has a JavaScript engine
(`lib/js/`), but the reentrant path from the parser into the engine and
back into the tokeniser does not exist. They count as failures here,
because that is what they are.

**`<selectedcontent>` — 4 cases** in `webkit02.dat`. The element mirrors the
content of the selected `option`. That is DOM behaviour of the element, not
parsing; the parser puts it in the right place, only the mirrored children
are missing.

**One adoption agency case** — `adoption02.dat #2`
(`<nobr><table><marquee></table><nobr>`). The second `nobr` lands one level
too deep. Not chased down.

Further limits that cost no test but are real:

* The tokeniser is not resumable. Every switch over (`<title>`, `<style>`,
  `<script>`, `<textarea>`, `<plaintext>`, CDATA) tokenises the REST of the
  input again, which is O(k·n). Round 54 wrote that down; it still holds
  and now has one more caller.
* `innerHTML` is READING only. Writing means a fragment parse plus a
  replacement of the children — the fragment parse exists now, the DOM
  mutation path does not.
* The collections are snapshots (see above).
* `cascade.fi` knows the property set of round 60. `font-weight`,
  `font-family`, `text-decoration`, `list-style`, `background` and
  `border-collapse` are missing, so the default stylesheet cannot express
  them either.

## 6. How to run it

```
bash tools/domb1/run.sh          # the whole round: five steps with numbers
bash tools/domb1/run.sh --fast   # without the second and third build stage
./test.sh                        # section 59 is this round
```

The parts on their own:

```
export FIRNLIB=$(pwd)/lib
compiler/target/release/firnc -o /tmp/b1parse lib/browser/b1_main.fi
python3 tools/domb1/harness.py /tmp/b1parse --show 5

compiler/target/release/firnc -o /tmp/b1dom lib/dom/b1_dom_main.fi
python3 tools/domb1/dom_harness.py /tmp/b1dom --show 5
```

The regression limit stands in `tools/domb1/minquota.txt` (1837). The
runner also refuses a run in which a case that used to pass now fails, even
if the total stayed the same (`--compare`).

The generated files come from their scripts:

```
python3 tools/domb1/gen_foreign.py   # lib/browser/foreign_data.fi
python3 tools/domb1/gen_quirks.py    # lib/browser/quirks_data.fi
python3 tools/domb1/gen_ua.py        # lib/dom/ua_data.fi  (out of lib/dom/ua.css)
```

## 7. The files of the round

| file | what |
| --- | --- |
| `tests/data/html5lib/` | the official suite, 62 `.dat` files, 1936 cases |
| `lib/browser/foreign.fi` | the four correction tables of foreign content, as atoms |
| `lib/browser/foreign_data.fi` | generated: the word lists |
| `lib/browser/quirks_data.fi` | generated: the DOCTYPE lists of the quirks mode |
| `lib/browser/b1_main.fi` | the driver with a fragment context and the scripting flag |
| `lib/dom/api.fi` | the DOM access layer |
| `lib/dom/ua.css` | the default stylesheet |
| `lib/dom/ua_data.fi` | generated out of it |
| `lib/dom/style.fi` | the style tree: one computed style per element |
| `lib/dom/b1_dom_main.fi` | the driver for the DOM and the style questions |
| `tools/domb1/` | the runner, the two harnesses, the three generators, the cases |

Changed: `lib/browser/tree.fi` (foreign content, template, fragment,
relaxed select, quirks), `lib/browser/node.fi` (`Attr.prefix`,
`Elem.content`), `lib/browser/write.fi` (the prefix in the output, the
template content), `lib/browser/driver.fi` (context, scripting flag),
`lib/css/sel.fi` (type selectors across namespaces),
`tools/html/cases/06_selection_frames.dat` (one case, to the new select
rule), `tools/english/check_names.py` (`tests/data/` is foreign data).
