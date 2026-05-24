// Six-service hybrid deployment layout.
// Merges per-component default values with topology rules for each service.
// Component defaults come from components.libsonnet; topology is hand-written.

local c = import "../lib/components.libsonnet";
local t = import "../lib/topologies.libsonnet";

{
  committer:
    c.general
    + c.committer
    + c.config_manager
    + c.monitoring_endpoint
    + t.hybrid.committer,

  core:
    c.general
    + c.batcher
    + c.class_manager
    + c.config_manager
    + c.consensus_manager
    + c.monitoring_endpoint
    + c.proof_manager
    + c.state_sync
    + t.hybrid.core,

  gateway:
    c.general
    + c.config_manager
    + c.gateway
    + c.http_server
    + c.monitoring_endpoint
    + t.hybrid.gateway,

  l1:
    c.general
    + c.base_layer
    + c.config_manager
    + c.l1_events_provider
    + c.l1_events_scraper
    + c.l1_gas_price_provider
    + c.l1_gas_price_scraper
    + c.monitoring_endpoint
    + t.hybrid.l1,

  mempool:
    c.general
    + c.config_manager
    + c.mempool
    + c.mempool_p2p
    + c.monitoring_endpoint
    + t.hybrid.mempool,

  sierra_compiler:
    c.general
    + c.config_manager
    + c.monitoring_endpoint
    + c.sierra_compiler
    + t.hybrid.sierra_compiler,
}
