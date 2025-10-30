# bench_tools

Benchmark framework for running Criterion benchmarks with CI integration, automated input management via GCS, and performance regression detection.

## Quick Start

```bash
# List available benchmarks
cargo run -p bench_tools -- list

# Run benchmarks for a package
cargo run -p bench_tools -- run --package starknet_committer_and_os_cli --out ./results

# Run with regression checks (requires baseline: cargo bench -p <package>)
cargo run -p bench_tools -- run-and-compare \
  --package starknet_committer_and_os_cli \
  --out ./results \
  --regression-limit 5.0
```

## CLI Commands

### `run`

Execute benchmarks and save results.

```bash
cargo run -p bench_tools -- run --package <PACKAGE> --out <DIR> [--input-dir <DIR>]
```

- Downloads inputs from GCS automatically, or use `--input-dir` for local inputs
- Saves results to output directory as `{bench_name}_estimates.json`

### `run-and-compare`

Execute benchmarks and fail if regressions exceed limits.

```bash
cargo run -p bench_tools -- run-and-compare \
  --package <PACKAGE> \
  --out <DIR> \
  --regression-limit 5.0 \
  [--set-absolute-time-ns-limit <BENCH_NAME> <LIMIT_NS>]
```

Example with absolute time limits:

```bash
cargo run -p bench_tools -- run-and-compare \
  --package starknet_committer_and_os_cli \
  --out ./results \
  --regression-limit 5.0 \
  --set-absolute-time-ns-limit full_committer_flow 50000000 \
  --set-absolute-time-ns-limit tree_computation_flow 30000000
```

Output:
```
✓ full_committer_flow: +2.34% | 45123456.78ns
✓ tree_computation_flow: -0.89% | 28765432.10ns
✅ All benchmarks passed!
```

Or if exceeded:
```
❌ full_committer_flow: +7.50% (EXCEEDS 5.0% limit)
```

### `list`

Show available benchmarks.

```bash
cargo run -p bench_tools -- list [--package <PACKAGE>]
```

### `upload-inputs`

Upload benchmark inputs to GCS. Authenticate with `gcloud auth login`.

```bash
cargo run -p bench_tools -- upload-inputs \
  --benchmark <NAME> \
  --input-dir <LOCAL_DIR>
```

## Adding New Benchmarks

Add a `BenchmarkConfig` to the `BENCHMARKS` array in `src/types/benchmark_config.rs`:

```rust
BenchmarkConfig {
    name: "my_benchmark",                    // Unique identifier
    package: "my_package",                   // Cargo package name
    cmd_args: &["bench", "-p", "my_package", "pattern"],  // cargo bench args
    input_dir: Some("crates/my_package/test_inputs"),     // Optional, for GCS inputs
    criterion_benchmark_names: None,         // None = single bench with same name
},
```

### Multiple Criterion Benchmarks

If your benchmark file has multiple `bench_function` calls:

```rust
BenchmarkConfig {
    name: "multi_bench",
    package: "my_package",
    cmd_args: &["bench", "-p", "my_package", "--bench", "my_bench"],
    input_dir: None,
    criterion_benchmark_names: Some(&["bench_1", "bench_2", "bench_3"]),
},
```

### Field Reference

| Field | Description |
|-------|-------------|
| `name` | Config identifier (for CLI) |
| `package` | Cargo package containing the benchmark |
| `cmd_args` | Arguments for `cargo bench` |
| `input_dir` | Where to place downloaded inputs (if needed) |
| `criterion_benchmark_names` | List of Criterion bench names (for regression checks). `None` = uses `name` |

### Steps

1. Write benchmark in your package's `benches/` directory
2. Add `[[bench]]` section to package's `Cargo.toml`
3. Add `BenchmarkConfig` to `BENCHMARKS` array
4. If inputs needed: `cargo run -p bench_tools -- upload-inputs --benchmark <name> --input-dir <path>`
5. Run: `cargo run -p bench_tools -- run --package <package> --out ./results`

## GCS Integration

Inputs are stored at `gs://apollo_benchmarks/{benchmark_name}/input/`. Authenticate with `gcloud auth login`.

- Inputs auto-download if `input_dir` is set and no `--input-dir` provided
- Use `--input-dir` to override with local inputs
- Use `upload-inputs` command to push new inputs to GCS

## Notes

- First run establishes baseline: `cargo bench -p <package>`
- Subsequent `run-and-compare` compares against baseline
- Positive % = slower (regression), Negative % = faster (improvement)
- See `src/benches/dummy_bench.rs` for example implementation
