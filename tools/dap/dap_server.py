#!/usr/bin/env python3
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from adapter import Adapter
from protocol import Channel


def main():
    gdb_path = os.environ.get("FIRN_GDB", "gdb")
    Adapter(Channel(), gdb_path).serve()


if __name__ == "__main__":
    main()
