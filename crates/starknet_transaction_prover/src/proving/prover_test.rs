use std::collections::BTreeMap;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::Arc;

use apollo_infra_utils::path::resolve_project_relative_path;
use assert_matches::assert_matches;
use cairo_vm::cairo_run::{cairo_run, CairoRunConfig};
use cairo_vm::hint_processor::builtin_hint_processor::builtin_hint_processor_definition::{
    BuiltinHintProcessor,
    HintFunc,
};
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_from_var_name;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use privacy_circuit_verify_v2::{verify_recursive_circuit, PrivacyProofOutput};
use privacy_prove::{
    prepare_recursive_prover_precomputes,
    privacy_recursive_prove,
    privacy_recursive_prove_large,
    RecursiveProverPrecomputes,
};
use rstest::rstest;
use starknet_api::transaction::fields::VIRTUAL_SNOS;
use starknet_proof_verifier::ProgramOutput;
use starknet_types_core::felt::Felt;

use crate::errors::ProvingError;
use crate::proving::prover::{prove, unsupported_builtin_usage};

/// Test resource file names.
const CAIRO_PIE_FILE: &str = "cairo_pie_10_transfers.zip";
const EXPECTED_PROOF_FACTS_FILE: &str = "proof_facts_10_transfers.json";
const BUILTIN_USAGE_PROGRAM_FILE: &str = "builtin_usage_program/builtin_usage_compiled.json";

/// The hint in `builtin_usage.cairo` that reads the `selected_builtin` program input.
const SELECTED_BUILTIN_HINT: &str = r#"ids.selected_builtin = program_input["selected_builtin"]"#;

/// The builtins that the small privacy prover supports, all of which every execution of the program
/// in `resources/builtin_usage_program` uses.
const SUPPORTED_BUILTINS: [BuiltinName; 7] = [
    BuiltinName::output,
    BuiltinName::pedersen,
    BuiltinName::range_check,
    BuiltinName::bitwise,
    BuiltinName::ec_op,
    BuiltinName::keccak,
    BuiltinName::poseidon,
];

/// The `selected_builtin` input of the program in `resources/builtin_usage_program` (see the README
/// there): a builtin that the small privacy prover does not support, which the execution uses once
/// on top of the supported builtins.
#[derive(Clone, Copy, Debug)]
enum SelectedBuiltin {
    NoBuiltin,
    Ecdsa,
    RangeCheck96,
    AddMod,
    MulMod,
}

impl SelectedBuiltin {
    /// The `selected_builtin` input value that `builtin_usage.cairo` defines for this variant.
    fn input_value(self) -> u8 {
        match self {
            Self::NoBuiltin => 0,
            Self::Ecdsa => 1,
            Self::RangeCheck96 => 2,
            Self::AddMod => 3,
            Self::MulMod => 4,
        }
    }

    fn builtin_name(self) -> Option<BuiltinName> {
        match self {
            Self::NoBuiltin => None,
            Self::Ecdsa => Some(BuiltinName::ecdsa),
            Self::RangeCheck96 => Some(BuiltinName::range_check96),
            Self::AddMod => Some(BuiltinName::add_mod),
            Self::MulMod => Some(BuiltinName::mul_mod),
        }
    }
}

const SELECTABLE_BUILTINS: [BuiltinName; 4] =
    [BuiltinName::ecdsa, BuiltinName::range_check96, BuiltinName::add_mod, BuiltinName::mul_mod];

fn resolve_resource_path(file_name: &str) -> std::path::PathBuf {
    let path: std::path::PathBuf =
        ["crates", "starknet_transaction_prover", "resources", file_name].iter().collect();
    resolve_project_relative_path(&path.to_string_lossy())
        .unwrap_or_else(|_| panic!("Failed to resolve path for {file_name}"))
}

fn prepare_precomputes() -> Arc<RecursiveProverPrecomputes> {
    prepare_recursive_prover_precomputes().expect("Failed to prepare precomputes")
}

