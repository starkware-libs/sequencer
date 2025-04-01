use crate::data_availability::DataAvailabilityMode;
use crate::state::SierraContractClass;
use crate::transaction::fields::AllResourceBounds;
use crate::StarknetApiError;

pub trait TransactionStatelessValidation {
    fn validate_contract_address(&self) -> Result<(), StarknetApiError>;
    fn account_deployment_data_is_empty(&self) -> bool;
    fn paymaster_data_is_empty(&self) -> bool;
    fn resource_bounds(&self) -> &AllResourceBounds;
    fn calldata_length(&self) -> Option<usize>;
    fn signature_length(&self) -> usize;
    fn nonce_data_availability_mode(&self) -> &DataAvailabilityMode;
    fn fee_data_availability_mode(&self) -> &DataAvailabilityMode;
    fn is_declare(&self) -> bool;
    fn contract_class(&self) -> Option<&SierraContractClass>;
}
