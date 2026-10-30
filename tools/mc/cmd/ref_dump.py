import json, sys


def tok(v):
    return v if isinstance(v, str) else str(v)


def props_text(p):
    if not p:
        return "-"
    return ";".join(f"{k}={tok(p[k])}" for k in sorted(p))


def perm_level(n):
    p = n.get("permissions")
    if not p:
        return "-"
    inner = p.get("permission", {})
    return inner.get("level", "-")


def main():
    raw = open(sys.argv[1], encoding="utf-8").read()
    j = json.loads(raw, parse_float=lambda s: s, parse_int=int)
    out = []
    paths = {}

    def walk(node, prefix):
        for name, c in node.get("children", {}).items():
            path = f"{prefix} {name}" if prefix else name
            paths[path] = c
            if c["type"] == "literal":
                kind, parser, props = "literal", "-", "-"
            else:
                kind, parser, props = "argument", c["parser"], props_text(c.get("properties"))
            red = " ".join(c["redirect"]) if c.get("redirect") else "-"
            out.append(f"{path}\t{kind}\t{parser}\t{props}\texec={1 if c.get('executable') else 0}\tperm={perm_level(c)}\tredirect={red}")
            walk(c, path)

    walk(j, "")
    sys.stdout.write("\n".join(out) + "\n")


main()
