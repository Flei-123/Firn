#!/usr/bin/env python3
import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import parse
import render


def main(argv):
    ap = argparse.ArgumentParser(prog="firn doc", description="API documentation of a package, a file or a library module")
    ap.add_argument("target", nargs="?", default=".", help="directory, file.fi or module name (std.json)")
    ap.add_argument("--html", action="store_true", help="one searchable HTML page instead of Markdown")
    ap.add_argument("--search", metavar="TEXT", help="print the matching items only")
    ap.add_argument("-o", "--out", help="write to FILE instead of stdout")
    ap.add_argument("--lib", default=os.environ.get("FIRNLIB") or os.path.join(parse.ROOT, "lib"))
    args = ap.parse_args(argv)
    found = parse.collect(args.target, args.lib)
    if found is None:
        print("firn doc: '%s' is not a directory, a .fi file or a library module" % args.target, file=sys.stderr)
        return 125
    title, version, modules = found
    if args.search:
        text = render.search(modules, args.search)
    elif args.html:
        text = render.html_page(title, version, modules)
    else:
        text = render.markdown(title, version, modules)
    if args.out:
        with open(args.out, "w", encoding="utf-8") as f:
            f.write(text)
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
