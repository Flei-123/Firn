import re
from collections import Counter

text = open("notes.txt").read()

emails = sorted(set(re.findall(r"[\w.]+@[\w.]+\.[a-z]{2,}", text)))
print("emails:", len(emails))
for e in emails:
    print(" ", e)

print("dates:", " ".join(re.findall(r"\d{4}-\d{2}-\d{2}", text)))

levels = Counter()
for m in re.finditer(r"^(\S+ \S+) \[(\w+)\] (\w+): (.*)$", text, re.M):
    ts, level, mod, msg = m.groups()
    levels[level] += 1
    if level == "ERROR":
        print(f"error at {ts} in {mod}: {msg}")
print("levels:", " ".join(f"{k}={v}" for k, v in sorted(levels.items())))

print(re.sub(r"(\d{4})-(\d{2})-(\d{2})", r"\3.\2.\1", text.splitlines()[-1]))

words = re.findall(r"[a-z]+", text.lower())
print("words:", len(words))
for w, n in sorted(Counter(words).items(), key=lambda kv: (-kv[1], kv[0]))[:10]:
    print(f"{w:<10}{n}")
