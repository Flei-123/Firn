import os


def fib(n):
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)


print(fib(int(os.environ.get("PYN_FIBN", "40"))))
