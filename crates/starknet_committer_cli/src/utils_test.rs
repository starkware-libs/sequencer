use std::time::Duration;

use starknet_committer::block_committer::timing_util::{Action, TimeMeasurementTrait};
use tokio::time::sleep;

use crate::utils::BenchmarkTimeMeasurement;

const READ_DURATION: u64 = 100;
const COMPUTE_DURATION: u64 = 100;
const WRITE_DURATION: u64 = 100;
const N_READ_FACTS: usize = 100;
const N_NEW_FACTS: usize = 100;

async fn measure_block(btm: &mut BenchmarkTimeMeasurement) {
    btm.start_measurement(Action::EndToEnd);
    btm.start_measurement(Action::Read);
    sleep(Duration::from_millis(READ_DURATION)).await;
    btm.stop_measurement(Action::Read, N_READ_FACTS);
    btm.start_measurement(Action::Compute);
    sleep(Duration::from_millis(COMPUTE_DURATION)).await;
    btm.stop_measurement(Action::Compute, 0);
    btm.start_measurement(Action::Write);
    sleep(Duration::from_millis(WRITE_DURATION)).await;
    btm.stop_measurement(Action::Write, N_NEW_FACTS);
    btm.stop_measurement(Action::EndToEnd, 0);
}

#[allow(clippy::as_conversions)]
fn assert_block_time_measurement(btm: &BenchmarkTimeMeasurement, number_of_blocks: usize) {
    assert!(
        btm.total_time
            >= u128::from(READ_DURATION + COMPUTE_DURATION + WRITE_DURATION)
                * number_of_blocks as u128
    );
    assert_eq!(btm.block_measurements.len(), number_of_blocks);
    assert_eq!(btm.block_number, number_of_blocks);
    assert_eq!(btm.total_facts, N_NEW_FACTS * number_of_blocks);

    for (measurement, facts_count) in
        btm.block_measurements.iter().zip(btm.initial_facts_in_db.iter())
    {
        assert!(
            measurement.block_duration
                >= u128::from(READ_DURATION + COMPUTE_DURATION + WRITE_DURATION)
        );
        assert!(measurement.read_duration >= u128::from(READ_DURATION));
        assert!(measurement.compute_duration >= u128::from(COMPUTE_DURATION));
        assert!(measurement.write_duration >= u128::from(WRITE_DURATION));
        assert_eq!(measurement.n_new_facts, N_NEW_FACTS);
        assert_eq!(measurement.n_read_facts, N_READ_FACTS);
        assert_eq!(*facts_count, N_NEW_FACTS * (number_of_blocks - 1));
    }
}

#[tokio::test]
async fn test_benchmark_time_measurement() {
    let number_of_blocks = 3;
    let mut btm = BenchmarkTimeMeasurement::new(number_of_blocks, vec![]);
    for _ in 0..number_of_blocks {
        measure_block(&mut btm).await;
    }
    assert_block_time_measurement(&btm, number_of_blocks);
}
