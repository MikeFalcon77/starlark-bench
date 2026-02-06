use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use serde::Serialize;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "bench", about = "Starlark vs CPython benchmark suite")]
struct Cli {
    /// Engine to benchmark.
    #[arg(long)]
    engine: EngineName,

    /// Workload to run.
    #[arg(long)]
    workload: WorkloadName,

    /// Predefined problem size (overridden by --n).
    #[arg(long, default_value = "M")]
    size: Size,

    /// Measurement iterations (excluding warmup).
    #[arg(long, default_value_t = 10)]
    iters: u32,

    /// Warmup iterations (results printed but flagged).
    #[arg(long, default_value_t = 3)]
    warmup: u32,

    /// RNG seed for deterministic workloads.
    #[arg(long, default_value_t = 42)]
    seed: u64,

    /// Override the N parameter directly.
    #[arg(long)]
    n: Option<usize>,

    /// Python interpreter binary.
    #[arg(long, default_value = "python3")]
    python: String,

    /// Root directory for workload scripts.
    #[arg(long)]
    scripts_dir: Option<PathBuf>,
}

#[derive(Clone, ValueEnum)]
enum EngineName {
    Starlark,
    Python,
}

#[derive(Clone, ValueEnum)]
#[clap(rename_all = "snake_case")]
enum WorkloadName {
    Arithmetic,
    DataStructures,
    StringParsing,
    JsonBuilding,
    FunctionCalls,
}

#[derive(Clone, ValueEnum)]
#[clap(rename_all = "UPPER")]
enum Size {
    S,
    M,
    L,
}

impl Size {
    fn to_n(&self) -> usize {
        match self {
            Size::S => 1_000,
            Size::M => 50_000,
            Size::L => 500_000,
        }
    }
}

impl std::fmt::Display for Size {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Size::S => f.write_str("S"),
            Size::M => f.write_str("M"),
            Size::L => f.write_str("L"),
        }
    }
}

impl std::fmt::Display for EngineName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineName::Starlark => f.write_str("starlark"),
            EngineName::Python => f.write_str("python"),
        }
    }
}

impl WorkloadName {
    fn file_stem(&self) -> &'static str {
        match self {
            WorkloadName::Arithmetic => "arithmetic",
            WorkloadName::DataStructures => "data_structures",
            WorkloadName::StringParsing => "string_parsing",
            WorkloadName::JsonBuilding => "json_building",
            WorkloadName::FunctionCalls => "function_calls",
        }
    }
}

impl std::fmt::Display for WorkloadName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.file_stem())
    }
}

// ---------------------------------------------------------------------------
// JSON-lines report record
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct BenchRecord {
    engine: String,
    workload: String,
    size: String,
    n: usize,
    seed: u64,
    iter: u32,
    warmup: bool,
    /// Starlark-only: time spent parsing the AST (nanoseconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    parse_ns: Option<u64>,
    /// Time spent evaluating the workload (nanoseconds).
    eval_ns: u64,
    /// Wall-clock time including overhead (nanoseconds).
    total_ns: u64,
    /// Checksum returned by the workload (for correctness verification).
    result: i64,
    /// Resident set size in KiB (best-effort, 0 if unavailable).
    rss_kb: u64,
    cpu_model: String,
    os: String,
    rustc: String,
}

// ---------------------------------------------------------------------------
// System information helpers
// ---------------------------------------------------------------------------

