use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::fields::{Fee, TransactionSignature};
use starknet_api::transaction::TransactionVersion;

use crate::transaction::objects::{
    CommonAccountFields,
    DeprecatedTransactionInfo,
    HasRelatedFeeType,
    TransactionInfo,
    TransactionInfoCreator,
};

impl HasRelatedFeeType for L1HandlerTransaction {
    fn version(&self) -> TransactionVersion {
        self.tx.version
    }

    fn is_l1_handler(&self) -> bool {
        true
    }
}

impl TransactionInfoCreator for L1HandlerTransaction {
    fn create_tx_info(&self) -> TransactionInfo {
        TransactionInfo::Deprecated(DeprecatedTransactionInfo {
            common_fields: CommonAccountFields {
                transaction_hash: self.tx_hash,
                version: self.tx.version,
                signature: TransactionSignature::default(),
                nonce: self.tx.nonce,
                sender_address: self.tx.contract_address,
                only_query: false,
            },
            max_fee: Fee::default(),
        })
    }
}
