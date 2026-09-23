use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use apollo_infra_utils::path::resolve_project_relative_path;
use cairo_vm::cairo_run::{cairo_run, CairoRunConfig};
use cairo_vm::hint_processor::builtin_hint_processor::builtin_hint_processor_definition::BuiltinHintProcessor;
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

use crate::proving::prover::prove;

/// Test resource file names.
const CAIRO_PIE_FILE: &str = "cairo_pie_10_transfers.zip";
const EXPECTED_PROOF_FACTS_FILE: &str = "proof_facts_10_transfers.json";

/// A program under `resources/mod_builtin_programs` (see the README there), with the mod-builtin
/// instance counts its execution must produce.
#[derive(Clone, Copy, Debug)]
struct ModBuiltinProgram {
    name: &'static str,
    n_add_mod_instances: usize,
    n_mul_mod_instances: usize,
}

const NO_MOD_BUILTIN_PROGRAM: ModBuiltinProgram =
    ModBuiltinProgram { name: "no_mod_builtin", n_add_mod_instances: 0, n_mul_mod_instances: 0 };
const ADD_MOD_BUILTIN_PROGRAM: ModBuiltinProgram =
    ModBuiltinProgram { name: "add_mod_builtin", n_add_mod_instances: 1, n_mul_mod_instances: 0 };
const MUL_MOD_BUILTIN_PROGRAM: ModBuiltinProgram =
    ModBuiltinProgram { name: "mul_mod_builtin", n_add_mod_instances: 0, n_mul_mod_instances: 1 };

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

/// Executes a program from `resources/mod_builtin_programs` and returns its Cairo PIE, after
/// checking that the PIE is valid and that the execution used the expected mod-builtin instances.
fn run_mod_builtin_program(program: ModBuiltinProgram) -> CairoPie {
    let program_path =
        resolve_resource_path(&format!("mod_builtin_programs/{}_compiled.json", program.name));
    let program_bytes = fs::read(&program_path)
        .unwrap_or_else(|_| panic!("Failed to read {}", program_path.display()));
    let cairo_run_config = CairoRunConfig {
        layout: LayoutName::all_cairo,
        secure_run: Some(true),
        ..Default::default()
    };
    let cairo_runner =
        cairo_run(&program_bytes, &cairo_run_config, &mut BuiltinHintProcessor::new_empty())
            .unwrap_or_else(|error| panic!("Failed to run {}: {error}", program.name));
    let cairo_pie = cairo_runner.get_cairo_pie().expect("Failed to get Cairo PIE");
    cairo_pie.run_validity_checks().expect("Invalid Cairo PIE");

    // Every program declares both mod builtins, so an unused one has an explicit zero counter.
    let builtin_instance_counter = &cairo_pie.execution_resources.builtin_instance_counter;
    assert_eq!(
        builtin_instance_counter.get(&BuiltinName::add_mod),
        Some(&program.n_add_mod_instances),
        "Unexpected add_mod usage in {}",
        program.name
    );
    assert_eq!(
        builtin_instance_counter.get(&BuiltinName::mul_mod),
        Some(&program.n_mul_mod_instances),
        "Unexpected mul_mod usage in {}",
        program.name
    );
    cairo_pie
}

#[rstest]
#[case::no_mod_builtin(NO_MOD_BUILTIN_PROGRAM)]
#[case::add_mod_builtin(ADD_MOD_BUILTIN_PROGRAM)]
#[case::mul_mod_builtin(MUL_MOD_BUILTIN_PROGRAM)]
fn test_mod_builtin_program_execution(#[case] program: ModBuiltinProgram) {
    run_mod_builtin_program(program);
}

/// The control for [`test_small_prover_rejects_mod_builtin_program`]: an execution that differs
/// from the mod-builtin programs only in not using a mod builtin proves and verifies.
///
/// Run with:
/// ```shell
/// cargo test -p starknet_transaction_prover --release --features stwo_proving \
///     small_prover_ -- --ignored --test-threads=1
/// ```
#[test]
#[ignore = "slow: runs the stwo prover"]
fn test_small_prover_proves_no_mod_builtin_program() {
    let cairo_pie = run_mod_builtin_program(NO_MOD_BUILTIN_PROGRAM);

    let proof_output = privacy_recursive_prove(cairo_pie, prepare_precomputes())
        .expect("Failed to prove the no-mod-builtin program");

    verify_recursive_circuit(&proof_output).expect("Failed to verify proof");
}

/// The small prover cannot prove an execution that uses `add_mod` or `mul_mod`.
///
/// Upstream fails either with an error or with a panic, and has no typed unsupported-builtin
/// error, so this test accepts any failure. Together with
/// [`test_small_prover_proves_no_mod_builtin_program`] it establishes that using the mod builtin
/// is what the small prover rejects, but it cannot tell a future unrelated failure apart.
// TODO(Avi): Assert the typed unsupported-builtin error once upstream returns one.
#[rstest]
#[case::add_mod_builtin(ADD_MOD_BUILTIN_PROGRAM)]
#[case::mul_mod_builtin(MUL_MOD_BUILTIN_PROGRAM)]
#[ignore = "slow: runs the stwo prover"]
fn test_small_prover_rejects_mod_builtin_program(#[case] program: ModBuiltinProgram) {
    let cairo_pie = run_mod_builtin_program(program);
    let precomputes = prepare_precomputes();

    let prove_result =
        catch_unwind(AssertUnwindSafe(|| privacy_recursive_prove(cairo_pie, precomputes)));

    assert!(
        !matches!(prove_result, Ok(Ok(_))),
        "The small prover unexpectedly proved {}",
        program.name
    );
}

/// The large prover currently proves executions that use `add_mod` or `mul_mod`.
///
/// Not run in CI, since each case takes minutes. Run with:
/// ```shell
/// cargo test -p starknet_transaction_prover --release --features stwo_proving \
///     large_prover_ -- --ignored --test-threads=1
/// ```
// TODO(Avi): Expect the mod-builtin cases to fail once the large prover rejects mod builtins, keep
// the no-mod-builtin case as the control, and run this test in CI.
#[rstest]
#[case::no_mod_builtin(NO_MOD_BUILTIN_PROGRAM)]
#[case::add_mod_builtin(ADD_MOD_BUILTIN_PROGRAM)]
#[case::mul_mod_builtin(MUL_MOD_BUILTIN_PROGRAM)]
#[ignore = "slow: runs the stwo prover"]
fn test_large_prover_proves_mod_builtin_program(#[case] program: ModBuiltinProgram) {
    let cairo_pie = run_mod_builtin_program(program);

    let proof_output = privacy_recursive_prove_large(cairo_pie)
        .unwrap_or_else(|error| panic!("Failed to prove {}: {error}", program.name));

    verify_recursive_circuit(&proof_output).expect("Failed to verify proof");
}
