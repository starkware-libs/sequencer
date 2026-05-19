from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_blake2s.blake2s import blake_with_opcode
from starkware.cairo.common.math import unsigned_div_rem

// Blake2s-256 over a slice of QM31 elements (only for multiples of 16 bytes).
//
// Input layout: `data` holds `n_qm31 * 4` consecutive felts; each group of 4
// felts is one QM31's M31 limbs (a, b, c, d), each a valid M31 (< 2^31 - 1).
// Serialization matches existing rust implementations: four little-endian u32s per QM31.
//
// Output: the 32-byte Blake2s digest as 8 M31 felts. Each output u32 word is
// reduced modulo P = 2^31 - 1.
func qm31_blake{range_check_ptr}(n_qm31: felt, data: felt*) -> (
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

    let (local digest: felt*) = alloc();
    blake_with_opcode(len=n_qm31 * 4, data=data, out=digest);

    // P = 2^31 - 1.
    const M31_P = 2147483647;
    let (_, r0) = unsigned_div_rem(digest[0], M31_P);
    let (_, r1) = unsigned_div_rem(digest[1], M31_P);
    let (_, r2) = unsigned_div_rem(digest[2], M31_P);
    let (_, r3) = unsigned_div_rem(digest[3], M31_P);
    let (_, r4) = unsigned_div_rem(digest[4], M31_P);
    let (_, r5) = unsigned_div_rem(digest[5], M31_P);
    let (_, r6) = unsigned_div_rem(digest[6], M31_P);
    let (_, r7) = unsigned_div_rem(digest[7], M31_P);

    return (out0_a=r0, out0_b=r1, out0_c=r2, out0_d=r3, out1_a=r4, out1_b=r5, out1_c=r6, out1_d=r7);
}
