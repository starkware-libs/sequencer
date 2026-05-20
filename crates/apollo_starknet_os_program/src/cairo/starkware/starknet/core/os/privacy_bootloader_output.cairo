from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_blake2s.blake2s import encode_felt252_data_and_calc_blake_hash
from starkware.cairo.common.math import split_felt, unsigned_div_rem
from starkware.cairo.common.memcpy import memcpy
from starkware.starknet.core.os.qm31_blake import qm31_blake

// Number of M31 limbs / QM31s holding the privacy bootloader output: the 252-bit blake hash of the
// bootloader output preimage is decomposed into 28 little-endian 9-bit M31 limbs, which group into
// 28 / 4 = 7 QM31s.
const PRIVACY_BOOTLOADER_OUTPUT_N_QM31S = 7;

// Decomposes a 252-bit felt into 28 little-endian 9-bit limbs written to `limbs`.
// Each limb is range-checked to [0, 2^9) via `unsigned_div_rem`.
//
// `unsigned_div_rem` only supports dividends < 2^128, so the felt is first split via
// `split_felt` into a 128-bit `low` half and a ~124-bit `high` half. Limbs 0..13 come
// entirely from `low` (covering bits 0..126); limbs 15..27 come entirely from `high`
// (covering bits 135..252); limb 14 spans the boundary (bits 126..127 from `low`,
// bits 128..134 from `high`).
func felt252_to_9bit_m31_limbs{range_check_ptr}(value: felt, limbs: felt*) {
    alloc_locals;
    const LIMB_BASE = 512;  // 2^9
    const LOW_TOP_BASE = 4;  // 2^2 — the top 2 bits of `low` that overflow into limb 14.
    const HIGH_FIRST_LIMB_BASE = 128;  // 2^7 — the low 7 bits of `high` that fill limb 14.

    let (high, low) = split_felt(value);

    // 14 little-endian 9-bit limbs from `low` (covers bits 0..126).
    let (low_after_1, l0) = unsigned_div_rem(low, LIMB_BASE);
    assert limbs[0] = l0;
    let (low_after_2, l1) = unsigned_div_rem(low_after_1, LIMB_BASE);
    assert limbs[1] = l1;
    let (low_after_3, l2) = unsigned_div_rem(low_after_2, LIMB_BASE);
    assert limbs[2] = l2;
    let (low_after_4, l3) = unsigned_div_rem(low_after_3, LIMB_BASE);
    assert limbs[3] = l3;
    let (low_after_5, l4) = unsigned_div_rem(low_after_4, LIMB_BASE);
    assert limbs[4] = l4;
    let (low_after_6, l5) = unsigned_div_rem(low_after_5, LIMB_BASE);
    assert limbs[5] = l5;
    let (low_after_7, l6) = unsigned_div_rem(low_after_6, LIMB_BASE);
    assert limbs[6] = l6;
    let (low_after_8, l7) = unsigned_div_rem(low_after_7, LIMB_BASE);
    assert limbs[7] = l7;
    let (low_after_9, l8) = unsigned_div_rem(low_after_8, LIMB_BASE);
    assert limbs[8] = l8;
    let (low_after_10, l9) = unsigned_div_rem(low_after_9, LIMB_BASE);
    assert limbs[9] = l9;
    let (low_after_11, l10) = unsigned_div_rem(low_after_10, LIMB_BASE);
    assert limbs[10] = l10;
    let (low_after_12, l11) = unsigned_div_rem(low_after_11, LIMB_BASE);
    assert limbs[11] = l11;
    let (low_after_13, l12) = unsigned_div_rem(low_after_12, LIMB_BASE);
    assert limbs[12] = l12;
    let (low_after_14, l13) = unsigned_div_rem(low_after_13, LIMB_BASE);
    assert limbs[13] = l13;

    // After 14 9-bit subtractions `low_after_14 = low >> 126` is at most 2 bits wide.
    // Range-check it as the *remainder* of a div-by-4 (the quotient must be zero).
    let (low_top_quotient, low_top) = unsigned_div_rem(low_after_14, LOW_TOP_BASE);
    assert low_top_quotient = 0;

    // Limb 14: 2 low bits from `low_top` || 7 low bits of `high`. Assignment also serves as the
    // 7-bit range check (the remainder of `unsigned_div_rem(high, 2^7)` is in [0, 2^7), and
    // limbs[14] = low_top + high_low_7 * 4 < 4 + 127 * 4 = 512).
    let (high_after_1, high_low_7) = unsigned_div_rem(high, HIGH_FIRST_LIMB_BASE);
    assert limbs[14] = low_top + high_low_7 * LOW_TOP_BASE;

    // 13 little-endian 9-bit limbs from `high_after_1` (covers bits 7..124 of `high`).
    let (high_after_2, h15) = unsigned_div_rem(high_after_1, LIMB_BASE);
    assert limbs[15] = h15;
    let (high_after_3, h16) = unsigned_div_rem(high_after_2, LIMB_BASE);
    assert limbs[16] = h16;
    let (high_after_4, h17) = unsigned_div_rem(high_after_3, LIMB_BASE);
    assert limbs[17] = h17;
    let (high_after_5, h18) = unsigned_div_rem(high_after_4, LIMB_BASE);
    assert limbs[18] = h18;
    let (high_after_6, h19) = unsigned_div_rem(high_after_5, LIMB_BASE);
    assert limbs[19] = h19;
    let (high_after_7, h20) = unsigned_div_rem(high_after_6, LIMB_BASE);
    assert limbs[20] = h20;
    let (high_after_8, h21) = unsigned_div_rem(high_after_7, LIMB_BASE);
    assert limbs[21] = h21;
    let (high_after_9, h22) = unsigned_div_rem(high_after_8, LIMB_BASE);
    assert limbs[22] = h22;
    let (high_after_10, h23) = unsigned_div_rem(high_after_9, LIMB_BASE);
    assert limbs[23] = h23;
    let (high_after_11, h24) = unsigned_div_rem(high_after_10, LIMB_BASE);
    assert limbs[24] = h24;
    let (high_after_12, h25) = unsigned_div_rem(high_after_11, LIMB_BASE);
    assert limbs[25] = h25;
    let (high_after_13, h26) = unsigned_div_rem(high_after_12, LIMB_BASE);
    assert limbs[26] = h26;
    let (high_final, h27) = unsigned_div_rem(high_after_13, LIMB_BASE);
    assert limbs[27] = h27;
    assert high_final = 0;
    return ();
}

