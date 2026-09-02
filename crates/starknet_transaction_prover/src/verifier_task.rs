//! Runs the privacy circuit verifier as a simple-bootloader task and checks its output
//! against the proof-fact fold the OS emits.
//!
//! The verifier is a Cairo1 executable, which the Cairo0 OS cannot call; the simple
//! bootloader runs it as a task. The bootloader writes the output page
//! `[n_tasks = 1, task_output_size = 10, verifier_program_hash, digest word 0..7]`,
//! where the digest is the verifier's `blake2s(circuit_hash || root output digest)` of
//! the recursive proof tree it verified. The verifier program hash (under the Blake
//! program hash function) is the task's only identity, so callers must check it against
//! a pinned value. An invalid proof makes the run fail - the executable's entry wrapper
//! asserts the panic indicator is zero - so there is no output to misinterpret.
//!
//! [`verify_circuit_verifier_task_output`] closes the loop: it recomputes the expected
//! digest from the packed root output digest the OS emitted in its output header
//! (`proof_facts_root_output_low/high`) and compares.

use std::rc::Rc;

use cairo_program_runner_lib::cairo_run_program;
use cairo_program_runner_lib::hints::types::{HashFunc, SimpleBootloaderInput, TaskSpec};
use cairo_program_runner_lib::tasks::create_cairo1_program_task;
use cairo_program_runner_lib::utils::{get_cairo_run_config, ProgramInput};
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use starknet_os::proof_fact_fold::{
    compute_fold_digest,
    unpack_output_digest,
    Blake2sDigestWords,
    FoldEntry,
    BLAKE2S_DIGEST_N_WORDS,
    MULTIVERIFIER_CIRCUIT_HASH,
};
use starknet_types_core::felt::Felt;

#[cfg(test)]
#[path = "verifier_task_test.rs"]
mod verifier_task_test;

/// The bootloader output page of a verifier task: the size felt, the program hash felt,
/// and the eight digest words.
const VERIFIER_TASK_OUTPUT_SIZE: usize = 2 + BLAKE2S_DIGEST_N_WORDS;

#[derive(Debug, thiserror::Error)]
pub enum VerifierTaskError {
    #[error("Failed to build the verifier task: {0}")]
    BuildTask(String),
    #[error(
        "The circuit verifier run failed (the proof is invalid, or the verifier and proof \
         configurations mismatch): {0}"
    )]
    VerifierRun(String),
    #[error("Unexpected bootloader output shape: {0}")]
    OutputShape(String),
    #[error("The verifier program hash {actual:#x} does not match the pinned hash {expected:#x}.")]
    UnexpectedVerifierProgramHash { expected: Felt, actual: Felt },
    #[error("A packed root output digest half is not under 2^128.")]
    MalformedPackedOutputDigest,
    #[error(
        "The verifier's output digest {verifier_digest:?} does not match the digest expected from \
         the OS proof-fact fold {expected_digest:?}."
    )]
    FoldDigestMismatch { verifier_digest: Blake2sDigestWords, expected_digest: Blake2sDigestWords },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

/// The parsed output page of a verifier bootloader run.
#[derive(Debug, Eq, PartialEq)]
pub struct CircuitVerifierTaskOutput {
    /// The verifier program's hash under the Blake program hash function - the task's
    /// sole identity; callers must compare it against a pinned value.
    pub verifier_program_hash: Felt,
    /// The verifier's fold digest: `blake2s(circuit_hash || root output digest)` of the
    /// verified recursive proof tree.
    pub fold_digest: Blake2sDigestWords,
}

