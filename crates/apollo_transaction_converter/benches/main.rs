//! Benchmark module for the apollo_transaction_converter crate.
//!
//! Run the benchmarks using `cargo bench -p apollo_transaction_converter --features testing`.

use std::time::Instant;

use apollo_transaction_converter::proof_verification::stwo_verify;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use proving_utils::proof_encoding::ProofBytes;
use starknet_api::test_utils::path_in_resources;
use starknet_api::transaction::fields::Proof;

/// Resource file name for testing.
const EXAMPLE_PROOF_FILE: &str = "example_proof.bz2";

fn stwo_verify_benchmark(c: &mut Criterion) {
    // Load proof once during setup.
    let start = Instant::now();
    let proof_path = path_in_resources(EXAMPLE_PROOF_FILE);
    let proof_bytes = ProofBytes::from_file(&proof_path).expect("Failed to load example_proof.bz2");
    let proof: Proof = proof_bytes.into();
    println!("Proof loading took: {:?}", start.elapsed());

    c.bench_function("stwo_verify", |b| {
        b.iter_batched(|| proof.clone(), |proof| stwo_verify(proof), BatchSize::SmallInput)
    });
}

criterion_group!(benches, stwo_verify_benchmark);
criterion_main!(benches);
