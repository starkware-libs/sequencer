//! Tests for the commit module.

use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ChainId, ContractAddress, Nonce};
use starknet_api::hash::HashOutput;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::transaction::{InvokeTransaction, Transaction, TransactionHash};
use starknet_api::{calldata, felt, invoke_tx_args};
use starknet_rust::providers::Provider;

use crate::commit::{
    commit_state_diff,
    create_facts_db_from_storage_proof,
    create_state_diff_from_execution_outputs,
};
use crate::storage_proofs::RpcStorageProofsProvider;
use crate::test_utils::{
    rpc_provider,
    rpc_state_reader_for_block,
    SENDER_ADDRESS,
    STRK_TOKEN_ADDRESS,
};
use crate::virtual_block_executor::{RpcVirtualBlockExecutor, VirtualBlockExecutor};

/// Constructs an Invoke transaction that calls `balanceOf` on the STRK token contract.
fn construct_balance_of_invoke() -> (InvokeTransaction, TransactionHash) {
    let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS).unwrap();
    let sender = ContractAddress::try_from(SENDER_ADDRESS).unwrap();

    let balance_of_selector = selector_from_name("balanceOf");
    let calldata = calldata![
        felt!("1"),            // call_array_len
        *strk_token.0.key(),   // to
        balance_of_selector.0, // selector
        felt!("0"),            // data_offset
        felt!("1"),            // data_len
        felt!("1"),            // calldata_len
        *sender.0.key()        // address to check balance of
    ];

    let invoke_tx = invoke_tx(invoke_tx_args! {
        sender_address: sender,
        calldata,
        nonce: Nonce(felt!("0x1000000")),
    });

    let tx_hash = Transaction::Invoke(invoke_tx.clone())
        .calculate_transaction_hash(&ChainId::Mainnet)
        .unwrap();
    (invoke_tx, tx_hash)
}

