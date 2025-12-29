use apollo_test_utils::{auto_impl_get_test_instance, GetTestInstance};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::transaction::TransactionHash;

use crate::writer::objects::response::{
    DeclareResponse,
    DeployAccountResponse,
    InvokeResponse,
    SuccessfulStarknetErrorCode,
};

auto_impl_get_test_instance! {
    pub struct InvokeResponse {
        pub code: SuccessfulStarknetErrorCode,
        pub transaction_hash: TransactionHash,
    }
    pub struct DeployAccountResponse {
        pub code: SuccessfulStarknetErrorCode,
        pub transaction_hash: TransactionHash,
        pub address: ContractAddress,
    }
    pub struct DeclareResponse {
        pub code: SuccessfulStarknetErrorCode,
        pub transaction_hash: TransactionHash,
        pub class_hash: ClassHash,
    }
    pub enum SuccessfulStarknetErrorCode {
        TransactionReceived = 0,
    }
}
