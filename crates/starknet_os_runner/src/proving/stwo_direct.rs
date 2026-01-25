//! Direct library integration with stwo_run_and_prove for proving VirtualOS.
//!
//! This module provides functionality to prove the VirtualOS execution directly using the
//! `stwo_run_and_prove` library, passing the `SnosHintProcessor` as an extra hint processor
//! to handle VirtualOS-specific hints during bootloader execution.
//!
//! ## Toolchain Requirement
//!
//! This module requires nightly Rust due to dependencies on the `stwo` crate which uses
//! nightly features like `#![feature(array_chunks)]`.
//!
//! To use this module:
//! ```bash
//! rustup run nightly-2025-07-14 cargo build -p starknet_os_runner --features stwo_native
//! ```

use std::path::PathBuf;
use std::rc::Rc;

use proving_utils_cairo_program_runner_lib::{
    Cairo0Executable,
    ProgramInput,
    SimpleBootloaderInput,
    Task,
    TaskSpec,
    types::HashFunc,
};
use proving_utils_stwo_run_and_prove::{
    ProofFormat,
    ProveConfig,
    StwoProverEntryPoint,
    stwo_run_and_prove,
};
use starknet_os::hint_processor::panicking_state_reader::PanickingStateReader;
use starknet_os::hint_processor::snos_hint_processor::SnosHintProcessor;
use starknet_os::io::os_input::{OsBlockInput, OsHints, StarknetOsInput};

use crate::errors::StwoDirectProvingError;

/// Resolves a path to a resource file in the crate's resources directory.
fn resolve_resource_path(file_name: &str) -> Result<PathBuf, StwoDirectProvingError> {
    let path = ["crates", "starknet_os_runner", "resources", file_name].iter().collect::<PathBuf>();
    apollo_infra_utils::path::resolve_project_relative_path(&path.to_string_lossy()).map_err(
        |source| StwoDirectProvingError::ResolveResourcePath { file_name: file_name.to_string(), source },
    )
}

/// Default bootloader program file name.
pub const BOOTLOADER_FILE: &str = "simple_bootloader_compiled.json";

/// Configuration for proving the VirtualOS with Stwo directly.
#[derive(Debug, Clone)]
pub struct StwoDirectProvingConfig {
    /// Path to the bootloader program. If None, uses the bundled simple_bootloader.
    pub bootloader_program_path: Option<PathBuf>,
    /// Path where the generated proof will be saved.
    pub proof_output_path: PathBuf,
    /// Whether to verify the proof after generation.
    pub verify: bool,
    /// Optional path to prover parameters JSON.
    pub prover_params_path: Option<PathBuf>,
    /// Optional directory for debug data.
    pub debug_data_dir: Option<PathBuf>,
    /// Whether to always save debug data (even on success).
    pub save_debug_data: bool,
}

/// Validates that all tracked resources in all execution infos are SierraGas.
fn validate_tracked_resources(os_block_inputs: &[OsBlockInput]) -> Result<(), StwoDirectProvingError>
{
    use blockifier::execution::contract_class::TrackedResource;

    for block_input in os_block_inputs.iter() {
        for (tx, tx_execution_info) in
            block_input.transactions.iter().zip(&block_input.tx_execution_infos)
        {
            for call_info in tx_execution_info.call_info_iter(tx.tx_type()) {
                if call_info.tracked_resource != TrackedResource::SierraGas {
                    return Err(StwoDirectProvingError::InvalidTrackedResource {
                        expected: TrackedResource::SierraGas,
                        actual: call_info.tracked_resource,
                    });
                }
            }
        }
    }
    Ok(())
}

