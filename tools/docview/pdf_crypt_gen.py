import sys, os
from pypdf import PdfReader, PdfWriter
out = sys.argv[1]
src = sys.argv[2:]
for s in src:
    base = os.path.basename(s)[:-4]
    for alg in ["RC4-40", "RC4-128", "AES-128"]:
        for pw in ["", "geheim"]:
            try:
                r = PdfReader(s)
                w = PdfWriter()
                for p in r.pages:
                    w.add_page(p)
                w.encrypt(user_password=pw, owner_password="owner", algorithm=alg)
                with open(f"{out}/{base}.{alg}.{'pw' if pw else 'empty'}.pdf", "wb") as f:
                    w.write(f)
            except Exception as e:
                print("skip", base, alg, e)
