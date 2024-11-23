use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

use axum::body::Body;
use blockifier::context::ChainInfo;
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::CairoVersion;
use mempool_test_utils::starknet_api_test_utils::{
    rpc_tx_to_json,
    AccountId,
    MultiAccountTransactionGenerator,
};
use papyrus_consensus::config::ConsensusConfig;
use papyrus_network::network_manager::test_utils::create_network_config_connected_to_broadcast_channels;
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::ProposalPart;
use papyrus_storage::StorageConfig;
use reqwest::{Client, Response};
use starknet_api::block::BlockNumber;
use starknet_api::contract_address;
use starknet_api::core::ContractAddress;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_batcher::block_builder::BlockBuilderConfig;
use starknet_batcher::config::BatcherConfig;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_gateway::config::{
    GatewayConfig,
    RpcStateReaderConfig,
    StatefulTransactionValidatorConfig,
    StatelessTransactionValidatorConfig,
};
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_http_server::config::HttpServerConfig;
use starknet_sequencer_infra::test_utils::get_available_socket;
use tokio::net::TcpListener;

use crate::config::node_config::SequencerNodeConfig;
use crate::config::test_utils::RequiredParams;

pub async fn create_config(
    rpc_server_addr: SocketAddr,
    batcher_storage_config: StorageConfig,
) -> (SequencerNodeConfig, RequiredParams, BroadcastTopicChannels<ProposalPart>) {
    let chain_id = batcher_storage_config.db_config.chain_id.clone();
    // TODO(Tsabary): create chain_info in setup, and pass relevant values throughout.
    let mut chain_info = ChainInfo::create_for_testing();
    chain_info.chain_id = chain_id.clone();
    let fee_token_addresses = chain_info.fee_token_addresses.clone();
    let batcher_config = create_batcher_config(batcher_storage_config, chain_info.clone());
    let gateway_config = create_gateway_config(chain_info).await;
    let http_server_config = create_http_server_config().await;
    let rpc_state_reader_config = test_rpc_state_reader_config(rpc_server_addr);
    let (consensus_manager_config, consensus_proposals_channels) =
        create_consensus_manager_config_and_channels();
    (
        SequencerNodeConfig {
            batcher_config,
            consensus_manager_config,
            gateway_config,
            http_server_config,
            rpc_state_reader_config,
            ..SequencerNodeConfig::default()
        },
        RequiredParams {
            chain_id,
            eth_fee_token_address: fee_token_addresses.eth_fee_token_address,
            strk_fee_token_address: fee_token_addresses.strk_fee_token_address,
            sequencer_address: ContractAddress::from(1312_u128), // Arbitrary non-zero value.
        },
        consensus_proposals_channels,
    )
}

fn create_consensus_manager_config_and_channels()
-> (ConsensusManagerConfig, BroadcastTopicChannels<ProposalPart>) {
    let (network_config, broadcast_channels) =
        create_network_config_connected_to_broadcast_channels(
            papyrus_network::gossipsub_impl::Topic::new(
                starknet_consensus_manager::consensus_manager::NETWORK_TOPIC,
            ),
        );
    let consensus_manager_config = ConsensusManagerConfig {
        consensus_config: ConsensusConfig {
            start_height: BlockNumber(1),
            consensus_delay: Duration::from_secs(1),
            network_config,
            ..Default::default()
        },
    };
    (consensus_manager_config, broadcast_channels)
}

pub fn test_rpc_state_reader_config(rpc_server_addr: SocketAddr) -> RpcStateReaderConfig {
    // TODO(Tsabary): get the latest version from the RPC crate.
    const RPC_SPEC_VERSION: &str = "V0_8";
    const JSON_RPC_VERSION: &str = "2.0";
    RpcStateReaderConfig {
        url: format!("http://{rpc_server_addr:?}/rpc/{RPC_SPEC_VERSION}"),
        json_rpc_version: JSON_RPC_VERSION.to_string(),
    }
}

pub async fn create_gateway_config(chain_info: ChainInfo) -> GatewayConfig {
    let stateless_tx_validator_config = StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: true,
        max_calldata_length: 10,
        max_signature_length: 2,
        ..Default::default()
    };
    let stateful_tx_validator_config = StatefulTransactionValidatorConfig::default();

    GatewayConfig { stateless_tx_validator_config, stateful_tx_validator_config, chain_info }
}

pub async fn create_http_server_config() -> HttpServerConfig {
    // TODO(Tsabary): use ser_generated_param.
    let socket = get_available_socket().await;
    HttpServerConfig { ip: socket.ip(), port: socket.port() }
}

pub fn create_batcher_config(
    batcher_storage_config: StorageConfig,
    chain_info: ChainInfo,
) -> BatcherConfig {
    // TODO(Arni): Create BlockBuilderConfig create for testing method and use here.
    const SEQUENCER_ADDRESS_FOR_TESTING: u128 = 1991;

    BatcherConfig {
        storage: batcher_storage_config,
        block_builder_config: BlockBuilderConfig {
            chain_info,
            sequencer_address: contract_address!(SEQUENCER_ADDRESS_FOR_TESTING),
            ..Default::default()
        },
        ..Default::default()
    }
}
