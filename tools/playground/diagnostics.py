import re

LOCATION = re.compile(r"^\s*--> (?P<file>[^:]+(?::[^:]+)*?):(?P<line>\d+):(?P<col>\d+)\s*$")
HEADER = re.compile(r"^(?P<severity>error|warning)(?:\[[^\]]*\])?: (?P<message>.*)$")


def parse_diagnostics(text, source_name):
    items = []
    current = None
    for raw in text.splitlines():
        header = HEADER.match(raw)
        if header:
            current = {"severity": header["severity"], "message": header["message"], "line": None, "col": None}
            items.append(current)
            continue
        location = LOCATION.match(raw)
        if location and current is not None and current["line"] is None:
            if location["file"].endswith(source_name):
                current["line"] = int(location["line"])
                current["col"] = int(location["col"])
    return items
