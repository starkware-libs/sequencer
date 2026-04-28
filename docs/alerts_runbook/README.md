# Alerts Runbook

This directory is the on-call's first stop when an Apollo alert fires. Every
alert defined in `crates/apollo_dashboard/` has an entry in the index below.
When a per-alert guide exists, it lives next to this file at `<alert_name>.md`.

## How to use

1. An alert fires. Note its **name** (it appears in the Slack alert payload and
   in the Grafana alert URL — it matches the `name` field in the Rust alert
   definition).
2. Find the alert in the [index](#index) below.
3. If the row is marked `[done]`, the guide is at `<alert_name>.md` — click
   the link and follow it.
4. If the row is marked `[TODO]`, no guide exists yet. Investigate using your
   usual tools (Grafana, GCP logs, the relevant code in
   `crates/apollo_dashboard/src/alert_scenarios/`). To start a guide, copy
   `_TEMPLATE.md` to `<alert_name>.md` and fill in whatever sections you can.

## Severity legend

The severity ladder lives in `crates/apollo_dashboard/src/alerts.rs`. Short
form:

| Code | Name            | Meaning                                                                 |
| ---- | --------------- | ----------------------------------------------------------------------- |
| `p1` | Sos             | High-impact incident affecting availability. Immediate attention.       |
| `p2` | Regular         | Production issue requiring round-the-clock attention.                   |
| `p3` | DayOnly         | Important but delayed during night hours.                               |
| `p4` | WorkingHours    | Triggers during official business hours only; off during holidays.      |
| `p5` | Informational   | Non-critical; not intended to wake anyone up. Dev team monitors.        |

Some alerts use a per-environment severity placeholder. For these, the
index lists the entry as `(configurable per env, pX for testnet and
integration)` — the value shown is the severity currently deployed on
both public testnet (sepolia-alpha) and integration (sepolia-integration),
which match. Mainnet may differ; its values live in a separate deployment
repo and are not inlined here.

## Where alerts are defined

- Scenario-grouped alerts: `crates/apollo_dashboard/src/alert_scenarios/<scenario>.rs`
- Misc alerts not yet moved into a scenario module: `crates/apollo_dashboard/src/alert_definitions.rs`
- Generated Grafana JSON (authoritative): `crates/apollo_dashboard/resources/dev_grafana_alerts.json`

## Operational context

A few patterns observed across `#starknet-mainnet-production`,
`#starknet-testnet-production`, and `#starknet-integration-production` that
are useful background for any on-call:

- **Planned upgrades silence reasoning.** If an upgrade is in progress, alerts
  are commonly expected and announced in the channel ("Alerts due to the
  upgrade, kindly ignore"). Always check the channel before paging owners.
- **`p4` alerts are often silenced during weekends and holidays.** If you see
  a stale `p4` and there is no recent context, this may be why.
- **Spot-instance churn produces recurring `p4`s.** Several alerts tied to
  `sequencer-l1` (gas price scraper, eth-to-strk, consensus L1 gas price)
  fire briefly when a spot instance is evicted, and self-resolve within
  ~5 minutes. Verify by checking whether the alert closed quickly on its own.
- **Simulator funding** running out triggers idle/no-tx alerts on testnet and
  integration. The fix is to top up the simulator account.
- **Threshold tuning** is a normal part of the runbook lifecycle. If an alert
  fires repeatedly with no real impact, propose a threshold change in a PR
  rather than tolerating noise.

## Index

Each entry: `[status]` `alert_name` `(severity)`. Status is `[TODO]` until a
guide exists, then `[done]` with a link. Severity
`(configurable per env, pX for testnet and integration)` means the
value is set per environment via a placeholder; the `pX` shown is the
current testnet+integration severity.

### Block production halt

Source: `alert_scenarios/block_production_halt.rs`

- `[TODO]` `batched_transactions_stuck` (configurable per env, p5 for testnet and integration)
- `[TODO]` `batched_transactions_stuck_long_time` (configurable per env, p5 for testnet and integration)
- `[TODO]` `consensus_block_number_stuck` (configurable per env, p3 for testnet and integration)
- `[TODO]` `consensus_block_number_stuck_long_time` (p2)
- `[TODO]` `consensus_p2p_not_enough_peers_for_quorum` (configurable per env, p4 for testnet and integration)
- `[TODO]` `consensus_p2p_not_enough_peers_for_quorum_long_time` (p2)
- `[TODO]` `consensus_round_high` (configurable per env, p4 for testnet and integration)

### Block production delay

Source: `alert_scenarios/block_production_delay.rs`

- `[TODO]` `cende_write_blob_failure` (configurable per env, p4 for testnet and integration)
- `[TODO]` `cende_write_blob_failure_once` (p5)
- `[TODO]` `consensus_block_number_progress_is_slow` (configurable per env, p4 for testnet and integration)
- `[TODO]` `consensus_p2p_peer_down` (configurable per env, p4 for testnet and integration)
- `[TODO]` `consensus_round_above_zero` (p5)
- `[TODO]` `consensus_round_above_zero_multiple_times` (configurable per env, p4 for testnet and integration)

### Other consensus / data availability

Source: `alert_definitions.rs`

- `[TODO]` `cende_write_prev_height_blob_latency_too_high` (p4)
- `[TODO]` `consensus_conflicting_votes` (p4)
- `[TODO]` `consensus_decisions_reached_by_consensus_ratio` (p4)
- `[TODO]` `consensus_inbound_peer_evicted` (p5)
- `[TODO]` `consensus_inbound_stream_buffer_full` (p4)
- `[TODO]` `consensus_inbound_stream_evicted` (p5)
- `[TODO]` `consensus_l1_gas_price_provider_failure` (p4)
- `[TODO]` `consensus_l1_gas_price_provider_failure_once` (p5)
- `[TODO]` `consensus_p2p_disconnections` (p4)
- `[TODO]` `consensus_proposal_fin_mismatch_once` (p4)
- `[TODO]` `consensus_retrospective_block_hash_mismatch` (p1)
- `[TODO]` `consensus_votes_num_sent_messages` (p5)

### State sync

Source: `alert_scenarios/sync_halt.rs`

- `[TODO]` `state_sync_lag` (configurable per env, p3 for testnet and integration)
- `[TODO]` `state_sync_stuck` (p2)
- `[TODO]` `state_sync_stuck_long_time` (p2)

### Storage open transactions

Source: `alert_definitions.rs`

- `[TODO]` `batcher_storage_open_read_transactions` (p4)
- `[TODO]` `class_manager_storage_open_read_transactions` (p4)
- `[TODO]` `sync_storage_open_read_transactions` (p4)

### Transaction throughput (TPS)

Source: `alert_scenarios/tps.rs`

- `[TODO]` `gateway_add_tx_idle_p2p_rpc` (p2)
- `[TODO]` `gateway_low_successful_transaction_rate` (configurable per env, p4 for testnet and integration)
- `[TODO]` `http_server_no_successful_transactions` (p5)
- `[TODO]` `mempool_add_tx_idle_p2p_rpc` (p1)

### Transaction delays

Source: `alert_scenarios/transaction_delays.rs`

- `[TODO]` `high_empty_blocks_ratio` (configurable per env, p2 for testnet and integration)
- `[TODO]` `http_server_avg_add_tx_latency` (configurable per env, p4 for testnet and integration)
- `[TODO]` `http_server_min_add_tx_latency` (configurable per env, p2 for testnet and integration)
- `[TODO]` `http_server_p95_add_tx_latency` (p5)
- `[TODO]` `mempool_p2p_disconnections` (p4) — defined in `alert_definitions.rs`
- `[TODO]` `mempool_p2p_peer_down` (configurable per env, p4 for testnet and integration)

### Transaction failures

Source: `alert_scenarios/transaction_failures.rs`

- `[TODO]` `http_server_high_deprecated_transaction_failure_ratio` (p5)
- `[TODO]` `http_server_high_transaction_failure_ratio` (p5)
- `[TODO]` `http_server_internal_error_once` (p4)
- `[TODO]` `http_server_internal_error_ratio` (configurable per env, p4 for testnet and integration)
- `[TODO]` `mempool_transaction_drop_ratio` (configurable per env, p4 for testnet and integration)

### Mempool size

Source: `alert_scenarios/mempool_size.rs`

- `[TODO]` `mempool_evictions_count` (configurable per env, p3 for testnet and integration)
- `[TODO]` `mempool_pool_size_increase` (configurable per env, p4 for testnet and integration)

### Preconfirmed block

Source: `alert_scenarios/preconfirmed.rs`

- `[TODO]` `preconfirmed_block_not_written` (configurable per env, p4 for testnet and integration)

### L1 message handlers

Sources: `alert_scenarios/l1_handlers.rs`, `alert_definitions.rs`

- `[TODO]` `batcher_l1_events_provider_errors` (p4) — `alert_definitions.rs`
- `[TODO]` `l1_message_no_successes` (configurable per env, p3 for testnet and integration) — `alert_scenarios/l1_handlers.rs`
- `[TODO]` `l1_message_scraper_baselayer_error_count` (p5) — `alert_definitions.rs`
- `[TODO]` `l1_message_scraper_reorg_detected` (p1) — `alert_definitions.rs`

### L1 gas prices

Sources: `alert_scenarios/l1_gas_prices.rs`, `alert_definitions.rs`

- `[TODO]` `eth_to_strk_error_count` (p5) — `alert_definitions.rs`
- `[TODO]` `eth_to_strk_success_count` (configurable per env, p4 for testnet and integration) — `alert_scenarios/l1_gas_prices.rs`
- `[TODO]` `l1_gas_price_provider_insufficient_history` (configurable per env, p4 for testnet and integration) — `alert_scenarios/l1_gas_prices.rs`
- `[TODO]` `l1_gas_price_scraper_baselayer_error_count` (p5) — `alert_definitions.rs`
- `[TODO]` `l1_gas_price_scraper_reorg_detected` (p5) — `alert_definitions.rs`
- `[TODO]` `l1_gas_price_scraper_success_count` (configurable per env, p4 for testnet and integration) — `alert_scenarios/l1_gas_prices.rs`

### Config manager

Source: `alert_scenarios/config_manager.rs`

- `[TODO]` `config_manager_update_error_increase` (p2)

### Other application alerts

Source: `alert_definitions.rs`

- `[TODO]` `gateway_proof_archive_write_failure` (p4)
- `[TODO]` `native_compilation_error` (p4)
- `[TODO]` `staking_epoch_id_mismatch` (p3)

### Pod / infra health

Source: `alert_scenarios/infra_alerts.rs`

- `[TODO]` `periodic_ping` (p2)
- `[TODO]` `pod_high_cpu_utilization` (p2)
- `[TODO]` `pod_state_crashloopbackoff` (p2)
- `[TODO]` `pod_state_critical_disk_utilization` (p4)
- `[TODO]` `pod_state_critical_memory_utilization` (p2)
- `[TODO]` `pod_state_disk_filling_soon` (p4)
- `[TODO]` `pod_state_high_memory_utilization` (p3)
- `[TODO]` `pod_state_not_ready` (p2)

### Remote server connections (per service)

Source: `alert_scenarios/remote_server_connections.rs`

- `[TODO]` `batcher_remote_server_number_of_connections` (p3)
- `[TODO]` `class_manager_remote_server_number_of_connections` (p3)
- `[TODO]` `committer_remote_server_number_of_connections` (p3)
- `[TODO]` `config_manager_remote_server_number_of_connections` (p3)
- `[TODO]` `gateway_remote_server_number_of_connections` (p3)
- `[TODO]` `l1_events_remote_server_number_of_connections` (p3)
- `[TODO]` `l1_gas_price_remote_server_number_of_connections` (p3)
- `[TODO]` `mempool_p2p_remote_server_number_of_connections` (p3)
- `[TODO]` `mempool_remote_server_number_of_connections` (p3)
- `[TODO]` `sierra_compiler_remote_server_number_of_connections` (p3)
- `[TODO]` `signature_manager_remote_server_number_of_connections` (p3)
- `[TODO]` `state_sync_remote_server_number_of_connections` (p3)

