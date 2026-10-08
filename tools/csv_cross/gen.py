#!/usr/bin/env python3
"""Random CSV cases with the expected answer of Python's csv module.

    gen.py SEED COUNT OUTFILE

One case per line, tokens separated by single spaces, all text hex encoded
(latin-1: one byte = one character, so any byte is a valid Python str):

  R DELIM QUOTE DQ STRICT xINPUT <expected tokens>
      expected: one token `r:HEX:HEX` per row (`r` = a row of no field,
      `r:` = a row of one empty field), then `end` or `E<code>@<line>`
      (code 1 = unexpected end of data, 2 = delimiter expected after quote;
      line = reader.line_num when the error was raised)

  W DELIM QUOTE DQ xTERMINATOR RT <row tokens> = xOUTPUT fFAILED
      RT is rt1 when Python reads its own output back to the same rows (the
      Firn program then has to as well). FAILED = number of rows Python refused
      (quote character in a field with doublequote off).

Python is the reference, never the code under test. Cases where Python itself
raises something unexpected are dropped.
"""
import csv
import io
import random
import sys


def hx(b):
    return b.hex()


def py_read(data, delim, quote, dq, strict):
    text = data.decode("latin-1")
    f = io.StringIO(text, newline="")
    rd = csv.reader(f, delimiter=delim, quotechar=quote, doublequote=dq, strict=strict)
    rows = []
    try:
        for row in rd:
            rows.append(row)
    except csv.Error as e:
        msg = str(e)
        if "unexpected end of data" in msg:
            code = 1
        elif "expected after" in msg:
            code = 2
        else:
            raise
        return rows, (code, rd.line_num)
    return rows, None


def row_token(row):
    return "r" + "".join(":" + hx(c.encode("latin-1")) for c in row)


def py_write(rows, delim, quote, dq, term):
    out = io.StringIO(newline="")
    wr = csv.writer(out, delimiter=delim, quotechar=quote, doublequote=dq, lineterminator=term)
    failed = 0
    done = []
    for row in rows:
        before = out.tell()
        try:
            wr.writerow(row)
            done.append(row)
        except csv.Error:
            failed += 1
            out.seek(before)
            out.truncate()
    return out.getvalue().encode("latin-1"), failed, done


def main():
    seed, count, outfile = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
    rnd = random.Random(seed)
    delims = [",", ",", ",", ";", "\t", "|", "a", ":"]
    quotes = ['"', '"', '"', "'", "`", "b"]
    lines = []
    nread = nwrite = 0
    while nread < count or nwrite < count // 2:
        delim = rnd.choice(delims)
        quote = rnd.choice(quotes)
        if quote == delim:
            continue
        dq = rnd.random() < 0.8
        strict = rnd.random() < 0.5
        alphabet = [delim, delim, quote, quote, quote, "\r", "\n", "\r\n", "\r\n", "a", "b", "c", " ", "\x00", "\xff", "\\", "x", "é"]
        kind = rnd.random()
        want_read = nread < count and (nwrite >= count // 2 or rnd.random() < 0.66)
        if want_read:
            if kind < 0.5:
                n = rnd.randint(0, 40)
                s = "".join(rnd.choice(alphabet) for _ in range(n))
                data = s.encode("latin-1")
            else:
                # a valid table written by Python, with a few random edits
                rows = []
                for _ in range(rnd.randint(0, 5)):
                    rows.append(["".join(rnd.choice(alphabet) for _ in range(rnd.randint(0, 5))) for _ in range(rnd.randint(0, 4))])
                out = io.StringIO(newline="")
                try:
                    w = csv.writer(out, delimiter=delim, quotechar=quote, doublequote=dq, lineterminator=rnd.choice(["\r\n", "\n", "\r"]))
                    for row in rows:
                        w.writerow(row)
                except csv.Error:
                    continue
                s = out.getvalue()
                for _ in range(rnd.choice([0, 0, 1, 2, 3])):
                    if not s:
                        break
                    i = rnd.randrange(len(s))
                    op = rnd.random()
                    if op < 0.4:
                        s = s[:i] + rnd.choice(alphabet) + s[i + 1:]
                    elif op < 0.7:
                        s = s[:i] + rnd.choice(alphabet) + s[i:]
                    else:
                        s = s[:i] + s[i + 1:]
                if rnd.random() < 0.3:
                    s = s.rstrip("\r\n")
                data = s.encode("latin-1")
            try:
                rows, err = py_read(data, delim, quote, dq, strict)
            except Exception:
                continue
            toks = [row_token(r) for r in rows]
            toks.append("end" if err is None else "E%d@%d" % err)
            lines.append("R %s %s %d %d x%s %s" % (hx(delim.encode()), hx(quote.encode()), dq, strict, hx(data), " ".join(toks)))
            nread += 1
        elif nwrite < count // 2:
            term = rnd.choice(["\r\n", "\r\n", "\n", "\r", "", "\n\n", "xy"])
            rows = []
            for _ in range(rnd.randint(0, 5)):
                rows.append(["".join(rnd.choice(alphabet) for _ in range(rnd.randint(0, 5))) for _ in range(rnd.choice([0, 1, 1, 2, 3, 4]))])
            try:
                data, failed, done = py_write(rows, delim, quote, dq, term)
            except Exception:
                continue
            try:
                back, err = py_read(data, delim, quote, dq, True)
                rt = "rt1" if (err is None and back == done) else "rt0"
            except Exception:
                rt = "rt0"
            lines.append("W %s %s %d x%s %s %s = x%s f%d" % (
                hx(delim.encode()), hx(quote.encode()), dq, hx(term.encode("latin-1")), rt,
                " ".join(row_token(r) for r in rows), hx(data), failed))
            nwrite += 1
    open(outfile, "w").write("\n".join(lines) + "\n")
    print("csv cases: %d read, %d write" % (nread, nwrite))


main()
