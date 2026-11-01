import re
import subprocess
import sys

URL = "https://aomedia.googlesource.com/aom/+/refs/heads/main/av1/decoder/grain_synthesis.c?format=TEXT"


def main():
    out = sys.argv[1]
    raw = subprocess.run(["curl", "-sfL", URL], capture_output=True, check=True).stdout
    text = subprocess.run(["base64", "-d"], input=raw, capture_output=True, check=True).stdout.decode()
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    start = text.index("gaussian_sequence[2048]")
    body = text[text.index("{", start) + 1 : text.index("};", start)]
    values = [int(v) for v in re.findall(r"-?\d+", body)]
    assert len(values) == 2048
    lines = ["// SPDX-License-Identifier: MPL-2.0", "export { GAUSSIAN_SEQUENCE }", "", "static GAUSSIAN_SEQUENCE: [i16; 2048] = ["]
    for i in range(0, 2048, 16):
        lines.append("    " + ", ".join(str(v) for v in values[i : i + 16]) + ",")
    lines.append("]")
    open(out, "w").write("\n".join(lines) + "\n")


main()
