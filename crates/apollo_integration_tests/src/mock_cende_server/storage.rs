use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use apollo_starknet_client::reader::objects::block::BlockPostV0_13_1;
use apollo_starknet_client::reader::{
    Block,
    ContractClass,
    DeclaredClassHashEntry,
    DeployedContract,
    GenericContractClass,
    StateDiff,
    StateUpdate,
    StorageEntry,
};
use blockifier::blockifier_versioned_constants::VersionedConstants;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use indexmap::IndexMap;
use starknet_api::block::{
    BlockHash,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPricePerToken,
    StarknetVersion,
};
use starknet_api::contract_address;
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    EventCommitment,
    GlobalRoot,
    SequencerContractAddress,
    TransactionCommitment,
};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::{SierraContractClass, ThinStateDiff};
use starknet_api::test_utils::{
    CURRENT_BLOCK_TIMESTAMP,
    DEFAULT_ETH_L1_GAS_PRICE,
    DEFAULT_STRK_L1_GAS_PRICE,
    TEST_SEQUENCER_ADDRESS,
};
use tokio::sync::RwLock;

use crate::state_reader::TestClasses;
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

    pub async fn initialize_with_block_0(&self, state_diff: ThinStateDiff, classes: TestClasses) {
        let block_0 = create_block_0();
        let state_update_0 = create_state_update_0(state_diff);

        let sierra_contract_classes = prepare_sierra_contracts(&classes);
        let (compiled_classes_vec, class_hash_to_compiled_class_hash) =
            prepare_compiled_classes(&classes);

        self.add_block_data(
            BlockNumber(0),
            block_0,
            state_update_0,
            sierra_contract_classes,
            compiled_classes_vec,
            class_hash_to_compiled_class_hash,
        )
        .await;
    }
}

fn create_block_0() -> Block {
    let block_number = BlockNumber(0);

    let block_hash = BlockHash(starknet_api::hash::StarkHash::from_bytes_be_slice(&[0u8; 32]));

    Block::PostV0_13_1(BlockPostV0_13_1 {
        block_hash,
        block_number,
        parent_block_hash: BlockHash::default(),
        sequencer_address: SequencerContractAddress(contract_address!(TEST_SEQUENCER_ADDRESS)),
        state_root: GlobalRoot::default(),
        status: apollo_starknet_client::reader::objects::block::BlockStatus::AcceptedOnL2,
        timestamp: BlockTimestamp(CURRENT_BLOCK_TIMESTAMP),
        transactions: Vec::new(),
        transaction_receipts: Vec::new(),
        starknet_version: StarknetVersion::default(),
        l1_da_mode: L1DataAvailabilityMode::Calldata,
        l1_gas_price: GasPricePerToken {
            price_in_wei: DEFAULT_ETH_L1_GAS_PRICE.into(),
            price_in_fri: DEFAULT_STRK_L1_GAS_PRICE.into(),
        },
        l1_data_gas_price: GasPricePerToken {
            price_in_wei: DEFAULT_ETH_L1_GAS_PRICE.into(),
            price_in_fri: DEFAULT_STRK_L1_GAS_PRICE.into(),
        },
        l2_gas_price: GasPricePerToken {
            price_in_wei: VersionedConstants::latest_constants()
                .convert_l1_to_l2_gas_price_round_up(DEFAULT_ETH_L1_GAS_PRICE.into()),
            price_in_fri: VersionedConstants::latest_constants()
                .convert_l1_to_l2_gas_price_round_up(DEFAULT_STRK_L1_GAS_PRICE.into()),
        },
        transaction_commitment: TransactionCommitment::default(),
        event_commitment: EventCommitment::default(),
        state_diff_commitment: None,
        receipt_commitment: None,
        state_diff_length: None,
        l2_gas_consumed: GasAmount(0),
        next_l2_gas_price: GasPrice(0),
    })
}

fn create_state_update_0(thin_state_diff: ThinStateDiff) -> StateUpdate {
    let state_diff = convert_thin_state_diff_to_feeder_gateway(thin_state_diff);

    let block_hash = BlockHash(starknet_api::hash::StarkHash::from_bytes_be_slice(&[0u8; 32]));

    StateUpdate {
        block_hash,
        new_root: GlobalRoot::default(),
        old_root: GlobalRoot::default(),
        state_diff,
    }
}

fn convert_thin_state_diff_to_feeder_gateway(thin_state_diff: ThinStateDiff) -> StateDiff {
    StateDiff {
        storage_diffs: thin_state_diff
            .storage_diffs
            .into_iter()
            .map(|(address, entries)| {
                (
                    address,
                    entries.into_iter().map(|(key, value)| StorageEntry { key, value }).collect(),
                )
            })
            .collect(),
        deployed_contracts: thin_state_diff
            .deployed_contracts
            .into_iter()
            .map(|(address, class_hash)| DeployedContract { address, class_hash })
            .collect(),
        declared_classes: thin_state_diff
            .class_hash_to_compiled_class_hash
            .into_iter()
            .map(|(class_hash, compiled_class_hash)| DeclaredClassHashEntry {
                class_hash,
                compiled_class_hash,
            })
            .collect(),
        migrated_compiled_classes: Vec::new(),
        old_declared_contracts: thin_state_diff.deprecated_declared_classes,
        nonces: thin_state_diff.nonces,
        replaced_classes: Vec::new(),
    }
}

fn prepare_sierra_contracts(classes: &TestClasses) -> Vec<(ClassHash, SierraContractClass)> {
    let mut sierras = Vec::new();
    for (class_hash, (sierra, _casm)) in classes.cairo1_contract_classes.iter() {
        sierras.push((*class_hash, sierra.clone()));
    }
    sierras
}

fn prepare_compiled_classes(
    classes: &TestClasses,
) -> (Vec<(CompiledClassHash, CasmContractClass)>, IndexMap<ClassHash, CompiledClassHash>) {
    let mut compiled_classes_vec = Vec::new();
    let mut class_hash_to_compiled_class_hash = IndexMap::new();

    for (class_hash, (_sierra, casm)) in classes.cairo1_contract_classes.iter() {
        let compiled_class_hash = casm.hash(&HashVersion::V2);
        compiled_classes_vec.push((compiled_class_hash, casm.clone()));
        class_hash_to_compiled_class_hash.insert(*class_hash, compiled_class_hash);
    }

    (compiled_classes_vec, class_hash_to_compiled_class_hash)
}
