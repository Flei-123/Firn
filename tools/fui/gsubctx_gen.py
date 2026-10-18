# SPDX-License-Identifier: MPL-2.0
# Generates the glyph sequences + expected SubstLookupRecords of tools/fui/gsubctx_main.fi (reference matcher, no skipping).
import glob,sys
from fontTools.ttLib import TTFont
def path(n): return [p for p in glob.glob('/usr/share/fonts/**/'+n,recursive=True)][0]
def cov(s): return s.Coverage.glyphs
def ref(f,lookup,gids,at):
    """reference matcher: first matching rule of the lookup (no skipping)"""
    go=f.getGlyphOrder(); g=f['GSUB'].table
    l=g.LookupList.Lookup[lookup]
    ids=[go[x] for x in gids]
    for st in l.SubTable:
        if l.LookupType==7: st=st.ExtSubTable
        fmt=st.Format; first=ids[at]
        chain=hasattr(st,'ChainSubRuleSet') or hasattr(st,'ChainSubClassSet') or hasattr(st,'BacktrackCoverage')
        def rules():
            if fmt==1:
                if first not in cov(st): return
                rs=(st.ChainSubRuleSet if chain else st.SubRuleSet)[cov(st).index(first)]
                if rs is None: return
                for r in (rs.ChainSubRule if chain else rs.SubRule):
                    yield ('g',r)
            elif fmt==2:
                if first not in cov(st): return
                cd=(st.InputClassDef if chain else st.ClassDef).classDefs
                c=cd.get(first,0)
                sets=st.ChainSubClassSet if chain else st.SubClassSet
                if c>=len(sets) or sets[c] is None: return
                for r in (sets[c].ChainSubClassRule if chain else sets[c].SubClassRule):
                    yield ('c',r)
            else:
                yield ('v',st)
        for kind,r in rules():
            if kind=='g': bt=getattr(r,'Backtrack',[]); inp=[first]+list(r.Input); la=getattr(r,'LookAhead',[])
            elif kind=='c':
                bt=list(getattr(r,'Backtrack',[])); inp=[None]+list(r.Input if chain else r.Class); la=list(getattr(r,'LookAhead',[]))
            else: pass
            def fit(seq,i,k,which):
                if kind=='g': return ids[i]==seq[k]
                if kind=='c':
                    cd={'b':getattr(st,'BacktrackClassDef',None),'i':getattr(st,'InputClassDef',None) or getattr(st,'ClassDef',None),'a':getattr(st,'LookAheadClassDef',None)}[which]
                    return (cd.classDefs.get(ids[i],0) if cd else 0)==seq[k]
            if kind=='v':
                if chain:
                    B=st.BacktrackCoverage; I=st.InputCoverage; A=st.LookAheadCoverage; recs=st.SubstLookupRecord
                else:
                    B=[];I=st.Coverage;A=[];recs=st.SubstLookupRecord
                    # non-chain fmt3 stores in Coverage list
                    I=st.Coverage
                ok=at+len(I)+len(A)<=len(ids) and at-len(B)>=0
                if not ok: continue
                if not all(ids[at+j] in I[j].glyphs for j in range(len(I))): continue
                if not all(ids[at-1-j] in B[j].glyphs for j in range(len(B))): continue
                if not all(ids[at+len(I)+j] in A[j].glyphs for j in range(len(A))): continue
                return [(at+a.SequenceIndex,a.LookupListIndex) for a in recs]
            else:
                ok=at+len(inp)+len(la)<=len(ids) and at-len(bt)>=0
                if not ok: continue
                if not all(fit(inp,at+j,j,'i') for j in range(1,len(inp))): continue
                if not all(fit(bt,at-1-j,j,'b') for j in range(len(bt))): continue
                if not all(fit(la,at+len(inp)+j,j,'a') for j in range(len(la))): continue
                return [(at+a.SequenceIndex,a.LookupListIndex) for a in r.SubstLookupRecord]
    return []
def build(f,lookup):
    """make a glyph sequence from the first usable rule of the first subtable"""
    go=f.getGlyphOrder(); g=f['GSUB'].table
    l=g.LookupList.Lookup[lookup]; st=l.SubTable[0]
    if l.LookupType==7: st=st.ExtSubTable
    fmt=st.Format; chain=hasattr(st,'ChainSubRuleSet') or hasattr(st,'ChainSubClassSet') or hasattr(st,'BacktrackCoverage')
    gid=lambda n: go.index(n)
    if fmt==1:
        sets=st.ChainSubRuleSet if chain else st.SubRuleSet
        for ci,rs in enumerate(sets):
            if rs is None: continue
            r=(rs.ChainSubRule if chain else rs.SubRule)[0]
            bt=getattr(r,'Backtrack',[])
            seq=[gid(x) for x in reversed(bt)]+[gid(cov(st)[ci])]+[gid(x) for x in r.Input]+[gid(x) for x in getattr(r,'LookAhead',[])]
            return seq,len(bt)
    if fmt==2:
        def rep(cd,c):
            for n,k in cd.classDefs.items():
                if k==c: return gid(n)
            return None
        sets=st.ChainSubClassSet if chain else st.SubClassSet
        icd=st.InputClassDef if chain else st.ClassDef
        for c,cs in enumerate(sets):
            if cs is None or c==0: continue
            for r in (cs.ChainSubClassRule if chain else cs.SubClassRule):
                bt=list(getattr(r,'Backtrack',[])); la=list(getattr(r,'LookAhead',[]))
                firstg=[gid(n) for n in cov(st) if icd.classDefs.get(n,0)==c]
                if not firstg: continue
                parts=[rep(st.BacktrackClassDef,x) if chain else None for x in reversed(bt)] if chain else []
                inp=[firstg[0]]+[rep(icd,x) for x in (r.Input if chain else r.Class)]
                las=[rep(st.LookAheadClassDef,x) for x in la] if chain else []
                seq=parts+inp+las
                if None in seq: continue
                return seq,len(parts)
    if fmt==3:
        if chain:
            B=st.BacktrackCoverage;I=st.InputCoverage;A=st.LookAheadCoverage
            seq=[gid(c.glyphs[0]) for c in reversed(B)]+[gid(c.glyphs[0]) for c in I]+[gid(c.glyphs[0]) for c in A]
            return seq,len(B)
    return None
cases=[('NotoSansMono-Bold.ttf',[3]),('NotoColorEmoji.ttf',[2,4]),('FreeSerifBoldItalic.ttf',[4])]
for fn,lks in cases:
    p=path(fn); f=TTFont(p)
    for lk in lks:
        b=build(f,lk)
        if not b: print('#nocase',fn,lk); continue
        seq,at=b
        exp=ref(f,lk,seq,at)
        print(p,lk,at,' '.join(map(str,seq)),'=>',exp)
