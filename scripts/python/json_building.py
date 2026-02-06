"""JSON-ish building: construct nested maps/lists, serialize manually."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from _harness import bench_main


def run(n, seed):
    x = seed % 1000000 + 1

    def json_str(s):
        return '"' + s + '"'

    def json_int(v):
        return str(v)

    def json_list(items):
        return "[" + ", ".join(items) + "]"

    def json_obj(pairs):
        parts = []
        for kv in pairs:
            parts.append(json_str(kv[0]) + ": " + kv[1])
        return "{" + ", ".join(parts) + "}"

    checksum = 0

    for i in range(n):
        x = (x * 1103515245 + 12345) % 2147483648
        items = []
        for j in range(5):
            val = (x + j * 7) % 10000
            entry = json_obj([
                ("id", json_int(i * 5 + j)),
                ("value", json_int(val)),
                ("name", json_str("item" + str(val))),
                ("tags", json_list([
                    json_str("t" + str(val % 10)),
                    json_str("t" + str(val % 20)),
                ])),
            ])
            items.append(entry)

        doc = json_obj([
            ("batch", json_int(i)),
            ("count", json_int(len(items))),
            ("items", json_list(items)),
        ])
        checksum = (checksum + len(doc)) % 2147483648

    return checksum


bench_main(run)
