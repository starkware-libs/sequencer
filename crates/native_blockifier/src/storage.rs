#![allow(non_local_definitions)]

use std::collections::HashMap;
use std::convert::TryFrom;
use std::path::PathBuf;

use apollo_storage::class::ClassStorageWriter;
use apollo_storage::class_hash::ClassHashStorageWriter;
use apollo_storage::compiled_class::CasmStorageWriter;
use apollo_storage::header::{HeaderStorageReader, HeaderStorageWriter};
use apollo_storage::state::{StateStorageReader, StateStorageWriter};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use indexmap::IndexMap;
use pyo3::prelude::*;
use starknet_api::block::{BlockHash, BlockHeader, BlockHeaderWithoutHash, BlockNumber};
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::core::{ChainId, ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::StarkHash;
use starknet_api::state::{SierraContractClass, StateDiff, StateNumber, ThinStateDiff};

use crate::errors::NativeBlockifierResult;
use crate::py_state_diff::PyBlockInfo;
use crate::py_utils::{int_to_chain_id, PyFelt};
use crate::PyStateDiff;

const GENESIS_BLOCK_ID: u64 = u64::MAX;

// Class hash to (raw_sierra, (compiled class hash, raw casm)).
pub type RawDeclaredClassMapping = HashMap<PyFelt, (String, (PyFelt, String))>;
// Class hash to (compiled class hash, raw casm).
pub type RawDeprecatedDeclaredClassMapping = HashMap<PyFelt, String>;

// Invariant: Only one instance of this struct should exist.
// Reader and writer fields must be cleared before the struct goes out of scope in Python;
// to prevent possible memory leaks
// TODO(Dori): see if this is indeed necessary.
pub struct PapyrusStorage {
    reader: Option<apollo_storage::StorageReader>,
    writer: Option<apollo_storage::StorageWriter>,
}

impl PapyrusStorage {
    pub fn new(config: StorageConfig) -> NativeBlockifierResult<PapyrusStorage> {
        log::debug!("Initializing Blockifier storage...");
        let db_config = apollo_storage::db::DbConfig {
            path_prefix: config.path_prefix,
            enforce_file_exists: config.enforce_file_exists,
            chain_id: config.chain_id,
            min_size: 1 << 20, // 1MB.
            max_size: config.max_size,
            growth_step: 1 << 26, // 64MB.
        };
        let storage_config = apollo_storage::StorageConfig {
            db_config,
            scope: apollo_storage::StorageScope::StateOnly, // Only stores blockifier-related data.
            // Storage for large objects (state-diffs, contracts). This sets total storage
            // allocated, maximum space an object can take, and how fast the storage grows.
            mmap_file_config: apollo_storage::mmap_file::MmapFileConfig {
                max_size: 1 << 40,        // 1TB
                growth_step: 2 << 30,     // 2GB
                max_object_size: 1 << 30, // 1GB
            },
        };
        let (reader, writer) = apollo_storage::open_storage(storage_config)?;
        log::debug!("Initialized Blockifier storage.");

        Ok(PapyrusStorage { reader: Some(reader), writer: Some(writer) })
    }

    /// Manually drops the storage reader and writer.
    /// Python does not necessarily drop them even if instance is no longer live.
    pub fn new_for_testing(path_prefix: PathBuf, chain_id: &ChainId) -> PapyrusStorage {
        let db_config = apollo_storage::db::DbConfig {
            path_prefix,
            chain_id: chain_id.clone(),
            enforce_file_exists: false,
            min_size: 1 << 20,    // 1MB
            max_size: 1 << 35,    // 32GB
            growth_step: 1 << 26, // 64MB
        };
        let storage_config = apollo_storage::StorageConfig { db_config, ..Default::default() };
        let (reader, writer) = apollo_storage::open_storage(storage_config).unwrap();

        PapyrusStorage { reader: Some(reader), writer: Some(writer) }
    }
}

impl Storage for PapyrusStorage {
    /// Returns the next block number, for which state diff was not yet appended.
    fn get_state_marker(&self) -> NativeBlockifierResult<u64> {
        let block_number = self.reader().begin_ro_txn()?.get_state_marker()?;
        Ok(block_number.0)
    }

    fn get_header_marker(&self) -> NativeBlockifierResult<u64> {
        let block_number = self.reader().begin_ro_txn()?.get_header_marker()?;
        Ok(block_number.0)
    }

    fn get_block_id(&self, block_number: u64) -> NativeBlockifierResult<Option<Vec<u8>>> {
        let block_number = BlockNumber(block_number);
        let block_hash = self
            .reader()
            .begin_ro_txn()?
            .get_block_header(block_number)?
            .map(|block_header| Vec::from(block_header.block_hash.to_bytes_be().as_slice()));
        Ok(block_hash)
    }

    fn revert_block(&mut self, block_number: u64) -> NativeBlockifierResult<()> {
        log::debug!("Reverting state diff for {block_number:?}.");
        let block_number = BlockNumber(block_number);
        let revert_txn = self.writer().begin_rw_txn()?;
        let (revert_txn, _) = revert_txn.revert_state_diff(block_number)?;
        let (revert_txn, _, _) = revert_txn.revert_header(block_number)?;

        revert_txn.commit()?;
        Ok(())
    }

    // TODO(Gilad): Refactor.
    fn append_block(
        &mut self,
        block_id: u64,
        previous_block_id: Option<PyFelt>,
        py_block_info: PyBlockInfo,
        py_state_diff: PyStateDiff,
        declared_class_hash_to_class: RawDeclaredClassMapping,
        deprecated_declared_class_hash_to_class: RawDeprecatedDeclaredClassMapping,
    ) -> NativeBlockifierResult<()> {
        log::debug!(
            "Appending state diff with {block_id:?} for block_number: {}.",
            py_block_info.block_number
        );
        let block_number = BlockNumber(py_block_info.block_number);
        let state_number = StateNumber(block_number);

        // Deserialize contract classes.
        let mut deprecated_declared_classes = IndexMap::<ClassHash, DeprecatedContractClass>::new();
        for (class_hash, raw_class) in deprecated_declared_class_hash_to_class {
            let class_hash = ClassHash(class_hash.0);
            let class_undeclared = self
                .reader()
                .begin_ro_txn()?
                .get_state_reader()?
                .get_deprecated_class_definition_at(state_number, &class_hash)?
                .is_none();

            if class_undeclared {
                let deprecated_contract_class: DeprecatedContractClass =
                    serde_json::from_str(&raw_class)?;
                deprecated_declared_classes.insert(class_hash, deprecated_contract_class);
            }
        }

        let mut declared_classes =
            IndexMap::<ClassHash, (CompiledClassHash, SierraContractClass)>::new();
        let mut undeclared_casm_contracts =
            Vec::<(ClassHash, CasmContractClass, CompiledClassHash)>::new();
        for (class_hash, (raw_sierra, (compiled_class_hash, raw_casm))) in
            declared_class_hash_to_class
        {
            let class_hash = ClassHash(class_hash.0);
            let class_undeclared = self
                .reader()
                .begin_ro_txn()?
                .get_state_reader()?
                .get_class_definition_at(state_number, &class_hash)?
                .is_none();

            if class_undeclared {
                let sierra_contract_class: SierraContractClass = serde_json::from_str(&raw_sierra)?;
                declared_classes.insert(
                    class_hash,
                    (CompiledClassHash(compiled_class_hash.0), sierra_contract_class),
                );
                let casm_contract_class: CasmContractClass = serde_json::from_str(&raw_casm)?;
                let compiled_class_hash_v2 = casm_contract_class.hash(&HashVersion::V2);
                undeclared_casm_contracts.push((
                    class_hash,
                    casm_contract_class,
                    compiled_class_hash_v2,
                ));
            }
        }

        let mut append_txn = self.writer().begin_rw_txn()?;
        for (class_hash, casm_contract_class, compiled_class_hash_v2) in undeclared_casm_contracts {
            append_txn = append_txn.append_casm(&class_hash, &casm_contract_class)?;
            append_txn =
                append_txn.set_executable_class_hash_v2(&class_hash, compiled_class_hash_v2)?;
        }

        // Construct state diff; manually add declared classes.
        let mut state_diff = StateDiff::try_from(py_state_diff)?;
        state_diff.deprecated_declared_classes = deprecated_declared_classes;
        state_diff.declared_classes = declared_classes;

        let (thin_state_diff, declared_classes, deprecated_declared_classes) =
            ThinStateDiff::from_state_diff(state_diff);

        append_txn = append_txn.append_state_diff(block_number, thin_state_diff)?.append_classes(
            block_number,
            &declared_classes
                .iter()
                .map(|(class_hash, contract_class)| (*class_hash, contract_class))
                .collect::<Vec<_>>(),
            &deprecated_declared_classes
                .iter()
                .map(|(class_hash, contract_class)| (*class_hash, contract_class))
                .collect::<Vec<_>>(),
        )?;

        let previous_block_id = previous_block_id.unwrap_or_else(|| PyFelt::from(GENESIS_BLOCK_ID));
        let block_header = BlockHeader {
            block_hash: BlockHash(StarkHash::from(block_id)),
            block_header_without_hash: BlockHeaderWithoutHash {
                parent_hash: BlockHash(previous_block_id.0),
                block_number,
                ..Default::default()
            },
            ..Default::default()
        };
        append_txn = append_txn.append_header(block_number, &block_header)?;

        append_txn.commit()?;
        Ok(())
    }

    fn set_executable_class_hash_v2(
        &mut self,
        class_hash: &ClassHash,
        compiled_class_hash_v2: CompiledClassHash,
    ) -> NativeBlockifierResult<()> {
        self.writer()
            .begin_rw_txn()?
            .set_executable_class_hash_v2(class_hash, compiled_class_hash_v2)?;
        Ok(())
    }

    fn validate_aligned(&self, source_block_number: u64) {
        let header_marker = self.get_header_marker().expect("Should have a header marker");
        let state_marker = self.get_state_marker().expect("Should have a state marker");

        assert_eq!(
            header_marker, state_marker,
            "Block header marker ({header_marker}) must be aligned to block state diff marker \
             ({state_marker}) before sequencing starts."
        );

        assert_eq!(
            state_marker, source_block_number,
            "Target storage (block number {state_marker}) should have been aligned to block \
             number {source_block_number}."
        );
    }

    fn reader(&self) -> &apollo_storage::StorageReader {
        self.reader.as_ref().expect("Storage should be initialized.")
    }

    fn writer(&mut self) -> &mut apollo_storage::StorageWriter {
        self.writer.as_mut().expect("Storage should be initialized.")
    }

    fn close(&mut self) {
        log::debug!("Closing Blockifier storage.");
        self.reader = None;
        self.writer = None;
    }
}

#[pyclass]
#[derive(Clone)]
pub struct StorageConfig {
    path_prefix: PathBuf,
    chain_id: ChainId,
    enforce_file_exists: bool,
    max_size: usize,
}

#[pymethods]
impl StorageConfig {
    #[new]
    #[pyo3(signature = (path_prefix, chain_id, enforce_file_exists, max_size))]
    pub fn new(
        path_prefix: PathBuf,
        #[pyo3(from_py_with = "int_to_chain_id")] chain_id: ChainId,
        enforce_file_exists: bool,
        max_size: usize,
    ) -> Self {
        Self { path_prefix, chain_id, enforce_file_exists, max_size }
    }
}

pub trait Storage {
    fn get_state_marker(&self) -> NativeBlockifierResult<u64>;
    fn get_header_marker(&self) -> NativeBlockifierResult<u64>;
    fn get_block_id(&self, block_number: u64) -> NativeBlockifierResult<Option<Vec<u8>>>;

    fn revert_block(&mut self, block_number: u64) -> NativeBlockifierResult<()>;
    fn append_block(
        &mut self,
        block_id: u64,
        previous_block_id: Option<PyFelt>,
        py_block_info: PyBlockInfo,
        py_state_diff: PyStateDiff,
        declared_class_hash_to_class: RawDeclaredClassMapping,
        deprecated_declared_class_hash_to_class: RawDeprecatedDeclaredClassMapping,
    ) -> NativeBlockifierResult<()>;

    fn validate_aligned(&self, source_block_number: u64);

    fn reader(&self) -> &apollo_storage::StorageReader;
    fn writer(&mut self) -> &mut apollo_storage::StorageWriter;

    fn close(&mut self);

    fn set_executable_class_hash_v2(
        &mut self,
        class_hash: &ClassHash,
        compiled_class_hash_v2: CompiledClassHash,
    ) -> NativeBlockifierResult<()>;
}
