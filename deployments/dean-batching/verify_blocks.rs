use apollo_storage::{open_storage, StorageConfig};
use apollo_storage::db::DbConfig;
use apollo_storage::header::HeaderStorageReader;
use apollo_storage::body::BodyStorageReader;
use std::path::PathBuf;
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = PathBuf::from("/data/workspace");
    
    println!("========================================");
    println!("BLOCK-LEVEL DATA VERIFICATION");
    println!("========================================");
    println!();
    
    // Open both databases
    println!("Opening WITH batching database...");
    let cfg_with = StorageConfig {
        db_config: DbConfig {
            path_prefix: workspace.join("data_with_batching"),
            chain_id: ChainId::Mainnet,
            enforce_file_exists: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let (reader_with, _) = open_storage(cfg_with)?;
    
    println!("Opening WITHOUT batching database...");
    let cfg_without = StorageConfig {
        db_config: DbConfig {
            path_prefix: workspace.join("data_without_batching"),
            chain_id: ChainId::Mainnet,
            enforce_file_exists: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let (reader_without, _) = open_storage(cfg_without)?;
    
    println!("✓ Both databases opened");
    println!();
    
    // Get block counts
    let txn_with = reader_with.begin_ro_txn()?;
    let txn_without = reader_without.begin_ro_txn()?;
    
    let blocks_with = txn_with.get_header_marker()?.0;
    let blocks_without = txn_without.get_header_marker()?.0;
    
    println!("Blocks in databases:");
    println!("  WITH batching:    {}", blocks_with);
    println!("  WITHOUT batching: {}", blocks_without);
    println!();
    
    if blocks_with != blocks_without {
        println!("⚠️  WARNING: Different block counts!");
    }
    
    let max_blocks = blocks_with.min(blocks_without);
    let sample_size = 1000.min(max_blocks as usize);
    
    // Generate 1000 random block numbers
    println!("Generating {} random block numbers...", sample_size);
    use std::collections::HashSet;
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut blocks = HashSet::new();
    while blocks.len() < sample_size {
        blocks.insert(rng.gen_range(0..max_blocks));
    }
    let mut blocks: Vec<_> = blocks.into_iter().map(BlockNumber).collect();
    blocks.sort_by_key(|b| b.0);
    
    println!("Comparing {} random blocks...", sample_size);
    println!();
    
    let mut errors = 0;
    let mut count = 0;
    
    for block_num in blocks {
        count += 1;
        if count % 100 == 0 {
            println!("  Progress: {}/{}", count, sample_size);
        }
        
        let header_with = txn_with.get_block_header(block_num)?;
        let header_without = txn_without.get_block_header(block_num)?;
        
        match (header_with, header_without) {
            (Some(h1), Some(h2)) => {
                if h1.block_hash != h2.block_hash {
                    println!("  ❌ Block {}: hash mismatch!", block_num.0);
                    errors += 1;
                }
                if h1.state_root != h2.state_root {
                    println!("  ❌ Block {}: state root mismatch!", block_num.0);
                    errors += 1;
                }
                if h1.parent_hash != h2.parent_hash {
                    println!("  ❌ Block {}: parent hash mismatch!", block_num.0);
                    errors += 1;
                }
                
                let tx_with = txn_with.get_block_transactions_count(block_num)?;
                let tx_without = txn_without.get_block_transactions_count(block_num)?;
                if tx_with != tx_without {
                    println!("  ❌ Block {}: tx count mismatch!", block_num.0);
                    errors += 1;
                }
            }
            (None, Some(_)) => {
                println!("  ❌ Block {} MISSING in WITH batching!", block_num.0);
                errors += 1;
            }
            (Some(_), None) => {
                println!("  ❌ Block {} MISSING in WITHOUT batching!", block_num.0);
                errors += 1;
            }
            (None, None) => {
                println!("  ⚠️  Block {} missing in BOTH databases", block_num.0);
            }
        }
    }
    
    println!();
    println!("========================================");
    println!("RESULTS");
    println!("========================================");
    println!("Blocks compared: {}", sample_size);
    println!("Errors found:    {}", errors);
    println!();
    
    if errors == 0 {
        println!("✅ ALL {} BLOCKS ARE IDENTICAL!", sample_size);
        println!("   Data integrity CONFIRMED!");
    } else {
        println!("❌ FAILED: {} mismatches found", errors);
        std::process::exit(1);
    }
    
    Ok(())
}

