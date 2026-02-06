BENCH   := cargo run --release --bin bench --
PYTHON  ?= python3
SIZES   := S M L
ENGINES := starlark python

WORKLOADS := arithmetic data_structures string_parsing json_building function_calls

.PHONY: build test smoke run-all verify plot chart clean help

help:
	@echo "Targets:"
	@echo "  build      - release build"
	@echo "  smoke      - quick S-size run of every engine/workload (1 iter, 1 warmup)"
	@echo "  run-all    - full M-size run of every engine/workload → results.jsonl"
	@echo "  plot       - generate chart from results.jsonl"
	@echo "  chart      - run-all + plot in one step"
	@echo "  verify     - check that starlark and python produce identical checksums"
	@echo "  clean      - cargo clean"

build:
	cargo build --release

# Quick sanity check: size S, 1 warmup, 1 measurement iteration.
smoke: build
	@for engine in $(ENGINES); do \
		for wl in $(WORKLOADS); do \
			echo "--- $$engine / $$wl / S ---"; \
			$(BENCH) --engine $$engine --workload $$wl --size S \
				--iters 1 --warmup 1 --python $(PYTHON); \
		done; \
	done

# Full run at size M (override with SIZES="S M L" for all sizes).
# Results go to results.jsonl (stderr shows progress).
run-all: build
	@rm -f results.jsonl; touch results.jsonl; \
	for size in $(SIZES); do \
		for engine in $(ENGINES); do \
			for wl in $(WORKLOADS); do \
				echo "--- $$engine / $$wl / $$size ---" >&2; \
				$(BENCH) --engine $$engine --workload $$wl --size $$size \
					--iters 10 --warmup 3 --python $(PYTHON) \
					| tee -a results.jsonl; \
			done; \
		done; \
	done; \
	echo ">>> results.jsonl written ($$(wc -l < results.jsonl) lines)" >&2

# Generate chart from results.jsonl.
plot: results.jsonl
	$(PYTHON) scripts/plot.py results.jsonl -o bench_chart.png

# Run benchmarks and plot in one step.
chart: run-all plot

# Verify both engines produce the same checksum for every workload at size S.
verify: build
	@fail=0; \
	for wl in $(WORKLOADS); do \
		star=$$($(BENCH) --engine starlark --workload $$wl --size S \
			--iters 1 --warmup 0 --python $(PYTHON) 2>/dev/null \
			| $(PYTHON) -c "import sys,json; print(json.loads(sys.stdin.readline())['result'])"); \
		py=$$($(BENCH) --engine python --workload $$wl --size S \
			--iters 1 --warmup 0 --python $(PYTHON) 2>/dev/null \
			| $(PYTHON) -c "import sys,json; print(json.loads(sys.stdin.readline())['result'])"); \
		if [ "$$star" = "$$py" ]; then \
			echo "OK  $$wl  checksum=$$star"; \
		else \
			echo "FAIL $$wl  starlark=$$star  python=$$py"; \
			fail=1; \
		fi; \
	done; \
	if [ $$fail -ne 0 ]; then echo "VERIFICATION FAILED"; exit 1; fi

clean:
	cargo clean
