use blockifier::execution::contract_class::{
    CompiledClassV0,
    CompiledClassV1,
    RunnableCompiledClass,
};
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader, StateResult};
use pyo3::types::PyTuple;
use pyo3::{FromPyObject, PyAny, PyErr, PyObject, PyResult, Python};
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::errors::{
    NativeBlockifierError,
    NativeBlockifierInputError,
    NativeBlockifierResult,
    UndeclaredClassHashError,
};
use crate::py_utils::PyFelt;

// The value of Python StorageDomain.ON_CHAIN enum.
const ON_CHAIN_STORAGE_DOMAIN: u8 = 0;

pub struct PyStateReader {
    // A reference to an RsStateReaderProxy Python object.
    //
    // This is a reference to memory allocated on Python's heap and can outlive the GIL.
    // Once PyObject is instantiated, the underlying Python object ref count is increased.
    // Once it is dropped, the ref count is decreased the next time the GIL is acquired in pyo3.
    state_reader_proxy: PyObject,
}

impl PyStateReader {
    pub fn new(state_reader_proxy: &PyAny) -> Self {
        Self { state_reader_proxy: PyObject::from(state_reader_proxy) }
    }
}

impl StateReader for PyStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        Python::with_gil(|py| -> PyResult<PyFelt> {
            let args = (ON_CHAIN_STORAGE_DOMAIN, PyFelt::from(contract_address), PyFelt::from(key));
            self.state_reader_proxy.as_ref(py).call_method1("get_storage_at", args)?.extract()
        })
        .map(|felt| felt.0)
        .map_err(|err| StateError::StateReadError(err.to_string()))
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        Python::with_gil(|py| -> PyResult<PyFelt> {
            let args = (ON_CHAIN_STORAGE_DOMAIN, PyFelt::from(contract_address));
            self.state_reader_proxy.as_ref(py).call_method1("get_nonce_at", args)?.extract()
        })
        .map(|nonce| Nonce(nonce.0))
        .map_err(|err| StateError::StateReadError(err.to_string()))
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        Python::with_gil(|py| -> PyResult<PyFelt> {
            let args = (PyFelt::from(contract_address),);
            self.state_reader_proxy.as_ref(py).call_method1("get_class_hash_at", args)?.extract()
        })
        .map(|felt| ClassHash(felt.0))
        .map_err(|err| StateError::StateReadError(err.to_string()))
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        Python::with_gil(|py| -> Result<RunnableCompiledClass, PyErr> {
            let args = (PyFelt::from(class_hash),);
            let py_versioned_raw_compiled_class: &PyTuple = self
                .state_reader_proxy
                .as_ref(py)
                .call_method1("get_versioned_raw_compiled_class", args)?
                .downcast()?;

            // Extract the raw compiled class
            let py_raw_compiled_class: PyRawCompiledClass =
                py_versioned_raw_compiled_class.get_item(0)?.extract()?;

            // Extract and process the Sierra version
            let (major, minor, patch): (u64, u64, u64) =
                py_versioned_raw_compiled_class.get_item(1)?.extract()?;

            let sierra_version = SierraVersion::new(major, minor, patch);

            let versioned_py_raw_compiled_class = VersionedPyRawClass {
                raw_compiled_class: py_raw_compiled_class,
                optional_sierra_version: Some(sierra_version),
            };

            let runnable_compiled_class =
                RunnableCompiledClass::try_from(versioned_py_raw_compiled_class)?;
            Ok(runnable_compiled_class)
        })
        .map_err(|err| {
            if Python::with_gil(|py| err.is_instance_of::<UndeclaredClassHashError>(py)) {
                StateError::UndeclaredClassHash(class_hash)
            } else {
                StateError::StateReadError(err.to_string())
            }
        })
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        Python::with_gil(|py| -> PyResult<PyFelt> {
            let args = (PyFelt::from(class_hash),);
            self.state_reader_proxy
                .as_ref(py)
                .call_method1("get_compiled_class_hash", args)?
                .extract()
        })
        .map(|felt| CompiledClassHash(felt.0))
        .map_err(|err| StateError::StateReadError(err.to_string()))
    }
}

#[derive(FromPyObject)]
pub struct PyRawCompiledClass {
    pub raw_compiled_class: String,
    pub version: usize,
}

pub struct VersionedPyRawClass {
    raw_compiled_class: PyRawCompiledClass,
    optional_sierra_version: Option<SierraVersion>,
}

impl TryFrom<VersionedPyRawClass> for RunnableCompiledClass {
    type Error = NativeBlockifierError;

    #[allow(clippy::result_large_err)]
    fn try_from(versioned_raw_compiled_class: VersionedPyRawClass) -> NativeBlockifierResult<Self> {
        let raw_compiled_class = versioned_raw_compiled_class.raw_compiled_class;

        match raw_compiled_class.version {
            0 => Ok(CompiledClassV0::try_from_json_string(&raw_compiled_class.raw_compiled_class)?
                .into()),
            1 => {
                let sierra_version = versioned_raw_compiled_class
                    .optional_sierra_version
                    .ok_or(NativeBlockifierInputError::MissingSierraVersion)?;
                Ok(CompiledClassV1::try_from_json_string(
                    &raw_compiled_class.raw_compiled_class,
                    sierra_version,
                )?
                .into())
            }
            _ => Err(NativeBlockifierInputError::UnsupportedContractClassVersion {
                version: raw_compiled_class.version,
            })?,
        }
    }
}
