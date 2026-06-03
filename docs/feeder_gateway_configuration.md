# Feeder Gateway Configuration

Configuration reference for the `apollo_feeder_gateway` component. All fields live under
`feeder_gateway_config` in the node config, and the component is enabled/disabled via
`components.feeder_gateway.execution_mode` (`Enabled` / `Disabled`).

## Fields

| Param | Type | Default | Description |
|---|---|---|---|
| `feeder_gateway_config.ip` | IP address | `0.0.0.0` | Address the feeder gateway HTTP server binds to. |
| `feeder_gateway_config.port` | u16 | `8082` | Port the server binds to. Intentionally NOT the legacy Python feeder gateway port `9713`, since the two run side by side during the migration. |
| `feeder_gateway_config.read_backend` | `Colocated` \| `Remote` | `Colocated` | How chain data is read (see Topology). |
| `feeder_gateway_config.read_pool_size` | optional usize | unset → `~1.5 × CPUs` | Max concurrent blocking storage reads (co-located backend only). `0` or unset derives `~1.5 ×` available CPUs at runtime. |
| `feeder_gateway_config.contract_addresses.starknet` | contract address | `0x0` | The Starknet core contract address served by `get_contract_addresses` (`"Starknet"` key). |
| `feeder_gateway_config.contract_addresses.gps_statement_verifier` | contract address | `0x0` | The GPS statement verifier address served by `get_contract_addresses` (`"GpsStatementVerifier"` key). |

## Topology (`read_backend`)

The feeder gateway re-serves chain data that already lives in the sequencer's sync storage. How it
reads that data depends on where it is deployed:

- **`Colocated` (default, highest throughput):** the feeder gateway runs in the same process as
  state-sync and reads MDBX directly via a co-located `StorageReader`. Reads are dispatched through a
  bounded read executor (`read_pool_size`) so they run in parallel on a blocking thread pool without
  blocking the async reactor and without unbounded thread growth. Requires state-sync to run locally
  in the same process.
- **`Remote`:** the feeder gateway runs in its own pod and reads over the network from the
  state-sync process via the state-sync client. The feeder gateway is stateless in this mode and
  holds no local storage; `read_pool_size` is unused. Horizontally scalable behind an HPA.

## Notes

- Responses are serialized byte-identically to the legacy Python feeder gateway (spaced separators,
  `ensure_ascii`, Python key order). See Reference B of the implementation plan.
- A top-of-stack response cache is planned but optional and off by default; the system is designed to
  meet its throughput target with caching disabled.
