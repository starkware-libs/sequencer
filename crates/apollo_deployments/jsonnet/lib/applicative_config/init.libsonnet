// Per-component applicative config assembler.

local baseLayer = import 'components/base_layer.libsonnet';
local batcher = import 'components/batcher.libsonnet';
local classManager = import 'components/class_manager.libsonnet';
local committer = import 'components/committer.libsonnet';
local configManager = import 'components/config_manager.libsonnet';
local consensusManager = import 'components/consensus_manager.libsonnet';
local gateway = import 'components/gateway.libsonnet';
local httpServer = import 'components/http_server.libsonnet';
local l1EventsProvider = import 'components/l1_events_provider.libsonnet';
local l1EventsScraper = import 'components/l1_events_scraper.libsonnet';
local l1GasPriceProvider = import 'components/l1_gas_price_provider.libsonnet';
local l1GasPriceScraper = import 'components/l1_gas_price_scraper.libsonnet';
local mempool = import 'components/mempool.libsonnet';
local mempoolP2p = import 'components/mempool_p2p.libsonnet';
local monitoring = import 'components/monitoring.libsonnet';
local monitoringEndpoint = import 'components/monitoring_endpoint.libsonnet';
local proofManager = import 'components/proof_manager.libsonnet';
local sierraCompiler = import 'components/sierra_compiler.libsonnet';
local stateSync = import 'components/state_sync.libsonnet';

function(chain_params, node_params)
  {
    base_layer_config: baseLayer(chain_params),
    batcher_config: batcher(chain_params),
    class_manager_config: classManager(chain_params),
    committer_config: committer(chain_params),
    config_manager_config: configManager(),
    consensus_manager_config: consensusManager(chain_params, node_params),
    gateway_config: gateway(chain_params),
    http_server_config: httpServer(),
    l1_events_provider_config: l1EventsProvider(),
    l1_events_scraper_config: l1EventsScraper(chain_params),
    l1_gas_price_provider_config: l1GasPriceProvider(),
    l1_gas_price_scraper_config: l1GasPriceScraper(chain_params),
    mempool_config: mempool(chain_params),
    mempool_p2p_config: mempoolP2p(chain_params, node_params),
    monitoring_config: monitoring(),
    monitoring_endpoint_config: monitoringEndpoint(),
    proof_manager_config: proofManager(),
    sierra_compiler_config: sierraCompiler(chain_params),
    state_sync_config: stateSync(chain_params),
  }
