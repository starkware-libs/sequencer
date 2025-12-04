use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use apollo_starknet_client::reader::{Block, ContractClass, GenericContractClass, StateUpdate};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use indexmap::IndexMap;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::state::SierraContractClass;
use tokio::sync::RwLock;

/// In-memory storage for mock CENDE server
pub struct MockCendeStorage {
    blocks: Arc<RwLock<BTreeMap<BlockNumber, String>>>,
    state_updates: Arc<RwLock<BTreeMap<BlockNumber, String>>>,
    contract_classes: Arc<RwLock<BTreeMap<ClassHash, String>>>,
    compiled_classes: Arc<RwLock<BTreeMap<CompiledClassHash, String>>>,
    class_hash_to_compiled_class_hashes: Arc<RwLock<IndexMap<ClassHash, CompiledClassHash>>>,
}

impl MockCendeStorage {
    pub fn new() -> Self {
        Self {
            blocks: Arc::new(RwLock::new(BTreeMap::new())),
            state_updates: Arc::new(RwLock::new(BTreeMap::new())),
            contract_classes: Arc::new(RwLock::new(BTreeMap::new())),
            compiled_classes: Arc::new(RwLock::new(BTreeMap::new())),
            class_hash_to_compiled_class_hashes: Arc::new(RwLock::new(IndexMap::new())),
        }
    }

    pub async fn add_block_data(
        &self,
        block_number: BlockNumber,
        block: Block,
        state_update: StateUpdate,
        sierra_contract_classes: Vec<(ClassHash, SierraContractClass)>,
        compiled_classes_vec: Vec<(CompiledClassHash, CasmContractClass)>,
        class_hash_to_compiled_class_hash: IndexMap<ClassHash, CompiledClassHash>,
    ) {
        let mut blocks = self.blocks.write().await;
        let mut state_updates = self.state_updates.write().await;
        let mut contract_classes = self.contract_classes.write().await;
        let mut compiled_classes = self.compiled_classes.write().await;
        let mut class_hash_to_compiled_class_hashes =
            self.class_hash_to_compiled_class_hashes.write().await;

        let block_str = serde_json::to_string(&block).unwrap();
        blocks.insert(block_number, block_str);

        let state_update_str = serde_json::to_string(&state_update).unwrap();
        state_updates.insert(block_number, state_update_str);

        for (class_hash, sierra_contract_class) in sierra_contract_classes {
            let generic_contract_class: GenericContractClass =
                GenericContractClass::Cairo1ContractClass(ContractClass {
                    sierra_program: sierra_contract_class.sierra_program,
                    entry_points_by_type: {
                        let mut entry_points_by_type = HashMap::new();
                        entry_points_by_type.insert(
                            EntryPointType::Constructor,
                            sierra_contract_class.entry_points_by_type.constructor,
                        );
                        entry_points_by_type.insert(
                            EntryPointType::External,
                            sierra_contract_class.entry_points_by_type.external,
                        );
                        entry_points_by_type.insert(
                            EntryPointType::L1Handler,
                            sierra_contract_class.entry_points_by_type.l1handler,
                        );
                        entry_points_by_type
                    },
                    contract_class_version: sierra_contract_class.contract_class_version,
                    abi: sierra_contract_class.abi,
                });
            contract_classes
                .insert(class_hash, serde_json::to_string(&generic_contract_class).unwrap());
        }

        for (compiled_class_hash, compiled_class) in compiled_classes_vec {
            let compiled_class_str = serde_json::to_string(&compiled_class).unwrap();
            compiled_classes.insert(compiled_class_hash, compiled_class_str);
        }

        for (class_hash, compiled_class_hash) in class_hash_to_compiled_class_hash {
            class_hash_to_compiled_class_hashes.insert(class_hash, compiled_class_hash);
        }
    }

    pub async fn get_block(&self, block_number: BlockNumber) -> Option<String> {
        let blocks = self.blocks.read().await;
        blocks.get(&block_number).cloned()
    }

    pub async fn get_state_update(&self, block_number: BlockNumber) -> Option<String> {
        let state_updates = self.state_updates.read().await;
        state_updates.get(&block_number).cloned()
    }

    pub async fn get_contract_class(&self, class_hash: ClassHash) -> Option<String> {
        let contract_classes = self.contract_classes.read().await;
        contract_classes.get(&class_hash).cloned()
    }

    pub async fn get_compiled_class(&self, class_hash: ClassHash) -> Option<String> {
        let class_hash_to_compiled_class_hashes =
            self.class_hash_to_compiled_class_hashes.read().await;
        let compiled_class_hash = class_hash_to_compiled_class_hashes.get(&class_hash).cloned()?;
        let compiled_classes = self.compiled_classes.read().await;
        compiled_classes.get(&compiled_class_hash).cloned()
    }

    pub async fn get_latest_block_number(&self) -> Option<BlockNumber> {
        let blocks = self.blocks.read().await;
        blocks.keys().max().copied()
    }
}
