# SPDX-License-Identifier: GPL-2.0-only
# Corrects `var x: [u8; N] = "..."` where N does not match the literal.
# The array length of a string literal has to agree exactly (SPEC 14.1 S1).
import re,sys
pat=re.compile(r'(\[u8; )(\d+)(\] = ")((?:[^"\\]|\\.)*)(")')
def declen(s):
    n=0;i=0
    while i<len(s):
        if s[i]=='\\':
            i+=2; n+=1
        else:
            i+=1; n+=1
    return n
for p in sys.argv[1:]:
    s=open(p).read()
    def f(m):
        want=declen(m.group(4))
        return m.group(1)+str(want)+m.group(3)+m.group(4)+m.group(5)
    open(p,'w').write(pat.sub(f,s))
