use starknet_api::core::calculate_contract_address;
use starknet_api::test_utils::deploy_account::DeployAccountTxArgs;
use starknet_api::test_utils::NonceManager;
use starknet_api::transaction::TransactionHash;

use crate::transaction::transactions::DeployAccountTransaction;

pub fn deploy_account_tx(
    deploy_tx_args: DeployAccountTxArgs,
    nonce_manager: &mut NonceManager,
) -> DeployAccountTransaction {
    let default_tx_hash = TransactionHash::default();
    let contract_address = calculate_contract_address(
        deploy_tx_args.contract_address_salt,
        deploy_tx_args.class_hash,
        &deploy_tx_args.constructor_calldata,
        deploy_tx_args.deployer_address,
    )
    .unwrap();

    let deploy_account_tx = starknet_api::test_utils::deploy_account::deploy_account_tx(
        deploy_tx_args,
        nonce_manager.next(contract_address),
    );
    DeployAccountTransaction::new(deploy_account_tx, default_tx_hash, contract_address)
}
