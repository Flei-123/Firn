import sys, os, random, struct, itertools
import pyarrow as pa
import pyarrow.parquet as pq
out = sys.argv[1]
os.makedirs(out, exist_ok=True)
random.seed(11)
words = "der die das und ist nicht ein eine zu den mit auf für von im dem sich auch es äöü 日本語 emoji😀".split()
def doc():
    r = random.random()
    if r < 0.05: return ""
    return " ".join(random.choice(words) for _ in range(random.randint(1, 60)))
N = 3000
texts = [None if random.random() < 0.1 else doc() for _ in range(N)]
convs = []
for _ in range(N):
    r = random.random()
    if r < 0.05: convs.append(None)
    elif r < 0.1: convs.append([])
    else: convs.append([{"from": random.choice(["human", "gpt"]), "value": (None if random.random() < 0.05 else doc())} for _ in range(random.randint(1, 5))])
conv_t = pa.list_(pa.struct([("from", pa.string()), ("value", pa.string())]))
tbl = pa.table({"text": pa.array(texts, pa.string()), "conversations": pa.array(convs, conv_t)})
def enc(v):
    return struct.pack("<I", 0xFFFFFFFF) if v is None else struct.pack("<I", len(v.encode())) + v.encode()
open(f"{out}/text.exp", "wb").write(b"".join(enc(v) for v in texts))
exp = bytearray()
for c in convs:
    if c is None: exp += struct.pack("<I", 0xFFFFFFFE); continue
    exp += struct.pack("<I", len(c))
    for it in c: exp += enc(it["from"]) + enc(it["value"])
open(f"{out}/conversations.exp", "wb").write(bytes(exp))
i = 0
for codec, dic, ver, dpv in itertools.product(["NONE", "SNAPPY", "ZSTD", "GZIP"], [True, False], ["1.0", "2.6"], ["1.0", "2.0"]):
    if ver == "1.0" and dpv == "2.0": continue
    f = f"{out}/t_{codec}_{int(dic)}_{ver}_{dpv}.parquet"
    pq.write_table(tbl, f, compression=codec, use_dictionary=dic, version=ver, data_page_version=dpv, row_group_size=1100, data_page_size=20000)
    i += 1
pq.write_table(tbl, f"{out}/t_delta.parquet", compression="SNAPPY", use_dictionary=False, column_encoding={"text": "DELTA_LENGTH_BYTE_ARRAY", "conversations.list.element.from": "DELTA_BYTE_ARRAY", "conversations.list.element.value": "DELTA_LENGTH_BYTE_ARRAY"}, data_page_version="2.0", row_group_size=1100)