fn cpu_model() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(info) = std::fs::read_to_string("/proc/cpuinfo") {
            for line in info.lines() {
                if line.starts_with("model name") {
                    if let Some(val) = line.split(':').nth(1) {
                        return val.trim().to_string();
                    }
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
        {
            if out.status.success() {
                return String::from_utf8_lossy(&out.stdout).trim().to_string();
            }
        }
    }
    "unknown".to_string()
}

fn os_info() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn rustc_version() -> String {
    Command::new("rustc")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

fn process_rss_kb() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if let Some(rest) = line.strip_prefix("VmRSS:") {
                    return rest
                        .split_whitespace()
                        .next()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
        {
            if out.status.success() {
                return String::from_utf8_lossy(&out.stdout)
                    .trim()
                    .parse()
                    .unwrap_or(0);
            }
        }
    }
    0
}

// ---------------------------------------------------------------------------
// Starlark engine
// ---------------------------------------------------------------------------

mod starlark_engine {
    use std::time::{Duration, Instant};

    use anyhow::{Result, anyhow};
    use starlark::environment::{FrozenModule, Globals, Module};
    use starlark::eval::Evaluator;
    use starlark::syntax::{AstModule, Dialect};
    use starlark::values::{OwnedFrozenValue, Value};

    pub struct PreparedScript {
        pub parse_dur: Duration,
        frozen: FrozenModule,
        run_fn: OwnedFrozenValue,
    }

    pub struct RunResult {
        pub eval_dur: Duration,
        pub result: i64,
    }

    /// Parse the script and freeze the module.
    /// The script **must** define a `run(n, seed)` function.
    pub fn prepare(script_body: &str) -> Result<PreparedScript> {
        let parse_start = Instant::now();
        let ast = AstModule::parse("bench.star", script_body.to_owned(), &Dialect::Extended)
            .map_err(|e| anyhow!("starlark parse error: {e}"))?;
        let parse_dur = parse_start.elapsed();

        let globals = Globals::standard();
        let module = Module::new();
        {
            let mut eval = Evaluator::new(&module);
            eval.eval_module(ast, &globals)
                .map_err(|e| anyhow!("starlark eval error during prepare: {e}"))?;
        }

        let frozen = module
            .freeze()
            .map_err(|e| anyhow!("starlark freeze error: {e:?}"))?;
        let run_fn = frozen
            .get("run")
            .map_err(|e| anyhow!("script must define run(n, seed): {e}"))?;

        Ok(PreparedScript {
            parse_dur,
            frozen,
            run_fn,
        })
    }

    fn extract_i64(value: Value) -> i64 {
        if value.is_none() {
            0
        } else if let Some(i) = value.unpack_i32() {
            i64::from(i)
        } else {
            value.to_repr().parse::<i64>().unwrap_or(0)
        }
    }

    /// Call the frozen `run(n, seed)` function once, measuring only eval time.
    pub fn call_run(prepared: &PreparedScript, n: usize, seed: u64) -> Result<RunResult> {
        let module = Module::new();
        // Import the frozen module so the evaluator can see the function's closure.
        module.import_public_symbols(&prepared.frozen);
        let mut eval = Evaluator::new(&module);

        let heap = module.heap();
        let n_val = heap.alloc(n as i64);
        let seed_val = heap.alloc(seed as i64);
        let func: Value = prepared.run_fn.value();

        let eval_start = Instant::now();
        let value = eval
            .eval_function(func, &[n_val, seed_val], &[])
            .map_err(|e| anyhow!("starlark eval error: {e}"))?;
        let eval_dur = eval_start.elapsed();

        let result = extract_i64(value);
        std::hint::black_box(result);

        Ok(RunResult { eval_dur, result })
    }
}

// ---------------------------------------------------------------------------
// Python engine (subprocess)
// ---------------------------------------------------------------------------

mod python_engine {
    use std::path::Path;
    use std::process::Command;
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result, bail};
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Output {
        timings_ns: Vec<u64>,
        result: i64,
        rss_kb: u64,
    }

    pub struct IterResult {
        pub eval_dur: Duration,
        pub result: i64,
    }

    pub struct RunResult {
        /// Per-iteration timings (measured inside Python).
        pub iters: Vec<IterResult>,
        /// Total subprocess wall time.
        pub total_dur: Duration,
        /// Max RSS reported by Python (KiB).
        pub rss_kb: u64,
    }

    /// Spawn CPython, run the workload `iter_count` times inside a single
    /// process, and collect per-iteration timings reported by the script.
    pub fn run(
        python_bin: &str,
        script_path: &Path,
        n: usize,
        seed: u64,
        iter_count: u32,
    ) -> Result<RunResult> {
        let wall_start = Instant::now();
        let output = Command::new(python_bin)
            .arg(script_path)
            .arg(n.to_string())
            .arg(seed.to_string())
            .arg(iter_count.to_string())
            .output()
            .with_context(|| format!("failed to spawn {python_bin}"))?;
        let total_dur = wall_start.elapsed();

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "Python script {} failed (exit {}):\n{}",
                script_path.display(),
                output.status,
                stderr
            );
        }

        let stdout = String::from_utf8(output.stdout)
            .context("Python stdout is not valid UTF-8")?;
        let parsed: Output = serde_json::from_str(stdout.trim())
            .with_context(|| format!("failed to parse Python JSON output: {stdout}"))?;

        let iters = parsed
            .timings_ns
            .iter()
            .map(|&ns| IterResult {
                eval_dur: Duration::from_nanos(ns),
                result: parsed.result,
            })
            .collect();

        Ok(RunResult {
            iters,
            total_dur,
            rss_kb: parsed.rss_kb,
        })
    }
}

// ---------------------------------------------------------------------------
// Script directory resolution
// ---------------------------------------------------------------------------