// Computes the privacy bootloader output hash directly from `proof_facts`.
// Reproduces the Rust pipeline used by the privacy recursive verifier:
//   1. Build the bootloader output preimage from `proof_facts`.
//   2. Blake-hash the preimage.
//   3. Decompose the 252-bit hash into 28 little-endian 9-bit M31 limbs (= 7 QM31s).
//   4. Blake-hash the 7 QM31s.
//
// Caller must guarantee `proof_facts_size > 0`. (Callers in the OS gate this on
// `proof_facts_size != 0`; the empty case has no proof to verify and the result would be
// discarded.) `check_proof_facts` further requires `proof_facts_size >= 8` when it is non-zero,
// so `proof_facts_size - 2 > 0` here.
func compute_privacy_bootloader_output_hash{range_check_ptr}(
    proof_facts_size: felt, proof_facts: felt*
) -> (
    out0_a: felt,
    out0_b: felt,
    out0_c: felt,
    out0_d: felt,
    out1_a: felt,
    out1_b: felt,
    out1_c: felt,
    out1_d: felt,
) {
    alloc_locals;

    // Preimage = [1, proof_facts_size - 1, proof_facts[2], ..., proof_facts[size - 1]].
    // Drops `proof_facts[0..2]` (proof version + variant) and prepends the bootloader's
    // single-task header `[num_tasks=1, output_size=size-1]`.
    let (local preimage: felt*) = alloc();
    assert preimage[0] = 1;
    assert preimage[1] = proof_facts_size - 1;
    memcpy(dst=&preimage[2], src=&proof_facts[2], len=proof_facts_size - 2);

    let (preimage_blake_hash) = encode_felt252_data_and_calc_blake_hash(
        data_len=proof_facts_size, data=preimage
    );
    let (local bootloader_output_m31s: felt*) = alloc();
    felt252_to_9bit_m31_limbs(value=preimage_blake_hash, limbs=bootloader_output_m31s);

    return qm31_blake(n_qm31=PRIVACY_BOOTLOADER_OUTPUT_N_QM31S, data=bootloader_output_m31s);
}
