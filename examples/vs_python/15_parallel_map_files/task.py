import hashlib, os, tempfile
from concurrent.futures import ThreadPoolExecutor


def work(path):
    with open(path, "rb") as f:
        data = f.read()
    return os.path.basename(path), hashlib.sha256(data).hexdigest(), data.count(b"\n")


with tempfile.TemporaryDirectory() as root:
    paths = []
    for i in range(40):
        p = os.path.join(root, f"file{i:02d}.txt")
        with open(p, "w") as f:
            for j in range((i + 1) * 50):
                f.write(f"line {i} {j}\n")
        paths.append(p)
    with ThreadPoolExecutor(max_workers=4) as pool:
        for name, digest, lines in sorted(pool.map(work, paths)):
            print(name, digest, lines)
