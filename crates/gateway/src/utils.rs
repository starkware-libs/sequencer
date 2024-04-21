use starknet_api::external_transaction::ExternalTransaction;
use starknet_api::transaction::{ResourceBoundsMapping, TransactionSignature};

macro_rules! implement_ref_getters {
    ($(($member_name:ident, $member_type:ty));* $(;)?) => {
        $(fn $member_name(&self) -> &$member_type {
            match self {
                starknet_api::external_transaction::ExternalTransaction::Declare(
                    starknet_api::external_transaction::ExternalDeclareTransaction::V3(tx)
                ) => &tx.$member_name,
                starknet_api::external_transaction::ExternalTransaction::DeployAccount(
                    starknet_api::external_transaction::ExternalDeployAccountTransaction::V3(tx)
                ) => &tx.$member_name,
                starknet_api::external_transaction::ExternalTransaction::Invoke(
                    starknet_api::external_transaction::ExternalInvokeTransaction::V3(tx)
                ) => &tx.$member_name,
            }
        })*
    };
}

impl ExternalTransactionExt for ExternalTransaction {
    implement_ref_getters!(
        (resource_bounds, ResourceBoundsMapping);
        (signature, TransactionSignature)
    );
}

// TODO(Arni, 1/5/2025): Remove this trait once it is implemented in StarkNet API.
pub trait ExternalTransactionExt {
    fn resource_bounds(&self) -> &ResourceBoundsMapping;
    fn signature(&self) -> &TransactionSignature;
}
