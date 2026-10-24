import re
import subprocess

DIE = re.compile(r"^\s*<(\d+)><[0-9a-f]+>: Abbrev Number: \d+ \((DW_TAG_\w+)\)")
ATTR = re.compile(r"^\s*<[0-9a-f]+>\s+(DW_AT_\w+)\s*:\s*(.*)$")


def attribute_text(raw):
    return re.sub(r"^\(.*?\):\s*", "", raw).strip()


def attribute_int(raw):
    match = re.search(r"0x[0-9a-fA-F]+|\d+", raw)
    return int(match.group(0), 0) if match else 0


def read_functions(path):
    text = subprocess.run(["readelf", "--debug-dump=info", path], capture_output=True, text=True, check=True).stdout
    functions = []
    current = None
    for row in text.splitlines():
        die = DIE.match(row)
        if die:
            if current is not None:
                functions.append(current)
            current = None
            if die.group(2) == "DW_TAG_compile_unit":
                current = {"kind": "unit"}
            elif die.group(2) == "DW_TAG_subprogram" and die.group(1) == "1":
                current = {"kind": "function"}
            continue
        attr = ATTR.match(row)
        if attr and current is not None:
            current[attr.group(1)] = attr.group(2)
    if current is not None:
        functions.append(current)
    units = []
    out = []
    for item in functions:
        if item["kind"] == "unit":
            units.append(attribute_int(item.get("DW_AT_stmt_list", "0")))
        else:
            if "DW_AT_low_pc" not in item or "DW_AT_name" not in item:
                continue
            low = attribute_int(item["DW_AT_low_pc"])
            high = attribute_int(item.get("DW_AT_high_pc", "0"))
            if high < low:
                high += low
            out.append({
                "name": attribute_text(item["DW_AT_name"]),
                "line": attribute_int(item.get("DW_AT_decl_line", "0")),
                "file_index": attribute_int(item.get("DW_AT_decl_file", "0")),
                "unit": units[-1] if units else 0,
                "low": low,
                "high": high,
            })
    return out
