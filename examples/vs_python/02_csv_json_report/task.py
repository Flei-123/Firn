import csv
import json
from collections import defaultdict

with open("regions.json") as f:
    managers = {k: v["manager"] for k, v in json.load(f).items()}

amounts = defaultdict(list)
with open("sales.csv", newline="") as f:
    for row in csv.DictReader(f):
        amounts[row["region"]].append(float(row["amount"]))

stats = sorted(
    ((r, len(a), sum(a), sum(a) / len(a)) for r, a in amounts.items()),
    key=lambda s: -s[2],
)
print(f"{'region':<9}{'manager':<8}{'n':>3}{'sum':>10}{'avg':>9}")
for r, n, total, avg in stats:
    print(f"{r:<9}{managers[r]:<8}{n:>3}{total:>10.2f}{avg:>9.2f}")

report = {r: {"manager": managers[r], "n": n, "sum": round(t, 2), "avg": round(a, 2)} for r, n, t, a in stats}
text = json.dumps(report, indent=2, sort_keys=True)
with open("out.json", "w") as f:
    f.write(text + "\n")
print(text)