/// Integration test: Execute transaction, create FactsDb from RPC proofs, and run the committer.
///
/// This test:
/// 1. Executes a balanceOf transaction using RpcVirtualBlockExecutor
/// 2. Fetches storage proofs for the accessed state
/// 3. Creates a FactsDb directly from StorageProof
/// 4. Extracts state diff from execution outputs
/// 5. Runs the committer to compute new state roots
/// 6. Verifies the committer ran successfully
///
/// # Environment Variables
///
/// - `NODE_URL`: Required. URL of a Starknet RPC node (e.g., http://localhost:9545/rpc/v0_10).
///
/// # Running
///
/// ```bash
/// NODE_URL=http://localhost:9545/rpc/v0_10 cargo test -p starknet_os_runner -- --ignored test_commit_with_rpc_proofs --nocapture
/// ```
#[rstest]
#[ignore] // Requires RPC access
#[tokio::test]
async fn test_commit_with_rpc_proofs(rpc_provider: RpcStorageProofsProvider) {
    // Fetch the latest block number directly in the async context
    let block_number =
        BlockNumber(rpc_provider.0.block_number().await.expect("Failed to fetch block number"));
    println!("Using block number: {}", block_number.0);

    // Step 1: Execute transaction to get execution data
    // Note: ContractClassManager::start creates its own runtime, so we run
    // the synchronous execution parts in spawn_blocking to avoid runtime conflicts.
    let (tx, tx_hash) = construct_balance_of_invoke();
    let execution_result = tokio::task::spawn_blocking(move || {
        // Create state reader and executor for the specific block
        let rpc_state_reader = rpc_state_reader_for_block(block_number);
        let rpc_virtual_block_executor =
            RpcVirtualBlockExecutor { rpc_state_reader, validate_txs: false };
        let contract_class_manager =
            ContractClassManager::start(ContractClassManagerConfig::default());
        rpc_virtual_block_executor
            .execute(BlockId::Number(block_number), contract_class_manager, vec![(tx, tx_hash)])
            .expect("Transaction execution should succeed")
    })
    .await
    .expect("spawn_blocking should succeed");

    println!("Transaction executed successfully");
    println!("Number of execution outputs: {}", execution_result.execution_outputs.len());

    // Step 2: Get raw RPC proof
    let query = RpcStorageProofsProvider::prepare_query(&execution_result);

    // Debug: Print query details
    println!("\n=== DEBUG: Query details ===");
    println!("Contract addresses queried:");
    for addr in &query.contract_addresses {
        println!("  0x{:x}", addr.0.key());
    }
    println!("Class hashes queried: {}", query.class_hashes.len());
    println!("Contract storage keys queried: {}", query.contract_storage_keys.len());
    for csk in &query.contract_storage_keys {
        println!(
            "  Contract 0x{:x}: {} storage keys",
            csk.contract_address,
            csk.storage_keys.len()
        );
    }
    println!("=== END Query details ===\n");

    let rpc_proof = rpc_provider
        .fetch_proofs(block_number, &query)
        .await
        .expect("Fetching RPC proofs should succeed");

    println!("Fetched RPC proofs successfully");
    println!("Classes proof nodes: {}", rpc_proof.classes_proof.len());
    println!("Contracts proof nodes: {}", rpc_proof.contracts_proof.nodes.len());
    println!("Contract leaves: {}", rpc_proof.contracts_proof.contract_leaves_data.len());
    println!("Storage proofs count: {}", rpc_proof.contracts_storage_proofs.len());

    // Step 3: Extract state diff from execution outputs BEFORE creating FactsDb
    let state_diff = create_state_diff_from_execution_outputs(&execution_result.execution_outputs);

    println!("State diff created:");
    println!("  - Nonce updates: {}", state_diff.address_to_nonce.len());
    for (addr, nonce) in &state_diff.address_to_nonce {
        println!("      Address: 0x{:x}, New nonce: {:?}", addr.0.key(), nonce);
    }
    println!("  - Class hash updates: {}", state_diff.address_to_class_hash.len());
    println!(
        "  - Compiled class hash updates: {}",
        state_diff.class_hash_to_compiled_class_hash.len()
    );
    println!("  - Storage updates: {} contracts", state_diff.storage_updates.len());

    // Step 4: Create FactsDb from storage proof with state diff
    let mut facts_db = create_facts_db_from_storage_proof(&rpc_proof, &state_diff);

    println!("Created FactsDb with {} entries", facts_db.storage.0.len());

    // Count different types of entries by prefix
    let patricia_node_count =
        facts_db.storage.0.keys().filter(|k| k.0.starts_with(b"patricia_node:")).count();
    let contract_state_count =
        facts_db.storage.0.keys().filter(|k| k.0.starts_with(b"contract_state:")).count();
    let class_leaf_count =
        facts_db.storage.0.keys().filter(|k| k.0.starts_with(b"contract_class_leaf:")).count();
    let storage_leaf_count =
        facts_db.storage.0.keys().filter(|k| k.0.starts_with(b"starknet_storage_leaf:")).count();

    println!(
        "FactsDb breakdown: {} patricia nodes, {} contract states, {} class leaves, {} storage \
         leaves",
        patricia_node_count, contract_state_count, class_leaf_count, storage_leaf_count
    );

    // Debug: Print full node structure from RPC proof
    println!("\n=== DEBUG: Contracts proof nodes (full structure) ===");
    for (hash, node) in &rpc_proof.contracts_proof.nodes {
        match node {
            starknet_rust_core::types::MerkleNode::BinaryNode(bn) => {
                println!(
                    "  0x{:x} = Binary {{ left: 0x{:x}, right: 0x{:x} }}",
                    hash, bn.left, bn.right
                );
            }
            starknet_rust_core::types::MerkleNode::EdgeNode(en) => {
                println!(
                    "  0x{:x} = Edge {{ path: 0x{:x}, length: {}, child: 0x{:x} }}",
                    hash, en.path, en.length, en.child
                );
            }
        }
    }

    // Print contract leaves with their computed hashes
    println!("\n=== DEBUG: Contract leaves ===");
    for (i, leaf_data) in rpc_proof.contracts_proof.contract_leaves_data.iter().enumerate() {
        println!(
            "  Leaf {}: class_hash=0x{:x}, nonce=0x{:x}, storage_root={:?}",
            i, leaf_data.class_hash, leaf_data.nonce, leaf_data.storage_root
        );
    }

    // Check if missing node is referenced by any node
    let missing_hash = starknet_rust_core::types::Felt::from_hex(
        "0x04d98632fb3e5f38c33da393365035cd51231a9cde03e8c6fe15db4fc109ab1d",
    )
    .unwrap();
    println!("\n=== DEBUG: Searching for missing node 0x04d98632... ===");
    for (hash, node) in &rpc_proof.contracts_proof.nodes {
        match node {
            starknet_rust_core::types::MerkleNode::BinaryNode(bn) => {
                if bn.left == missing_hash || bn.right == missing_hash {
                    println!("  FOUND! Node 0x{:x} references missing hash as child", hash);
                    println!("    Binary {{ left: 0x{:x}, right: 0x{:x} }}", bn.left, bn.right);
                }
            }
            starknet_rust_core::types::MerkleNode::EdgeNode(en) => {
                if en.child == missing_hash {
                    println!("  FOUND! Node 0x{:x} references missing hash as child", hash);
                    println!(
                        "    Edge {{ path: 0x{:x}, length: {}, child: 0x{:x} }}",
                        en.path, en.length, en.child
                    );
                }
            }
        }
    }

    println!("\n=== DEBUG: Classes proof node hashes ===");
    for (hash, _node) in &rpc_proof.classes_proof {
        println!("  0x{:x}", hash);
    }
    for (i, storage_proof) in rpc_proof.contracts_storage_proofs.iter().enumerate() {
        println!("Storage proof {} node hashes:", i);
        for (hash, _node) in storage_proof {
            println!("  0x{:x}", hash);
        }
    }

    // Debug: Print all keys in FactsDb
    println!("\n=== DEBUG: Keys in FactsDb ===");
    for key in facts_db.storage.0.keys() {
        // Extract the hash part (after the prefix)
        if key.0.starts_with(b"patricia_node:") {
            let hash_bytes = &key.0[14..]; // "patricia_node:" is 14 bytes
            if hash_bytes.len() == 32 {
                let hash_hex: String = hash_bytes.iter().map(|b| format!("{:02x}", b)).collect();
                println!("  patricia_node: 0x{}", hash_hex);
            }
        } else if key.0.starts_with(b"contract_state:") {
            let hash_bytes = &key.0[15..];
            if hash_bytes.len() == 32 {
                let hash_hex: String = hash_bytes.iter().map(|b| format!("{:02x}", b)).collect();
                println!("  contract_state: 0x{}", hash_hex);
            }
        }
    }
    println!("=== END DEBUG ===\n");

    // Step 5: Get previous state roots from the RPC proof
    let prev_contracts_root = HashOutput(rpc_proof.global_roots.contracts_tree_root);
    let prev_classes_root = HashOutput(rpc_proof.global_roots.classes_tree_root);

    println!("Previous state roots:");
    println!("  - Contracts root: {:?}", prev_contracts_root);
    println!("  - Classes root: {:?}", prev_classes_root);

    // Step 6: Run the committer
    println!("\n=== Attempting to commit... ===");
    let commit_result =
        commit_state_diff(&mut facts_db, prev_contracts_root, prev_classes_root, state_diff).await;

    // If commit failed, try to decode the missing key
    let new_roots = match commit_result {
        Ok(roots) => roots,
        Err(e) => {
            let err_str = format!("{:?}", e);
            // Try to extract the missing key bytes from the error
            if let Some(start) = err_str.find("MissingKey(DbKey([") {
                let after_start = &err_str[start + 18..];
                if let Some(end) = after_start.find("]))") {
                    let bytes_str = &after_start[..end];
                    let bytes: Vec<u8> =
                        bytes_str.split(", ").filter_map(|s| s.trim().parse::<u8>().ok()).collect();

                    if bytes.len() > 14 {
                        let prefix = String::from_utf8_lossy(&bytes[..14]);
                        let hash_bytes = &bytes[14..];
                        let hash_hex: String =
                            hash_bytes.iter().map(|b| format!("{:02x}", b)).collect();
                        println!("\n=== MISSING KEY DECODED ===");
                        println!("Prefix: {}", prefix);
                        println!("Hash: 0x{}", hash_hex);
                        println!("===========================\n");
                    }
                }
            }
            panic!("Failed to commit: {:?}", e);
        }
    };

    println!("Committer completed successfully!");
    println!("New state roots:");
    println!("  - Contracts root: {:?}", new_roots.contracts_trie_root_hash);
    println!("  - Classes root: {:?}", new_roots.classes_trie_root_hash);

    // Verify the committer produced results
    // Note: For a read-only transaction, the classes root should not change
    // but the contracts root may change due to nonce updates and fee payments
    assert_ne!(
        new_roots.contracts_trie_root_hash, prev_contracts_root,
        "Contracts root should change due to nonce/fee updates"
    );
    assert_eq!(
        new_roots.classes_trie_root_hash, prev_classes_root,
        "Classes root should not change for invoke transactions"
    );

    println!("All assertions passed!");
}

