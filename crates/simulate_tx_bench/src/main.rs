//! Benchmark tool for starknet_simulateTransactions RPC method.
//!
//! This tool creates batches of transfer invoke transactions and measures
//! the time taken by the simulateTransactions RPC call.

use std::time::{Duration, Instant};

use clap::Parser;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    Calldata,
    PaymasterData,
    ResourceBounds,
    Tip,
    TransactionSignature,
};
use starknet_types_core::felt::Felt;

/// CLI arguments for the simulate transactions benchmark.
#[derive(Parser, Debug)]
#[command(name = "simulate_tx_bench", about = "Benchmark starknet_simulateTransactions RPC")]
struct Args {
    /// Sender address (hex string with 0x prefix)
    #[arg(long)]
    sender_address: String,

    /// Recipient address for transfers (hex string with 0x prefix)
    #[arg(long, default_value = "0x1234")]
    recipient_address: String,

    /// ERC20 token contract address (STRK fee token by default)
    #[arg(
        long,
        default_value = "0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
    )]
    token_address: String,

    /// Number of benchmark iterations
    #[arg(long, default_value = "10")]
    iterations: usize,

    /// Number of transactions per simulation call
    #[arg(long, default_value = "10")]
    txs_per_call: usize,

    /// Node URL
    #[arg(long, default_value = "http://127.0.0.1:9545")]
    node_url: String,

    /// Starting nonce for transactions
    #[arg(long, default_value = "0")]
    start_nonce: u64,
}

/// Resource bounds mapping for the RPC format.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResourceBoundsMapping {
    l1_gas: ResourceBounds,
    l1_data_gas: ResourceBounds,
    l2_gas: ResourceBounds,
}

impl Default for ResourceBoundsMapping {
    fn default() -> Self {
        Self {
            l1_gas: ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: 0_u64.into() },
            l1_data_gas: ResourceBounds {
                max_amount: GasAmount(1000000),
                max_price_per_unit: 100000000000_u128.into(),
            },
            l2_gas: ResourceBounds {
                max_amount: GasAmount(10000000),
                max_price_per_unit: 100000000000_u128.into(),
            },
        }
    }
}

/// InvokeTransactionV3 for the RPC format.
#[derive(Debug, Clone, Serialize)]
struct InvokeTransactionV3 {
    sender_address: ContractAddress,
    calldata: Calldata,
    version: &'static str,
    signature: TransactionSignature,
    nonce: Nonce,
    resource_bounds: ResourceBoundsMapping,
    tip: Tip,
    paymaster_data: PaymasterData,
    account_deployment_data: AccountDeploymentData,
    nonce_data_availability_mode: DataAvailabilityMode,
    fee_data_availability_mode: DataAvailabilityMode,
}

/// Broadcasted transaction wrapper.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
enum BroadcastedTransaction {
    #[serde(rename = "INVOKE")]
    Invoke(InvokeTransactionV3),
}

/// Simulation flags.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum SimulationFlag {
    SkipValidate,
    SkipFeeCharge,
}

/// Result of a simulation call.
#[derive(Debug)]
struct SimulationResult {
    /// Number of successful (non-reverted) transactions.
    successful: usize,
    /// Number of reverted transactions.
    reverted: usize,
    /// Revert reasons if any.
    revert_reasons: Vec<String>,
}

/// RPC client for simulateTransactions.
struct RpcClient {
    client: Client,
    url: String,
}

impl RpcClient {
    fn new(url: &str) -> Self {
        Self { client: Client::new(), url: url.to_string() }
    }

