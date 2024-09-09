use starknet_api::test_utils::deploy_account::DeployAccountTxArgs;
use starknet_api::test_utils::NonceManager;

use crate::transaction::transactions::DeployAccountTransaction;

pub fn deploy_account_tx(
    deploy_tx_args: DeployAccountTxArgs,
    nonce_manager: &mut NonceManager,
) -> DeployAccountTransaction {
    let deploy_account_tx = starknet_api::test_utils::deploy_account::executable_deploy_account_tx(
        deploy_tx_args,
        nonce_manager,
    );
    DeployAccountTransaction { tx: deploy_account_tx, only_query: false }
}
