use starknet_api::transaction::fields::PROOF_VERSION;
use starknet_types_core::felt::Felt;

use crate::proof_verification::{reconstruct_output_preimage, ProgramOutput};

/// Verifies that converting ProgramOutput -> ProofFacts -> output preimage produces the
/// original program output.
#[test]
fn roundtrip_program_output_to_proof_facts_and_back() {
    let program_hash = Felt::from(0xABCD_u64);
    let task_output = [Felt::from(1_u64), Felt::from(2_u64), Felt::from(3_u64)];
    let task_content_len = 1 + task_output.len(); // program_hash + task_output
    let output_size = Felt::from((task_content_len + 1) as u64); // includes itself

    let original_output: Vec<Felt> = [Felt::ONE, output_size, program_hash]
        .into_iter()
        .chain(task_output)
        .collect();
    let program_output = ProgramOutput::from(original_output.clone());

    let program_variant = Felt::from(0x42_u64);
    let proof_facts = program_output.try_into_proof_facts(program_variant).unwrap();

    // Verify the proof facts structure: [PROOF_VERSION, variant, program_hash, ...task_output].
    assert_eq!(proof_facts.0[0], PROOF_VERSION);
    assert_eq!(proof_facts.0[1], program_variant);
    assert_eq!(proof_facts.0[2], program_hash);
    assert_eq!(&proof_facts.0[3..], &task_output);

    let reconstructed = reconstruct_output_preimage(&proof_facts).unwrap();
    assert_eq!(reconstructed, original_output);
}
