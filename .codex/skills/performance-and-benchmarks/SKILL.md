---
name: performance-and-benchmarks
description: Use this skill when investigating slow code, performance regressions, Criterion benches, `bench_tools`, or CI benchmark limits for blockifier and committer flows. It should also trigger when a PR claims a speedup or when benchmark artifacts or GCS inputs are involved.
---

# Performance and Benchmarks

<purpose>
Measure performance with the repo's actual benchmark tooling instead of relying on intuition.
</purpose>

<context>
- `crates/bench_tools` wraps Criterion runs, baseline comparison, and GCS input download/upload.
- Blockifier and committer CI workflows use `bench_tools -- run` and `-- run-and-compare`.
- Comparison mode requires an existing Criterion baseline in `target/criterion`.
</context>

<procedure>
1. List available benchmarks first:
   - `cargo run -p bench_tools -- list --package <package>`
2. Run the package benchmark suite:
   - `cargo run -p bench_tools -- run --package <package> --out /tmp/<package>-bench`
3. Compare against a baseline only after a prior run exists:
   - `cargo run -p bench_tools -- run-and-compare --package <package> --out /tmp/<package>-bench --regression-limit <percent>`
4. If the benchmark needs input files, provide `--input-dir` locally or ensure GCS auth in CI.
5. Pair benchmark claims with functional verification in the affected crate.
</procedure>

<patterns>
<do>
- Use `bench_tools` instead of ad hoc Criterion invocations for repo-standard comparisons.
- Make a baseline run before `run-and-compare`.
- Read benchmark output from `target/criterion` when diagnosing regressions.
</do>
<dont>
- Don't claim a perf win without measured output.
- Don't use comparison mode before a baseline exists.
- Don't treat benchmark-only changes as safe if the affected code path lacks normal tests.
</dont>
</patterns>

<examples>
Example: local benchmark listing
```bash
cargo run -p bench_tools -- list --package blockifier
```
</examples>

<troubleshooting>
| Symptom | Cause | Fix |
|---------|-------|-----|
| `No benchmarks found for package` | wrong package name or package has no configured benchmarks | list packages or inspect `bench_tools` config |
| `Change file not found` | compare mode has no baseline | run `bench_tools -- run` first |
| Missing benchmark inputs | input files are stored in GCS or not passed locally | use `--input-dir` or set up CI auth |
</troubleshooting>

<references>
- `crates/bench_tools/src/main.rs`: CLI entrypoints
- `crates/bench_tools/src/runner.rs`: benchmark run and compare flow
- `crates/bench_tools/src/comparison.rs`: threshold checks
- `.github/workflows/blockifier_ci.yml`: blockifier benchmark CI
- `.github/workflows/committer_ci.yml`: committer benchmark CI
</references>
