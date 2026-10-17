import html
import re

# The Python standard library has no Markdown converter. A normal dev would
# `pip install markdown` (markdown.markdown(text, extensions=["tables", "fenced_code"])),
# which is 3 lines but a third-party package. Stdlib-only version of the same subset:

def inline(s):
    s = html.escape(s, quote=False)
    s = re.sub(r"`([^`]+)`", r"<code>\1</code>", s)
    s = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", s)
    s = re.sub(r"\*(.+?)\*", r"<em>\1</em>", s)
    s = re.sub(r"~~(.+?)~~", r"<del>\1</del>", s)
    return re.sub(r"\[([^\]]+)\]\(([^)]+)\)", r'<a href="\2">\1</a>', s)

def cells(line):
    return [c.strip() for c in line.strip().strip("|").split("|")]

def convert(text):
    out, lines, i = [], text.split("\n"), 0
    while i < len(lines):
        line = lines[i]
        m = re.match(r"(#{1,6}) (.*)", line)
        if not line.strip():
            i += 1
        elif m:
            out.append(f"<h{len(m[1])}>{inline(m[2])}</h{len(m[1])}>")
            i += 1
        elif line.startswith("```"):
            lang, i, code = line[3:].strip(), i + 1, []
            while not lines[i].startswith("```"):
                code.append(html.escape(lines[i], quote=False))
                i += 1
            cls = f' class="language-{lang}"' if lang else ""
            out.append(f"<pre><code{cls}>" + "".join(c + "\n" for c in code) + "</code></pre>")
            i += 1
        elif re.match(r"(-|\d+\.) ", line):
            tag = "ul" if line[0] == "-" else "ol"
            out.append(f"<{tag}>")
            while i < len(lines) and re.match(r"(-|\d+\.) ", lines[i]):
                out.append("<li>" + inline(re.sub(r"^(-|\d+\.) ", "", lines[i])) + "</li>")
                i += 1
            out.append(f"</{tag}>")
        elif line.startswith("|"):
            head, sep = cells(line), cells(lines[i + 1])
            style = ['' if not s.endswith(":") else ' style="text-align:right"' for s in sep]
            out.append("<table>\n<thead>\n<tr>")
            out += [f"<th{style[k]}>{inline(c)}</th>" for k, c in enumerate(head)]
            out.append("</tr>\n</thead>\n<tbody>")
            i += 2
            while i < len(lines) and lines[i].startswith("|"):
                out.append("<tr>")
                out += [f"<td{style[k]}>{inline(c)}</td>" for k, c in enumerate(cells(lines[i]))]
                out.append("</tr>")
                i += 1
            out.append("</tbody>\n</table>")
        elif line.startswith(">"):
            out.append("<blockquote>\n<p>" + inline(line[1:].strip()) + "</p>\n</blockquote>")
            i += 1
        elif line == "---":
            out.append("<hr />")
            i += 1
        else:
            para = []
            while i < len(lines) and lines[i].strip():
                para.append(lines[i])
                i += 1
            out.append("<p>" + inline(" ".join(para)) + "</p>")
    return "\n".join(out) + "\n"

print(convert(open("doc.md").read()), end="")
