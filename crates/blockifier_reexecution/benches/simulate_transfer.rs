//! Benchmark for simulating ERC20 transfer transactions using RpcStateReader.
//!
//! This benchmark executes transfer transactions directly via the blockifier,
//! with validation and fee charging disabled (equivalent to SKIP_VALIDATE + SKIP_FEE_CHARGE
//! simulation flags).
//!
//! ## Usage
//!
//! Single run (for testing):
//! ```bash
//! cargo bench -p blockifier_reexecution --bench simulate_transfer -- \
//!   --node-url http://127.0.0.1:9545 \
//!   --sender-address 0x271e7b3b1c8e8fb6f93866edd386f50ae02e9a67b63f90e9e800bdb1e48785 \
//!   --single-run
//! ```
//!
//! Full benchmark:
//! ```bash
//! cargo bench -p blockifier_reexecution --bench simulate_transfer -- \
//!   --node-url http://127.0.0.1:9545 \
//!   --sender-address 0x271e7b3b1c8e8fb6f93866edd386f50ae02e9a67b63f90e9e800bdb1e48785
//! ```

use std::time::Instant;

use apollo_gateway_config::config::RpcStateReaderConfig;
use blockifier::blockifier::config::{ContractClassManagerConfig, TransactionExecutorConfig};
use blockifier::blockifier::transaction_executor::TransactionExecutor;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::state_api::StateReader;
use blockifier::state::state_reader_and_contract_manager::StateReaderAndContractManager;
use blockifier::transaction::account_transaction::ExecutionFlags;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
use blockifier_reexecution::state_reader::reexecution_state_reader::ConsecutiveReexecutionStateReaders;
use blockifier_reexecution::state_reader::rpc_state_reader::{
    ConsecutiveRpcStateReaders,
    RpcStateReader,
};
use clap::Parser;
use criterion::{BatchSize, Criterion};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ChainId, ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    PaymasterData,
    ResourceBounds,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::{
    InvokeTransaction,
    InvokeTransactionV3,
    Transaction,
    TransactionHash,
};
use starknet_types_core::felt::Felt;

/// CLI arguments for the simulate transfer benchmark.
#[derive(Parser, Debug, Clone)]
#[command(name = "simulate_transfer", about = "Benchmark ERC20 transfer simulation")]
struct Args {
    /// Node URL
    #[arg(long, default_value = "http://127.0.0.1:9545")]
    node_url: String,

    /// Sender address (hex with 0x prefix)
    #[arg(long)]
    sender_address: String,

    /// Block number (optional, defaults to latest - 1)
    #[arg(long)]
    block_number: Option<u64>,

    /// Number of transfers in the multicall (default: 10)
    #[arg(long, default_value = "10")]
    num_transfers: usize,

    /// Run once without benchmarking (for testing)
    #[arg(long)]
    single_run: bool,

    /// Chain ID (mainnet or sepolia)
    #[arg(long, default_value = "mainnet")]
    chain_id: String,
}

/// STRK token address on mainnet
const STRK_TOKEN_ADDRESS: &str =
    "0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d";

/// Transfer entry point selector: sn_keccak("transfer")
const TRANSFER_SELECTOR: &str = "0x83afd3f4caedc6eebf44246fe54e38c95e3179a5ec9ea81740eca5b482d12e";

fn parse_felt(hex_str: &str) -> Felt {
    let hex_str = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    Felt::from_hex(hex_str).expect("Failed to parse felt")
}

fn parse_chain_id(chain_id: &str) -> ChainId {
    match chain_id.to_lowercase().as_str() {
        "mainnet" => ChainId::Mainnet,
        "sepolia" => ChainId::Sepolia,
        other => ChainId::Other(other.to_string()),
    }
}