/// Runs and proves the VirtualOS using Stwo directly.
///
/// This function runs the VirtualOS program inside the bootloader and generates a Stwo proof.
/// The `SnosHintProcessor` is passed as an extra hint processor to handle VirtualOS-specific
/// hints.
///
/// # Arguments
///
/// * `os_hints` - The OS hints containing block inputs and configuration.
/// * `proving_config` - Configuration for the proving process.
///
/// # Returns
///
/// Returns `Ok(())` on success, or an error if proving fails.
pub fn run_and_prove_virtual_os(
    OsHints {
        os_hints_config,
        os_input: StarknetOsInput { os_block_inputs, deprecated_compiled_classes, compiled_classes },
    }: OsHints,
    proving_config: StwoDirectProvingConfig,
) -> Result<(), StwoDirectProvingError> {
    use apollo_starknet_os_program::VIRTUAL_OS_PROGRAM;

    // Validate that all tracked resources are SierraGas.
    validate_tracked_resources(&os_block_inputs)?;

    // Create the hint processor - reuse the SNOS hint processor with the virtual OS program.
    let mut snos_hint_processor = SnosHintProcessor::new(
        &VIRTUAL_OS_PROGRAM,
        os_hints_config,
        os_block_inputs.iter().collect(),
        deprecated_compiled_classes,
        compiled_classes,
        vec![PanickingStateReader; os_block_inputs.len()],
    )?;

    // Create the task for the bootloader to run - the VirtualOS program as a Cairo0 program.
    let task = Task::Cairo0Program(Cairo0Executable {
        program: VIRTUAL_OS_PROGRAM.clone(),
        program_input: None,
    });

    let task_spec = TaskSpec { task: Rc::new(task), program_hash_function: HashFunc::Blake };

    let simple_bootloader_input =
        SimpleBootloaderInput { fact_topologies_path: None, single_page: true, tasks: vec![task_spec] };

    // Wrap the bootloader input as ProgramInput.
    let program_input = ProgramInput::from_value(simple_bootloader_input);

    // Resolve bootloader path.
    let bootloader_program_path = match proving_config.bootloader_program_path {
        Some(path) => path,
        None => resolve_resource_path(BOOTLOADER_FILE)?,
    };

    // Configure the proving process.
    let prove_config = ProveConfig {
        proof_path: proving_config.proof_output_path,
        proof_format: ProofFormat::Binary,
        verify: proving_config.verify,
        prover_params_json: proving_config.prover_params_path,
    };

    // Run the bootloader and generate a proof, passing the SNOS hint processor for VirtualOS
    // hints.
    stwo_run_and_prove(
        bootloader_program_path,
        Some(program_input),
        None, // program_output
        prove_config,
        Box::new(StwoProverEntryPoint),
        proving_config.debug_data_dir,
        proving_config.save_debug_data,
        Some(&mut snos_hint_processor),
    )
    .map_err(StwoDirectProvingError::StwoProving)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use blockifier::execution::call_info::CallInfo;
    use blockifier::execution::contract_class::TrackedResource;
    use blockifier::transaction::objects::TransactionExecutionInfo;
    use blockifier_reexecution::state_reader::rpc_objects::BlockId;
    use proving_utils::proof_encoding::ProofBytes;
    use rstest::rstest;
    use starknet_api::abi::abi_utils::selector_from_name;
    use starknet_api::block::GasPrice;
    use starknet_api::core::{ChainId, ContractAddress};
    use starknet_api::executable_transaction::Transaction as ExecutableTransaction;
    use starknet_api::execution_resources::GasAmount;
    use starknet_api::test_utils::invoke::{executable_invoke_tx, invoke_tx};
    use starknet_api::transaction::fields::{AllResourceBounds, ResourceBounds, ValidResourceBounds};
    use starknet_api::transaction::{InvokeTransaction, Transaction, TransactionHash};
    use starknet_api::{calldata, felt, invoke_tx_args};
    use starknet_os::io::os_input::{OsBlockInput, OsHintsConfig, StarknetOsInput};
    use tempfile::NamedTempFile;

    use super::*;
    use crate::runner::RpcRunnerFactory;
    use crate::test_utils::{
        fetch_sepolia_block_number,
        sepolia_runner_factory,
        DUMMY_ACCOUNT_ADDRESS,
        STRK_TOKEN_ADDRESS_SEPOLIA,
    };

    #[test]
    fn test_stwo_direct_proving_config_creation() {
        let config = StwoDirectProvingConfig {
            bootloader_program_path: Some(PathBuf::from("/path/to/bootloader.json")),
            proof_output_path: PathBuf::from("/path/to/proof.json"),
            verify: true,
            prover_params_path: None,
            debug_data_dir: Some(PathBuf::from("/tmp/debug")),
            save_debug_data: false,
        };

        assert!(config.verify);
        assert!(config.prover_params_path.is_none());
        assert!(config.debug_data_dir.is_some());
    }

    #[test]
    fn test_run_and_prove_validates_tracked_resources() {
        // Create minimal OsBlockInput with an invalid tracked resource.
        let mut call_info = CallInfo::default();
        call_info.tracked_resource = TrackedResource::CairoSteps; // Invalid for virtual OS.

        let tx_execution_info =
            TransactionExecutionInfo { execute_call_info: Some(call_info), ..Default::default() };

        // Create a dummy invoke transaction to pair with the execution info.
        let account_tx = executable_invoke_tx(Default::default());
        let tx = ExecutableTransaction::Account(account_tx);

        let mut os_block_input = OsBlockInput::default();
        os_block_input.transactions = vec![tx];
        os_block_input.tx_execution_infos = vec![tx_execution_info.into()];

        let os_hints = OsHints {
            os_hints_config: OsHintsConfig::default(),
            os_input: StarknetOsInput {
                os_block_inputs: vec![os_block_input],
                deprecated_compiled_classes: BTreeMap::new(),
                compiled_classes: BTreeMap::new(),
            },
        };

        let proving_config = StwoDirectProvingConfig {
            bootloader_program_path: Some(PathBuf::from("nonexistent.json")),
            proof_output_path: PathBuf::from("output.json"),
            verify: false,
            prover_params_path: None,
            debug_data_dir: None,
            save_debug_data: false,
        };

        // Should fail with InvalidTrackedResource error before attempting to prove.
        let result = run_and_prove_virtual_os(os_hints, proving_config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        match err {
            StwoDirectProvingError::InvalidTrackedResource { expected, actual } => {
                assert_eq!(expected, TrackedResource::SierraGas);
                assert_eq!(actual, TrackedResource::CairoSteps);
            }
            _ => panic!("Expected InvalidTrackedResource error, got: {:?}", err),
        }
    }

    /// Creates an invoke transaction that calls `balanceOf` on the STRK token.
    ///
    /// Uses the dummy account which requires no signature validation.
    /// The dummy account's `__execute__` format is: (contract_address, selector, calldata).
    fn strk_balance_of_invoke() -> (InvokeTransaction, TransactionHash) {
        let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS_SEPOLIA).unwrap();
        let account = ContractAddress::try_from(DUMMY_ACCOUNT_ADDRESS).unwrap();

        // Calldata matches dummy account's __execute__(contract_address, selector, calldata).
        let calldata = calldata![
            *strk_token.0.key(),
            selector_from_name("balanceOf").0,
            felt!("1"),
            *account.0.key()
        ];

        let resource_bounds = ValidResourceBounds::AllResources(AllResourceBounds {
            l1_gas: ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(0) },
            l2_gas: ResourceBounds {
                max_amount: GasAmount(10_000_000),
                max_price_per_unit: GasPrice(0),
            },
            l1_data_gas: ResourceBounds {
                max_amount: GasAmount(0),
                max_price_per_unit: GasPrice(0),
            },
        });

        let invoke = invoke_tx(invoke_tx_args! {
            sender_address: account,
            calldata,
            resource_bounds,
        });

        let tx_hash = Transaction::Invoke(invoke.clone())
            .calculate_transaction_hash(&ChainId::Sepolia)
            .unwrap();

        (invoke, tx_hash)
    }

    /// Integration test for the full stwo_direct proving flow with a balance_of transaction.
    ///
    /// Uses a dummy account on Sepolia that requires no signature validation.
    ///
    /// # Running
    ///
    /// ```bash
    /// SEPOLIA_NODE_URL=https://your-rpc-node \
    /// rustup run nightly-2025-07-14 cargo test -p starknet_os_runner \
    ///   --features stwo_native \
    ///   test_run_and_prove_virtual_os_with_balance_of_direct \
    ///   -- --ignored --nocapture
    /// ```
    #[rstest]
    #[tokio::test(flavor = "multi_thread")]
    #[ignore] // Requires RPC access and nightly Rust.
    async fn test_run_and_prove_virtual_os_with_balance_of_direct(
        sepolia_runner_factory: RpcRunnerFactory,
    ) {
        use std::time::Instant;

        let total_start = Instant::now();

        // Fetch the latest Sepolia block.
        let step_start = Instant::now();
        let block_number = fetch_sepolia_block_number().await;
        println!("[DIRECT] Fetch block number: {:?}", step_start.elapsed());

        // Create balance_of invoke transaction.
        let step_start = Instant::now();
        let (tx, tx_hash) = strk_balance_of_invoke();
        println!("[DIRECT] Create transaction: {:?}", step_start.elapsed());

        // Create runner and generate OS hints.
        let step_start = Instant::now();
        let runner = sepolia_runner_factory.create_runner(BlockId::Number(block_number));
        let os_hints = runner
            .create_virtual_os_hints(vec![(tx, tx_hash)])
            .await
            .expect("create_virtual_os_hints should succeed");
        println!("[DIRECT] Create OS hints: {:?}", step_start.elapsed());

        // Create temp file for proof output.
        let proof_file = NamedTempFile::new().expect("Failed to create temp file");
        let proof_path = proof_file.path().to_path_buf();

        // Configure proving.
        let proving_config = StwoDirectProvingConfig {
            bootloader_program_path: None,
            proof_output_path: proof_path.clone(),
            verify: false,
            prover_params_path: None,
            debug_data_dir: None,
            save_debug_data: false,
        };

        // Run and prove the virtual OS.
        let step_start = Instant::now();
        run_and_prove_virtual_os(os_hints, proving_config)
            .expect("run_and_prove_virtual_os should succeed");
        println!("[DIRECT] Run and prove (combined): {:?}", step_start.elapsed());

        // Verify proof file was created and is non-empty.
        let metadata = std::fs::metadata(&proof_path).expect("Proof file should exist");
        assert!(metadata.len() > 0, "Proof file should not be empty");
        println!("[DIRECT] Proof file size: {} bytes", metadata.len());

        // Read and verify the proof.
        let step_start = Instant::now();
        let proof_bytes = ProofBytes::from_file(&proof_path).expect("Failed to read proof file");
        let verify_output = apollo_transaction_converter::proof_verification::verify_proof(proof_bytes.into())
            .expect("Failed to verify proof");
        println!("[DIRECT] Verify proof: {:?}", step_start.elapsed());

        // Check that the program hash matches the expected bootloader hash.
        let expected_program_hash = starknet_types_core::felt::Felt::from_hex(
            apollo_transaction_converter::transaction_converter::BOOTLOADER_PROGRAM_HASH
        ).expect("Invalid bootloader hash");
        assert_eq!(
            verify_output.program_hash, expected_program_hash,
            "Program hash does not match expected bootloader hash"
        );

        println!("[DIRECT] Total time: {:?}", total_start.elapsed());
    }

    /// Integration test for the proving flow using CairoPie with a balance_of transaction.
    ///
    /// This test uses the same transaction as the direct test but proves via CairoPie
    /// (run OS first, then prove separately) for comparison.
    ///
    /// # Running
    ///
    /// ```bash
    /// SEPOLIA_NODE_URL=https://your-rpc-node \
    /// cargo test -p starknet_os_runner \
    ///   test_run_and_prove_virtual_os_with_balance_of_via_pie \
    ///   -- --ignored --nocapture
    /// ```
    #[rstest]
    #[tokio::test(flavor = "multi_thread")]
    #[ignore] // Requires RPC access.
    async fn test_run_and_prove_virtual_os_with_balance_of_via_pie(
        sepolia_runner_factory: RpcRunnerFactory,
    ) {
        use std::time::Instant;

        use starknet_os::runner::run_virtual_os;

        use crate::proving::prover::prove;

        let total_start = Instant::now();

        // Fetch the latest Sepolia block.
        let step_start = Instant::now();
        let block_number = fetch_sepolia_block_number().await;
        println!("[VIA_PIE] Fetch block number: {:?}", step_start.elapsed());

        // Create balance_of invoke transaction.
        let step_start = Instant::now();
        let (tx, tx_hash) = strk_balance_of_invoke();
        println!("[VIA_PIE] Create transaction: {:?}", step_start.elapsed());

        // Create runner and generate OS hints.
        let step_start = Instant::now();
        let runner = sepolia_runner_factory.create_runner(BlockId::Number(block_number));
        let os_hints = runner
            .create_virtual_os_hints(vec![(tx, tx_hash)])
            .await
            .expect("create_virtual_os_hints should succeed");
        println!("[VIA_PIE] Create OS hints: {:?}", step_start.elapsed());

        // Run the virtual OS to get a CairoPie.
        let step_start = Instant::now();
        let runner_output = run_virtual_os(os_hints).expect("run_virtual_os should succeed");
        println!("[VIA_PIE] Run virtual OS: {:?}", step_start.elapsed());

        // Prove the CairoPie.
        let step_start = Instant::now();
        let prover_output = prove(runner_output.cairo_pie).await.expect("prove should succeed");
        println!("[VIA_PIE] Prove CairoPie: {:?}", step_start.elapsed());

        // Verify the proof.
        let step_start = Instant::now();
        let verify_output = apollo_transaction_converter::proof_verification::verify_proof(prover_output.proof_bytes.clone().into())
            .expect("Failed to verify proof");
        println!("[VIA_PIE] Verify proof: {:?}", step_start.elapsed());

        // Check that the verified proof facts match the prover output.
        assert_eq!(
            verify_output.proof_facts, prover_output.proof_facts,
            "Verified proof facts do not match prover output"
        );

        // Check that the program hash matches the expected bootloader hash.
        let expected_program_hash = starknet_types_core::felt::Felt::from_hex(
            apollo_transaction_converter::transaction_converter::BOOTLOADER_PROGRAM_HASH
        ).expect("Invalid bootloader hash");
        assert_eq!(
            verify_output.program_hash, expected_program_hash,
            "Program hash does not match expected bootloader hash"
        );

        println!("[VIA_PIE] Total time: {:?}", total_start.elapsed());
    }
}
