def parse(s):
    try:
        return int(s)
    except ValueError:
        return -1


for s in ["12", "x", "7"]:
    print(parse(s))
