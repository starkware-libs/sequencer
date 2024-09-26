use starknet_api::core::calculate_contract_address;
use starknet_api::executable_transaction::{DeployAccountTransaction, Transaction};
use starknet_api::test_utils::deploy_account::DeployAccountTxArgs;
use starknet_api::test_utils::NonceManager;

use crate::transaction::account_transaction::AccountTransaction;

pub fn deploy_account_tx(
    deploy_tx_args: DeployAccountTxArgs,
    nonce_manager: &mut NonceManager,
) -> AccountTransaction {
    let tx_hash = deploy_tx_args.tx_hash;
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

    AccountTransaction::new(Transaction::DeployAccount(DeployAccountTransaction {
        tx: deploy_account_tx,
        tx_hash,
        contract_address,
    }))
}
