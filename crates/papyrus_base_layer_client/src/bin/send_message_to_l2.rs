use std::convert::TryFrom;
use std::sync::Arc;

// TODO(Arni): Use Alloy instead of ethers.
use ethers::prelude::*;
use papyrus_base_layer_client::send_l1_tx_utils::constants::{
    L2_CONTRACT_ADDRESS,
    SEPOLIA_CHAIN_ID,
    STARKNET_L1_CONTRACT_ADDRESS,
};
use papyrus_base_layer_client::send_l1_tx_utils::secretes::{
    L1_SENDER_ADDRESS,
    PRIVATE_KEY,
    SEPOLIA_RPC_URL,
};
use papyrus_base_layer_client::send_l1_tx_utils::starknet_core_contract::L1Messenger;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::calldata;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::transaction::L1HandlerTransaction;
use starknet_types_core::felt::Felt;

#[tokio::main]
async fn main() {
    // Load private key from env or hardcode it (keep it safe!)
    let private_key = PRIVATE_KEY;

    // Connect to Sepolia via Infura
    let infura_url = SEPOLIA_RPC_URL;
    let provider = Provider::<Http>::try_from(infura_url).expect("Failed to initialize provider");

    // Wallet setup
    let wallet: LocalWallet = private_key.parse().expect("Failed to parse private key");
    let wallet = wallet.with_chain_id(SEPOLIA_CHAIN_ID);

    // Combine wallet and provider
    let client = SignerMiddleware::new(provider, wallet);
    let client = Arc::new(client);

    // Initialize contract instance
    let starknet_l1_contract_address: Address = STARKNET_L1_CONTRACT_ADDRESS
        .parse()
        .expect("Failed to parse Starknet Core contract address");
    let contract = L1Messenger::new(starknet_l1_contract_address, client.clone());

    // ABI Encoding logic for my contract
    let l2_contract_address = ContractAddress(PatriciaKey::from_hex_unchecked(L2_CONTRACT_ADDRESS));
    let l1_sender_address = L1_SENDER_ADDRESS;

    let l1_handler = L1HandlerTransaction {
        contract_address: l2_contract_address,
        // TODO(Arni): Consider saving this value as a lazy constant.
        entry_point_selector: selector_from_name("l1_handler_set_value"),
        calldata: calldata![
            Felt::from_hex_unchecked(l1_sender_address),
            // Arbitrary key and value.
            Felt::from_hex_unchecked("0x876"), // key
            Felt::from_hex_unchecked("0x44")   // value
        ],
        ..Default::default()
    };

    let to_address = l1_handler.contract_address.to_bytes_be().into();
    let entry_point_selector = l1_handler.entry_point_selector.0.to_bytes_be().into();
    let payload =
        l1_handler.calldata.0[1..].iter().map(|x| x.to_bytes_be().into()).collect::<Vec<_>>();

    let fee_on_l1: u64 = 1_u64 << 15; // 0.001 ETH in wei
    let binding = contract
        .send_message_to_l2(to_address, entry_point_selector, payload)
        .value(U256::from(fee_on_l1)); // Sending 0.001 ETH (in wei)

    let tx = binding.send().await.expect("Failed to send transaction");

    println!("Sent tx; tx hash: {:?}", tx.tx_hash());
}