    /// Fetch the current nonce for an account.
    fn get_nonce(&self, address: &str) -> Result<u64, Box<dyn std::error::Error>> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "starknet_getNonce",
            "params": {
                "block_id": "latest",
                "contract_address": address
            }
        });

        let response = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()?;

        let result: Value = response.json()?;

        if let Some(error) = result.get("error") {
            return Err(format!("RPC error: {}", error).into());
        }

        let nonce_hex =
            result.get("result").and_then(|v| v.as_str()).ok_or("Missing nonce in response")?;

        let nonce_hex = nonce_hex.strip_prefix("0x").unwrap_or(nonce_hex);
        let nonce = u64::from_str_radix(nonce_hex, 16)?;
        Ok(nonce)
    }

    fn simulate_transactions(
        &self,
        transactions: &[BroadcastedTransaction],
        simulation_flags: &[SimulationFlag],
    ) -> Result<SimulationResult, Box<dyn std::error::Error>> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "starknet_simulateTransactions",
            "params": {
                "block_id": "latest",
                "transactions": transactions,
                "simulation_flags": simulation_flags,
            }
        });

        let response = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()?;

        if !response.status().is_success() {
            return Err(format!("HTTP error: {}", response.status()).into());
        }

        let result: Value = response.json()?;

        if let Some(error) = result.get("error") {
            return Err(format!("RPC error: {}", error).into());
        }

        // Parse the simulation results to check for reverts
        let sim_result = self.parse_simulation_result(&result)?;
        Ok(sim_result)
    }

    fn parse_simulation_result(
        &self,
        response: &Value,
    ) -> Result<SimulationResult, Box<dyn std::error::Error>> {
        let results = response
            .get("result")
            .ok_or("Missing 'result' field in response")?
            .as_array()
            .ok_or("'result' is not an array")?;

        let mut successful = 0;
        let mut reverted = 0;
        let mut revert_reasons = Vec::new();

        for (i, tx_result) in results.iter().enumerate() {
            let trace = tx_result
                .get("transaction_trace")
                .ok_or(format!("Missing transaction_trace for tx {}", i))?;

            // Check for revert in execute_invocation
            if let Some(execute_inv) = trace.get("execute_invocation") {
                // If execute_invocation has a revert_reason, it reverted
                if let Some(revert_reason) = execute_inv.get("revert_reason") {
                    reverted += 1;
                    let reason = revert_reason.as_str().unwrap_or("unknown").to_string();
                    revert_reasons.push(format!("TX {}: {}", i, reason));
                } else {
                    successful += 1;
                }
            } else {
                // No execute_invocation means something went wrong
                reverted += 1;
                revert_reasons.push(format!("TX {}: missing execute_invocation", i));
            }
        }

        Ok(SimulationResult { successful, reverted, revert_reasons })
    }
}

/// Parse a hex string to Felt.
fn parse_felt(hex_str: &str) -> Result<Felt, Box<dyn std::error::Error>> {
    let hex_str = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    Ok(Felt::from_hex(hex_str)?)
}

/// Build a transfer invoke transaction.
fn build_transfer_tx(
    sender_address: Felt,
    recipient: Felt,
    token_address: Felt,
    nonce: u64,
) -> BroadcastedTransaction {
    // Transfer entry point selector: sn_keccak("transfer")
    let transfer_selector =
        Felt::from_hex("0x83afd3f4caedc6eebf44246fe54e38c95e3179a5ec9ea81740eca5b482d12e").unwrap();

    // Calldata for account's __execute__ in multicall format:
    // [num_calls, contract_address, entry_point_selector, calldata_len, ...calldata]
    // For transfer: calldata = [recipient, amount_low, amount_high]
    let calldata = vec![
        Felt::ONE,         // Number of calls (1)
        token_address,     // Contract to call (ERC20)
        transfer_selector, // Entry point selector
        Felt::from(3u64),  // Calldata length
        recipient,         // Calldata[0]: recipient
        Felt::from(1u64),  // Calldata[1]: amount low (1 token)
        Felt::ZERO,        // Calldata[2]: amount high
    ];

    let tx = InvokeTransactionV3 {
        sender_address: ContractAddress::try_from(sender_address).unwrap(),
        calldata: Calldata(calldata.into()),
        version: "0x3",
        signature: TransactionSignature(vec![Felt::ONE, Felt::TWO].into()), // Dummy signature
        nonce: Nonce(Felt::from(nonce)),
        resource_bounds: ResourceBoundsMapping::default(),
        tip: Tip::default(),
        paymaster_data: PaymasterData::default(),
        account_deployment_data: AccountDeploymentData::default(),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
    };

    BroadcastedTransaction::Invoke(tx)
}

/// Statistics for benchmark results.
struct BenchStats {
    times: Vec<Duration>,
}

impl BenchStats {
    fn new() -> Self {
        Self { times: Vec::new() }
    }

