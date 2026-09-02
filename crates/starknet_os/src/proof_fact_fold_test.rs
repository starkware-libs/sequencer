use std::collections::HashMap;

use apollo_starknet_os_program::test_programs::PROOF_FACT_FOLD_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use expect_test::expect;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::{
    compute_fold_digest,
    compute_leaf_output_digest,
    fold_block_proof_facts,
    fold_block_root_entries,
    pack_output_digest,
    Blake2sDigestWords,
    FoldEntry,
    TransactionProofFacts,
    BLAKE2S_DIGEST_N_WORDS,
    LEAF_VERIFIER_CIRCUIT_HASHES,
    MULTIVERIFIER_CIRCUIT_HASH,
};
use crate::test_utils::cairo_runner::{
    initialize_and_run_cairo_0_entry_point,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};

const FOLD_ENTRY_N_WORDS: usize = 2 * BLAKE2S_DIGEST_N_WORDS;

fn entrypoint_runner_config() -> EntryPointRunnerConfig {
    EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        trace_enabled: false,
        verify_secure: false,
        proof_mode: false,
        add_main_prefix_to_entrypoint: true,
        validate_builtins_offset: true,
    }
}

fn felt_array_arg(felts: &[Felt]) -> EndpointArg {
    EndpointArg::Pointer(PointerArg::Array(
        felts.iter().map(|felt| MaybeRelocatable::Int(*felt)).collect(),
    ))
}

/// Runs a function of `proof_fact_fold.cairo` that returns a single pointer to
/// `n_returned_words` u32 words, and returns those words.
fn run_cairo_function_returning_words(
    function_name: &str,
    explicit_args: &[EndpointArg],
    implicit_args: &[ImplicitArg],
    n_returned_words: usize,
) -> Vec<u32> {
    let expected_return_values = vec![EndpointArg::Pointer(PointerArg::Array(vec![
        MaybeRelocatable::from(Felt::ZERO);
        n_returned_words
    ]))];
    let (_, explicit_return_values, _) = initialize_and_run_cairo_0_entry_point(
        &entrypoint_runner_config(),
        PROOF_FACT_FOLD_BYTES,
        function_name,
        explicit_args,
        implicit_args,
        &expected_return_values,
        HashMap::new(),
        None,
    )
    .unwrap_or_else(|error| panic!("Failed to run Cairo function {function_name}: {error:?}"));
    let [EndpointArg::Pointer(PointerArg::Array(returned_words))] =
        explicit_return_values.as_slice()
    else {
        panic!("Expected {function_name} to return a single words-array pointer.");
    };
    returned_words
        .iter()
        .map(|returned_word| {
            let MaybeRelocatable::Int(word_felt) = returned_word else {
                panic!("Expected a felt digest word, got {returned_word:?}.");
            };
            u32::try_from(word_felt.to_biguint()).expect("A digest word must fit in a u32.")
        })
        .collect()
}

fn fold_entry_from_words(entry_words: &[u32]) -> FoldEntry {
    FoldEntry {
        circuit_hash: entry_words[..BLAKE2S_DIGEST_N_WORDS].try_into().unwrap(),
        output_digest: entry_words[BLAKE2S_DIGEST_N_WORDS..].try_into().unwrap(),
    }
}

