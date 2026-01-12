#!/usr/bin/env rust-script

use std::path::PathBuf;
use apollo_storage::{open_storage, StorageConfig, db::DbConfig};
use apollo_storage::header::HeaderStorageReader;
use apollo_storage::body::BodyStorageReader;
use apollo_storage::state::StateStorageReader;
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use clap::{Parser, Subcommand};
use anyhow::Result;

/// Tool to verify storage integrity by querying blocks directly from the database.
#[derive(Parser)]
#[clap(name = "verify_storage_direct")]
#[clap(about = "Query blocks directly from storage without RPC", long_about = None)]
struct Cli {
    /// Path to the storage directory (e.g., ./data or your PVC mount path).
    #[clap(short, long, default_value = "./data")]
    storage_path: PathBuf,
    
    /// Chain ID (mainnet, sepolia-testnet, etc.).
    #[clap(short, long, default_value = "SN_MAIN")]
    chain_id: String,
    
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Get the latest synced block number.
    Latest,
    
    /// Get block header for a specific block number.
    Block {
        /// Block number to query
        block_number: u64,
    },
    
    /// Get state diff for a specific block number.
    StateDiff {
        /// Block number to query
        block_number: u64,
    },
    
    /// Get transaction count for a specific block.
    TxCount {
        /// Block number to query
        block_number: u64,
    },
    
    /// Verify random blocks.
    Verify {
        /// Number of random blocks to test
        #[clap(default_value = "10")]
        count: u64,
    },
    
    /// Get storage statistics.
    Stats,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Parse chain ID.
    let chain_id = match cli.chain_id.as_str() {
        "SN_MAIN" => ChainId::Mainnet,
        "SN_SEPOLIA" => ChainId::Sepolia,
        _ => {
            eprintln!("Unknown chain ID: {}. Using Mainnet.", cli.chain_id);
            ChainId::Mainnet
        }
    };
    
    // Configure storage.
    let db_config = DbConfig {
        path_prefix: cli.storage_path,
        chain_id,
        enforce_file_exists: true, // Make sure storage exists.
        min_size: 1 << 20,          // 1MB
        max_size: 1 << 40,          // 1TB
        growth_step: 1 << 32,       // 4GB
        max_readers: 1 << 13,       // 8K readers
    };
    
    let storage_config = StorageConfig {
        db_config,
        ..Default::default()
    };
    
    // Open storage (read-only).
    println!("Opening storage at:{:?}", storage_config.db_config.path_prefix);
    let (reader, _) = open_storage(storage_config)?;
    println!("Storage opened successfully.");
    
