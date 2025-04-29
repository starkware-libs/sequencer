use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use cairo_lang_sierra::program::Program;
use cairo_lang_starknet_classes::compiler_version::VersionId;
use cairo_lang_starknet_classes::contract_class::ContractEntryPoints;
use cairo_native::execution_result::ContractExecutionResult;
use cairo_native::executor::AotContractExecutor;
use cairo_native::starknet::StarknetSyscallHandler;
use cairo_native::utils::BuiltinCosts;
use itertools::Itertools;
use sierra_emu::VirtualMachine;
use starknet_types_core::felt::Felt;

use super::syscall_handler::NativeSyscallHandler;

#[derive(Debug)]
pub enum ContractExecutor {
    Aot(AotContractExecutor),
    Emu((Arc<Program>, ContractEntryPoints, VersionId)),
    // must use a different variant as we need `Program` for trace feature
    #[cfg(feature = "with-trace-dump")]
    AotTrace((AotContractExecutor, Program)),
}

impl From<AotContractExecutor> for ContractExecutor {
    fn from(value: AotContractExecutor) -> Self {
        Self::Aot(value)
    }
}
impl From<(Arc<Program>, ContractEntryPoints, VersionId)> for ContractExecutor {
    fn from(value: (Arc<Program>, ContractEntryPoints, VersionId)) -> Self {
        Self::Emu(value)
    }
}

impl ContractExecutor {
    pub fn run(
        &self,
        selector: Felt,
        args: &[Felt],
        gas: u64,
        builtin_costs: Option<BuiltinCosts>,
        mut syscall_handler: &mut NativeSyscallHandler<'_>,
    ) -> cairo_native::error::Result<ContractExecutionResult> {
        match self {
            ContractExecutor::Aot(aot_contract_executor) => {
                aot_contract_executor.run(selector, args, gas, builtin_costs, syscall_handler)
            }
            ContractExecutor::Emu((program, entrypoints, version)) => {
                let mut virtual_machine =
                    VirtualMachine::new_starknet(program.to_owned(), entrypoints, *version);

                let builtin_costs = builtin_costs.map(|builtin_costs| sierra_emu::BuiltinCosts {
                    r#const: builtin_costs.r#const,
                    pedersen: builtin_costs.pedersen,
                    bitwise: builtin_costs.bitwise,
                    ecop: builtin_costs.ecop,
                    poseidon: builtin_costs.poseidon,
                    add_mod: builtin_costs.add_mod,
                    mul_mod: builtin_costs.mul_mod,
                });

                let args = args.to_owned();
                virtual_machine.call_contract(selector, gas, args, builtin_costs);

                let result = if cfg!(feature = "with-trace-dump") {
                    static COUNTER: AtomicU64 = AtomicU64::new(0);
                    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                    let trace = virtual_machine.run_with_trace(&mut syscall_handler);

                    let trace_path = PathBuf::from(format!("traces/emu/{counter}.json"));
                    let trace_parent_path = trace_path.parent().unwrap();
                    fs::create_dir_all(trace_parent_path).unwrap();
                    let trace_file = File::create(&trace_path).unwrap();
                    serde_json::to_writer_pretty(trace_file, &trace).unwrap();

                    let sierra_path = PathBuf::from(format!("traces/{counter}.sierra"));
                    let mut sierra_file = File::create(&sierra_path).unwrap();
                    write!(sierra_file, "{}", program).unwrap();

                    sierra_emu::ContractExecutionResult::from_trace(&trace).unwrap()
                } else {
                    virtual_machine.run(&mut syscall_handler).unwrap()
                };

                Ok(ContractExecutionResult {
                    remaining_gas: result.remaining_gas,
                    failure_flag: result.failure_flag,
                    return_values: result.return_values,
                    error_msg: result.error_msg,
                })
            }
            #[cfg(feature = "with-trace-dump")]
            ContractExecutor::AotTrace((executor, program)) => {
                use cairo_lang_sierra::program_registry::ProgramRegistry;
                use cairo_native::metadata::trace_dump::trace_dump_runtime::{
                    TraceDump,
                    TRACE_DUMP,
                };
                use cairo_native::metadata::trace_dump::TraceBinding;

                static COUNTER: AtomicU64 = AtomicU64::new(0);
                let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                TRACE_DUMP
                    .lock()
                    .unwrap()
                    .insert(counter, TraceDump::new(ProgramRegistry::new(&program).unwrap()));

                let trace_id = unsafe {
                    let trace_id_ptr =
                        executor.find_symbol_ptr(TraceBinding::TraceId.symbol()).unwrap();
                    trace_id_ptr.cast::<u64>().as_mut().unwrap()
                };

                let old_trace_id = *trace_id;
                *trace_id = counter;

                let result = executor.run(selector, args, gas, builtin_costs, syscall_handler);

                // Retreive trace dump for current execution
                let trace = TRACE_DUMP
                    .lock()
                    .unwrap()
                    .remove(&u64::try_from(counter).unwrap())
                    .unwrap()
                    .trace;

                // Save trace dump to file
                let trace_path = PathBuf::from(format!("traces/native/{counter}.json"));
                let trace_parent_path = trace_path.parent().unwrap();
                fs::create_dir_all(trace_parent_path).unwrap();
                let trace_file = File::create(&trace_path).unwrap();
                serde_json::to_writer_pretty(trace_file, &trace).unwrap();

                *trace_id = old_trace_id;

                result
            }
        }
    }
}

