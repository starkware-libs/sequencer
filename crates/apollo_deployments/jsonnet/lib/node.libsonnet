// Shared helpers for building per-node sequencer config overrides.
// Produces nested JSON objects that deep-merge into layout service configs via +:
// when devops environments overlay chain- and node-specific values.

// Port assignments for the hybrid layout. Consistent across environments.
local PORTS = {
  batcher: 55000,
  class_manager: 55001,
  gateway: 55002,
  l1_gas_price_provider: 55003,
  l1_events_provider: 55004,
  mempool: 55006,
  sierra_compiler: 55007,
  signature_manager: 55008,
  state_sync: 55009,
  state_sync_network: 55010,
  batcher_storage_reader: 55011,
  proof_manager: 55012,
  committer: 55013,
  class_manager_storage_reader: 55210,
  consensus_p2p: 53080,
  mempool_p2p: 53200,
  http_server: 8080,
  monitoring: 8082,
  state_sync_rpc: 8090,
};

// K8s service names for the hybrid layout. Consistent across environments.
local SERVICES = {
  core: 'sequencer-core-service',
  committer: 'sequencer-committer-service',
  gateway: 'sequencer-gateway-service',
  l1: 'sequencer-l1-service',
  mempool: 'sequencer-mempool-service',
  sierra_compiler: 'sequencer-sierracompiler-service',
};

// Generates the bootstrap peer multiaddr string for a p2p network.
// nodes: array of { name: string, peer_id: string }
// Nodes with an empty peer_id are skipped.
local bootstrapPeers(service, port, nodes, dns_domain) =
  std.join(',', [
    '/dns/' + service + '.' + node.name + '.' + dns_domain +
    '/tcp/' + std.toString(port) + '/p2p/' + node.peer_id
    for node in nodes
    if node.peer_id != ''
  ]);

// A node advertises itself on public DNS. Internal-cluster nodes (svc.cluster.local)
// and nodes without a p2p identity (peer_id='') do not advertise.
local shouldAdvertise(node, dns_domain) = node.peer_id != '' && dns_domain != 'svc.cluster.local';

// Returns chain[key] if it exists, else empty object.
local extra(chain, key) = if std.objectHas(chain, key) then chain[key] else {};

// Builds the consensus p2p config for the core service.
local consensusP2pConfig(node, nodes, dns_domain) =
  local advertise = shouldAdvertise(node, dns_domain);
  {
    consensus_manager_config+: {
      network_config+: {
        advertised_multiaddr:
          if advertise
          then '/dns/' + SERVICES.core + '.' + node.name + '.' + dns_domain +
               '/tcp/' + std.toString(PORTS.consensus_p2p) + '/p2p/' + node.peer_id
          else null,
        bootstrap_peer_multiaddr:
          bootstrapPeers(SERVICES.core, PORTS.consensus_p2p, nodes, dns_domain),
        port: PORTS.consensus_p2p,
      },
    },
  };

// Builds the mempool p2p config for the mempool service.
// When enabled=false, bootstrap_peer_multiaddr is set to null (None).
local mempoolP2pConfig(node, nodes, dns_domain, enabled) =
  if !enabled
  then {
    mempool_p2p_config+: {
      network_config+: { bootstrap_peer_multiaddr: null },
    },
  }
  else
    local advertise = shouldAdvertise(node, dns_domain);
    {
      mempool_p2p_config+: {
        network_config+: {
          advertised_multiaddr:
            if advertise
            then '/dns/' + SERVICES.mempool + '.' + node.name + '.' + dns_domain +
                 '/tcp/' + std.toString(PORTS.mempool_p2p) + '/p2p/' + node.peer_id
            else null,
          bootstrap_peer_multiaddr:
            bootstrapPeers(SERVICES.mempool, PORTS.mempool_p2p, nodes, dns_domain),
          port: PORTS.mempool_p2p,
        },
      },
    };

// state_sync network config — null (None) when disabled, or a full config object when enabled.
// chain_id and port are included here since they are only meaningful when the network is enabled.
local stateSyncNetworkConfig(chain) = {
  state_sync_config+: {
    static_config+: {
      network_config:
        if chain.state_sync_network_enabled
        then { chain_id: chain.chain_id, port: PORTS.state_sync_network }
        else null,
    },
  },
};