/// Integration test that verifies proving works with a real Cairo PIE.
///
/// Run with:
/// ```shell
/// rustup run nightly-2026-01-15 cargo test -p starknet_transaction_prover --release --features \
///     stwo_proving test_prove_cairo_pie_10_transfers
/// ```
#[tokio::test]
async fn test_prove_cairo_pie_10_transfers() {
    let cairo_pie_path = resolve_resource_path(CAIRO_PIE_FILE);
    let expected_program_output_path = resolve_resource_path(EXPECTED_PROOF_FACTS_FILE);

    // Read CairoPie from zip file.
    let cairo_pie =
        CairoPie::read_zip_file(&cairo_pie_path).expect("Failed to read Cairo PIE from zip file");

    // Prepare precomputes and prove the Cairo PIE.
    let precomputes = prepare_precomputes();
    let output = prove(cairo_pie, precomputes).await.expect("Failed to prove Cairo PIE");

    // Verify the proof using the circuit verifier.
    let output_preimage: Vec<starknet_types_core::felt::Felt> = output.program_output.0.to_vec();
    let proof_output = PrivacyProofOutput { proof: output.proof.0.to_vec(), output_preimage };
    verify_recursive_circuit(&proof_output).expect("Failed to verify proof");

    // Read expected program output.
    let expected_program_output_str = fs::read_to_string(&expected_program_output_path)
        .expect("Failed to read expected program output file");
    let expected_program_output: ProgramOutput = serde_json::from_str(&expected_program_output_str)
        .expect("Failed to parse expected program output");

    // Compare program output.
    assert_eq!(
        output.program_output, expected_program_output,
        "Generated program output does not match expected program output"
    );
}

/// Regenerates the example proof fixtures used by `apollo_transaction_converter` tests.
///
/// Run manually with:
/// ```bash
/// cargo test -p starknet_transaction_prover --features stwo_proving -- --ignored regenerate_proof_fixtures
/// ```
#[tokio::test]
#[ignore]
async fn regenerate_proof_fixtures() {
    let cairo_pie_path = resolve_resource_path(CAIRO_PIE_FILE);
    let cairo_pie =
        CairoPie::read_zip_file(&cairo_pie_path).expect("Failed to read Cairo PIE from zip file");

    let precomputes = prepare_precomputes();
    let output = prove(cairo_pie, precomputes).await.expect("Failed to prove Cairo PIE");

    // Save proof as raw binary.
    let raw_bytes: Vec<u8> = output.proof.0.to_vec();
    let proof_path = resolve_transaction_converter_resource("example_proof.bin");
    fs::write(&proof_path, &raw_bytes).expect("Failed to write proof file");
    println!("Wrote proof to {}", proof_path.display());

    // Save proof facts as JSON.
    let proof_facts = output
        .program_output
        .try_into_proof_facts(VIRTUAL_SNOS)
        .expect("Failed to convert program output to proof facts");
    let proof_facts_json =
        serde_json::to_string_pretty(&proof_facts).expect("Failed to serialize proof facts");
    let proof_facts_path = resolve_transaction_converter_resource("example_proof_facts.json");
    fs::write(&proof_facts_path, proof_facts_json).expect("Failed to write proof facts file");
    println!("Wrote proof facts to {}", proof_facts_path.display());
}

fn resolve_transaction_converter_resource(file_name: &str) -> std::path::PathBuf {
    let relative_path: std::path::PathBuf =
        ["crates", "apollo_transaction_converter", "resources", file_name].iter().collect();
    resolve_project_relative_path(&relative_path.to_string_lossy())
        .unwrap_or_else(|_| panic!("Failed to resolve path for {file_name}"))
}

