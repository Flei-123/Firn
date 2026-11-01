#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import os
import sys

from pypdf import PdfReader, PdfWriter

ALGORITHMS = ["AES-256-R5", "AES-256"]
PASSWORDS = [("empty", ""), ("pw", "geheim"), ("owner", "owner")]


def main():
    out = sys.argv[1]
    for source in sys.argv[2:]:
        base = os.path.basename(source)[:-4]
        for algorithm in ALGORITHMS:
            for label, password in PASSWORDS:
                reader = PdfReader(source)
                writer = PdfWriter()
                for page in reader.pages:
                    writer.add_page(page)
                user = "" if label == "empty" else ("geheim" if label == "pw" else "other")
                writer.encrypt(user_password=user, owner_password="owner", algorithm=algorithm)
                with open(f"{out}/{base}.{algorithm}.{label}.pdf", "wb") as handle:
                    writer.write(handle)


main()
