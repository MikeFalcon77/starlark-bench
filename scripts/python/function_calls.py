"""Function-call overhead: hot loop calling small functions."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from _harness import bench_main


def run(n, seed):
    x = seed % 1000000 + 1

    def small_fn(a, b):
        return (a * 31 + b) % 2147483648

    def medium_fn(a, b, c):
        r = small_fn(a, b)
        r = small_fn(r, c)
        return r

    checksum = 0
    y = x
    for i in range(n):
        y = small_fn(y, i)
        checksum = medium_fn(checksum, y, i)

    return checksum


bench_main(run)