// doesn't contain any logic, it calls the underlying sequencer implementation
impl sierra_emu::starknet::StarknetSyscallHandler for &mut NativeSyscallHandler<'_> {
    fn get_block_hash(
        &mut self,
        block_number: u64,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<Felt> {
        StarknetSyscallHandler::get_block_hash(self, block_number, remaining_gas)
    }

    fn get_execution_info(
        &mut self,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<sierra_emu::starknet::ExecutionInfo> {
        StarknetSyscallHandler::get_execution_info(self, remaining_gas).map(convert_execution_info)
    }

    fn get_execution_info_v2(
        &mut self,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<sierra_emu::starknet::ExecutionInfoV2> {
        StarknetSyscallHandler::get_execution_info_v2(self, remaining_gas)
            .map(convert_execution_info_v2)
    }

    fn deploy(
        &mut self,
        class_hash: Felt,
        contract_address_salt: Felt,
        calldata: Vec<Felt>,
        deploy_from_zero: bool,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<(Felt, Vec<Felt>)> {
        StarknetSyscallHandler::deploy(
            self,
            class_hash,
            contract_address_salt,
            &calldata,
            deploy_from_zero,
            remaining_gas,
        )
    }

    fn replace_class(
        &mut self,
        class_hash: Felt,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<()> {
        StarknetSyscallHandler::replace_class(self, class_hash, remaining_gas)
    }

    fn library_call(
        &mut self,
        class_hash: Felt,
        function_selector: Felt,
        calldata: Vec<Felt>,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<Vec<Felt>> {
        StarknetSyscallHandler::library_call(
            self,
            class_hash,
            function_selector,
            &calldata,
            remaining_gas,
        )
    }

    fn call_contract(
        &mut self,
        address: Felt,
        entry_point_selector: Felt,
        calldata: Vec<Felt>,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<Vec<Felt>> {
        StarknetSyscallHandler::call_contract(
            self,
            address,
            entry_point_selector,
            &calldata,
            remaining_gas,
        )
    }

    fn storage_read(
        &mut self,
        address_domain: u32,
        address: Felt,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<Felt> {
        StarknetSyscallHandler::storage_read(self, address_domain, address, remaining_gas)
    }

    fn storage_write(
        &mut self,
        address_domain: u32,
        address: Felt,
        value: Felt,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<()> {
        StarknetSyscallHandler::storage_write(self, address_domain, address, value, remaining_gas)
    }

    fn emit_event(
        &mut self,
        keys: Vec<Felt>,
        data: Vec<Felt>,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<()> {
        StarknetSyscallHandler::emit_event(self, &keys, &data, remaining_gas)
    }

    fn send_message_to_l1(
        &mut self,
        to_address: Felt,
        payload: Vec<Felt>,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<()> {
        StarknetSyscallHandler::send_message_to_l1(self, to_address, &payload, remaining_gas)
    }

    fn keccak(
        &mut self,
        input: Vec<u64>,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<sierra_emu::starknet::U256> {
        StarknetSyscallHandler::keccak(self, &input, remaining_gas).map(convert_u256)
    }

    fn secp256k1_new(
        &mut self,
        x: sierra_emu::starknet::U256,
        y: sierra_emu::starknet::U256,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<Option<sierra_emu::starknet::Secp256k1Point>> {
        StarknetSyscallHandler::secp256k1_new(
            self,
            convert_from_u256(x),
            convert_from_u256(y),
            remaining_gas,
        )
        .map(|x| x.map(convert_secp_256_k1_point))
    }

    fn secp256k1_add(
        &mut self,
        p0: sierra_emu::starknet::Secp256k1Point,
        p1: sierra_emu::starknet::Secp256k1Point,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<sierra_emu::starknet::Secp256k1Point> {
        StarknetSyscallHandler::secp256k1_add(
            self,
            convert_from_secp_256_k1_point(p0),
            convert_from_secp_256_k1_point(p1),
            remaining_gas,
        )
        .map(convert_secp_256_k1_point)
    }

    fn secp256k1_mul(
        &mut self,
        p: sierra_emu::starknet::Secp256k1Point,
        m: sierra_emu::starknet::U256,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<sierra_emu::starknet::Secp256k1Point> {
        StarknetSyscallHandler::secp256k1_mul(
            self,
            convert_from_secp_256_k1_point(p),
            convert_from_u256(m),
            remaining_gas,
        )
        .map(convert_secp_256_k1_point)
    }

    fn secp256k1_get_point_from_x(
        &mut self,
        x: sierra_emu::starknet::U256,
        y_parity: bool,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<Option<sierra_emu::starknet::Secp256k1Point>> {
        StarknetSyscallHandler::secp256k1_get_point_from_x(
            self,
            convert_from_u256(x),
            y_parity,
            remaining_gas,
        )
        .map(|x| x.map(convert_secp_256_k1_point))
    }

    fn secp256k1_get_xy(
        &mut self,
        p: sierra_emu::starknet::Secp256k1Point,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<(sierra_emu::starknet::U256, sierra_emu::starknet::U256)>
    {
        StarknetSyscallHandler::secp256k1_get_xy(
            self,
            convert_from_secp_256_k1_point(p),
            remaining_gas,
        )
        .map(|(x, y)| (convert_u256(x), convert_u256(y)))
    }

    fn secp256r1_new(
        &mut self,
        x: sierra_emu::starknet::U256,
        y: sierra_emu::starknet::U256,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<Option<sierra_emu::starknet::Secp256r1Point>> {
        StarknetSyscallHandler::secp256r1_new(
            self,
            convert_from_u256(x),
            convert_from_u256(y),
            remaining_gas,
        )
        .map(|x| x.map(convert_secp_256_r1_point))
    }

    fn secp256r1_add(
        &mut self,
        p0: sierra_emu::starknet::Secp256r1Point,
        p1: sierra_emu::starknet::Secp256r1Point,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<sierra_emu::starknet::Secp256r1Point> {
        StarknetSyscallHandler::secp256r1_add(
            self,
            convert_from_secp_256_r1_point(p0),
            convert_from_secp_256_r1_point(p1),
            remaining_gas,
        )
        .map(convert_secp_256_r1_point)
    }

    fn secp256r1_mul(
        &mut self,
        p: sierra_emu::starknet::Secp256r1Point,
        m: sierra_emu::starknet::U256,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<sierra_emu::starknet::Secp256r1Point> {
        StarknetSyscallHandler::secp256r1_mul(
            self,
            convert_from_secp_256_r1_point(p),
            convert_from_u256(m),
            remaining_gas,
        )
        .map(convert_secp_256_r1_point)
    }

    fn secp256r1_get_point_from_x(
        &mut self,
        x: sierra_emu::starknet::U256,
        y_parity: bool,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<Option<sierra_emu::starknet::Secp256r1Point>> {
        StarknetSyscallHandler::secp256r1_get_point_from_x(
            self,
            convert_from_u256(x),
            y_parity,
            remaining_gas,
        )
        .map(|x| x.map(convert_secp_256_r1_point))
    }

    fn secp256r1_get_xy(
        &mut self,
        p: sierra_emu::starknet::Secp256r1Point,
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<(sierra_emu::starknet::U256, sierra_emu::starknet::U256)>
    {
        StarknetSyscallHandler::secp256r1_get_xy(
            self,
            convert_from_secp_256_r1_point(p),
            remaining_gas,
        )
        .map(|(x, y)| (convert_u256(x), convert_u256(y)))
    }

    fn sha256_process_block(
        &mut self,
        mut prev_state: [u32; 8],
        current_block: [u32; 16],
        remaining_gas: &mut u64,
    ) -> sierra_emu::starknet::SyscallResult<[u32; 8]> {
        StarknetSyscallHandler::sha256_process_block(
            self,
            &mut prev_state,
            &current_block,
            remaining_gas,
        )?;

        Ok(prev_state)
    }
}

// The Sierra Emu and the Native syscall handler have different types (although they are identical).
// The following functions help to convert between them.

fn convert_u256(x: cairo_native::starknet::U256) -> sierra_emu::starknet::U256 {
    sierra_emu::starknet::U256 { lo: x.lo, hi: x.hi }
}

fn convert_from_u256(x: sierra_emu::starknet::U256) -> cairo_native::starknet::U256 {
    cairo_native::starknet::U256 { lo: x.lo, hi: x.hi }
}

fn convert_secp_256_k1_point(
    x: cairo_native::starknet::Secp256k1Point,
) -> sierra_emu::starknet::Secp256k1Point {
    sierra_emu::starknet::Secp256k1Point { x: convert_u256(x.x), y: convert_u256(x.y) }
}

fn convert_from_secp_256_k1_point(
    x: sierra_emu::starknet::Secp256k1Point,
) -> cairo_native::starknet::Secp256k1Point {
    cairo_native::starknet::Secp256k1Point {
        x: convert_from_u256(x.x),
        y: convert_from_u256(x.y),
        is_infinity: false,
    }
}

fn convert_secp_256_r1_point(
    x: cairo_native::starknet::Secp256r1Point,
) -> sierra_emu::starknet::Secp256r1Point {
    sierra_emu::starknet::Secp256r1Point { x: convert_u256(x.x), y: convert_u256(x.y) }
}
fn convert_from_secp_256_r1_point(
    x: sierra_emu::starknet::Secp256r1Point,
) -> cairo_native::starknet::Secp256r1Point {
    cairo_native::starknet::Secp256r1Point {
        x: convert_from_u256(x.x),
        y: convert_from_u256(x.y),
        is_infinity: false,
    }
}

fn convert_execution_info(
    x: cairo_native::starknet::ExecutionInfo,
) -> sierra_emu::starknet::ExecutionInfo {
    sierra_emu::starknet::ExecutionInfo {
        block_info: convert_block_info(x.block_info),
        tx_info: convert_tx_info(x.tx_info),
        caller_address: x.caller_address,
        contract_address: x.contract_address,
        entry_point_selector: x.entry_point_selector,
    }
}

fn convert_tx_info(x: cairo_native::starknet::TxInfo) -> sierra_emu::starknet::TxInfo {
    sierra_emu::starknet::TxInfo {
        version: x.version,
        account_contract_address: x.account_contract_address,
        max_fee: x.max_fee,
        signature: x.signature,
        transaction_hash: x.transaction_hash,
        chain_id: x.chain_id,
        nonce: x.nonce,
    }
}

fn convert_execution_info_v2(
    x: cairo_native::starknet::ExecutionInfoV2,
) -> sierra_emu::starknet::ExecutionInfoV2 {
    sierra_emu::starknet::ExecutionInfoV2 {
        block_info: convert_block_info(x.block_info),
        tx_info: convert_tx_v2_info(x.tx_info),
        caller_address: x.caller_address,
        contract_address: x.contract_address,
        entry_point_selector: x.entry_point_selector,
    }
}

fn convert_tx_v2_info(x: cairo_native::starknet::TxV2Info) -> sierra_emu::starknet::TxV2Info {
    sierra_emu::starknet::TxV2Info {
        version: x.version,
        account_contract_address: x.account_contract_address,
        max_fee: x.max_fee,
        signature: x.signature,
        transaction_hash: x.transaction_hash,
        chain_id: x.chain_id,
        nonce: x.nonce,
        resource_bounds: x.resource_bounds.into_iter().map(convert_resource_bounds).collect_vec(),
        tip: x.tip,
        paymaster_data: x.paymaster_data,
        nonce_data_availability_mode: x.nonce_data_availability_mode,
        fee_data_availability_mode: x.fee_data_availability_mode,
        account_deployment_data: x.account_deployment_data,
    }
}

fn convert_resource_bounds(
    resource_bounds: cairo_native::starknet::ResourceBounds,
) -> sierra_emu::starknet::ResourceBounds {
    sierra_emu::starknet::ResourceBounds {
        resource: resource_bounds.resource,
        max_amount: resource_bounds.max_amount,
        max_price_per_unit: resource_bounds.max_price_per_unit,
    }
}

fn convert_block_info(x: cairo_native::starknet::BlockInfo) -> sierra_emu::starknet::BlockInfo {
    sierra_emu::starknet::BlockInfo {
        block_number: x.block_number,
        block_timestamp: x.block_timestamp,
        sequencer_address: x.sequencer_address,
    }
}
