import re

LOCATION = re.compile(r"^\s*--> (?P<file>[^:]+(?::[^:]+)*?):(?P<line>\d+):(?P<col>\d+)\s*$")
HEADER = re.compile(r"^(?P<severity>error|warning)(?:\[[^\]]*\])?: (?P<message>.*)$")
MARKER = re.compile(r"^\s*\|\s(?P<pad>\s*)(?P<marks>\^+)")
NOTE = re.compile(r"^\s*= (?:help|note): (?P<text>.*)$")


def parse_diagnostics(text, source_name):
    items = []
    current = None
    for raw in text.splitlines():
        header = HEADER.match(raw)
        if header:
            current = {"severity": header["severity"], "message": header["message"], "line": None, "col": None, "length": 1, "hint": ""}
            items.append(current)
            continue
        if current is None:
            continue
        location = LOCATION.match(raw)
        if location and current["line"] is None:
            if location["file"].endswith(source_name):
                current["line"] = int(location["line"])
                current["col"] = int(location["col"])
            continue
        marker = MARKER.match(raw)
        if marker and current["line"] is not None and current["length"] == 1:
            current["length"] = len(marker["marks"])
            continue
        note = NOTE.match(raw)
        if note and not current["hint"]:
            current["hint"] = note["text"]
    return items