    fn add(&mut self, duration: Duration) {
        self.times.push(duration);
    }

    fn min(&self) -> Duration {
        *self.times.iter().min().unwrap_or(&Duration::ZERO)
    }

    fn max(&self) -> Duration {
        *self.times.iter().max().unwrap_or(&Duration::ZERO)
    }

    fn avg(&self) -> Duration {
        if self.times.is_empty() {
            return Duration::ZERO;
        }
        let total: Duration = self.times.iter().sum();
        total / self.times.len() as u32
    }

    fn percentile(&self, p: f64) -> Duration {
        if self.times.is_empty() {
            return Duration::ZERO;
        }
        let mut sorted = self.times.clone();
        sorted.sort();
        let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
        sorted[idx]
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("=== SimulateTransactions Benchmark ===");
    println!("Node URL: {}", args.node_url);
    println!("Sender: {}", args.sender_address);
    println!("Recipient: {}", args.recipient_address);
    println!("Token: {}", args.token_address);
    println!("Iterations: {}", args.iterations);
    println!("Transactions per call: {}", args.txs_per_call);
    println!();

    let sender = parse_felt(&args.sender_address)?;
    let recipient = parse_felt(&args.recipient_address)?;
    let token = parse_felt(&args.token_address)?;

    let client = RpcClient::new(&args.node_url);

    // Fetch current nonce if not specified
    let start_nonce = if args.start_nonce == 0 {
        match client.get_nonce(&args.sender_address) {
            Ok(nonce) => {
                println!("Fetched current nonce: {}", nonce);
                nonce
            }
            Err(e) => {
                eprintln!("Warning: Could not fetch nonce ({}), using 0", e);
                0
            }
        }
    } else {
        args.start_nonce
    };

    // Build transactions
    let transactions: Vec<BroadcastedTransaction> = (0..args.txs_per_call)
        .map(|i| build_transfer_tx(sender, recipient, token, start_nonce + i as u64))
        .collect();

    let simulation_flags = vec![SimulationFlag::SkipValidate, SimulationFlag::SkipFeeCharge];
    let mut stats = BenchStats::new();
    let mut total_successful_txs = 0;
    let mut total_reverted_txs = 0;
    let mut all_revert_reasons: Vec<String> = Vec::new();

    println!("Running {} iterations...", args.iterations);

    for i in 0..args.iterations {
        let start = Instant::now();
        let result = client.simulate_transactions(&transactions, &simulation_flags);
        let elapsed = start.elapsed();

        match result {
            Ok(sim_result) => {
                stats.add(elapsed);
                total_successful_txs += sim_result.successful;
                total_reverted_txs += sim_result.reverted;

                let status = if sim_result.reverted > 0 {
                    all_revert_reasons.extend(sim_result.revert_reasons);
                    format!("⚠️  {} ok, {} reverted", sim_result.successful, sim_result.reverted)
                } else {
                    format!("✓ {} ok", sim_result.successful)
                };
                println!("  Iteration {}: {:?} - {}", i + 1, elapsed, status);
            }
            Err(e) => {
                eprintln!("  Iteration {} FAILED: {}", i + 1, e);
            }
        }
    }

    println!();
    println!("=== Results ===");
    println!("Successful RPC calls: {}/{}", stats.times.len(), args.iterations);
    println!(
        "Transaction results: {} successful, {} reverted",
        total_successful_txs, total_reverted_txs
    );

    if !all_revert_reasons.is_empty() {
        println!();
        println!("=== Revert Reasons (first 5) ===");
        for reason in all_revert_reasons.iter().take(5) {
            println!("  {}", reason);
        }
        if all_revert_reasons.len() > 5 {
            println!("  ... and {} more", all_revert_reasons.len() - 5);
        }
    }

    println!();
    println!("=== Timing ===");
    println!("Min:  {:?}", stats.min());
    println!("Max:  {:?}", stats.max());
    println!("Avg:  {:?}", stats.avg());
    println!("P50:  {:?}", stats.percentile(50.0));
    println!("P95:  {:?}", stats.percentile(95.0));
    println!("P99:  {:?}", stats.percentile(99.0));

    Ok(())
}
