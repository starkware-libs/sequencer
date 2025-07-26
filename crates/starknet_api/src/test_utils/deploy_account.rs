use starknet_crypto::Felt;

use super::{NonceManager, TestingTxArgs};
use crate::core::{ClassHash, Nonce};
use crate::data_availability::DataAvailabilityMode;
use crate::executable_transaction::{
    AccountTransaction,
    DeployAccountTransaction as ExecutableDeployAccountTransaction,
};
use crate::rpc_transaction::{
    InternalRpcDeployAccountTransaction,
    InternalRpcTransaction,
    InternalRpcTransactionWithoutTxHash,
    RpcDeployAccountTransaction,
    RpcDeployAccountTransactionV3,
    RpcTransaction,
};
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
    CalculateContractAddress,
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
            version: TransactionVersion::THREE,
            resource_bounds: ValidResourceBounds::create_for_testing(),
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
    // TODO(Arni): Make TransactionVersion an enum and use match here.
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

// TODO(Arni): Consider using [ExecutableDeployAccountTransaction::create] in the body of this
// function. We don't use it now to avoid tx_hash calculation.
pub fn executable_deploy_account_tx(deploy_tx_args: DeployAccountTxArgs) -> AccountTransaction {
    let tx_hash = deploy_tx_args.tx_hash;
    let tx = deploy_account_tx(deploy_tx_args, Nonce(Felt::ZERO));
    let contract_address = tx.calculate_contract_address().unwrap();
    let deploy_account_tx = ExecutableDeployAccountTransaction { tx, tx_hash, contract_address };

    AccountTransaction::DeployAccount(deploy_account_tx)
}

pub fn create_executable_deploy_account_tx_and_update_nonce(
    deploy_tx_args: DeployAccountTxArgs,
    nonce_manager: &mut NonceManager,
) -> AccountTransaction {
    let tx = executable_deploy_account_tx(deploy_tx_args);
    let contract_address = tx.contract_address();
    let nonce = nonce_manager.next(contract_address);
    assert_eq!(
        nonce,
        Nonce(Felt::ZERO),
        "Account already deployed at this address: {contract_address}."
    );
    tx
}

pub fn rpc_deploy_account_tx(deploy_tx_args: DeployAccountTxArgs) -> RpcTransaction {
    if deploy_tx_args.version != TransactionVersion::THREE {
        panic!("Unsupported transaction version: {:?}.", deploy_tx_args.version);
    }

    let ValidResourceBounds::AllResources(resource_bounds) = deploy_tx_args.resource_bounds else {
        panic!("Unsupported resource bounds type: {:?}.", deploy_tx_args.resource_bounds)
    };

    RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(RpcDeployAccountTransactionV3 {
        resource_bounds,
        tip: deploy_tx_args.tip,
        contract_address_salt: deploy_tx_args.contract_address_salt,
        class_hash: deploy_tx_args.class_hash,
        constructor_calldata: deploy_tx_args.constructor_calldata,
        nonce: deploy_tx_args.nonce,
        signature: deploy_tx_args.signature,
        nonce_data_availability_mode: deploy_tx_args.nonce_data_availability_mode,
        fee_data_availability_mode: deploy_tx_args.fee_data_availability_mode,
        paymaster_data: deploy_tx_args.paymaster_data,
    }))
}

pub fn internal_deploy_account_tx(deploy_tx_args: DeployAccountTxArgs) -> InternalRpcTransaction {
    let tx_hash = deploy_tx_args.tx_hash;
    let rpc_tx = rpc_deploy_account_tx(deploy_tx_args);
    let RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(tx)) = rpc_tx else {
        unreachable!();
    };

    let contract_address = tx.calculate_contract_address().unwrap();
    let tx_without_hash =
        InternalRpcTransactionWithoutTxHash::DeployAccount(InternalRpcDeployAccountTransaction {
            tx: RpcDeployAccountTransaction::V3(tx),
            contract_address,
        });
    InternalRpcTransaction { tx: tx_without_hash, tx_hash }
}

impl TestingTxArgs for DeployAccountTxArgs {
    fn get_rpc_tx(&self) -> RpcTransaction {
        rpc_deploy_account_tx(self.clone())
    }

    fn get_internal_tx(&self) -> InternalRpcTransaction {
        internal_deploy_account_tx(self.clone())
    }
}