fn resolve_scripts_dir(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(p) = explicit {
        return p;
    }
    // Try CWD/scripts first (most common when running from repo root).
    let cwd = PathBuf::from("scripts");
    if cwd.is_dir() {
        return cwd;
    }
    // Try relative to the executable (for installed / CI layouts).
    if let Ok(exe) = std::env::current_exe() {
        let candidate = exe
            .parent()
            .unwrap_or(exe.as_path())
            .join("../share/starlark_bench/scripts");
        if candidate.is_dir() {
            return candidate;
        }
    }
    cwd // fall back, will error later with a clear message
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();
    let n = cli.n.unwrap_or_else(|| cli.size.to_n());
    let total_iters = cli.warmup + cli.iters;

    let scripts_dir = resolve_scripts_dir(cli.scripts_dir.clone());
    let stem = cli.workload.file_stem();

    // Collect system metadata once.
    let sys_cpu = cpu_model();
    let sys_os = os_info();
    let sys_rustc = rustc_version();

    match cli.engine {
        EngineName::Starlark => run_starlark(
            &cli, n, total_iters, &scripts_dir, stem, &sys_cpu, &sys_os, &sys_rustc,
        )?,
        EngineName::Python => run_python(
            &cli, n, total_iters, &scripts_dir, stem, &sys_cpu, &sys_os, &sys_rustc,
        )?,
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Starlark benchmark loop
// ---------------------------------------------------------------------------

fn run_starlark(
    cli: &Cli,
    n: usize,
    total_iters: u32,
    scripts_dir: &PathBuf,
    stem: &str,
    cpu: &str,
    os: &str,
    rustc: &str,
) -> Result<()> {
    let path = scripts_dir.join("starlark").join(format!("{stem}.star"));
    let script_body = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;

    // Parse once, freeze the module, extract the `run` function.
    let prepared = starlark_engine::prepare(&script_body)?;
    let parse_ns = prepared.parse_dur.as_nanos() as u64;

    for i in 0..total_iters {
        let is_warmup = i < cli.warmup;

        let r = starlark_engine::call_run(&prepared, n, cli.seed)?;

        let rss = process_rss_kb();

        let record = BenchRecord {
            engine: "starlark".into(),
            workload: stem.into(),
            size: cli.size.to_string(),
            n,
            seed: cli.seed,
            iter: if is_warmup { i } else { i - cli.warmup },
            warmup: is_warmup,
            parse_ns: if i == 0 { Some(parse_ns) } else { None },
            eval_ns: r.eval_dur.as_nanos() as u64,
            total_ns: r.eval_dur.as_nanos() as u64,
            result: r.result,
            rss_kb: rss,
            cpu_model: cpu.into(),
            os: os.into(),
            rustc: rustc.into(),
        };
        println!("{}", serde_json::to_string(&record)?);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Python benchmark loop
// ---------------------------------------------------------------------------

fn run_python(
    cli: &Cli,
    n: usize,
    _total_iters: u32,
    scripts_dir: &PathBuf,
    stem: &str,
    cpu: &str,
    os: &str,
    rustc: &str,
) -> Result<()> {
    let path = scripts_dir.join("python").join(format!("{stem}.py"));
    if !path.exists() {
        bail!("Python script not found: {}", path.display());
    }

    // Helper to emit records from a python run.
    let emit = |pr: &python_engine::RunResult, warmup: bool| -> Result<()> {
        let avg_total_ns = pr.total_dur.as_nanos() as u64
            / u64::from(pr.iters.len().max(1) as u32);
        for (j, ir) in pr.iters.iter().enumerate() {
            let record = BenchRecord {
                engine: "python".into(),
                workload: stem.into(),
                size: cli.size.to_string(),
                n,
                seed: cli.seed,
                iter: j as u32,
                warmup,
                parse_ns: None,
                eval_ns: ir.eval_dur.as_nanos() as u64,
                total_ns: avg_total_ns,
                result: ir.result,
                rss_kb: pr.rss_kb,
                cpu_model: cpu.into(),
                os: os.into(),
                rustc: rustc.into(),
            };
            println!("{}", serde_json::to_string(&record)?);
        }
        Ok(())
    };

    // --- warmup (single subprocess invocation) ---
    if cli.warmup > 0 {
        let wr = python_engine::run(&cli.python, &path, n, cli.seed, cli.warmup)?;
        emit(&wr, true)?;
    }

    // --- measurement ---
    if cli.iters > 0 {
        let mr = python_engine::run(&cli.python, &path, n, cli.seed, cli.iters)?;
        emit(&mr, false)?;
    }

    Ok(())
}
