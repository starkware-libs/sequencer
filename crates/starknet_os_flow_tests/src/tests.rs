use blockifier::test_utils::dict_state_reader::DictStateReader;
use rstest::rstest;
use tokio::test as async_test;

use crate::initial_state::create_default_initial_state_data;
use crate::utils::divide_vec_into_n_parts;

#[async_test]
async fn test_initial_state_creation() {
    let _initial_state = create_default_initial_state_data::<DictStateReader>().await;
}

#[rstest]
#[case(10, 2, vec![5, 5])]
#[case(10, 3, vec![4, 3, 3])]
#[case(10, 4, vec![3, 3, 2, 2])]
#[case(8, 5, vec![2, 2, 2, 1, 1])]
#[case(0, 1, vec![0])]
#[case(0, 2, vec![0, 0])]
#[case(1, 1, vec![1])]
#[case(12, 7, vec![2, 2, 2, 2, 2, 1, 1])]
fn division(#[case] length: usize, #[case] n_parts: usize, #[case] expected_lengths: Vec<usize>) {
    let to_divide = vec![0; length];
    let divided = divide_vec_into_n_parts(to_divide, n_parts);
    let actual_lengths: Vec<usize> = divided.iter().map(|part| part.len()).collect();
    assert_eq!(actual_lengths, expected_lengths);
}
