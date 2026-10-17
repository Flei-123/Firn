# Module under test: money in cents.
def parse_cents(s):
    """'12.34' -> 1234, '5' -> 500, '5.5' -> 550; None when invalid."""
    whole, _, frac = s.partition(".")
    if not whole.isdigit() or len(frac) > 2 or (frac and not frac.isdigit()):
        return None
    return int(whole) * 100 + int(frac.ljust(2, "0") or 0)


def format_cents(c):
    return f"{c // 100}.{c % 100:02d}"


def add_vat(c, pct):
    return (c * (100 + pct) + 50) // 100


def share(c, n, i):
    """Part i of c split into n shares that differ by at most one cent."""
    return c // n + (1 if i < c % n else 0)
