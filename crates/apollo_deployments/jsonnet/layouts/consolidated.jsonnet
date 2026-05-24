// Single-service consolidated deployment layout.
// Merges all component default values with topology rules.
// Component defaults come from components.libsonnet; topology is hand-written.

local c = import "../lib/components.libsonnet";
local t = import "../lib/topologies.libsonnet";

{
  node:
    c.general
    + c.base_layer
    + c.batcher
    + c.class_manager
    + c.committer
    + c.config_manager
    + c.consensus_manager
    + c.gateway
    + c.http_server
    + c.l1_events_provider
    + c.l1_events_scraper
    + c.l1_gas_price_provider
    + c.l1_gas_price_scraper
    + c.mempool
    + c.mempool_p2p
    + c.monitoring_endpoint
    + c.proof_manager
    + c.sierra_compiler
    + c.state_sync
    + t.consolidated.node,
}
