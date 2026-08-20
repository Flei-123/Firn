#!/usr/bin/env bash
# tools/gen_gctext.sh -- produces lib/firnc1/gctext.fi from lib/gc/gc.fi.
#
# WHY: `firnc0` embeds the collector runtime with `include_str!`
# (compiler/src/gc.rs, LAUFZEIT). Firn has no `include_str` -- the
# honest equivalent is this generated file: the same text, packed as
# u64 words (8 bytes per word, little-endian), so that the
# self-hosting compiler can pull it in without file access.
# After a change to lib/gc/gc.fi run this script and
# commit lib/firnc1/gctext.fi along with it.
#
# ROUND 53: what is packed is the CONCATENATION of gc.fi + gcvec.fi + gcmap.fi,
# exactly as `runtime_source` in gc.rs puts it together. The collections
# hang at the end and are only unpacked along when the program needs
# them -- which is why there are two lengths: GCTEXT_N (only gc.fi) and
# GCTEXT_ALL (with the collections).
set -euo pipefail
cd "$(dirname "$0")/.."
python3 - <<'PYEOF'
kern = open('lib/gc/gc.fi','rb').read()
samml = open('lib/gc/gcvec.fi','rb').read() + open('lib/gc/gcmap.fi','rb').read()
src = kern + samml
n = len(kern)
alle = len(src)
words = []
for i in range(0, alle, 8):
    chunk = src[i:i+8].ljust(8, b'\0')
    words.append(int.from_bytes(chunk, 'little'))
out = []
out.append('// lib/firnc1/gctext.fi — GENERIERT von tools/gen_gctext.sh aus')
out.append('// lib/gc/gc.fi. NICHT VON HAND AENDERN.')
out.append('//')
out.append('// Der Quelltext der Sammler-Laufzeit als Daten (Runde 34): `firnc0`')
out.append('// bettet ihn per `include_str!` ein; diese Stufe traegt ihn als')
out.append('// u64-Woerter im eigenen Text. `gctext_write` packt ihn wieder')
out.append('// aus — der Treiber zieht ihn als zusaetzliches Modul ein, sobald')
out.append('// im Programm ein `gc class` steht (laufzeit_quelle in gc.rs).')
out.append('import rt')
out.append('')
out.append('export { GCTEXT_N, GCTEXT_ALL, gctext_write }')
out.append('')
out.append('// Laenge des Kerns (lib/gc/gc.fi) in Oktetten.')
out.append('const GCTEXT_N: u64 = %d' % n)
out.append('// Laenge mit den Sammlungen (gcvec.fi + gcmap.fi) dahinter.')
out.append('const GCTEXT_ALL: u64 = %d' % alle)
out.append('')
out.append('// `with_collections` entscheidet, ob GcVec/GcMap mit ausgepackt')
out.append('// werden — genau wie der dritte Parameter von laufzeit_quelle.')
out.append('fn gctext_write(b: *mut rt.Buf, with_collections: bool) {')
out.append('    var limit: u64 = GCTEXT_N')
out.append('    if with_collections {')
out.append('        limit = GCTEXT_ALL')
out.append('    }')
out.append('    var w: [u64; %d] = [' % len(words))
for i in range(0, len(words), 10):
    out.append('        ' + ' '.join(str(x) + ',' for x in words[i:i+10]))
out.append('    ]')
out.append('    var written: u64 = 0')
out.append('    var i: usize = 0')
out.append('    while i < %d {' % len(words))
out.append('        let v: u64 = w[i]')
out.append('        var k: u64 = 0')
out.append('        while k < 8 {')
out.append('            if written >= limit {')
out.append('                return')
out.append('            }')
out.append('            rt.buf_push(b, ((v >> (k * 8)) & 255) as u8)')
out.append('            written = written + 1')
out.append('            k = k + 1')
out.append('        }')
out.append('        i = i + 1')
out.append('    }')
out.append('}')
open('lib/firnc1/gctext.fi','w').write('\n'.join(out) + '\n')
print('lib/firnc1/gctext.fi:', alle, 'Bytes (kern', n, ') als', len(words), 'Woerter')
PYEOF