    // Execute command.
    match cli.command {
        Commands::Latest => {
            let txn = reader.begin_ro_txn()?;
            let header_marker = txn.get_header_marker()?;
            let body_marker = txn.get_body_marker()?;
            let state_marker = txn.get_state_marker()?;
            
            println!("Latest synced blocks:");
            println!("Header marker:{} (last header is {})", header_marker, header_marker.0.saturating_sub(1));
            println!("Body marker:{} (last body is {})", body_marker, body_marker.0.saturating_sub(1));
            println!("State marker:{} (last state is {})", state_marker, state_marker.0.saturating_sub(1));
            
            let latest = header_marker.0.saturating_sub(1);
            if latest > 0 {
                println!("Latest synced block:{}", latest);
            }
        }
        
        Commands::Block { block_number } => {
            let txn = reader.begin_ro_txn()?;
            let block_num = BlockNumber(block_number);
            
            match txn.get_block_header(block_num)? {
                Some(header) => {
                    println!("Block #{}", block_number);
                    println!("Block hash:{:?}", header.block_hash);
                    println!("Parent hash:{:?}", header.block_header_without_hash.parent_hash);
                    println!("Timestamp:{:?}", header.block_header_without_hash.timestamp);
                    println!("Sequencer:{:?}", header.block_header_without_hash.sequencer);
                    println!("State root:{:?}", header.block_header_without_hash.state_root);
                    println!("L1 gas price:{:?}", header.block_header_without_hash.l1_gas_price);
                    println!("TX count:{}", header.n_transactions);
                    println!("Events:{}", header.n_events);
                    println!("Block found");
                }
                None => {
                    println!("Block {} not found in storage", block_number);
                }
            }
        }
        
        Commands::StateDiff { block_number } => {
            let txn = reader.begin_ro_txn()?;
            let block_num = BlockNumber(block_number);
            
            match txn.get_state_diff(block_num)? {
                Some(state_diff) => {
                    println!("State diff for block #{}", block_number);
                    println!("Deployed contracts:{}", state_diff.deployed_contracts.len());
                    println!("Storage diffs:{}", state_diff.storage_diffs.len());
                    println!("Declared classes:{}", state_diff.declared_classes.len());
                    println!("Deprecated declared:{}", state_diff.deprecated_declared_classes.len());
                    println!("Nonces updated:{}", state_diff.nonces.len());
                    println!("Replaced classes:{}", state_diff.replaced_classes.len());
                    println!("State diff found");
                }
                None => {
                    println!("State diff for block {} not found", block_number);
                }
            }
        }
        
        Commands::TxCount { block_number } => {
            let txn = reader.begin_ro_txn()?;
            let block_num = BlockNumber(block_number);
            
            match txn.get_block_transactions_count(block_num)? {
                Some(count) => {
                    println!("Block #{} has {} transactions", block_number, count);
                    
                    // Try to get transaction hashes.
                    if let Some(tx_hashes) = txn.get_block_transaction_hashes(block_num)? {
                        println!("\nTransaction hashes:");
                        for (i, hash) in tx_hashes.iter().enumerate() {
                            println!("  [{}] {:?}", i, hash);
                        }
                    }
                }
                None => {
                    println!("Block {} not found", block_number);
                }
            }
        }
        
        Commands::Verify { count } => {
            let txn = reader.begin_ro_txn()?;
            let latest = txn.get_header_marker()?.0.saturating_sub(1);
            
            if latest == 0 {
                println!("No blocks in storage to verify");
                return Ok(());
            }
            
            println!("Verifying {} random blocks out of {}.", count, latest);
            
            let mut success = 0;
            let mut failed = 0;
            
            use rand::Rng;
            let mut rng = rand::thread_rng();
            
            for i in 0..count {
                let random_block = rng.gen_range(0..=latest);
                let block_num = BlockNumber(random_block);
                
                print!("[{}/{}] Testing block {:>8}.", i + 1, count, random_block);
                
                // Check header.
                let has_header = txn.get_block_header(block_num)?.is_some();
                // Check state diff.
                let has_state_diff = txn.get_state_diff(block_num)?.is_some();
                // Check transactions.
                let has_txs = txn.get_block_transaction_hashes(block_num)?.is_some();
                
                if has_header && has_state_diff && has_txs {
                    println!("OK");
                    success += 1;
                } else {
                    println!("FAILED (header:{}, state:{}, txs:{})", 
                        has_header, has_state_diff, has_txs);
                    failed += 1;
                }
            }
            
            println!("\nResults:");
            println!("Success: {}", success);
            println!("Failed:  {}", failed);
            
            if failed == 0 {
                println!("\nAll blocks verified successfully!");
            } else {
                println!("\nSome blocks failed verification");
            }
        }
        
        Commands::Stats => {
            let txn = reader.begin_ro_txn()?;
            
            let header_marker = txn.get_header_marker()?;
            let body_marker = txn.get_body_marker()?;
            let state_marker = txn.get_state_marker()?;
            
            println!("Storage Statistics:");
            println!("Markers:");
            println!("Headers synced:{}", header_marker);
            println!("Bodies synced:{}", body_marker);
            println!("States synced:{}", state_marker);
            
            let min_marker = header_marker.0.min(body_marker.0).min(state_marker.0);
            let max_marker = header_marker.0.max(body_marker.0).max(state_marker.0);
            
            if min_marker == max_marker {
                println!("All components in sync at block {}", min_marker.saturating_sub(1));
            } else {
                println!("Components out of sync:");
                println!("Blocks {}-{}", min_marker, max_marker);
            }
            
            // Sample some blocks for transaction stats.
            if header_marker.0 > 100 {
                println!("Sampling recent blocks for transaction stats.");
                let mut total_txs = 0;
                let sample_size = 100;
                let start_block = header_marker.0.saturating_sub(sample_size);
                
                for i in start_block..header_marker.0 {
                    if let Some(count) = txn.get_block_transactions_count(BlockNumber(i))? {
                        total_txs += count;
                    }
                }
                
                println!("Last {} blocks: {} total transactions", sample_size, total_txs);
                println!("Average: {:.2} txs/block", total_txs as f64 / sample_size as f64);
            }
        }
    }
    
    Ok(())
}