/// Runs the Cairo `fold_block_proof_facts` and returns the root entry.
fn run_cairo_fold_block_proof_facts(
    per_transaction_proof_facts: &[TransactionProofFacts<'_>],
) -> FoldEntry {
    let root_entry_words = run_cairo_function_returning_words(
        "fold_block_proof_facts",
        &fold_block_proof_facts_args(per_transaction_proof_facts),
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        FOLD_ENTRY_N_WORDS,
    );
    fold_entry_from_words(&root_entry_words)
}

/// Runs the Cairo `fold_block_proof_facts` and returns the run's execution resources.
fn run_cairo_fold_execution_resources(
    per_transaction_proof_facts: &[TransactionProofFacts<'_>],
) -> ExecutionResources {
    let expected_return_values = vec![EndpointArg::Pointer(PointerArg::Array(vec![
            MaybeRelocatable::from(Felt::ZERO);
            FOLD_ENTRY_N_WORDS
        ]))];
    let (_, _, cairo_runner) = initialize_and_run_cairo_0_entry_point(
        &entrypoint_runner_config(),
        PROOF_FACT_FOLD_BYTES,
        "fold_block_proof_facts",
        &fold_block_proof_facts_args(per_transaction_proof_facts),
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        &expected_return_values,
        HashMap::new(),
        None,
    )
    .unwrap_or_else(|error| panic!("Failed to run fold_block_proof_facts: {error:?}"));
    cairo_runner.get_execution_resources().unwrap().filter_unused_builtins()
}

/// The Cairo arguments: `n_transactions`, then an array of `ProofFactsReference`s - a
/// (size, pointer, leaf circuit index) triple per transaction.
fn fold_block_proof_facts_args(
    per_transaction_proof_facts: &[TransactionProofFacts<'_>],
) -> Vec<EndpointArg> {
    let proof_facts_references = EndpointArg::Pointer(PointerArg::Composed(
        per_transaction_proof_facts
            .iter()
            .flat_map(|transaction_proof_facts| {
                [
                    EndpointArg::from(Felt::from(transaction_proof_facts.proof_facts.len())),
                    felt_array_arg(transaction_proof_facts.proof_facts),
                    EndpointArg::from(Felt::from(transaction_proof_facts.leaf_circuit_index)),
                ]
            })
            .collect(),
    ));
    vec![EndpointArg::from(Felt::from(per_transaction_proof_facts.len())), proof_facts_references]
}

/// Proof facts shaped like a real transaction's, with values derived from
/// `transaction_index` so different transactions get different digests.
fn synthetic_proof_facts(transaction_index: u64) -> Vec<Felt> {
    vec![
        Felt::from_hex("0x50524f4f4631").unwrap(), // 'PROOF1'
        Felt::from_hex("0x5649525455414c5f534e4f53").unwrap(), // 'VIRTUAL_SNOS'
        // A program hash and block hash above 2^63, exercising the 8-word felt encoding.
        Felt::from(2u64).pow(200u64) + Felt::from(transaction_index),
        Felt::from_hex("0x5649525455414c5f534e4f5330").unwrap(), // 'VIRTUAL_SNOS0'
        Felt::from(1000 + transaction_index),
        Felt::from(2u64).pow(150u64) + Felt::from(transaction_index),
        Felt::from(transaction_index * 17),
        Felt::ONE,
        Felt::from(2u64).pow(100u64) + Felt::from(transaction_index),
    ]
}

#[test]
fn test_leaf_output_digest_matches_proving_side_golden() {
    let proving_side_preimage = [
        Felt::from_dec_str(
            "1433852663250257978909904594223798547176815246431631498282706690602142197827",
        )
        .unwrap(),
        Felt::from(11),
        Felt::from(13),
        Felt::from(17),
    ];
    let proof_facts: Vec<Felt> =
        [Felt::ZERO, Felt::ZERO].into_iter().chain(proving_side_preimage).collect();
    let expected_digest_words: Blake2sDigestWords = [
        1603116091, 3258597502, 2711032228, 4175407283, 343882323, 1898618121, 1344732087,
        1064799167,
    ];
    assert_eq!(compute_leaf_output_digest(&proof_facts), expected_digest_words);
}

/// Golden from proving-dev's `four_leaves` fixture: four identical leaves folded over
/// two layers under the `canonical_small` registry (this module's circuit hashes).
#[test]
fn test_four_leaf_fold_matches_proving_side_golden() {
    let proving_side_preimage = [
        Felt::from_hex("0x32b88272d54b83880ebebd9c4292a650bee27d1575e82123391b6df2932e843")
            .unwrap(),
        Felt::from_hex("0xb").unwrap(),
        Felt::from_hex("0xd").unwrap(),
        Felt::from_hex("0x11").unwrap(),
    ];
    let proof_facts: Vec<Felt> =
        [Felt::ZERO, Felt::ZERO].into_iter().chain(proving_side_preimage).collect();
    let root_entry = fold_block_proof_facts(
        &[TransactionProofFacts { proof_facts: &proof_facts, leaf_circuit_index: 0 }; 4],
    );
    let expected_root_output_digest: Blake2sDigestWords = [
        897652633, 1382572116, 3969946465, 347296500, 2153515991, 2472657789, 1975506022,
        3786147232,
    ];
    assert_eq!(root_entry.output_digest, expected_root_output_digest);
    assert_eq!(root_entry.circuit_hash, MULTIVERIFIER_CIRCUIT_HASH);
}

#[rstest]
#[case::minimal_facts(vec![Felt::ZERO, Felt::ZERO, Felt::ONE])]
#[case::small_values(vec![Felt::ZERO, Felt::ZERO, Felt::from(42), Felt::from(1337)])]
#[case::boundary_below_2_63(vec![Felt::ZERO, Felt::ZERO, Felt::from((1u64 << 63) - 1)])]
#[case::boundary_at_2_63(vec![Felt::ZERO, Felt::ZERO, Felt::from(1u64 << 63)])]
#[case::realistic_facts(synthetic_proof_facts(0))]
fn test_cairo_leaf_output_digest_matches_rust(#[case] proof_facts: Vec<Felt>) {
    let cairo_digest_words = run_cairo_function_returning_words(
        "compute_leaf_output_digest",
        &[EndpointArg::from(Felt::from(proof_facts.len())), felt_array_arg(&proof_facts)],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        BLAKE2S_DIGEST_N_WORDS,
    );
    assert_eq!(cairo_digest_words, compute_leaf_output_digest(&proof_facts));
}

#[test]
fn test_cairo_multiverifier_circuit_hash_matches_rust() {
    let cairo_circuit_hash_words = run_cairo_function_returning_words(
        "get_multiverifier_circuit_hash",
        &[],
        &[],
        BLAKE2S_DIGEST_N_WORDS,
    );
    assert_eq!(cairo_circuit_hash_words, MULTIVERIFIER_CIRCUIT_HASH);
}

#[test]
fn test_cairo_leaf_verifier_circuit_hash_table_matches_rust() {
    for (leaf_circuit_index, expected_circuit_hash) in
        LEAF_VERIFIER_CIRCUIT_HASHES.iter().enumerate()
    {
        let cairo_circuit_hash_words = run_cairo_function_returning_words(
            "get_leaf_verifier_circuit_hash",
            &[EndpointArg::from(Felt::from(leaf_circuit_index))],
            &[ImplicitArg::Builtin(BuiltinName::range_check)],
            BLAKE2S_DIGEST_N_WORDS,
        );
        assert_eq!(&cairo_circuit_hash_words, expected_circuit_hash);
    }
}

/// The leaf circuit index is unconstrained input, so the Cairo getter must reject an
/// index past the end of the table.
#[test]
#[should_panic(expected = "Failed to run Cairo function get_leaf_verifier_circuit_hash")]
fn test_cairo_leaf_verifier_circuit_hash_rejects_out_of_bounds_index() {
    run_cairo_function_returning_words(
        "get_leaf_verifier_circuit_hash",
        &[EndpointArg::from(Felt::from(LEAF_VERIFIER_CIRCUIT_HASHES.len()))],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        BLAKE2S_DIGEST_N_WORDS,
    );
}

/// Covers the self-fold (one transaction), a full layer (two, four), the carry rule at
/// one layer (three) and at two layers (five), and a deeper tree (seven).
#[rstest]
#[case::single_transaction_self_fold(1)]
#[case::two_transactions(2)]
#[case::three_transactions_carry(3)]
#[case::four_transactions(4)]
#[case::five_transactions_double_carry(5)]
#[case::seven_transactions(7)]
fn test_cairo_fold_block_proof_facts_matches_rust(#[case] n_transactions: u64) {
    let per_transaction_proof_facts: Vec<Vec<Felt>> =
        (0..n_transactions).map(synthetic_proof_facts).collect();
    let transaction_proof_facts: Vec<TransactionProofFacts<'_>> = per_transaction_proof_facts
        .iter()
        .map(|proof_facts| TransactionProofFacts { proof_facts, leaf_circuit_index: 0 })
        .collect();
    let cairo_root_entry = run_cairo_fold_block_proof_facts(&transaction_proof_facts);
    assert_eq!(cairo_root_entry, fold_block_proof_facts(&transaction_proof_facts));
}

#[test]
fn test_cairo_fold_digest_matches_rust() {
    let proof_facts = synthetic_proof_facts(0);
    let root_entry = fold_block_proof_facts(
        &[TransactionProofFacts { proof_facts: &proof_facts, leaf_circuit_index: 0 }; 2],
    );
    let root_entry_words: Vec<Felt> = root_entry
        .circuit_hash
        .iter()
        .chain(root_entry.output_digest.iter())
        .map(|word| Felt::from(*word))
        .collect();
    let cairo_fold_digest_words = run_cairo_function_returning_words(
        "compute_fold_digest",
        &[felt_array_arg(&root_entry_words)],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        BLAKE2S_DIGEST_N_WORDS,
    );
    assert_eq!(cairo_fold_digest_words, compute_fold_digest(&root_entry));
}

/// Block-root entries with distinct output digests, as the combine level sees them.
fn synthetic_block_root_entries(n_blocks: u64) -> Vec<FoldEntry> {
    (0..n_blocks)
        .map(|block_index| {
            let proof_facts = synthetic_proof_facts(block_index);
            fold_block_proof_facts(&[TransactionProofFacts {
                proof_facts: &proof_facts,
                leaf_circuit_index: 0,
            }])
        })
        .collect()
}

/// Covers the single-entry carry (no self-fold at the combine level), a full layer, and
/// the carry rule.
#[rstest]
#[case::single_block_carried(1)]
#[case::two_blocks(2)]
#[case::three_blocks_carry(3)]
#[case::five_blocks_double_carry(5)]
fn test_cairo_fold_block_root_entries_matches_rust(#[case] n_blocks: u64) {
    let block_root_entries = synthetic_block_root_entries(n_blocks);
    let entry_words: Vec<Felt> = block_root_entries
        .iter()
        .flat_map(|entry| {
            entry
                .circuit_hash
                .iter()
                .chain(entry.output_digest.iter())
                .map(|word| Felt::from(*word))
        })
        .collect();
    let cairo_root_entry_words = run_cairo_function_returning_words(
        "fold_block_root_entries",
        &[EndpointArg::from(Felt::from(n_blocks)), felt_array_arg(&entry_words)],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        FOLD_ENTRY_N_WORDS,
    );
    assert_eq!(
        fold_entry_from_words(&cairo_root_entry_words),
        fold_block_root_entries(block_root_entries)
    );
}

#[test]
fn test_cairo_pack_and_unpack_output_digest_match_rust() {
    let proof_facts = synthetic_proof_facts(0);
    let root_entry = fold_block_proof_facts(&[TransactionProofFacts {
        proof_facts: &proof_facts,
        leaf_circuit_index: 0,
    }]);
    let (expected_low, expected_high) = pack_output_digest(&root_entry.output_digest);

    // Pack in Cairo: takes the 16-word entry, returns (low, high).
    let entry_words: Vec<Felt> = root_entry
        .circuit_hash
        .iter()
        .chain(root_entry.output_digest.iter())
        .map(|word| Felt::from(*word))
        .collect();
    let expected_return_values = vec![EndpointArg::from(Felt::ZERO), EndpointArg::from(Felt::ZERO)];
    let (_, packed_return_values, _) = initialize_and_run_cairo_0_entry_point(
        &entrypoint_runner_config(),
        PROOF_FACT_FOLD_BYTES,
        "pack_output_digest",
        &[felt_array_arg(&entry_words)],
        &[],
        &expected_return_values,
        HashMap::new(),
        None,
    )
    .unwrap_or_else(|error| panic!("Failed to run pack_output_digest: {error:?}"));
    let [
        EndpointArg::Value(ValueArg::Single(MaybeRelocatable::Int(cairo_low))),
        EndpointArg::Value(ValueArg::Single(MaybeRelocatable::Int(cairo_high))),
    ] = packed_return_values.as_slice()
    else {
        panic!("Expected pack_output_digest to return two felts.");
    };
    assert_eq!((*cairo_low, *cairo_high), (expected_low, expected_high));

    // Unpack in Cairo: takes (low, high), returns the reconstituted block-root entry.
    let unpacked_entry_words = run_cairo_function_returning_words(
        "unpack_block_root_entry",
        &[EndpointArg::from(expected_low), EndpointArg::from(expected_high)],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        FOLD_ENTRY_N_WORDS,
    );
    assert_eq!(fold_entry_from_words(&unpacked_entry_words), root_entry);
}

/// One verifier list's circuit hashes from the vendored registry, in registry order.
fn registry_circuit_hashes(
    registry: &serde_json::Value,
    verifier_list_key: &str,
) -> Vec<Blake2sDigestWords> {
    registry[verifier_list_key]
        .as_array()
        .unwrap_or_else(|| panic!("The registry must list {verifier_list_key}."))
        .iter()
        .map(|verifier_entry| {
            let circuit_hash_words: Vec<u32> = verifier_entry["circuit_hash"]
                .as_array()
                .expect("A circuit hash must be an array of words.")
                .iter()
                .map(|circuit_hash_word| {
                    let word_hex =
                        circuit_hash_word.as_str().expect("A circuit hash word must be a string.");
                    u32::from_str_radix(word_hex.trim_start_matches("0x"), 16)
                        .expect("A circuit hash word must be a hex u32.")
                })
                .collect();
            circuit_hash_words.try_into().expect("A circuit hash must have exactly 8 words.")
        })
        .collect()
}

/// Pins the circuit hash constants to the vendored circuit registry
/// (`resources/circuit_registry_canonical_small.json`): the `canonical_small` registry,
/// taken verbatim from proving-dev commit b75d21f91fe846401e002ad169dbcbe57f289ebf at
/// crates/stwo_run_and_prove_recursive_tree/test_data/circuit_registry.json. Together
/// with the Cairo constant tests (`test_cairo_leaf_verifier_circuit_hash_table_matches_rust`
/// and `test_cairo_multiverifier_circuit_hash_matches_rust`) this pins the Cairo tables
/// to the registry, so swapping in the production registry is a values-only change whose
/// omissions or drift are caught here. The production registry lists one leaf verifier
/// per trace size (five today); swapping it in means growing the allowed-circuits tables
/// to all of them and feeding each transaction's real leaf circuit index into the OS -
/// this comparison failing on such a registry forces that work.
#[test]
fn test_circuit_hash_constants_match_vendored_registry() {
    let registry: serde_json::Value =
        serde_json::from_str(include_str!("../resources/circuit_registry_canonical_small.json"))
            .expect("The vendored circuit registry must be valid JSON.");
    assert_eq!(
        registry_circuit_hashes(&registry, "leaf_verifiers"),
        LEAF_VERIFIER_CIRCUIT_HASHES.to_vec()
    );
    assert_eq!(
        registry_circuit_hashes(&registry, "multiverifiers"),
        vec![MULTIVERIFIER_CIRCUIT_HASH]
    );
}

/// Pins the Cairo fold's execution cost, answering the design's per-block budget
/// question: `fold_block_proof_facts` over `n_transactions` realistic transactions
/// (9-felt proof facts with three large felts) costs `n_transactions` leaf digests
/// (felt encoding + blake) plus the fold hashes - one self-fold for a single
/// transaction, `n_transactions - 1` pair folds otherwise. A change here means the
/// fold's cost profile changed; rerun with `UPDATE_EXPECT=1` after verifying the cause.
#[test]
fn test_fold_block_proof_facts_execution_resources() {
    let fold_resources_summary = |n_transactions: u64| {
        let per_transaction_proof_facts: Vec<Vec<Felt>> =
            (0..n_transactions).map(synthetic_proof_facts).collect();
        let transaction_proof_facts: Vec<TransactionProofFacts<'_>> = per_transaction_proof_facts
            .iter()
            .map(|proof_facts| TransactionProofFacts { proof_facts, leaf_circuit_index: 0 })
            .collect();
        let execution_resources = run_cairo_fold_execution_resources(&transaction_proof_facts);
        format!(
            "{} steps, {} range checks",
            execution_resources.n_steps,
            execution_resources
                .builtin_instance_counter
                .get(&BuiltinName::range_check)
                .copied()
                .unwrap_or(0)
        )
    };
    expect!["970 steps, 15 range checks"].assert_eq(&fold_resources_summary(1));
    expect!["1335 steps, 27 range checks"].assert_eq(&fold_resources_summary(2));
    expect!["5979 steps, 117 range checks"].assert_eq(&fold_resources_summary(8));
}
