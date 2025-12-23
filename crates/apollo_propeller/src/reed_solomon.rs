use reed_solomon_simd::{ReedSolomonDecoder, ReedSolomonEncoder};

// TODO(AndrewL): Consider combining this with `generate_coding_shards`.
// TODO(AndrewL): Consider adding custom error type and using it here.
pub fn split_data_into_shards(message: Vec<u8>, num_data_shards: usize) -> Option<Vec<Vec<u8>>> {
    if !message.len().is_multiple_of(num_data_shards) {
        return None;
    }
    let shard_size = message.len() / num_data_shards;
    Some(message.chunks_exact(shard_size).map(|chunk| chunk.to_vec()).collect())
}

/// Generate coding shards using Reed-Solomon encoding.
// TODO(AndrewL): Consider adding custom error type and using it here.
pub fn generate_coding_shards(
    data_shards: &[Vec<u8>],
    num_coding_shards: usize,
) -> Result<Vec<Vec<u8>>, String> {
    if num_coding_shards == 0 {
        // ReedSolomonEncoder does not support 0 coding shards
        return Ok(Vec::new());
    }

    let num_data_shards = data_shards.len();
    // TODO(AndrewL): Consider accepting a shard size as an argument.
    let shard_size = data_shards.first().ok_or("No data shards".to_string())?.len();

    let mut encoder = ReedSolomonEncoder::new(num_data_shards, num_coding_shards, shard_size)
        .map_err(|e| format!("Failed to create Reed-Solomon encoder: {}", e))?;

    for shard in data_shards.iter().take(num_data_shards) {
        encoder
            .add_original_shard(shard)
            .map_err(|e| format!("Failed to add data shard: {}", e))?;
    }

    let result = encoder.encode().map_err(|e| format!("Failed to encode: {}", e))?;

    let coding_shards = result.recovery_iter().map(|shard| shard.to_vec()).collect();

    Ok(coding_shards)
}

/// Reconstruct the original message from available shards using Reed-Solomon error correction.
// TODO(AndrewL): Consider adding custom error type and using it here.
// TODO(AndrewL): Rename this to `reconstruct_data_shards`.
pub fn reconstruct_message_from_shards(
    // TODO(AndrewL): Change this to a HashMap<usize, Vec<u8>>.
    shards: &[(usize, Vec<u8>)],
    num_data_shards: usize,
    num_coding_shards: usize,
) -> Result<Vec<Vec<u8>>, String> {
    if num_coding_shards == 0 {
        return Ok(shards.iter().map(|(_, s)| s.to_vec()).collect());
    }
    // TODO(AndrewL): Consider accepting a shard size as an argument.
    let shard_size = shards.first().ok_or("No shards".to_string())?.1.len();

    let mut decoder = ReedSolomonDecoder::new(num_data_shards, num_coding_shards, shard_size)
        .map_err(|e| format!("Failed to create Reed-Solomon decoder: {}", e))?;

    for (index, shard_data) in shards {
        if *index < num_data_shards {
            decoder
                .add_original_shard(*index, shard_data)
                .map_err(|e| format!("Failed to add original shard: {}", e))?;
        } else {
            decoder
                .add_recovery_shard(index - num_data_shards, shard_data)
                .map_err(|e| format!("Failed to add coding shard: {}", e))?;
        }
    }

    let result = decoder.decode().map_err(|e| format!("Failed to decode: {}", e))?;

    let mut shard_map = std::collections::HashMap::new();
    for (index, shard) in shards {
        shard_map.insert(index, shard);
    }

    let mut data_shards = Vec::with_capacity(num_data_shards);
    for index in 0..num_data_shards {
        if let Some(shard_shard) = shard_map.get(&index) {
            data_shards.push(shard_shard.to_vec());
        } else if let Some(restored_data) = result.restored_original(index) {
            data_shards.push(restored_data.to_vec());
        } else {
            return Err(format!(
                "Missing data shard at index {} and no restored data available",
                index
            ));
        }
    }

    Ok(data_shards)
}

pub fn combine_data_shards(data_shards: Vec<Vec<u8>>) -> Vec<u8> {
    data_shards.iter().flatten().copied().collect()
}
