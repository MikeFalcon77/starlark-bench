"""Data-structure heavy: dict updates/lookups, list growth/slicing/iteration."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from _harness import bench_main


def run(n, seed):
    x = seed % 1000000 + 1
    half = n // 2 + 1

    # --- dict insertions ---
    d = {}
    for i in range(n):
        x = (x * 1103515245 + 12345) % 2147483648
        key = "k" + str(x % half)
        d[key] = x

    # --- dict lookups ---
    checksum = 0
    for i in range(n):
        x = (x * 1103515245 + 12345) % 2147483648
        key = "k" + str(x % half)
        v = d.get(key, 0)
        checksum = (checksum + v) % 2147483648

    # --- list growth ---
    lst = []
    for i in range(n):
        x = (x * 1103515245 + 12345) % 2147483648
        lst.append(x % 10000)

    # --- list iteration ---
    for v in lst:
        checksum = (checksum + v) % 2147483648

    # --- list slicing ---
    if len(lst) > 100:
        s = lst[10:60]
        for v in s:
            checksum = (checksum + v) % 2147483648

    return checksum


bench_main(run)
