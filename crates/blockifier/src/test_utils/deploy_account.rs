use starknet_api::executable_transaction::AccountTransaction;
use starknet_api::test_utils::deploy_account::{executable_deploy_account_tx, DeployAccountTxArgs};
use starknet_api::test_utils::NonceManager;

// TODO(AvivG): remove this func & file.
pub fn deploy_account_tx(
    deploy_tx_args: DeployAccountTxArgs,
    nonce_manager: &mut NonceManager,
) -> AccountTransaction {
    // TODO(AvivG): see into making 'executable_deploy_account_tx' ret type AccountTransaction.
    let deploy_account_tx = executable_deploy_account_tx(deploy_tx_args, nonce_manager);

    AccountTransaction::DeployAccount(deploy_account_tx)
}
