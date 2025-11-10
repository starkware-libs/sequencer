use alloy::network::TransactionBuilder;
use alloy::primitives::{address as ethereum_address, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use starknet_api::hash::StarkHash;
use tracing::debug;
use url::Url;

use crate::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
    EthereumContractAddress,
};

// This address is commonly used as the L1 address of the Starknet core contract.
// TODO(Arni): Replace with constant with use of `AnvilInstance::address(&self)`.
pub const DEFAULT_ANVIL_L1_DEPLOYED_ADDRESS: &str = "0x5fbdb2315678afecb367f032d93f642f64180aa3";

// Default funded accounts.
// This address is the sender address of messages sent to L2 by Anvil.
// Given an `AnvilInstance`, this address can be retrieved by calling `anvil.addresses()[0]`.
pub const DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS: StarkHash =
    StarkHash::from_hex_unchecked("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
/// One of the 10 pre-funded Anvil preloaded accounts. Retrieved by calling `anvil.addresses()[3]`.
// TODO(Gilad): consider moving into anvil base layer.
pub const ARBITRARY_ANVIL_L1_ACCOUNT_ADDRESS: EthereumContractAddress =
    ethereum_address!("0x90F79bf6EB2c4f870365E785982E1f101E93b906");
/// One of the 10 pre-funded Anvil preloaded accounts. Retrieved by calling `anvil.addresses()[4]`.
// TODO(Gilad): consider moving into anvil base layer.
pub const OTHER_ARBITRARY_ANVIL_L1_ACCOUNT_ADDRESS: EthereumContractAddress =
    ethereum_address!("0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65");

// FIXME: This should be part of AnvilBaseLayer, however the usage in the simulator doesn't allow
// that, since it is coupled with a manual invocation of an anvil instance that is managed inside
// the github workflow.
pub async fn make_block_history_on_anvil(
    sender_address: EthereumContractAddress,
    receiver_address: EthereumContractAddress,
    base_layer_config: EthereumBaseLayerConfig,
    url: &Url,
    num_blocks: usize,
) {
    let base_layer = EthereumBaseLayerContract::new(base_layer_config.clone(), url.clone());
    let provider = base_layer.contract.provider();
    let mut prev_block_number =
        usize::try_from(provider.get_block_number().await.unwrap()).unwrap();
    for _ in 0..num_blocks {
        let tx = TransactionRequest::default()
            .with_from(sender_address)
            .with_to(receiver_address)
            .with_value(U256::from(100));
        let pending =
            provider.send_transaction(tx).await.expect("Could not post transaction to base layer");
        let receipt: alloy::rpc::types::TransactionReceipt = pending
            .get_receipt()
            .await
            .expect("Could not get receipt for transaction to base layer");
        debug!(
            "Added L1 transaction to L1 block: {} with gas price: {}, blob price: {}",
            receipt.block_number.unwrap(),
            receipt.effective_gas_price,
            receipt.blob_gas_price.unwrap()
        );
        // Make sure the transactions trigger creation of new blocks.
        let new_block_number = usize::try_from(receipt.block_number.unwrap()).unwrap();
        assert!(new_block_number > prev_block_number);
        prev_block_number = new_block_number;
    }
}

/// Mine multiple blocks instantly on Anvil using the `anvil_mine` RPC method.
///
/// Note: This creates empty blocks. For blocks with transactions, use the
/// `make_block_history_on_anvil` function instead.
pub async fn anvil_mine_blocks(
    base_layer_config: EthereumBaseLayerConfig,
    num_blocks: u64,
    url: &Url,
) {
    let base_layer = EthereumBaseLayerContract::new(base_layer_config.clone(), url.clone());
    let provider = base_layer.contract.provider();

    let block_before = provider.get_block_number().await.expect("Failed to get block number");
    debug!("Block number before mining: {}", block_before);

    let _result: Option<String> = provider
        .raw_request("anvil_mine".into(), [num_blocks])
        .await
        .expect("Failed to mine blocks on Anvil");

    let block_after = provider.get_block_number().await.expect("Failed to get block number");
    debug!("Block number after mining: {}", block_after);
}
