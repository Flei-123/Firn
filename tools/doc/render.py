import html
import json


def markdown(title, version, modules):
    out = ["# %s%s" % (title, " " + version if version else ""), ""]
    for mod in modules:
        out += ["## %s" % mod["name"], ""]
        if mod["purpose"]:
            out += [mod["purpose"], ""]
        for item in mod["items"]:
            out.append("### `%s`" % item["sig"])
            out.append("")
            if item["doc"]:
                out += [item["doc"], ""]
    return "\n".join(out) + "\n"


def search(modules, text):
    needle = text.lower()
    rows = []
    for mod in modules:
        for item in mod["items"]:
            if needle in (item["name"] + " " + item["doc"] + " " + mod["name"]).lower():
                rows.append("%s: %s" % (mod["name"], item["sig"]))
    return "\n".join(rows) + ("\n" if rows else "")


PAGE = """<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>%(title)s</title>
<style>body{font:15px system-ui;max-width:56rem;margin:2rem auto;padding:0 1rem}code{background:#eee;padding:0 .2rem}
input{width:100%%;padding:.6rem;font-size:1rem}.item{margin:.8rem 0}.mod{margin-top:2rem}</style></head>
<body><h1>%(title)s</h1><input id="q" placeholder="search" autofocus><div id="out"></div>
<script>
const DATA=%(data)s;
const q=document.getElementById('q'),out=document.getElementById('out');
function esc(s){return s.replace(/[&<>]/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;'}[c]))}
function draw(){const n=q.value.toLowerCase();let h='';
for(const m of DATA){let rows='';
for(const i of m.items){if(!n||(i.name+' '+i.doc+' '+m.name).toLowerCase().includes(n))rows+='<div class="item"><code>'+esc(i.sig)+'</code><div>'+esc(i.doc)+'</div></div>'}
if(rows)h+='<div class="mod"><h2>'+esc(m.name)+'</h2><p>'+esc(m.purpose)+'</p>'+rows+'</div>'}
out.innerHTML=h}
q.addEventListener('input',draw);draw();
</script></body></html>
"""


def html_page(title, version, modules):
    data = json.dumps(modules).replace("</", "<\\/")
    return PAGE % {"title": html.escape(title + (" " + version if version else "")), "data": data}