/// Simpler test that just verifies FactsDb creation from storage proofs.
#[rstest]
#[ignore] // Requires RPC access
#[tokio::test]
async fn test_create_facts_db_from_storage_proof(rpc_provider: RpcStorageProofsProvider) {
    // Fetch the latest block number directly in the async context
    let block_number =
        BlockNumber(rpc_provider.0.block_number().await.expect("Failed to fetch block number"));
    println!("Using block number: {}", block_number.0);

    // Execute transaction
    let (tx, tx_hash) = construct_balance_of_invoke();
    let execution_result = tokio::task::spawn_blocking(move || {
        // Create state reader and executor for the specific block
        let rpc_state_reader = rpc_state_reader_for_block(block_number);
        let rpc_virtual_block_executor =
            RpcVirtualBlockExecutor { rpc_state_reader, validate_txs: false };
        let contract_class_manager =
            ContractClassManager::start(ContractClassManagerConfig::default());
        rpc_virtual_block_executor
            .execute(BlockId::Number(block_number), contract_class_manager, vec![(tx, tx_hash)])
            .expect("Transaction execution should succeed")
    })
    .await
    .expect("spawn_blocking should succeed");

    // Get RPC proof
    let query = RpcStorageProofsProvider::prepare_query(&execution_result);
    let rpc_proof = rpc_provider
        .fetch_proofs(block_number, &query)
        .await
        .expect("Fetching RPC proofs should succeed");

    // Extract state diff from execution outputs
    let state_diff = create_state_diff_from_execution_outputs(&execution_result.execution_outputs);

    // Create FactsDb
    let facts_db = create_facts_db_from_storage_proof(&rpc_proof, &state_diff);

    // Verify FactsDb has entries
    assert!(!facts_db.storage.0.is_empty(), "FactsDb should contain entries");

    let patricia_node_count =
        facts_db.storage.0.keys().filter(|k| k.0.starts_with(b"patricia_node:")).count();
    let contract_state_count =
        facts_db.storage.0.keys().filter(|k| k.0.starts_with(b"contract_state:")).count();

    println!(
        "Created FactsDb with {} total entries: {} patricia nodes, {} contract states",
        facts_db.storage.0.len(),
        patricia_node_count,
        contract_state_count
    );

    assert!(patricia_node_count > 0, "Should have patricia node entries");
    assert!(contract_state_count > 0, "Should have contract state entries");
}
