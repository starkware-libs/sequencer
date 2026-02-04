use starknet_api::contract_address;
use starknet_api::core::{ChainId, OsChainInfo};

#[test]
fn compute_requested_os_config_hash() {
    let info = OsChainInfo {
        chain_id: ChainId::IntegrationSepolia,
        strk_fee_token_address: contract_address!("0x70a5da4f557b77a9c54546e4bcc900806e28793d8e3eaaa207428d2387249b7"),
    };

    let hash = info.compute_virtual_os_config_hash().unwrap();
    println!("hash_dec={}", hash);
    println!("hash_hex={:#x}", hash);
}
