use std::collections::BTreeMap;

use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::transaction_execution::Transaction;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use starknet_api::block::GasPrice;
use starknet_api::contract_class::{ClassInfo, ContractClass, SierraVersion};
use starknet_api::executable_transaction::{
    AccountTransaction as ExecutableTransaction,
    TransactionType,
};
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::{
    DeprecatedResourceBoundsMapping,
    Resource,
    ResourceBounds,
    ValidResourceBounds,
};
use starknet_api::StarknetApiError;

use crate::errors::{NativeBlockifierInputError, NativeBlockifierResult};
use crate::py_declare::py_declare;
use crate::py_deploy_account::py_deploy_account;
use crate::py_invoke_function::py_invoke_function;
use crate::py_l1_handler::py_l1_handler;

pub(crate) const PY_TX_PARSING_ERR: &str = "Failed parsing Py transaction.";

// Structs.

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub enum PyResource {
    L1Gas,
    L1DataGas,
    L2Gas,
}

impl From<PyResource> for Resource {
    fn from(py_resource: PyResource) -> Self {
        match py_resource {
            PyResource::L1Gas => Resource::L1Gas,
            PyResource::L1DataGas => Resource::L1DataGas,
            PyResource::L2Gas => Resource::L2Gas,
        }
    }
}

impl FromPyObject<'_> for PyResource {
    fn extract(resource: &PyAny) -> PyResult<Self> {
        let resource_name: &str = resource.getattr("name")?.extract()?;
        match resource_name {
            "L1_GAS" => Ok(PyResource::L1Gas),
            "L1_DATA_GAS" => Ok(PyResource::L1DataGas),
            "L2_GAS" => Ok(PyResource::L2Gas),
            _ => Err(PyValueError::new_err(format!("Invalid resource: {resource_name}"))),
        }
    }
}

#[derive(Clone, Copy, Default, FromPyObject)]
pub struct PyResourceBounds {
    pub max_amount: u64,
    pub max_price_per_unit: u128,
}

impl From<PyResourceBounds> for ResourceBounds {
    fn from(py_resource_bounds: PyResourceBounds) -> Self {
        Self {
            max_amount: GasAmount(py_resource_bounds.max_amount),
            max_price_per_unit: GasPrice(py_resource_bounds.max_price_per_unit),
        }
    }
}

#[derive(Clone, FromPyObject)]
pub struct PyResourceBoundsMapping(pub BTreeMap<PyResource, PyResourceBounds>);

impl TryFrom<PyResourceBoundsMapping> for ValidResourceBounds {
    type Error = StarknetApiError;
    fn try_from(py_resource_bounds_mapping: PyResourceBoundsMapping) -> Result<Self, Self::Error> {
        let map = py_resource_bounds_mapping
            .0
            .into_iter()
            .map(|(py_resource_type, py_resource_bounds)| {
                (Resource::from(py_resource_type), ResourceBounds::from(py_resource_bounds))
            })
            .collect();
        DeprecatedResourceBoundsMapping(map).try_into()
    }
}

#[derive(Clone)]
pub enum PyDataAvailabilityMode {
    L1 = 0,
    L2 = 1,
}

impl FromPyObject<'_> for PyDataAvailabilityMode {
    fn extract(data_availability_mode: &PyAny) -> PyResult<Self> {
        let data_availability_mode: u8 = data_availability_mode.extract()?;
        match data_availability_mode {
            0 => Ok(PyDataAvailabilityMode::L1),
            1 => Ok(PyDataAvailabilityMode::L2),
            _ => Err(PyValueError::new_err(format!(
                "Invalid data availability mode: {data_availability_mode}"
            ))),
        }
    }
}

impl From<PyDataAvailabilityMode> for starknet_api::data_availability::DataAvailabilityMode {
    fn from(py_data_availability_mode: PyDataAvailabilityMode) -> Self {
        match py_data_availability_mode {
            PyDataAvailabilityMode::L1 => starknet_api::data_availability::DataAvailabilityMode::L1,
            PyDataAvailabilityMode::L2 => starknet_api::data_availability::DataAvailabilityMode::L2,
        }
    }
}

// Transaction creation.

#[allow(clippy::result_large_err)]
pub fn py_account_tx(
    tx: &PyAny,
    optional_py_class_info: Option<PyClassInfo>,
) -> NativeBlockifierResult<AccountTransaction> {
    let Transaction::Account(account_tx) = py_tx(tx, optional_py_class_info)? else {
        panic!("Not an account transaction.");
    };

    Ok(account_tx)
}

#[allow(clippy::result_large_err)]
pub fn py_tx(
    tx: &PyAny,
    optional_py_class_info: Option<PyClassInfo>,
) -> NativeBlockifierResult<Transaction> {
    let tx_type = get_py_tx_type(tx)?;
    let tx_type: TransactionType =
        tx_type.parse().map_err(NativeBlockifierInputError::StarknetApiError)?;

    Ok(match tx_type {
        TransactionType::Declare => {
            let non_optional_py_class_info: PyClassInfo = optional_py_class_info
                .expect("A class info must be passed in a Declare transaction.");
            let tx = ExecutableTransaction::Declare(py_declare(tx, non_optional_py_class_info)?);
            AccountTransaction::new_for_sequencing(tx).into()
        }
        TransactionType::DeployAccount => {
            let tx = ExecutableTransaction::DeployAccount(py_deploy_account(tx)?);
            AccountTransaction::new_for_sequencing(tx).into()
        }
        TransactionType::InvokeFunction => {
            let tx = ExecutableTransaction::Invoke(py_invoke_function(tx)?);
            AccountTransaction::new_for_sequencing(tx).into()
        }
        TransactionType::L1Handler => py_l1_handler(tx)?.into(),
    })
}

#[allow(clippy::result_large_err)]
pub fn get_py_tx_type(tx: &PyAny) -> NativeBlockifierResult<&str> {
    Ok(tx.getattr("tx_type")?.getattr("name")?.extract()?)
}

#[derive(FromPyObject)]
pub struct PyClassInfo {
    raw_contract_class: String,
    sierra_program_length: usize,
    abi_length: usize,
    sierra_version: (u64, u64, u64),
}

impl PyClassInfo {
    #[allow(clippy::result_large_err)]
    pub fn try_from(
        py_class_info: PyClassInfo,
        tx: &starknet_api::transaction::DeclareTransaction,
    ) -> NativeBlockifierResult<ClassInfo> {
        let contract_class = match tx {
            starknet_api::transaction::DeclareTransaction::V0(_)
            | starknet_api::transaction::DeclareTransaction::V1(_) => {
                ContractClass::V0(serde_json::from_str(&py_class_info.raw_contract_class)?)
            }
            starknet_api::transaction::DeclareTransaction::V2(_)
            | starknet_api::transaction::DeclareTransaction::V3(_) => ContractClass::V1((
                serde_json::from_str(&py_class_info.raw_contract_class)?,
                SierraVersion::from(py_class_info.sierra_version),
            )),
        };
        let (major, minor, patch) = py_class_info.sierra_version;

        let class_info = ClassInfo::new(
            &contract_class,
            py_class_info.sierra_program_length,
            py_class_info.abi_length,
            SierraVersion::new(major, minor, patch),
        )?;
        Ok(class_info)
    }
}