/// Executes the program in `resources/builtin_usage_program` with `selected_builtin` as its input
/// and returns its Cairo PIE, after checking that the PIE is valid, that the execution used every
/// supported builtin, and that it used one instance of the selected builtin and none of the other
/// selectable builtins.
fn run_builtin_usage_program(selected_builtin: SelectedBuiltin) -> CairoPie {
    let program_path = resolve_resource_path(BUILTIN_USAGE_PROGRAM_FILE);
    let program_bytes = fs::read(&program_path)
        .unwrap_or_else(|_| panic!("Failed to read {}", program_path.display()));
    let cairo_run_config = CairoRunConfig {
        layout: LayoutName::all_cairo,
        secure_run: Some(true),
        ..Default::default()
    };
    let selected_builtin_value = Felt::from(selected_builtin.input_value());
    let mut hint_processor = BuiltinHintProcessor::new_empty();
    hint_processor.add_hint(
        SELECTED_BUILTIN_HINT.to_string(),
        Rc::new(HintFunc(Box::new(move |vm, _exec_scopes, ids_data, ap_tracking, _constants| {
            insert_value_from_var_name(
                "selected_builtin",
                selected_builtin_value,
                vm,
                ids_data,
                ap_tracking,
            )
        }))),
    );
    let cairo_runner = cairo_run(&program_bytes, &cairo_run_config, &mut hint_processor)
        .unwrap_or_else(|error| panic!("Failed to run with {selected_builtin:?}: {error}"));
    let cairo_pie = cairo_runner.get_cairo_pie().expect("Failed to get Cairo PIE");
    cairo_pie.run_validity_checks().expect("Invalid Cairo PIE");

    let builtin_instance_counter = &cairo_pie.execution_resources.builtin_instance_counter;
    for builtin_name in SUPPORTED_BUILTINS {
        assert!(
            builtin_instance_counter.get(&builtin_name).is_some_and(|n_instances| *n_instances > 0),
            "Unused supported builtin {builtin_name:?} with {selected_builtin:?}"
        );
    }
    // The program declares every selectable builtin, so an unused one has an explicit zero counter.
    for builtin_name in SELECTABLE_BUILTINS {
        let expected_n_instances =
            usize::from(selected_builtin.builtin_name() == Some(builtin_name));
        assert_eq!(
            builtin_instance_counter.get(&builtin_name),
            Some(&expected_n_instances),
            "Unexpected {builtin_name:?} usage with {selected_builtin:?}"
        );
    }
    cairo_pie
}

#[rstest]
#[case::no_builtin(SelectedBuiltin::NoBuiltin)]
#[case::ecdsa(SelectedBuiltin::Ecdsa)]
#[case::range_check96(SelectedBuiltin::RangeCheck96)]
#[case::add_mod(SelectedBuiltin::AddMod)]
#[case::mul_mod(SelectedBuiltin::MulMod)]
fn test_builtin_usage_program_execution(#[case] selected_builtin: SelectedBuiltin) {
    run_builtin_usage_program(selected_builtin);
}

/// The small prover proves an execution that uses every supported builtin, including `ec_op` and
/// `keccak`, whose prover components are outside its component set: it runs the privacy bootloader
/// in a layout without those builtins, and the bootloader verifies their instances with Cairo code,
/// so their usage adds no component.
///
/// This is also the control for [`test_small_prover_rejects_unsupported_builtin`]: the rejected
/// executions differ from this one only in using an unsupported builtin.
///
/// Run with:
/// ```shell
/// cargo test -p starknet_transaction_prover --release --features stwo_proving \
///     small_prover_ -- --ignored --test-threads=1
/// ```
#[test]
#[ignore = "slow: runs the stwo prover"]
fn test_small_prover_proves_supported_builtins() {
    let cairo_pie = run_builtin_usage_program(SelectedBuiltin::NoBuiltin);

    let proof_output = privacy_recursive_prove(cairo_pie, prepare_precomputes())
        .expect("Failed to prove the execution that uses only supported builtins");

    verify_recursive_circuit(&proof_output).expect("Failed to verify proof");
}

/// The small prover cannot prove an execution that uses `ecdsa`, `range_check96`, `add_mod` or
/// `mul_mod`.
///
/// Upstream fails either with an error or with a panic, and has no typed unsupported-builtin
/// error, so this test accepts any failure. Together with
/// [`test_small_prover_proves_supported_builtins`] it establishes that using the selected builtin
/// is what the small prover rejects, but it cannot tell a future unrelated failure apart.
// TODO(Avi): Assert the typed unsupported-builtin error once upstream returns one.
#[rstest]
#[case::ecdsa(SelectedBuiltin::Ecdsa)]
#[case::range_check96(SelectedBuiltin::RangeCheck96)]
#[case::add_mod(SelectedBuiltin::AddMod)]
#[case::mul_mod(SelectedBuiltin::MulMod)]
#[ignore = "slow: runs the stwo prover"]
fn test_small_prover_rejects_unsupported_builtin(#[case] selected_builtin: SelectedBuiltin) {
    let cairo_pie = run_builtin_usage_program(selected_builtin);
    let precomputes = prepare_precomputes();

    let prove_result =
        catch_unwind(AssertUnwindSafe(|| privacy_recursive_prove(cairo_pie, precomputes)));

    assert!(
        !matches!(prove_result, Ok(Ok(_))),
        "The small prover unexpectedly proved an execution that uses {selected_builtin:?}"
    );
}

