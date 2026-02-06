"""Plot benchmark results as grouped bar chart.

Usage:
    make run-all | python3 scripts/plot.py
    python3 scripts/plot.py results.jsonl
    python3 scripts/plot.py results.jsonl -o chart.png
"""

import json
import sys
from collections import defaultdict
from pathlib import Path

def parse_records(source):
    records = []
    for line in source:
        line = line.strip()
        if not line:
            continue
        try:
            rec = json.loads(line)
            records.append(rec)
        except json.JSONDecodeError:
            continue
    return records


def median(values):
    s = sorted(values)
    n = len(s)
    if n == 0:
        return 0.0
    if n % 2 == 1:
        return s[n // 2]
    return (s[n // 2 - 1] + s[n // 2]) / 2


def main():
    try:
        import matplotlib
        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
        import matplotlib.ticker as ticker
    except ImportError:
        print("ERROR: matplotlib is required.  pip install matplotlib", file=sys.stderr)
        sys.exit(1)

    # --- parse args ---
    out_path = None
    input_file = None
    args = sys.argv[1:]
    i = 0
    while i < len(args):
        if args[i] in ("-o", "--output") and i + 1 < len(args):
            out_path = args[i + 1]
            i += 2
        else:
            input_file = args[i]
            i += 1

    # --- read data ---
    if input_file:
        with open(input_file) as f:
            records = parse_records(f)
    elif not sys.stdin.isatty():
        records = parse_records(sys.stdin)
    else:
        print("Usage: make run-all | python3 scripts/plot.py [-o chart.png]", file=sys.stderr)
        sys.exit(1)

    # --- filter: only measurement runs (warmup=false) ---
    records = [r for r in records if not r.get("warmup", False)]
    if not records:
        print("No measurement records found (all warmup?).", file=sys.stderr)
        sys.exit(1)

    # --- group by (size, workload, engine) → list of eval_ns ---
    grouped = defaultdict(list)
    for r in records:
        key = (r["size"], r["workload"], r["engine"])
        grouped[key].append(r["eval_ns"])

    # --- collect sizes present ---
    sizes = sorted({r["size"] for r in records}, key=lambda s: {"S": 0, "M": 1, "L": 2}.get(s, 3))

    workloads = []
    seen = set()
    for r in records:
        wl = r["workload"]
        if wl not in seen:
            workloads.append(wl)
            seen.add(wl)

    engines = []
    seen_e = set()
    for r in records:
        e = r["engine"]
        if e not in seen_e:
            engines.append(e)
            seen_e.add(e)

    COLORS = {"starlark": "#3b82f6", "python": "#ef4444"}
    default_colors = ["#3b82f6", "#ef4444", "#22c55e", "#f59e0b"]

    # --- one subplot per size ---
    n_sizes = len(sizes)
    fig, axes = plt.subplots(1, n_sizes, figsize=(5 * n_sizes + 1, 6), squeeze=False)
    fig.suptitle("Starlark vs CPython — eval time (lower is better)", fontsize=14, fontweight="bold")

    for ax_idx, size in enumerate(sizes):
        ax = axes[0][ax_idx]
        n_wl = len(workloads)
        n_eng = len(engines)
        bar_width = 0.8 / n_eng
        x_positions = list(range(n_wl))

        for eng_idx, engine in enumerate(engines):
            medians = []
            for wl in workloads:
                vals = grouped.get((size, wl, engine), [])
                medians.append(median(vals) / 1e6)  # ns → ms

            offsets = [x + eng_idx * bar_width for x in x_positions]
            color = COLORS.get(engine, default_colors[eng_idx % len(default_colors)])
            bars = ax.bar(offsets, medians, bar_width, label=engine, color=color, edgecolor="white", linewidth=0.5)

            # value labels on top
            for bar, val in zip(bars, medians):
                if val > 0:
                    label = f"{val:.1f}" if val >= 1 else f"{val:.2f}"
                    ax.text(
                        bar.get_x() + bar.get_width() / 2,
                        bar.get_height(),
                        label,
                        ha="center", va="bottom", fontsize=7, color="#333",
                    )

        ax.set_title(f"Size {size}", fontsize=12)
        ax.set_ylabel("Median eval time (ms)")
        ax.set_xticks([x + bar_width * (n_eng - 1) / 2 for x in x_positions])
        ax.set_xticklabels(workloads, rotation=30, ha="right", fontsize=8)
        ax.yaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{v:,.0f}" if v >= 10 else f"{v:.1f}"))
        ax.set_ylim(bottom=0)
        ax.grid(axis="y", alpha=0.3, linewidth=0.5)
        ax.legend(fontsize=9)

    plt.tight_layout(rect=[0, 0, 1, 0.94])

    if out_path:
        fig.savefig(out_path, dpi=150, bbox_inches="tight")
        print(f"Chart saved to {out_path}")
    else:
        default = "bench_chart.png"
        fig.savefig(default, dpi=150, bbox_inches="tight")
        print(f"Chart saved to {default}")


if __name__ == "__main__":
    main()
