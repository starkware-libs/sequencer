use starknet_api::executable_transaction::AccountTransaction as ExecutableTransaction;
use starknet_api::test_utils::deploy_account::{executable_deploy_account_tx, DeployAccountTxArgs};
use starknet_api::test_utils::NonceManager;

use crate::transaction::account_transaction::AccountTransaction;

pub fn deploy_account_tx(
    deploy_tx_args: DeployAccountTxArgs,
    nonce_manager: &mut NonceManager,
) -> AccountTransaction {
    let deploy_account_tx = executable_deploy_account_tx(deploy_tx_args, nonce_manager);

    AccountTransaction {
        tx: ExecutableTransaction::DeployAccount(deploy_account_tx),
        only_query: false,
    }
}