/// The large prover currently proves executions that use builtins the small prover does not
/// support.
///
/// Not run in CI, since each case takes minutes. Run with:
/// ```shell
/// cargo test -p starknet_transaction_prover --release --features stwo_proving \
///     large_prover_ -- --ignored --test-threads=1
/// ```
// TODO(Avi): Expect the selected-builtin cases to fail once the large prover rejects them, keep the
// no-builtin case as the control, and run this test in CI.
#[rstest]
#[case::no_builtin(SelectedBuiltin::NoBuiltin)]
#[case::ecdsa(SelectedBuiltin::Ecdsa)]
#[case::range_check96(SelectedBuiltin::RangeCheck96)]
#[case::add_mod(SelectedBuiltin::AddMod)]
#[case::mul_mod(SelectedBuiltin::MulMod)]
#[ignore = "slow: runs the stwo prover"]
fn test_large_prover_proves_selected_builtin(#[case] selected_builtin: SelectedBuiltin) {
    let cairo_pie = run_builtin_usage_program(selected_builtin);

    let proof_output = privacy_recursive_prove_large(cairo_pie)
        .unwrap_or_else(|error| panic!("Failed to prove with {selected_builtin:?}: {error}"));

    verify_recursive_circuit(&proof_output).expect("Failed to verify proof");
}

#[rstest]
#[case::ecdsa_only(vec![(BuiltinName::ecdsa, 1)], vec![(BuiltinName::ecdsa, 1)])]
#[case::range_check96_only(
    vec![(BuiltinName::range_check96, 4)],
    vec![(BuiltinName::range_check96, 4)]
)]
#[case::add_mod_only(vec![(BuiltinName::add_mod, 2)], vec![(BuiltinName::add_mod, 2)])]
#[case::mul_mod_only(vec![(BuiltinName::mul_mod, 3)], vec![(BuiltinName::mul_mod, 3)])]
#[case::add_mod_and_mul_mod(
    vec![(BuiltinName::mul_mod, 3), (BuiltinName::add_mod, 2)],
    vec![(BuiltinName::add_mod, 2), (BuiltinName::mul_mod, 3)]
)]
#[case::explicit_zero_counts(vec![(BuiltinName::add_mod, 0), (BuiltinName::mul_mod, 0)], vec![])]
#[case::supported_builtins_only(
    vec![(BuiltinName::range_check, 5), (BuiltinName::ec_op, 1), (BuiltinName::keccak, 2)],
    vec![]
)]
fn test_unsupported_builtin_usage(
    #[case] builtin_instance_counts: Vec<(BuiltinName, usize)>,
    #[case] expected_unsupported_builtins: Vec<(BuiltinName, usize)>,
) {
    let builtin_instance_counter = BTreeMap::from_iter(builtin_instance_counts);

    assert_eq!(unsupported_builtin_usage(&builtin_instance_counter), expected_unsupported_builtins);
}

#[rstest]
#[case::ecdsa(SelectedBuiltin::Ecdsa, vec![(BuiltinName::ecdsa, 1)])]
#[case::range_check96(SelectedBuiltin::RangeCheck96, vec![(BuiltinName::range_check96, 1)])]
#[case::add_mod(SelectedBuiltin::AddMod, vec![(BuiltinName::add_mod, 1)])]
#[case::mul_mod(SelectedBuiltin::MulMod, vec![(BuiltinName::mul_mod, 1)])]
#[tokio::test]
async fn test_prove_rejects_unsupported_builtin(
    #[case] selected_builtin: SelectedBuiltin,
    #[case] expected_unsupported_builtins: Vec<(BuiltinName, usize)>,
) {
    let cairo_pie = run_builtin_usage_program(selected_builtin);

    let error = prove(cairo_pie, prepare_precomputes()).await.unwrap_err();

    assert_matches!(
        error,
        ProvingError::UnsupportedBuiltins { unsupported_builtins }
            if unsupported_builtins == expected_unsupported_builtins
    );
}
