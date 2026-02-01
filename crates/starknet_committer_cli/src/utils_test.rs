use std::time::Duration;

use starknet_committer::block_committer::measurements_util::{
    Action,
    BlockModificationsCounts,
    MeasurementsTrait,
};
use tokio::time::sleep;

use crate::utils::BenchmarkMeasurements;

const READ_DURATION: u64 = 100;
const COMPUTE_DURATION: u64 = 100;
const WRITE_DURATION: u64 = 100;
const N_READ_ENTRIES: usize = 100;
const N_WRITE_ENTRIES: usize = 100;
const N_MODIFICATIONS: usize = 100;
const N_EMPTY_LEAVES: usize = 10;

async fn measure_block(measurements: &mut BenchmarkMeasurements) {
    measurements.set_number_of_modifications(BlockModificationsCounts {
        storage_tries: N_MODIFICATIONS,
        contracts_trie: N_MODIFICATIONS,
        classes_trie: N_MODIFICATIONS,
        emptied_storage_leaves: N_EMPTY_LEAVES,
    });
    measurements.start_measurement(Action::EndToEnd);
    measurements.start_measurement(Action::Read);
    sleep(Duration::from_millis(READ_DURATION)).await;
    measurements.attempt_to_stop_measurement(Action::Read, N_READ_ENTRIES).unwrap();
    measurements.start_measurement(Action::Compute);
    sleep(Duration::from_millis(COMPUTE_DURATION)).await;
    measurements.attempt_to_stop_measurement(Action::Compute, 0).unwrap();
    measurements.start_measurement(Action::Write);
    sleep(Duration::from_millis(WRITE_DURATION)).await;
    measurements.attempt_to_stop_measurement(Action::Write, N_WRITE_ENTRIES).unwrap();
    measurements.attempt_to_stop_measurement(Action::EndToEnd, 0).unwrap();
}

fn assert_block_measurement(measurements: &BenchmarkMeasurements, number_of_blocks: usize) {
    assert_eq!(
        measurements.total_time,
        measurements
            .block_measurements
            .iter()
            .map(|measurement| measurement.durations.block)
            .sum::<u128>()
    );
    assert_eq!(measurements.block_measurements.len(), number_of_blocks);
    assert_eq!(measurements.block_number, number_of_blocks);
    assert_eq!(measurements.total_db_entry_count, N_WRITE_ENTRIES * number_of_blocks);

    for (i, (measurement, db_entry_count)) in measurements
        .block_measurements
        .iter()
        .zip(measurements.initial_db_entry_count.iter())
        .enumerate()
    {
        assert!(
            measurement.durations.block
                >= u128::from(READ_DURATION + COMPUTE_DURATION + WRITE_DURATION)
        );
        assert!(measurement.durations.read >= u128::from(READ_DURATION));
        assert!(measurement.durations.compute >= u128::from(COMPUTE_DURATION));
        assert!(measurement.durations.write >= u128::from(WRITE_DURATION));
        assert_eq!(measurement.n_writes, N_WRITE_ENTRIES);
        assert_eq!(measurement.n_reads, N_READ_ENTRIES);
        assert_eq!(*db_entry_count, N_WRITE_ENTRIES * i);
        assert_eq!(
            measurement.modifications_counts,
            BlockModificationsCounts {
                storage_tries: N_MODIFICATIONS,
                contracts_trie: N_MODIFICATIONS,
                classes_trie: N_MODIFICATIONS,
                emptied_storage_leaves: N_EMPTY_LEAVES,
            }
        );
    }
}

#[tokio::test]
async fn test_benchmark_block_measurement() {
    let number_of_blocks = 3;
    let mut measurements = BenchmarkMeasurements::new(number_of_blocks, vec![]);
    for _ in 0..number_of_blocks {
        measure_block(&mut measurements).await;
    }
    assert_block_measurement(&measurements, number_of_blocks);
}
