use super::NonceManager;
use crate::core::{calculate_contract_address, ClassHash, ContractAddress, Nonce};
use crate::data_availability::DataAvailabilityMode;
use crate::executable_transaction::DeployAccountTransaction as ExecutableDeployAccountTransaction;
use crate::transaction::fields::{
    Calldata,
    ContractAddressSalt,
    Fee,
    PaymasterData,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use crate::transaction::{
    DeployAccountTransaction,
    DeployAccountTransactionV1,
    DeployAccountTransactionV3,
    TransactionHash,
    TransactionVersion,
};

#[derive(Clone)]
pub struct DeployAccountTxArgs {
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub deployer_address: ContractAddress,
    pub version: TransactionVersion,
    pub resource_bounds: ValidResourceBounds,
    pub tip: Tip,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
    pub tx_hash: TransactionHash,
}

impl Default for DeployAccountTxArgs {
    fn default() -> Self {
        DeployAccountTxArgs {
            max_fee: Fee::default(),
            signature: TransactionSignature::default(),
            deployer_address: ContractAddress::default(),
            version: TransactionVersion::THREE,
            resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
            tip: Tip::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: PaymasterData::default(),
            nonce: Nonce::default(),
            class_hash: ClassHash::default(),
            contract_address_salt: ContractAddressSalt::default(),
            constructor_calldata: Calldata::default(),
            tx_hash: TransactionHash::default(),
        }
    }
}

/// Utility macro for creating `DeployAccountTxArgs` to reduce boilerplate.
#[macro_export]
macro_rules! deploy_account_tx_args {
    ($($field:ident $(: $value:expr)?),* $(,)?) => {
        $crate::test_utils::deploy_account::DeployAccountTxArgs {
            $($field $(: $value)?,)*
            ..Default::default()
        }
    };
    ($($field:ident $(: $value:expr)?),* , ..$defaults:expr) => {
        $crate::test_utils::deploy_account::DeployAccountTxArgs {
            $($field $(: $value)?,)*
            ..$defaults
        }
    };
}

pub fn deploy_account_tx(
    deploy_tx_args: DeployAccountTxArgs,
    nonce: Nonce,
) -> DeployAccountTransaction {
    // TODO: Make TransactionVersion an enum and use match here.
    if deploy_tx_args.version == TransactionVersion::ONE {
        DeployAccountTransaction::V1(DeployAccountTransactionV1 {
            max_fee: deploy_tx_args.max_fee,
            signature: deploy_tx_args.signature,
            nonce,
            class_hash: deploy_tx_args.class_hash,
            contract_address_salt: deploy_tx_args.contract_address_salt,
            constructor_calldata: deploy_tx_args.constructor_calldata,
        })
    } else if deploy_tx_args.version == TransactionVersion::THREE {
        DeployAccountTransaction::V3(DeployAccountTransactionV3 {
            signature: deploy_tx_args.signature,
            resource_bounds: deploy_tx_args.resource_bounds,
            tip: deploy_tx_args.tip,
            nonce_data_availability_mode: deploy_tx_args.nonce_data_availability_mode,
            fee_data_availability_mode: deploy_tx_args.fee_data_availability_mode,
            paymaster_data: deploy_tx_args.paymaster_data,
            nonce,
            class_hash: deploy_tx_args.class_hash,
            contract_address_salt: deploy_tx_args.contract_address_salt,
            constructor_calldata: deploy_tx_args.constructor_calldata,
        })
    } else {
        panic!("Unsupported transaction version: {:?}.", deploy_tx_args.version)
    }
}

pub fn executable_deploy_account_tx(
    deploy_tx_args: DeployAccountTxArgs,
    nonce_manager: &mut NonceManager,
) -> ExecutableDeployAccountTransaction {
    let tx_hash = deploy_tx_args.tx_hash;
    let contract_address = calculate_contract_address(
        deploy_tx_args.contract_address_salt,
        deploy_tx_args.class_hash,
        &deploy_tx_args.constructor_calldata,
        deploy_tx_args.deployer_address,
    )
    .unwrap();
    let nonce = nonce_manager.next(contract_address);
    let tx = deploy_account_tx(deploy_tx_args, nonce);

    ExecutableDeployAccountTransaction { tx, tx_hash, contract_address }
}
