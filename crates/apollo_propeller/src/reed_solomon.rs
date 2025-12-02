use reed_solomon_simd::{ReedSolomonDecoder, ReedSolomonEncoder};

pub fn split_data_into_shards(message: Vec<u8>, data_shard_count: usize) -> Option<Vec<Vec<u8>>> {
    if message.len() % data_shard_count != 0 {
        return None;
    }
    let shard_size = message.len() / data_shard_count;
    Some(message.chunks_exact(shard_size).map(|chunk| chunk.to_vec()).collect())
}

/// Generate coding shards using Reed-Solomon encoding.
pub fn generate_coding_shards(
    data_shards: &[Vec<u8>],
    coding_count: usize,
) -> Result<Vec<Vec<u8>>, String> {
    if coding_count == 0 {
        return Ok(Vec::new());
    }

    let data_count = data_shards.len();

    // Get shard size from the first data shard (all data shards should be the same size)
    let shard_size = data_shards.first().ok_or("No data shards".to_string())?.len();

    // Create Reed-Solomon encoder
    let mut encoder = ReedSolomonEncoder::new(data_count, coding_count, shard_size)
        .map_err(|e| format!("Failed to create Reed-Solomon encoder: {}", e))?;

    // Add data shards (all should be the same size)
    for shard in data_shards.iter().take(data_count) {
        encoder
            .add_original_shard(shard)
            .map_err(|e| format!("Failed to add data shard: {}", e))?;
    }

    // Perform Reed-Solomon encoding
    let result = encoder.encode().map_err(|e| format!("Failed to encode: {}", e))?;

    let coding_shards = result.recovery_iter().map(|shard| shard.to_vec()).collect();

    Ok(coding_shards)
}

/// Reconstruct the original message from available shards using Reed-Solomon error correction.
pub fn reconstruct_message_from_shards(
    shards: &[(usize, Vec<u8>)],
    data_count: usize,
    coding_count: usize,
) -> Result<Vec<Vec<u8>>, String> {
    if coding_count == 0 {
        return Ok(shards.iter().map(|(_, s)| s.to_vec()).collect());
    }
    let shard_size = shards.first().ok_or("No shards".to_string())?.1.len();

    // Create Reed-Solomon decoder
    let mut decoder = ReedSolomonDecoder::new(data_count, coding_count, shard_size)
        .map_err(|e| format!("Failed to create Reed-Solomon decoder: {}", e))?;

    // Add available shards to decoder in index order
    for (index, shard_data) in shards {
        if *index < data_count {
            decoder
                .add_original_shard(*index, shard_data)
                .map_err(|e| format!("Failed to add original shard: {}", e))?;
        } else {
            decoder
                .add_recovery_shard(index - data_count, shard_data)
                .map_err(|e| format!("Failed to add coding shard: {}", e))?;
        }
    }

    // Perform Reed-Solomon decoding to reconstruct missing data shards
    let result = decoder.decode().map_err(|e| format!("Failed to decode: {}", e))?;

    let mut shard_map = std::collections::HashMap::new();
    for (index, shard) in shards {
        shard_map.insert(index, shard);
    }

    // Combine the reconstructed data shards to form the original message
    let mut data_shards = Vec::new();
    for index in 0..data_count {
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
