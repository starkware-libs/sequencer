use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::runners::cairo_runner::CairoRunner;
use starknet_types_core::felt::Felt;

fn instruction_to_u128(felt: &Felt) -> Result<u128, &'static str> {
    let bytes: [u8; 32] = felt.to_bytes_be();
    assert!(bytes[0..16] == [0; 16], "Instruction should fit in 128 bits");
    let u128_bytes: [u8; 16] = bytes[16..32].try_into().unwrap();
    Ok(u128::from_be_bytes(u128_bytes))
}

#[derive(Debug, PartialEq, Eq)]
enum OpcodeExt {
    Stone,         // 0
    Blake,         // 1
    BlakeFinalize, // 2
    QM31Operation, // 3
    Unknown,
}

pub struct OpcodeInstanceCounts {
    pub blake_opcode_count: usize,
}

/// Decode an encoded instruction (u128) into its components.
fn decode_instruction(mut encoded: u128) -> (bool, bool, bool, OpcodeExt) {
    // Skip the 3 offsets (3 * 16 = 48 bits).
    encoded >>= 48;

    // Read flags in the correct order (LSB-first).
    let _dst_base_fp = (encoded & 1) != 0;
    encoded >>= 1;
    let _op0_base_fp = (encoded & 1) != 0;
    encoded >>= 1;

    let op_1_imm = (encoded & 1) != 0;
    encoded >>= 1;
    let op_1_base_fp = (encoded & 1) != 0;
    encoded >>= 1;
    let op_1_base_ap = (encoded & 1) != 0;
    encoded >>= 1;

    let res_add = (encoded & 1) != 0;
    encoded >>= 1;
    let res_mul = (encoded & 1) != 0;
    encoded >>= 1;
    let pc_update_jump = (encoded & 1) != 0;
    encoded >>= 1;
    let pc_update_jump_rel = (encoded & 1) != 0;
    encoded >>= 1;
    let pc_update_jnz = (encoded & 1) != 0;
    encoded >>= 1;
    let ap_update_add = (encoded & 1) != 0;
    encoded >>= 1;
    let _ap_update_add_1 = (encoded & 1) != 0;
    encoded >>= 1;
    let opcode_call = (encoded & 1) != 0;
    encoded >>= 1;
    let opcode_ret = (encoded & 1) != 0;
    encoded >>= 1;
    let opcode_assert_eq = (encoded & 1) != 0;
    encoded >>= 1;

    let unwanted_flags = op_1_imm
        || res_add
        || res_mul
        || pc_update_jump
        || pc_update_jump_rel
        || pc_update_jnz
        || ap_update_add
        || opcode_call
        || opcode_ret
        || opcode_assert_eq;

    // Remaining bits are the opcode extension.
    let opcode_extension = match encoded {
        0 => OpcodeExt::Stone,
        1 => OpcodeExt::Blake,
        2 => OpcodeExt::BlakeFinalize,
        3 => OpcodeExt::QM31Operation,
        _ => OpcodeExt::Unknown,
    };

    (unwanted_flags, op_1_base_fp, op_1_base_ap, opcode_extension)
}

/// Check if a decoded instruction is a Blake opcode.
fn is_blake_opcode(
    unwanted_flags: bool,
    op_1_base_fp: bool,
    op_1_base_ap: bool,
    opcode_extension: &OpcodeExt,
) -> bool {
    // Blake opcodes have:
    // - opcode_extension is Blake or BlakeFinalize
    // - no unwanted flags (op_1_imm, res_add, res_mul, etc.)
    // - the data we hash is stored on ap or fp exclusively (XOR)
    matches!(opcode_extension, OpcodeExt::Blake | OpcodeExt::BlakeFinalize)
        && !unwanted_flags
        && (op_1_base_fp ^ op_1_base_ap)
}

/// Count Blake opcodes from the Cairo runner's execution trace.
pub fn get_opcode_instances(runner: &CairoRunner) -> OpcodeInstanceCounts {
    let Ok(info) = runner.get_prover_input_info() else {
        eprintln!("Failed to get prover input info. Returning zero count.");
        return OpcodeInstanceCounts { blake_opcode_count: 0 };
    };

    let count = info
        .relocatable_trace
        .iter()
        .filter(|entry| {
            (|| {
                let seg = usize::try_from(entry.pc.segment_index).ok()?;
                let value = info.relocatable_memory.get(seg)?.get(entry.pc.offset)?.as_ref()?;
                let MaybeRelocatable::Int(felt) = value else { return None };
                let instr = instruction_to_u128(felt).ok()?;
                let (unwanted_flags, op1_fp, op1_ap, opcode_ext) = decode_instruction(instr);
                Some(is_blake_opcode(unwanted_flags, op1_fp, op1_ap, &opcode_ext))
            })()
            .unwrap_or(false)
        })
        .count();

    OpcodeInstanceCounts { blake_opcode_count: count }
}