/// Build multicall calldata for multiple ERC20 transfers.
/// Format: [num_calls, (contract, selector, calldata_len, ...calldata)...]
fn build_multicall_transfer_calldata(num_transfers: usize) -> Vec<Felt> {
    let token_address = parse_felt(STRK_TOKEN_ADDRESS);
    let transfer_selector = parse_felt(TRANSFER_SELECTOR);

    let mut calldata = vec![Felt::from(num_transfers as u64)];

    for i in 0..num_transfers {
        // Each call: contract_address, entry_point_selector, calldata_len, ...calldata
        calldata.push(token_address); // Contract to call (ERC20)
        calldata.push(transfer_selector); // Entry point selector
        calldata.push(Felt::from(3u64)); // Calldata length

        // Transfer calldata: recipient, amount_low, amount_high
        calldata.push(Felt::from(0x1000u64 + i as u64)); // Different recipient for each transfer
        calldata.push(Felt::ONE); // Amount low (1 token)
        calldata.push(Felt::ZERO); // Amount high
    }

    calldata
}

/// Build an InvokeTransactionV3 for the simulation.
fn build_invoke_transaction(
    sender_address: ContractAddress,
    nonce: Nonce,
    num_transfers: usize,
) -> Transaction {
    let calldata = build_multicall_transfer_calldata(num_transfers);

    let invoke_tx = InvokeTransactionV3 {
        resource_bounds: ValidResourceBounds::AllResources(AllResourceBounds {
            l1_gas: ResourceBounds { max_amount: 0u64.into(), max_price_per_unit: 0u128.into() },
            l1_data_gas: ResourceBounds {
                max_amount: 1_000_000u64.into(),
                max_price_per_unit: 100_000_000_000u128.into(),
            },
            l2_gas: ResourceBounds {
                max_amount: 10_000_000u64.into(),
                max_price_per_unit: 100_000_000_000u128.into(),
            },
        }),
        tip: Tip::default(),
        signature: TransactionSignature(vec![Felt::ONE, Felt::TWO].into()),
        nonce,
        sender_address,
        calldata: Calldata(calldata.into()),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
        paymaster_data: PaymasterData::default(),
        account_deployment_data: AccountDeploymentData::default(),
    };

    Transaction::Invoke(InvokeTransaction::V3(invoke_tx))
}

/// Convert API transaction to blockifier transaction with simulation flags.
fn api_tx_to_blockifier_tx_with_simulation_flags(
    tx: Transaction,
    tx_hash: TransactionHash,
) -> BlockifierTransaction {
    // Simulation flags: skip validation and fee charge
    let execution_flags = ExecutionFlags {
        only_query: true,  // Simulation mode
        charge_fee: false, // SKIP_FEE_CHARGE
        validate: false,   // SKIP_VALIDATE
        strict_nonce_check: false,
    };

    BlockifierTransaction::from_api(tx, tx_hash, None, None, None, execution_flags)
        .expect("Failed to create blockifier transaction")
}

/// Get the latest block number from the node.
fn get_latest_block_number(config: &RpcStateReaderConfig, _chain_id: &ChainId) -> BlockNumber {
    use apollo_gateway::rpc_state_reader::RpcStateReader as GatewayRpcStateReader;
    use serde_json::json;

    let gateway_reader = GatewayRpcStateReader::from_latest(config);
    let response = gateway_reader
        .send_rpc_request("starknet_getBlockWithTxHashes", json!({"block_id": "latest"}))
        .expect("Failed to get latest block");

    let block_number: u64 =
        response["block_number"].as_u64().expect("Failed to parse block number from response");

    BlockNumber(block_number)
}

