pub fn create_chain_info() -> ChainInfo {
     chain_info
 }
 
pub async fn create_config(
    chain_info: ChainInfo,
    rpc_server_addr: SocketAddr,
    batcher_storage_config: StorageConfig,
) -> (SequencerNodeConfig, RequiredParams, BroadcastTopicChannels<ProposalPart>) {
    let fee_token_addresses = chain_info.fee_token_addresses.clone();
    let batcher_config = create_batcher_config(batcher_storage_config, chain_info.clone());
    let gateway_config = create_gateway_config(chain_info.clone()).await;
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
            chain_id: chain_info.chain_id,
            eth_fee_token_address: fee_token_addresses.eth_fee_token_address,
            strk_fee_token_address: fee_token_addresses.strk_fee_token_address,
            sequencer_address: ContractAddress::from(1312_u128), // Arbitrary non-zero value.
        },
        consensus_proposals_channels,
    )
}

fn create_consensus_manager_config_and_channels()
-> (ConsensusManagerConfig, BroadcastTopicChannels<ProposalPart>) {
    let rpc_txs = create_account_txs(tx_generator, n_txs, account_id);
    send_rpc_txs(rpc_txs, send_rpc_tx_fn).await
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
