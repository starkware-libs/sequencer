//! Tests for Reed-Solomon Forward Error Correction functionality.

#![allow(clippy::as_conversions)]

use apollo_propeller::reed_solomon;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

fn generate_random_message<R: Rng>(rng: &mut R, message_size: usize) -> Vec<u8> {
    (0..message_size).map(|_| rng.gen()).collect()
}

fn lose_random_shards(
    shards: &mut Vec<(usize, Vec<u8>)>,
    rng: &mut impl Rng,
    data_shards_count: usize,
) {
    shards.shuffle(rng);
    let shards_to_keep = rng.gen_range(data_shards_count..shards.len());
    shards.truncate(shards_to_keep);
}

fn roundtrip_test(
    rng: &mut impl Rng,
    data_shards_count: usize,
    coding_shards_count: usize,
    message_size: usize,
) -> usize {
    let message = generate_random_message(rng, message_size);

    let data_shards =
        reed_solomon::split_data_into_shards(message.clone(), data_shards_count).unwrap();
    let coding_shards =
        reed_solomon::generate_coding_shards(&data_shards, coding_shards_count).unwrap();
    let all_shards = [data_shards, coding_shards].concat();
    let mut all_shards: Vec<(usize, Vec<u8>)> = all_shards.into_iter().enumerate().collect();

    lose_random_shards(&mut all_shards, rng, data_shards_count);

    let reconstructed_message_data_shards = reed_solomon::reconstruct_message_from_shards(
        &all_shards,
        data_shards_count,
        coding_shards_count,
    )
    .unwrap();
    let reconstructed_message =
        reed_solomon::combine_data_shards(reconstructed_message_data_shards);
    assert_eq!(
        reconstructed_message, message,
        "Reconstructed message does not match original message"
    );

    reconstructed_message.len()
}

#[test]
fn test_reed_solomon_fec_generation() {
    const ITERATIONS: u64 = 2_000;
    for seed in 0..ITERATIONS {
        if seed % 100 == 0 {
            println!("Progress: {}/{}", seed, ITERATIONS);
        }
        let mut rng = StdRng::seed_from_u64(seed);
        let data_shards_count = rng.gen_range(1..10);
        let coding_shards_count = rng.gen_range(1..20);
        let message_size = data_shards_count * 2 * rng.gen_range(1..100);
        roundtrip_test(&mut rng, data_shards_count, coding_shards_count, message_size);
    }
}

/// Reed-solomon encoding and decoding performance test.
/// average message size -> MB/s:
///
/// Run with `cargo test --release --package apollo_propeller --test fuzz_reed_solomon -- --ignored
/// --exact --nocapture test_reed_solomon_fec_performance`:
///
/// ```md
/// Message Size    | Throughput   | Latency      | Throughput/Latency  
/// (bytes)         | (MB/s)       | (ms)         | (MB/s/ms)           
/// ----------------+--------------+--------------+---------------------
/// 66              | 0.238        | 0.265        | 0.899               
/// 660             | 2.353        | 0.267        | 8.798               
/// 6600            | 21.941       | 0.287        | 76.485              
/// 66000           | 120.488      | 0.522        | 230.646             
/// 132000          | 157.939      | 0.797        | 198.154             
/// 198000          | 178.725      | 1.057        | 169.163             
/// 264000          | 183.087      | 1.375        | 133.141             
/// 330000          | 195.955      | 1.606        | 122.011             
/// 660000          | 189.394      | 3.323        | 56.988              
/// 1320000         | 171.819      | 7.327        | 23.451              
/// ```
#[test]
#[ignore]
fn test_reed_solomon_fec_performance() {
    const ITERATIONS_PER_SIZE: u64 = 1000;
    println!(
        "\n{:<15} | {:<12} | {:<12} | {:<20}",
        "Message Size", "Throughput", "Latency", "Throughput/Latency"
    );
    println!("{:<15} | {:<12} | {:<12} | {:<20}", "(bytes)", "(MB/s)", "(ms)", "(MB/s/ms)");
    println!("{:-<15}-+-{:-<12}-+-{:-<12}-+-{:-<20}", "", "", "", "");

    for message_multiplier in [1, 10, 100, 1000, 2000, 3000, 4000, 5000, 10_000, 20_000] {
        let mut total_size = 0;
        let start_time = std::time::Instant::now();
        for seed in 0..ITERATIONS_PER_SIZE {
            let mut rng = StdRng::seed_from_u64(seed);
            let data_shards_count = 33;
            let coding_shards_count = 66;
            let message_size = data_shards_count * 2 * message_multiplier;
            total_size +=
                roundtrip_test(&mut rng, data_shards_count, coding_shards_count, message_size);
        }
        let elapsed_seconds = start_time.elapsed().as_secs_f64();
        let message_size = total_size / ITERATIONS_PER_SIZE as usize;
        let throughput = total_size as f64 / elapsed_seconds / 1024_f64 / 1024_f64;
        let latency = elapsed_seconds / ITERATIONS_PER_SIZE as f64 * 1000.0;
        let throughput_over_latency = throughput / latency;
        println!(
            "{:<15} | {:<12.3} | {:<12.3} | {:<20.3}",
            message_size, throughput, latency, throughput_over_latency
        );
    }
}
