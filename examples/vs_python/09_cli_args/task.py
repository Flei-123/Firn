import argparse


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise ValueError(f"{self.prog}: error: {message}")


def build():
    p = Parser(prog="tool", description="item tool")
    sub = p.add_subparsers(dest="cmd", required=True)
    a = sub.add_parser("add", help="add an item")
    a.add_argument("--name", required=True, help="item name")
    a.add_argument("--count", type=int, default=1, help="how many")
    a.add_argument("-v", "--verbose", action="store_true", help="verbose output")
    r = sub.add_parser("rm", help="remove an item")
    r.add_argument("id", type=int, help="item id")
    r.add_argument("--force", action="store_true", help="no questions")
    return p


def run(argv):
    try:
        ns = build().parse_args(argv)
    except ValueError as e:
        print(e)
        return
    except SystemExit:
        return
    if ns.cmd == "add":
        print(f"add name={ns.name} count={ns.count} verbose={str(ns.verbose).lower()}")
    else:
        print(f"rm id={ns.id} force={str(ns.force).lower()}")


for line in ["add --name X --count 3 -v", "add --name Y", "rm 7 --force", "add --name Z --count abc",
             "add", "frob", "add --name Q --bogus", "rm", "add --help", "--help"]:
    print("$ tool " + line)
    run(line.split())
