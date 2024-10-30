// Define custom serializer to sort by keys.
fn serialize_sorted<S, K, V>(map: &IndexMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    K: Ord + Serialize,
    V: Serialize,
{
    // Convert the IndexMap to BTreeMap, which automatically sorts keys.
    let sorted_map: BTreeMap<_, _> = map.iter().collect();
    sorted_map.serialize(serializer)
}

// Custom serializer for nested `IndexMap` with different key types.
fn serialize_nested_sorted<S, OuterK, InnerK, V>(
    map: &IndexMap<OuterK, IndexMap<InnerK, V>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    OuterK: Ord + Serialize,
    InnerK: Ord + Serialize,
    V: Serialize,
{
    // Sort each inner `IndexMap` and store in a `BTreeMap` for serialization.
    let sorted_map: BTreeMap<_, BTreeMap<_, _>> = map
        .iter()
        .map(|(outer_key, inner_map)| {
            let sorted_inner_map: BTreeMap<_, _> = inner_map.iter().collect();
            (outer_key, sorted_inner_map)
        })
        .collect();

    sorted_map.serialize(serializer)
}
