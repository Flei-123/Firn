# Python has no move semantics; the same code runs
class Guard:
    def __init__(s, i): s.id = i
def eat(g): return g.id
a = Guard(1)
print(eat(a) + eat(a))
