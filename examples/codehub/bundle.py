#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# examples/codehub/bundle.py -- ONE HTML FILE that runs the page anywhere:
# the loader (demos/webdemo/firn.js), codehub.wasm and the three fonts
# inlined as data: URLs. Opened straight from the disk (file://) it works,
# because nothing is fetched from a server any more.
#     python3 examples/codehub/bundle.py out.html
import base64, os, sys
H = os.path.dirname(os.path.abspath(__file__))
S = os.path.join(H, 'site')
def data(name, mime):
    return 'data:%s;base64,%s' % (mime, base64.b64encode(open(os.path.join(S, name), 'rb').read()).decode())
js = open(os.path.join(S, 'firn.js')).read()
html = open(os.path.join(S, 'index.html')).read()
tag = ('<script data-wasm="%s" data-font="%s" data-fonts="%s %s">\n%s\n</script>' % (
    data('codehub.wasm', 'application/wasm'), data('DejaVuSans.ttf', 'font/ttf'),
    data('DejaVuSansMono.ttf', 'font/ttf'), data('DejaVuSans-Bold.ttf', 'font/ttf'), js))
a = html.index('<script src="firn.js"')
b = html.index('</script>', a) + len('</script>')
out = html[:a] + tag + html[b:]
open(sys.argv[1], 'w').write(out)
print(sys.argv[1], len(out), 'octets')