/// Run a single execution and print timing results.
fn run_single_execution(args: &Args) {
    println!("=== Single Execution Mode ===");
    println!("Node URL: {}", args.node_url);
    println!("Sender: {}", args.sender_address);
    println!("Num transfers: {}", args.num_transfers);
    println!();

    let config = RpcStateReaderConfig::from_url(args.node_url.clone());
    let chain_id = parse_chain_id(&args.chain_id);

    // Get block number
    let block_number = match args.block_number {
        Some(bn) => BlockNumber(bn),
        None => {
            let latest = get_latest_block_number(&config, &chain_id);
            println!("Latest block: {}", latest.0);
            // Use latest - 1 for state (we execute "at" latest, reading state from latest-1)
            BlockNumber(latest.0.saturating_sub(1))
        }
    };
    println!("Using block number: {}", block_number.0);

    // Initialize contract class manager
    let contract_class_manager_config = ContractClassManagerConfig::default();
    let contract_class_manager = ContractClassManager::start(contract_class_manager_config);

    // Create consecutive state readers
    let readers = ConsecutiveRpcStateReaders::new(
        block_number,
        Some(config.clone()),
        chain_id.clone(),
        false,
        contract_class_manager.clone(),
    );

    // Fetch nonce from node
    let sender_address = ContractAddress::try_from(parse_felt(&args.sender_address))
        .expect("Invalid sender address");
    let nonce =
        readers.last_block_state_reader.get_nonce_at(sender_address).expect("Failed to get nonce");
    println!("Current nonce: {:?}", nonce);

    // Build transaction
    let tx = build_invoke_transaction(sender_address, nonce, args.num_transfers);
    let tx_hash = tx.calculate_transaction_hash(&chain_id).expect("Failed to calculate tx hash");
    println!("Transaction hash: {:?}", tx_hash);

    // Convert to blockifier transaction with simulation flags
    let blockifier_tx = api_tx_to_blockifier_tx_with_simulation_flags(tx, tx_hash);

    // Create executor for warmup
    let mut warmup_executor = readers
        .pre_process_and_create_executor(Some(TransactionExecutorConfig::default()))
        .expect("Failed to create executor");

    // Warmup run to fill class cache
    println!("\n=== Warmup Run (filling class cache) ===");
    let warmup_start = Instant::now();
    let warmup_result = warmup_executor.execute(&blockifier_tx);
    let warmup_elapsed = warmup_start.elapsed();
    match warmup_result {
        Ok((info, _)) => {
            if info.is_reverted() {
                println!(
                    "Warmup completed in {:?} (reverted: {:?})",
                    warmup_elapsed, info.revert_error
                );
            } else {
                println!("Warmup completed in {:?} - classes now cached", warmup_elapsed);
            }
        }
        Err(e) => {
            println!("Warmup failed in {:?}: {:?}", warmup_elapsed, e);
        }
    }

    // Create new executor for actual measurement (same class manager, so cache is warm)
    let readers2 = ConsecutiveRpcStateReaders::new(
        block_number,
        Some(config.clone()),
        chain_id.clone(),
        false,
        contract_class_manager,
    );
    let mut executor = readers2
        .pre_process_and_create_executor(Some(TransactionExecutorConfig::default()))
        .expect("Failed to create executor");

    println!("\n=== Actual Execution (warm cache) ===");
    let start = Instant::now();
    let result = executor.execute(&blockifier_tx);
    let elapsed = start.elapsed();

    match result {
        Ok((execution_info, _state_diff)) => {
            println!("✓ Execution succeeded in {:?}", elapsed);
            if execution_info.is_reverted() {
                println!("  ⚠ Transaction reverted: {:?}", execution_info.revert_error);
            } else {
                println!("  Transaction completed successfully");
                println!("  Gas used: {:?}", execution_info.receipt.gas);
            }
        }
        Err(e) => {
            println!("✗ Execution failed in {:?}: {:?}", elapsed, e);
        }
    }
}

/// Setup function for criterion benchmark - returns transaction and executor.
/// Uses a shared ContractClassManager so class cache is warm across all iterations.
fn setup_benchmark(
    args: &Args,
    contract_class_manager: ContractClassManager,
) -> (BlockifierTransaction, TransactionExecutor<StateReaderAndContractManager<RpcStateReader>>) {
    let config = RpcStateReaderConfig::from_url(args.node_url.clone());
    let chain_id = parse_chain_id(&args.chain_id);

    // Get block number
    let block_number = match args.block_number {
        Some(bn) => BlockNumber(bn),
        None => {
            let latest = get_latest_block_number(&config, &chain_id);
            BlockNumber(latest.0.saturating_sub(1))
        }
    };

    // Create consecutive state readers with shared contract class manager
    let readers = ConsecutiveRpcStateReaders::new(
        block_number,
        Some(config.clone()),
        chain_id.clone(),
        false,
        contract_class_manager,
    );

    // Fetch nonce from node
    let sender_address = ContractAddress::try_from(parse_felt(&args.sender_address))
        .expect("Invalid sender address");
    let nonce =
        readers.last_block_state_reader.get_nonce_at(sender_address).expect("Failed to get nonce");

    // Build transaction
    let tx = build_invoke_transaction(sender_address, nonce, args.num_transfers);
    let tx_hash = tx.calculate_transaction_hash(&chain_id).expect("Failed to calculate tx hash");

    // Convert to blockifier transaction with simulation flags
    let blockifier_tx = api_tx_to_blockifier_tx_with_simulation_flags(tx, tx_hash);

    // Create executor
    let executor = readers
        .pre_process_and_create_executor(Some(TransactionExecutorConfig::default()))
        .expect("Failed to create executor");

    (blockifier_tx, executor)
}