// Produces all six hybrid service configs for one node.
// chain: import from lib/chains/*.libsonnet; may contain optional per-service config dicts:
//   chain.committer_config / core_config / gateway_config / l1_config /
//   chain.mempool_config / chain.sierra_compiler_config  — per-service nested config
// node: { name: string, validator_id: string, peer_id: string }
// nodes: full node list for bootstrap peer generation
// consensus_peers: override peer list for consensus p2p (defaults to nodes)
// mempool_peers: override peer list for mempool p2p (defaults to nodes)
local makeNodeConfig(chain, node, nodes, consensus_peers=null, mempool_peers=null) =
  local cp = if consensus_peers == null then nodes else consensus_peers;
  local mp = if mempool_peers == null then nodes else mempool_peers;
  {
    committer:
      {
        components+: {
          batcher+: { port: PORTS.batcher, url: SERVICES.core },
          committer+: { port: PORTS.committer, url: SERVICES.committer },
        },
        monitoring_endpoint_config+: { port: PORTS.monitoring },
      }
      + extra(chain, 'committer_config'),

    core:
      {
        batcher_config+: {
          dynamic_config+: { native_classes_whitelist: '[]' },
          static_config+: {
            block_builder_config+: {
              chain_info+: {
                chain_id: chain.chain_id,
                fee_token_addresses+: {
                  eth_fee_token_address: chain.eth_fee_token_address,
                  strk_fee_token_address: chain.strk_fee_token_address,
                },
              },
              versioned_constants_overrides: null,
            },
            contract_class_manager_config+: {
              native_compiler_config+: { max_cpu_time: 600 },
            },
            pre_confirmed_cende_config+: { recorder_url: chain.recorder_url },
            storage+: { db_config+: { chain_id: chain.chain_id } },
            storage_reader_server_static_config+: { port: PORTS.batcher_storage_reader },
            validation_only: false,
          },
        },
        class_manager_config+: {
          static_config+: {
            class_storage_config+: {
              class_hash_storage_config+: {
                db_config+: { chain_id: chain.chain_id },
              },
              storage_reader_server_static_config+: { port: PORTS.class_manager_storage_reader },
            },
          },
        },
        consensus_manager_config+: {
          cende_config+: { recorder_url: chain.recorder_url },
          consensus_manager_config+: {
            dynamic_config+: { validator_id: node.validator_id },
            static_config+: {
              storage_config+: { db_config+: { chain_id: chain.chain_id } },
            },
          },
          context_config+: {
            static_config+: {
              behavior_mode: 'starknet',
              chain_id: chain.chain_id,
            },
          },
          network_config+: { chain_id: chain.chain_id },
          revert_config: {
            revert_up_to_and_including: 18446744073709551615,
            should_revert: false,
          },
        },
        components+: {
          batcher+: { port: PORTS.batcher, url: SERVICES.core },
          class_manager+: { port: PORTS.class_manager, url: SERVICES.core },
          committer+: { port: PORTS.committer, url: SERVICES.committer },
          l1_events_provider+: { port: PORTS.l1_events_provider, url: SERVICES.l1 },
          l1_gas_price_provider+: { port: PORTS.l1_gas_price_provider, url: SERVICES.l1 },
          mempool+: { port: PORTS.mempool, url: SERVICES.mempool },
          proof_manager+: { port: PORTS.proof_manager, url: SERVICES.core },
          sierra_compiler+: { port: PORTS.sierra_compiler, url: SERVICES.sierra_compiler },
          signature_manager+: { port: PORTS.signature_manager, url: SERVICES.core },
          state_sync+: { port: PORTS.state_sync, url: SERVICES.core },
        },
        monitoring_endpoint_config+: { port: PORTS.monitoring },
        state_sync_config+: {
          static_config+: {
            central_sync_client_config+: {
              central_source_config+: { starknet_url: chain.starknet_url },
            },
            revert_config: {
              revert_up_to_and_including: 18446744073709551615,
              should_revert: false,
            },
            rpc_config+: {
              chain_id: chain.chain_id,
              eth_fee_contract_address: chain.eth_fee_token_address,
              port: PORTS.state_sync_rpc,
              starknet_url: chain.starknet_url,
              strk_fee_contract_address: chain.strk_fee_token_address,
            },
            storage_config+: { db_config+: { chain_id: chain.chain_id } },
            storage_reader_server_static_config+: { port: PORTS.proof_manager },
          },
        },
      }
      + consensusP2pConfig(node, cp, chain.dns_domain)
      + stateSyncNetworkConfig(chain)
      + extra(chain, 'core_config'),

    gateway:
      {
        components+: {
          class_manager+: { port: PORTS.class_manager, url: SERVICES.core },
          gateway+: { port: PORTS.gateway, url: SERVICES.gateway },
          mempool+: { port: PORTS.mempool, url: SERVICES.mempool },
          proof_manager+: { port: PORTS.proof_manager, url: SERVICES.core },
          state_sync+: { port: PORTS.state_sync, url: SERVICES.core },
        },
        gateway_config+: {
          dynamic_config+: { native_classes_whitelist: '[]' },
          static_config+: {
            chain_info+: {
              chain_id: chain.chain_id,
              fee_token_addresses+: {
                eth_fee_token_address: chain.eth_fee_token_address,
                strk_fee_token_address: chain.strk_fee_token_address,
              },
            },
            contract_class_manager_config+: {
              native_compiler_config+: { max_cpu_time: 600 },
            },
            proof_archive_writer_config+: { bucket_name: chain.proof_archive_bucket },
            stateful_tx_validator_config+: {
              validate_resource_bounds: true,
              versioned_constants_overrides: null,
            },
            stateless_tx_validator_config+: { validate_resource_bounds: true },
          },
        },
        http_server_config+: { static_config+: { port: PORTS.http_server } },
        monitoring_endpoint_config+: { port: PORTS.monitoring },
      }
      + extra(chain, 'gateway_config'),

    l1:
      {
        components+: {
          batcher+: { port: PORTS.batcher, url: SERVICES.core },
          l1_events_provider+: { port: PORTS.l1_events_provider, url: SERVICES.l1 },
          l1_gas_price_provider+: { port: PORTS.l1_gas_price_provider, url: SERVICES.l1 },
          state_sync+: { port: PORTS.state_sync, url: SERVICES.core },
        },
        l1_events_scraper_config+: { chain_id: chain.chain_id },
        l1_gas_price_scraper_config+: { chain_id: chain.chain_id },
        monitoring_endpoint_config+: { port: PORTS.monitoring },
      }
      + extra(chain, 'l1_config'),

    mempool:
      {
        components+: {
          class_manager+: { port: PORTS.class_manager, url: SERVICES.core },
          gateway+: { port: PORTS.gateway, url: SERVICES.gateway },
          mempool+: { port: PORTS.mempool, url: SERVICES.mempool },
          proof_manager+: { port: PORTS.proof_manager, url: SERVICES.core },
        },
        mempool_config+: {
          static_config+: {
            behavior_mode: 'starknet',
            recorder_url: chain.recorder_url,
            validate_resource_bounds: true,
          },
        },
        mempool_p2p_config+: {
          network_config+: { chain_id: chain.chain_id },
        },
        monitoring_endpoint_config+: { port: PORTS.monitoring },
      }
      + mempoolP2pConfig(node, mp, chain.dns_domain, chain.mempool_p2p_enabled)
      + extra(chain, 'mempool_config'),

    sierra_compiler:
      {
        components+: {
          sierra_compiler+: { port: PORTS.sierra_compiler, url: SERVICES.sierra_compiler },
        },
        monitoring_endpoint_config+: { port: PORTS.monitoring },
        sierra_compiler_config+: { max_cpu_time: 600 },
      }
      + extra(chain, 'sierra_compiler_config'),
  };

{
  makeNodeConfig: makeNodeConfig,
  PORTS: PORTS,
  SERVICES: SERVICES,
}
