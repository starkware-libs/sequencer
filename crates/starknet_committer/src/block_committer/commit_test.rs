use std::collections::HashMap;
use std::sync::LazyLock;

use expect_test::{expect, Expect};
use rand::rngs::SmallRng;
use rand::SeedableRng;
use rstest::rstest;
use rstest_reuse::{apply, template};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::hash::StateRoots;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_types_core::felt::Felt;

use crate::block_committer::commit::{CommitBlockImpl, CommitBlockTrait};
use crate::block_committer::input::{
    Input,
    ReaderConfig,
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use crate::block_committer::measurements_util::NoMeasurements;
use crate::block_committer::state_diff_generator::generate_random_state_diff;
use crate::db::facts_db::types::FactsDbInitialRead;
use crate::db::facts_db::FactsDb;
use crate::db::forest_trait::{
    EmptyInitialReadContext,
    ForestReader,
    ForestWriter,
    StorageInitializer,
};
use crate::db::index_db::{IndexDb, IndexDbReadContext};
use crate::patricia_merkle_tree::types::CompiledClassHash;

static FIRST_CONTRACT_ADDRESS: LazyLock<ContractAddress> =
    LazyLock::new(|| ContractAddress::from(1_u128));
static SECOND_CONTRACT_ADDRESS: LazyLock<ContractAddress> =
    LazyLock::new(|| ContractAddress::from(1_u128 << 100));

const EXPECTED_ROOTS_SIMPLE_CASE: Expect = expect![[r#"
    StateRoots {
        contracts_trie_root_hash: HashOutput(
            0x6a70cc05bcb982d269d9c7a66f92a5e9265c451871bc25487ca61c40e0f1829,
        ),
        classes_trie_root_hash: HashOutput(
            0x7ae8007b033a1c032c6a791331f56a652833b0b25633587318276f4462c8c1c,
        ),
    }
"#]];

const EXPECTED_ROOTS_RANDOM_CASE: Expect = expect![[r#"
    StateRoots {
        contracts_trie_root_hash: HashOutput(
            0x635a7ee9e0e7a2c89e98bf2ce5589aea094ccaf1d125b9a9af38e584912e66c,
        ),
        classes_trie_root_hash: HashOutput(
            0x0,
        ),
    }
"#]];

const N_RANDOM_STATE_UPDATES: usize = 200;

impl From<StateRoots> for FactsDbInitialRead {
    fn from(roots: StateRoots) -> Self {
        Self(roots)
    }
}

impl From<StateRoots> for IndexDbReadContext {
    fn from(_roots: StateRoots) -> Self {
        Self
    }
}

#[template]
#[rstest]
#[case([get_first_state_diff(), get_second_state_diff()], &EXPECTED_ROOTS_SIMPLE_CASE)]
#[case(get_random_state_diffs(), &EXPECTED_ROOTS_RANDOM_CASE)]
fn state_diff_cases(#[case] state_diffs: [StateDiff; 2], #[case] expected_roots: &Expect) {}

#[apply(state_diff_cases)]
#[tokio::test]
#[rstest]
async fn test_commit_two_consecutive_blocks_facts_layout(
    #[case] state_diffs: [StateDiff; 2],
    #[case] expected_roots: &Expect,
) {
    test_commit_two_consecutive_blocks::<FactsDb<MapStorage>>(
        FactsDb::new,
        FactsDbInitialRead(StateRoots::default()),
        state_diffs,
        expected_roots,
    )
    .await;
}

#[apply(state_diff_cases)]
#[tokio::test]
#[rstest]
async fn test_commit_two_consecutive_blocks_index_layout(
    #[case] state_diffs: [StateDiff; 2],
    #[case] expected_roots: &Expect,
) {
    test_commit_two_consecutive_blocks::<IndexDb<MapStorage>>(
        IndexDb::new,
        IndexDbReadContext::create_empty(),
        state_diffs,
        expected_roots,
    )
    .await;
}

async fn test_commit_two_consecutive_blocks<Db: ForestReader + ForestWriter>(
    db_generator: impl FnOnce(MapStorage) -> Db,
    initial_read_context: Db::InitialReadContext,
    [first_state_diff, second_state_diff]: [StateDiff; 2],
    expected_roots: &Expect,
) where
    Db::InitialReadContext: From<StateRoots>,
{
    let storage = MapStorage::default();
    let mut db = db_generator(storage);

    let mut input = Input {
        state_diff: first_state_diff,
        initial_read_context,
        config: ReaderConfig::default(),
    };
    let filled_forest =
        CommitBlockImpl::commit_block(input, &mut db, &mut NoMeasurements).await.unwrap();
    db.write(&filled_forest).await.unwrap();

    input = Input {
        state_diff: second_state_diff,
        initial_read_context: filled_forest.state_roots().into(),
        config: ReaderConfig::default(),
    };

    let filled_forest =
        CommitBlockImpl::commit_block(input, &mut db, &mut NoMeasurements).await.unwrap();
    db.write(&filled_forest).await.unwrap();

    expected_roots.assert_debug_eq(&filled_forest.state_roots());
}

fn get_first_state_diff() -> StateDiff {
    let mut contract_class_changes = HashMap::new();
    contract_class_changes.insert(*FIRST_CONTRACT_ADDRESS, ClassHash(Felt::ONE));
    contract_class_changes.insert(*SECOND_CONTRACT_ADDRESS, ClassHash(Felt::TWO));

    let mut individual_storage_changes = HashMap::new();
    individual_storage_changes
        .insert(StarknetStorageKey::from(1_u128), StarknetStorageValue(Felt::from(1_u128)));
    individual_storage_changes
        .insert(StarknetStorageKey::from(2_u128), StarknetStorageValue(Felt::from(2_u128)));

    let mut storage_updates = HashMap::new();
    storage_updates.insert(*FIRST_CONTRACT_ADDRESS, individual_storage_changes.clone());
    storage_updates.insert(*SECOND_CONTRACT_ADDRESS, individual_storage_changes);

    StateDiff {
        address_to_class_hash: contract_class_changes,
        address_to_nonce: HashMap::new(),
        class_hash_to_compiled_class_hash: HashMap::new(),
        storage_updates,
    }
}

fn get_second_state_diff() -> StateDiff {
    let mut contract_class_changes = HashMap::new();
    contract_class_changes.insert(*FIRST_CONTRACT_ADDRESS, ClassHash(Felt::TWO));

    let mut individual_storage_changes = HashMap::new();
    individual_storage_changes
        .insert(StarknetStorageKey::from(1_u128), StarknetStorageValue(Felt::from(2_u128)));
    individual_storage_changes
        .insert(StarknetStorageKey::from(2_u128), StarknetStorageValue(Felt::from(4_u128)));

    let mut storage_updates = HashMap::new();
    storage_updates.insert(*SECOND_CONTRACT_ADDRESS, individual_storage_changes);

    let mut declarations = HashMap::new();
    declarations.insert(ClassHash(Felt::THREE), CompiledClassHash(Felt::THREE));

    StateDiff {
        address_to_class_hash: contract_class_changes,
        address_to_nonce: HashMap::new(),
        class_hash_to_compiled_class_hash: declarations,
        storage_updates,
    }
}

fn get_random_state_diffs() -> [StateDiff; 2] {
    // Use a constant seed for reproducibility.
    let mut seed = 42_u64;
    let mut rng = SmallRng::seed_from_u64(seed);
    let first_state_diff = generate_random_state_diff(&mut rng, N_RANDOM_STATE_UPDATES, None);
    seed += 1;
    rng = SmallRng::seed_from_u64(seed);
    let second_state_diff = generate_random_state_diff(&mut rng, N_RANDOM_STATE_UPDATES, None);
    [first_state_diff, second_state_diff]
}
