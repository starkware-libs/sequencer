use rstest::rstest;

use crate::reed_solomon::{generate_coding_shards, reconstruct_message_from_shards};

#[test]
fn test_empty_generate_coding_shards() {
    let data_shards = vec![vec![0, 1], vec![2, 3], vec![4, 5]];
    let num_coding_shards = 0;
    let coding_shards = generate_coding_shards(&data_shards, num_coding_shards).unwrap();
    assert!(coding_shards.is_empty());
}

#[rstest]
#[case(3, 2, 1, 8)]
#[case(4, 2, 2, 16)]
#[case(5, 3, 3, 8)]
#[case(4, 4, 4, 32)]
fn test_reed_solomon_with_lost_shards(
    #[case] num_data_shards: usize,
    #[case] num_coding_shards: usize,
    #[case] num_lost_shards: usize,
    #[case] shard_size: usize,
) {
    let data_shards: Vec<Vec<u8>> =
        (0..num_data_shards.try_into().unwrap()).map(|i| vec![i; shard_size]).collect();
    let original_data = data_shards.clone();

    let coding_shards = generate_coding_shards(&data_shards, num_coding_shards).unwrap();
    assert_eq!(coding_shards.len(), num_coding_shards);

    let all_shards: Vec<(usize, Vec<u8>)> = data_shards
        .iter()
        .enumerate()
        .map(|(i, s)| (i, s.clone()))
        .chain(coding_shards.iter().enumerate().map(|(i, s)| (num_data_shards + i, s.clone())))
        .collect();

    let available_shards: Vec<(usize, Vec<u8>)> = all_shards
        .into_iter()
        .enumerate()
        .filter(|(i, _)| *i % 2 != 0 || *i >= num_lost_shards * 2)
        .map(|(_, shard)| shard)
        .collect();

    assert!(
        available_shards.len() >= num_data_shards,
        "Not enough shards to reconstruct (have {}, need {})",
        available_shards.len(),
        num_data_shards
    );

    let reconstructed_data =
        reconstruct_message_from_shards(&available_shards, num_data_shards, num_coding_shards)
            .unwrap();

    assert_eq!(reconstructed_data, original_data, "Reconstructed data doesn't match original");
}
