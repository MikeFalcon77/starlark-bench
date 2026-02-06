"""Shared benchmark harness for Python workloads.

Usage in each workload script:
    from _harness import bench_main
    def run(n, seed):
        ...
        return checksum
    bench_main(run)

The script is invoked as:
    python3 <script>.py <N> <SEED> <ITERS>

It prints a single JSON line:
    {"timings_ns": [...], "result": <int>, "rss_kb": <int>}
"""

import json
import sys
import time


def bench_main(workload_fn):
    n = int(sys.argv[1])
    seed = int(sys.argv[2])
    iters = int(sys.argv[3])

    timings_ns = []
    result = None

    for i in range(iters):
        start = time.perf_counter_ns()
        r = workload_fn(n, seed)
        elapsed = time.perf_counter_ns() - start
        timings_ns.append(elapsed)

        if result is None:
            result = r
        elif r != result:
            print(
                f"ERROR: result mismatch at iter {i}: expected {result}, got {r}",
                file=sys.stderr,
            )
            sys.exit(1)

    # Best-effort RSS (KiB on Linux, bytes/1024 on macOS).
    rss_kb = 0
    try:
        import resource
        usage = resource.getrusage(resource.RUSAGE_SELF)
        rss_kb = usage.ru_maxrss
        if sys.platform == "darwin":
            rss_kb //= 1024  # macOS reports bytes
    except ImportError:
        pass  # Windows

    print(json.dumps({"timings_ns": timings_ns, "result": result, "rss_kb": rss_kb}))
