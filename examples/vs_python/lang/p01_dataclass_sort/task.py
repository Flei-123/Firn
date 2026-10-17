from dataclasses import dataclass


@dataclass
class P:
    name: str
    age: int


ps = [P("ann", 31), P("bob", 25), P("cy", 40)]
ps.sort(key=lambda p: p.age)
for p in ps:
    print(f"{p.name} {p.age}")
print(sum(p.age for p in ps))