/// Runs the circuit verifier executable on `root_proof_felts` (the recursive tree's root
/// proof as the verifier's felt252 argument stream) as a single simple-bootloader task,
/// and parses the bootloader's output page.
///
/// The run aborts - and this returns [`VerifierTaskError::VerifierRun`] - when the proof
/// is invalid, and also when the verifier and proof disagree on any configuration (PCS
/// config, output arity, circuit version), which is indistinguishable from an invalid
/// proof by design of the verifier.
pub fn run_circuit_verifier_task(
    simple_bootloader_program_bytes: &[u8],
    verifier_executable_bytes: &[u8],
    root_proof_felts: &[Felt],
) -> Result<CircuitVerifierTaskOutput, VerifierTaskError> {
    // `create_cairo1_program_task` consumes files: the executable, and the arguments as
    // a JSON array of hex strings (the exact wire format the recursive tree emits).
    let temporary_directory = tempfile::tempdir()?;
    let executable_path = temporary_directory.path().join("circuit_verifier.executable.json");
    std::fs::write(&executable_path, verifier_executable_bytes)?;
    let root_proof_path = temporary_directory.path().join("root_proof.json");
    let root_proof_hex: Vec<String> =
        root_proof_felts.iter().map(|proof_felt| format!("{proof_felt:#x}")).collect();
    std::fs::write(&root_proof_path, serde_json::to_vec(&root_proof_hex)?)?;

    let verifier_task = create_cairo1_program_task(&executable_path, None, Some(root_proof_path))
        .map_err(|error| VerifierTaskError::BuildTask(format!("{error:?}")))?;
    let simple_bootloader_input = SimpleBootloaderInput {
        fact_topologies_path: None,
        single_page: true,
        tasks: vec![TaskSpec {
            task: Rc::new(verifier_task),
            program_hash_function: HashFunc::Blake,
        }],
    };

    let simple_bootloader_program =
        Program::from_bytes(simple_bootloader_program_bytes, Some("main"))
            .map_err(|error| VerifierTaskError::BuildTask(format!("{error:?}")))?;
    // The bootloader's %builtins include ecdsa and keccak, which the all_cairo_stwo
    // layout lacks; the verifier task uses neither, so missing builtins are allowed.
    let cairo_run_config =
        get_cairo_run_config(&None, LayoutName::all_cairo_stwo, false, false, true, false)?;

    let mut cairo_runner = cairo_run_program(
        &simple_bootloader_program,
        Some(ProgramInput::from_value(simple_bootloader_input)),
        cairo_run_config,
        None,
    )
    .map_err(|error| VerifierTaskError::VerifierRun(format!("{error:?}")))?;

    let mut output_buffer = String::new();
    cairo_runner
        .vm
        .write_output(&mut output_buffer)
        .map_err(|error| VerifierTaskError::OutputShape(format!("{error:?}")))?;
    let output_felts: Vec<Felt> = output_buffer
        .lines()
        .map(|output_line| {
            Felt::from_dec_str(output_line).map_err(|error| {
                VerifierTaskError::OutputShape(format!(
                    "non-felt output line {output_line:?}: {error:?}"
                ))
            })
        })
        .collect::<Result<_, _>>()?;
    parse_verifier_task_output(&output_felts)
}

/// Checks a verifier task's output against its pinned program hash and the packed root
/// output digest the OS emitted (`proof_facts_root_output_low/high` of the combined OS
/// output): the expected digest is `blake2s(multiverifier circuit hash || root output
/// digest)`, since a fold's root entry is always a multiverifier node.
pub fn verify_circuit_verifier_task_output(
    task_output: &CircuitVerifierTaskOutput,
    expected_verifier_program_hash: Felt,
    proof_facts_root_output_low: Felt,
    proof_facts_root_output_high: Felt,
) -> Result<(), VerifierTaskError> {
    if task_output.verifier_program_hash != expected_verifier_program_hash {
        return Err(VerifierTaskError::UnexpectedVerifierProgramHash {
            expected: expected_verifier_program_hash,
            actual: task_output.verifier_program_hash,
        });
    }
    let root_output_digest =
        unpack_output_digest(proof_facts_root_output_low, proof_facts_root_output_high)
            .ok_or(VerifierTaskError::MalformedPackedOutputDigest)?;
    let expected_digest = compute_fold_digest(&FoldEntry {
        circuit_hash: MULTIVERIFIER_CIRCUIT_HASH,
        output_digest: root_output_digest,
    });
    if task_output.fold_digest != expected_digest {
        return Err(VerifierTaskError::FoldDigestMismatch {
            verifier_digest: task_output.fold_digest,
            expected_digest,
        });
    }
    Ok(())
}

/// Parses the bootloader output page `[1, 10, verifier_program_hash, digest word 0..7]`.
fn parse_verifier_task_output(
    output_felts: &[Felt],
) -> Result<CircuitVerifierTaskOutput, VerifierTaskError> {
    // The page is [n_tasks, task section]: the task section is `size` (= 2 + the 8-word
    // digest), `program_hash`, and the digest words.
    if output_felts.len() != 1 + VERIFIER_TASK_OUTPUT_SIZE {
        return Err(VerifierTaskError::OutputShape(format!(
            "expected {} output felts, got {}",
            1 + VERIFIER_TASK_OUTPUT_SIZE,
            output_felts.len()
        )));
    }
    if output_felts[0] != Felt::ONE {
        return Err(VerifierTaskError::OutputShape(format!(
            "expected a single bootloader task, got {} tasks",
            output_felts[0]
        )));
    }
    if output_felts[1] != Felt::from(VERIFIER_TASK_OUTPUT_SIZE) {
        return Err(VerifierTaskError::OutputShape(format!(
            "expected a task output of size {VERIFIER_TASK_OUTPUT_SIZE}, got {}",
            output_felts[1]
        )));
    }
    let fold_digest_words: Vec<u32> = output_felts[3..]
        .iter()
        .map(|digest_word_felt| {
            u32::try_from(digest_word_felt.to_biguint()).map_err(|_| {
                VerifierTaskError::OutputShape(format!(
                    "digest word {digest_word_felt} does not fit in a u32"
                ))
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(CircuitVerifierTaskOutput {
        verifier_program_hash: output_felts[2],
        fold_digest: fold_digest_words
            .try_into()
            .expect("The digest word count is checked by the output length above."),
    })
}
