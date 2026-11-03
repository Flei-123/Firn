from collections import Counter


def table(title, counter, limit=0):
    lines = [title]
    items = counter.most_common(limit or None)
    width = max((len(str(k)) for k, _ in items), default=0)
    for k, v in items:
        lines.append("  %-*s %d" % (width, k, v))
    return lines


def render(stats, rename_plan, edges, odd, conflicts, warnings):
    out = []
    out.append("files with content matches: %d" % stats["files"])
    out.append("lines with content matches: %d" % stats["lines"])
    out.append("occurrences:                %d" % stats["occurrences"])
    out.append("files skipped (binary):     %d" % stats["binary"])
    out.append("symlinks with matching target: %d" % stats["symlinks"])
    out.append("paths to rename:            %d" % len(rename_plan))
    out.append("")
    out.extend(table("occurrences by category:", stats["by_category"]))
    out.append("")
    out.extend(table("lines by top-level directory:", stats["by_dir"]))
    out.append("")
    out.extend(table("distinct environment variables:", Counter({"count": len(stats["env_names"])})))
    out.append("")
    out.append("edge cases (tokens that contain the name inside a longer word; they ARE renamed):")
    if not edges:
        out.append("  none")
    for (token, reason), n in sorted(edges.items(), key=lambda kv: -kv[1]):
        out.append("  %-40s %-20s x%d" % (token, reason, n))
    out.append("")
    out.append("odd-case spellings (left untouched):")
    if not odd:
        out.append("  none")
    for token, n in odd.most_common():
        out.append("  %-40s x%d" % (token, n))
    if conflicts:
        out.append("")
        out.append("CONFLICTS (apply refuses):")
        out.extend("  " + c for c in conflicts)
    if warnings:
        out.append("")
        out.append("warnings:")
        out.extend("  " + w for w in warnings)
    return "\n".join(out) + "\n"
