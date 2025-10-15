use crate::size_of_serialized;

#[test]
fn test_size_of_serialized() {
    let value = serde_json::json!({
        "a": 1,
        "b": "hello",
        "c": [1, 2, 3],
    });

    let size = size_of_serialized(&value).unwrap();
    let serialized_size = serde_json::to_vec(&value).unwrap().len();

    assert_eq!(size, serialized_size);
    assert_eq!(size, 31);
}
