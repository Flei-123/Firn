from collections import Counter

text = "to be or not to be that is the question to be"
words = text.split()
counts = Counter(words)
top = sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))[:3]
for w, n in top:
    print(w, n)
print(",".join(sorted(set(words))))
