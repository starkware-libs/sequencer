---
name: adding-metric-and-panel
description: Add a new metric and corresponding Grafana panel in the Apollo sequencer dashboard. Use when adding observability for a component (committer, gateway, batcher, etc.), defining metrics in Rust, or adding panels to dev_grafana.
---

# Adding a Metric and Panel

Use this workflow when adding a new metric and its Grafana panel to the Apollo dashboard.

## 1. Define the metric (component crate)

In the component’s `metrics.rs` (e.g. `crates/apollo_committer/src/metrics.rs`):

- Add an entry inside `define_metrics!(ComponentName => { ... })` using one of:
  - **MetricGauge** – current value (e.g. offset, status).
  - **MetricCounter** – monotonically increasing (e.g. blocks committed, durations). Use `init = 0` and document units (e.g. milliseconds, microseconds).
  - **MetricHistogram** – distribution (e.g. counts or latencies per block).
- Export the constant (it implements `MetricQueryName`).
- In `register_metrics()` (or the component’s registration path), call `<METRIC>.register()` for the new metric.

Follow existing naming: `snake_case`, descriptive (e.g. `total_block_duration`, `count_storage_tries_modifications_per_block`).

## 2. Add the panel (apollo_dashboard)

In the matching panel module under `crates/apollo_dashboard/src/panels/<component>.rs`:

- Import the new metric from the component’s `metrics` module.
- Create a panel using:
  - **Gauge**: `Panel::new(name, description, metric.get_name_with_filter(), PanelType::Stat)` or `Panel::from_gauge(&metric, PanelType::Stat)`.
  - **Counter**: use `query_builder::increase(&metric, "1m")` (or another window) as the expression; for “per block” averages use existing helpers like `average_per_block_panel` (see committer panels).
  - **Histogram**: `Panel::from_hist(&metric, "Title", "Description")` then `.with_unit(Unit::Seconds)` etc. if needed.
- Optionally chain `.with_unit(Unit::...)`, `.with_log_query("...")`, `.with_legends([...])`.
- Add the new panel function to the component’s row in the same file, e.g. in `get_committer_row()` add the new panel to the `vec![...]` passed to `Row::new()`.

Do **not** add the panel to `dashboard_definitions.rs` unless you are adding a new row; rows are already wired there.

## 3. Regenerate dev Grafana JSON

After changing panels, regenerate the dev dashboard so the test passes:

```bash
cargo run --bin sequencer_dashboard_generator -q
```

The test `default_dev_grafana_dashboard` in `dashboard_definitions_test.rs` checks that `crates/apollo_dashboard/resources/dev_grafana.json` (and alerts) are up to date; the command above updates them.

## Reference

- **Panel helpers**: `crates/apollo_dashboard/src/panel.rs` – `Panel::new`, `from_hist`, `from_gauge`, `ratio_time_series`, `with_unit`, `with_log_query`.
- **Query helpers**: `crates/apollo_dashboard/src/query_builder.rs` – `increase()`, `sum_by_label()`.
- **Example rows**: `panels/committer.rs`, `panels/gateway.rs` for counters/histograms and row layout.