/// Run criterion benchmark.
fn run_criterion_benchmark(c: &mut Criterion, args: Args) {
    println!("Setting up benchmark...");
    println!("Node URL: {}", args.node_url);
    println!("Sender: {}", args.sender_address);
    println!("Num transfers: {}", args.num_transfers);

    // Create shared contract class manager - cache will be warm after warmup run
    let contract_class_manager_config = ContractClassManagerConfig::default();
    let contract_class_manager = ContractClassManager::start(contract_class_manager_config);
    println!("Contract class manager initialized (shared across all iterations)");

    // Warmup run to fill the class cache before benchmarking
    println!("\n=== Warmup Run (filling class cache) ===");
    let (warmup_tx, mut warmup_executor) = setup_benchmark(&args, contract_class_manager.clone());
    let warmup_start = Instant::now();
    let warmup_result = warmup_executor.execute(&warmup_tx);
    let warmup_elapsed = warmup_start.elapsed();
    match warmup_result {
        Ok((info, _)) => {
            if info.is_reverted() {
                println!(
                    "Warmup completed in {:?} (reverted: {:?})",
                    warmup_elapsed, info.revert_error
                );
            } else {
                println!("Warmup completed in {:?} - classes now cached", warmup_elapsed);
            }
        }
        Err(e) => {
            println!("Warmup failed: {:?}", e);
        }
    }
    println!("=== Starting Benchmark ===\n");

    c.bench_function(&format!("simulate_{}_transfers", args.num_transfers), |b| {
        b.iter_batched(
            || setup_benchmark(&args, contract_class_manager.clone()),
            |(tx, mut executor)| {
                let result = executor.execute(&tx);
                assert!(result.is_ok(), "Execution failed: {:?}", result.err());
                let (execution_info, _) = result.unwrap();
                assert!(
                    !execution_info.is_reverted(),
                    "Transaction reverted: {:?}",
                    execution_info.revert_error
                );
            },
            BatchSize::SmallInput,
        )
    });
}

fn parse_args_from_criterion() -> Args {
    // Filter out criterion-specific args (like --bench)
    let all_args: Vec<String> = std::env::args().collect();

    let filtered_args: Vec<String> = all_args
        .into_iter()
        .filter(|arg| arg != "--bench" && arg != "--test" && !arg.starts_with("--bench="))
        .collect();

    Args::try_parse_from(filtered_args).unwrap_or_else(|e| {
        eprintln!("Error parsing arguments: {}", e);
        eprintln!(
            "\nUsage: cargo bench -p blockifier_reexecution --bench simulate_transfer -- \
             --sender-address <ADDRESS> [OPTIONS]"
        );
        eprintln!("\nExample:");
        eprintln!("  cargo bench -p blockifier_reexecution --bench simulate_transfer -- \\");
        eprintln!(
            "    --sender-address \
             0x271e7b3b1c8e8fb6f93866edd386f50ae02e9a67b63f90e9e800bdb1e48785 \\"
        );
        eprintln!("    --single-run");
        std::process::exit(1);
    })
}

fn main() {
    let args = parse_args_from_criterion();

    if args.single_run {
        run_single_execution(&args);
        return;
    }

    // Run criterion benchmark
    let mut criterion = Criterion::default().sample_size(10);
    run_criterion_benchmark(&mut criterion, args);
    criterion.final_summary();
}
