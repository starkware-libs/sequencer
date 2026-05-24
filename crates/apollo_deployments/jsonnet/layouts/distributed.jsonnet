// Twelve-service distributed deployment layout.
// Merges per-component default values with topology rules for each service.
// Component defaults come from components.libsonnet; topology is hand-written.

local c = import "../lib/components.libsonnet";
local t = import "../lib/topologies.libsonnet";

{
  batcher:
    c.general
    + c.batcher
    + c.config_manager
    + c.monitoring_endpoint
    + t.distributed.batcher,

  class_manager:
    c.general
    + c.class_manager
    + c.config_manager
    + c.monitoring_endpoint
    + t.distributed.class_manager,

  committer:
    c.general
    + c.committer
    + c.config_manager
    + c.monitoring_endpoint
    + t.distributed.committer,

  consensus_manager:
    c.general
    + c.config_manager
    + c.consensus_manager
    + c.monitoring_endpoint
    + t.distributed.consensus_manager,

  gateway:
    c.general
    + c.config_manager
    + c.gateway
    + c.monitoring_endpoint
    + t.distributed.gateway,

  http_server:
    c.general
    + c.config_manager
    + c.http_server
    + c.monitoring_endpoint
    + t.distributed.http_server,

  l1:
    c.general
    + c.base_layer
    + c.config_manager
    + c.l1_events_provider
    + c.l1_events_scraper
    + c.l1_gas_price_provider
    + c.l1_gas_price_scraper
    + c.monitoring_endpoint
    + t.distributed.l1,

  mempool:
    c.general
    + c.config_manager
    + c.mempool
    + c.mempool_p2p
    + c.monitoring_endpoint
    + t.distributed.mempool,

  proof_manager:
    c.general
    + c.config_manager
    + c.monitoring_endpoint
    + c.proof_manager
    + t.distributed.proof_manager,

  sierra_compiler:
    c.general
    + c.config_manager
    + c.monitoring_endpoint
    + c.sierra_compiler
    + t.distributed.sierra_compiler,

  // signature_manager has no dedicated config section in SequencerNodeConfig.
  signature_manager:
    c.general
    + c.config_manager
    + c.monitoring_endpoint
    + t.distributed.signature_manager,

  state_sync:
    c.general
    + c.config_manager
    + c.monitoring_endpoint
    + c.state_sync
    + t.distributed.state_sync,
}
