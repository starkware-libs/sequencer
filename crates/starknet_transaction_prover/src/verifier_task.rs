use std::rc::Rc;

use cairo_program_runner_lib::cairo_run_program;
use cairo_program_runner_lib::hints::types::{HashFunc, SimpleBootloaderInput, TaskSpec};
use cairo_program_runner_lib::tasks::create_cairo1_program_task;
use cairo_program_runner_lib::utils::{get_cairo_run_config, ProgramInput};
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::program::Program;
use starknet_os::proof_fact_fold::{
    compute_verification_digest,
    unpack_output_digest,
    Blake2sDigestWords,
    BLAKE2S_DIGEST_N_WORDS,
};
use starknet_types_core::felt::Felt;

#[cfg(any(test, feature = "testing"))]
pub mod test_utils;

#[cfg(test)]
#[path = "verifier_task_test.rs"]
mod verifier_task_test;

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
    #[error("A packed output digest half is not under 2^128.")]
    MalformedPackedOutputDigest,
    #[error(
        "The verifier's output digest {verifier_digest:?} does not match the verification digest \
         expected from the OS output {expected_digest:?}."
    )]
    VerificationDigestMismatch {
        verifier_digest: Blake2sDigestWords,
        expected_digest: Blake2sDigestWords,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

#[derive(Debug, Eq, PartialEq)]
pub struct CircuitVerifierTaskOutput {
    pub verifier_program_hash: Felt,
    pub verification_digest: Blake2sDigestWords,
}

pub fn run_circuit_verifier_task(
    simple_bootloader_program_bytes: &[u8],
    verifier_executable_bytes: &[u8],
    processed_proof_felts: &[Felt],
) -> Result<CircuitVerifierTaskOutput, VerifierTaskError> {
    let temporary_directory = tempfile::tempdir()?;
    let executable_path = temporary_directory.path().join("circuit_verifier.executable.json");
    std::fs::write(&executable_path, verifier_executable_bytes)?;
    let processed_proof_path = temporary_directory.path().join("processed_proof.json");
    let processed_proof_hex: Vec<String> =
        processed_proof_felts.iter().map(|proof_felt| format!("{proof_felt:#x}")).collect();
    std::fs::write(&processed_proof_path, serde_json::to_vec(&processed_proof_hex)?)?;

    let verifier_task =
        create_cairo1_program_task(&executable_path, None, Some(processed_proof_path))
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

pub fn verify_circuit_verifier_task_output(
    task_output: &CircuitVerifierTaskOutput,
    expected_verifier_program_hash: Felt,
    processed_proof_output_low: Felt,
    processed_proof_output_high: Felt,
) -> Result<(), VerifierTaskError> {
    if task_output.verifier_program_hash != expected_verifier_program_hash {
        return Err(VerifierTaskError::UnexpectedVerifierProgramHash {
            expected: expected_verifier_program_hash,
            actual: task_output.verifier_program_hash,
        });
    }
    let processed_proof_output_digest =
        unpack_output_digest(processed_proof_output_low, processed_proof_output_high)
            .ok_or(VerifierTaskError::MalformedPackedOutputDigest)?;
    let expected_digest = compute_verification_digest(&processed_proof_output_digest);
    if task_output.verification_digest != expected_digest {
        return Err(VerifierTaskError::VerificationDigestMismatch {
            verifier_digest: task_output.verification_digest,
            expected_digest,
        });
    }
    Ok(())
}

fn parse_verifier_task_output(
    output_felts: &[Felt],
) -> Result<CircuitVerifierTaskOutput, VerifierTaskError> {
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
    let verification_digest_words: Vec<u32> = output_felts[3..]
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
        verification_digest: verification_digest_words
            .try_into()
            .expect("The digest word count is checked by the output length above."),
    })
}
