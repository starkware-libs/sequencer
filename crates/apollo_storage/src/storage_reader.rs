#[cfg(test)]
#[path = "storage_reader_test.rs"]
mod storage_reader_test;

use crate::body::TransactionIndex;
use crate::db::table_types::Table;
use crate::db::TransactionKind;
use crate::mmap_file::LocationInFile;
use crate::{
    IndexedDeprecatedContractClass, MarkerKind, StorageError, StorageResult, StorageTxn,
};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use starknet_api::block::{BlockHash, BlockNumber, BlockSignature};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{SierraContractClass, StorageKey, ThinStateDiff};
use starknet_api::transaction::{Transaction, TransactionHash, TransactionOutput};
use starknet_types_core::felt::Felt;

/// Low-level storage reader trait providing direct access to all tables and files.
pub trait StorageReaderApi<Mode: TransactionKind> {
    
    /// Returns the location of the state diff for a given block number.
    /// This is step 1 of reading a state diff: get the location from the table.
    fn get_state_diff_location(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<LocationInFile>>;

    /// Returns the state diff from a file given its location.
    /// This is step 2 of reading a state diff: read from the file.
    fn get_state_diff_from_file(
        &self,
        location: LocationInFile,
    ) -> StorageResult<ThinStateDiff>;
    
    /// Returns the location of a contract class for a given class hash.
    fn get_class_location(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<LocationInFile>>;

    /// Returns the contract class from a file given its location.
    fn get_class_from_file(
        &self,
        location: LocationInFile,
    ) -> StorageResult<SierraContractClass>;

    /// Returns the block number at which a class was declared.
    fn get_class_declaration_block(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<BlockNumber>>;
    
    /// Returns the indexed deprecated class data (includes block number and location).
    fn get_deprecated_class_data(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<IndexedDeprecatedContractClass>>;

    /// Returns the deprecated contract class from a file given its location.
    fn get_deprecated_class_from_file(
        &self,
        location: LocationInFile,
    ) -> StorageResult<DeprecatedContractClass>;

    /// Returns the block number at which a deprecated class was first declared.
    fn get_deprecated_class_declaration_block(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<BlockNumber>>;
    
    /// Returns the location of a CASM (compiled class) for a given class hash.
    fn get_casm_location(&self, class_hash: &ClassHash)
        -> StorageResult<Option<LocationInFile>>;

    /// Returns the CASM from a file given its location.
    fn get_casm_from_file(&self, location: LocationInFile) -> StorageResult<CasmContractClass>;

    /// Returns the executable class hash for a given class hash.
    fn get_executable_class_hash(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<CompiledClassHash>>;
    
    /// Returns the class hash of a contract deployed at a specific address and block.
    fn get_deployed_contract_class_hash(
        &self,
        address: &ContractAddress,
        block_number: BlockNumber,
    ) -> StorageResult<Option<ClassHash>>;
    
    /// Returns the storage value for a contract at a specific storage key and block.
    fn get_contract_storage_value(
        &self,
        address: &ContractAddress,
        storage_key: &StorageKey,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Felt>>;
    
    /// Returns the nonce of a contract at a specific block.
    fn get_nonce_at_block(
        &self,
        address: &ContractAddress,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Nonce>>;
    
    /// Returns the block number for a given block hash.
    fn get_block_number_by_hash(
        &self,
        block_hash: &BlockHash,
    ) -> StorageResult<Option<BlockNumber>>;
    
    /// Returns the signature of a block.
    fn get_block_signature_by_number(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<BlockSignature>>;
    
    /// Returns the location of a transaction for a given transaction index.
    fn get_transaction_location(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<LocationInFile>>;

    /// Returns the transaction from a file given its location.
    fn get_transaction_from_file(
        &self,
        location: LocationInFile,
    ) -> StorageResult<Transaction>;

    /// Returns the transaction index for a given transaction hash.
    fn get_transaction_index_by_hash(
        &self,
        tx_hash: &TransactionHash,
    ) -> StorageResult<Option<TransactionIndex>>;
    
    /// Returns the location of a transaction output for a given transaction index.
    fn get_transaction_output_location(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<LocationInFile>>;

    /// Returns the transaction output from a file given its location.
    fn get_transaction_output_from_file(
        &self,
        location: LocationInFile,
    ) -> StorageResult<TransactionOutput>;
    
    /// Returns the value of a specific marker (header, body, state, class, etc.).
    fn get_marker(&self, marker_kind: MarkerKind) -> StorageResult<BlockNumber>;
}

impl<Mode: TransactionKind> StorageReaderApi<Mode> for StorageTxn<'_, Mode> {
    
    fn get_state_diff_location(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<LocationInFile>> {
        let state_diffs_table = self.open_table(&self.tables.state_diffs)?;
        Ok(state_diffs_table.get(&self.txn, &block_number)?)
    }

    fn get_state_diff_from_file(
        &self,
        location: LocationInFile,
    ) -> StorageResult<ThinStateDiff> {
        self.file_handlers.get_thin_state_diff_unchecked(location)
    }

    fn get_class_location(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<LocationInFile>> {
        let declared_classes_table = self.open_table(&self.tables.declared_classes)?;
        Ok(declared_classes_table.get(&self.txn, class_hash)?)
    }

    fn get_class_from_file(
        &self,
        location: LocationInFile,
    ) -> StorageResult<SierraContractClass> {
        self.file_handlers.get_contract_class_unchecked(location)
    }

    fn get_class_declaration_block(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<BlockNumber>> {
        let declared_classes_block_table = self.open_table(&self.tables.declared_classes_block)?;
        Ok(declared_classes_block_table.get(&self.txn, class_hash)?)
    }

    fn get_deprecated_class_data(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<IndexedDeprecatedContractClass>> {
        let deprecated_declared_classes_table =
            self.open_table(&self.tables.deprecated_declared_classes)?;
        Ok(deprecated_declared_classes_table.get(&self.txn, class_hash)?)
    }

    fn get_deprecated_class_from_file(
        &self,
        location: LocationInFile,
    ) -> StorageResult<DeprecatedContractClass> {
        self.file_handlers.get_deprecated_contract_class_unchecked(location)
    }

    fn get_deprecated_class_declaration_block(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<BlockNumber>> {
        let deprecated_declared_classes_block_table =
            self.open_table(&self.tables.deprecated_declared_classes_block)?;
        Ok(deprecated_declared_classes_block_table.get(&self.txn, class_hash)?)
    }

    fn get_casm_location(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<LocationInFile>> {
        let casms_table = self.open_table(&self.tables.casms)?;
        Ok(casms_table.get(&self.txn, class_hash)?)
    }

    fn get_casm_from_file(&self, location: LocationInFile) -> StorageResult<CasmContractClass> {
        self.file_handlers.get_casm_unchecked(location)
    }

    fn get_executable_class_hash(
        &self,
        class_hash: &ClassHash,
    ) -> StorageResult<Option<CompiledClassHash>> {
        let class_hash_to_executable_table =
            self.open_table(&self.tables.class_hash_to_executable_class_hash)?;
        Ok(class_hash_to_executable_table.get(&self.txn, class_hash)?)
    }

    fn get_deployed_contract_class_hash(
        &self,
        address: &ContractAddress,
        block_number: BlockNumber,
    ) -> StorageResult<Option<ClassHash>> {
        let deployed_contracts_table = self.open_table(&self.tables.deployed_contracts)?;
        Ok(deployed_contracts_table.get(&self.txn, &(*address, block_number))?)
    }

    fn get_contract_storage_value(
        &self,
        address: &ContractAddress,
        storage_key: &StorageKey,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Felt>> {
        let contract_storage_table = self.open_table(&self.tables.contract_storage)?;
        Ok(contract_storage_table.get(&self.txn, &((*address, *storage_key), block_number))?)
    }

    fn get_nonce_at_block(
        &self,
        address: &ContractAddress,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Nonce>> {
        let nonces_table = self.open_table(&self.tables.nonces)?;
        Ok(nonces_table.get(&self.txn, &(*address, block_number))?)
    }

    fn get_block_number_by_hash(
        &self,
        block_hash: &BlockHash,
    ) -> StorageResult<Option<BlockNumber>> {
        let block_hash_to_number_table = self.open_table(&self.tables.block_hash_to_number)?;
        Ok(block_hash_to_number_table.get(&self.txn, block_hash)?)
    }

    fn get_block_signature_by_number(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<BlockSignature>> {
        let block_signatures_table = self.open_table(&self.tables.block_signatures)?;
        Ok(block_signatures_table.get(&self.txn, &block_number)?)
    }

    fn get_transaction_location(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<LocationInFile>> {
        let transaction_metadata_table = self.open_table(&self.tables.transaction_metadata)?;
        let metadata = transaction_metadata_table.get(&self.txn, &transaction_index)?;
        Ok(metadata.map(|m| m.tx_location))
    }

    fn get_transaction_from_file(
        &self,
        location: LocationInFile,
    ) -> StorageResult<Transaction> {
        self.file_handlers.get_transaction_unchecked(location)
    }

    fn get_transaction_index_by_hash(
        &self,
        tx_hash: &TransactionHash,
    ) -> StorageResult<Option<TransactionIndex>> {
        let transaction_hash_to_idx_table =
            self.open_table(&self.tables.transaction_hash_to_idx)?;
        Ok(transaction_hash_to_idx_table.get(&self.txn, tx_hash)?)
    }

    fn get_transaction_output_location(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<LocationInFile>> {
        let transaction_metadata_table = self.open_table(&self.tables.transaction_metadata)?;
        let metadata = transaction_metadata_table.get(&self.txn, &transaction_index)?;
        Ok(metadata.map(|m| m.tx_output_location))
    }

    fn get_transaction_output_from_file(
        &self,
        location: LocationInFile,
    ) -> StorageResult<TransactionOutput> {
        self.file_handlers.get_transaction_output_unchecked(location)
    }

    fn get_marker(&self, marker_kind: MarkerKind) -> StorageResult<BlockNumber> {
        let markers_table = self.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &marker_kind)?.unwrap_or_default())
    }
}

