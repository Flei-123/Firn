import base64
import re
import sys

WORD = re.compile(r"^    w\(p, (\d+), 0x([0-9a-f]+)\)$")


def to_blob(text):
    lines = text.split("\n")
    words = {}
    for line in lines:
        m = WORD.match(line)
        if m:
            words[int(m.group(1))] = int(m.group(2), 16)
    if not words:
        raise SystemExit("blobpack: no table words found")
    count = max(words) + 1
    data = b"".join(words.get(i, 0).to_bytes(8, "little") for i in range(count))
    total = int(re.search(r"^const BYTES: usize = (\d+)$", text, re.M).group(1))
    data = data.ljust(total, b"\0")
    cut = lines.index("#[no_gc]")
    head = lines[:cut]
    at = next(i for i, line in enumerate(head) if line.startswith("export"))
    head[at:at] = ["import str.tableblob", ""]
    consts = [line for line in lines[cut:] if line.startswith("const ")]
    blob = base64.b64encode(data).decode("ascii")
    out = head + consts + ["", 'const BLOB: str = "%s"' % blob, "",
                          "fn load(p: *mut u8) {",
                          "    tableblob.unpack(p, BYTES, BLOB)", "}", ""]
    return "\n".join(out)


if __name__ == "__main__":
    sys.stdout.write(to_blob(open(sys.argv[1], encoding="utf-8").read()))
