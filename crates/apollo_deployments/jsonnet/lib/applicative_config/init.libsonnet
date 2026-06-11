// Per-component applicative config assembler.

local baseLayer = import 'base_layer.libsonnet';
local batcher = import 'batcher.libsonnet';
local classManager = import 'class_manager.libsonnet';
local committer = import 'committer.libsonnet';
local configManager = import 'config_manager.libsonnet';
local consensusManager = import 'consensus_manager.libsonnet';
local gateway = import 'gateway.libsonnet';
local httpServer = import 'http_server.libsonnet';
local l1EventsProvider = import 'l1_events_provider.libsonnet';
local l1EventsScraper = import 'l1_events_scraper.libsonnet';
local l1GasPriceProvider = import 'l1_gas_price_provider.libsonnet';
local l1GasPriceScraper = import 'l1_gas_price_scraper.libsonnet';
local mempool = import 'mempool.libsonnet';
local mempoolP2p = import 'mempool_p2p.libsonnet';
local monitoring = import 'monitoring.libsonnet';
local monitoringEndpoint = import 'monitoring_endpoint.libsonnet';
local proofManager = import 'proof_manager.libsonnet';
local sierraCompiler = import 'sierra_compiler.libsonnet';
local stateSync = import 'state_sync.libsonnet';

function(chain_params, node_params, replacers)
  {
    base_layer_config: baseLayer(chain_params),
    batcher_config: batcher(chain_params, replacers),
    class_manager_config: classManager(chain_params),
    committer_config: committer(replacers),
    config_manager_config: configManager(),
    consensus_manager_config: consensusManager(chain_params, node_params, replacers),
    gateway_config: gateway(chain_params, replacers),
    http_server_config: httpServer(),
    l1_events_provider_config: l1EventsProvider(),
    l1_events_scraper_config: l1EventsScraper(chain_params),
    l1_gas_price_provider_config: l1GasPriceProvider(),
    l1_gas_price_scraper_config: l1GasPriceScraper(chain_params),
    mempool_config: mempool(chain_params, replacers),
    mempool_p2p_config: mempoolP2p(chain_params),
    monitoring_config: monitoring(),
    monitoring_endpoint_config: monitoringEndpoint(),
    proof_manager_config: proofManager(),
    sierra_compiler_config: sierraCompiler(replacers),
    state_sync_config: stateSync(chain_params, replacers),
  }
