import os
import urllib.request
from concurrent.futures import ThreadPoolExecutor

base = "http://127.0.0.1:" + os.environ["VSPY_PORT"]


def fetch(i):
    with urllib.request.urlopen(f"{base}/n/{i}") as resp:
        return i, resp.status, len(resp.read())


with ThreadPoolExecutor(20) as pool:
    for i, status, length in sorted(pool.map(fetch, range(20))):
        print(f"/n/{i} {status} {length}")
