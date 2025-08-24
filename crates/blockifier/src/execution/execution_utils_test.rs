

// TODO(AvivG): Add tests for:
// - `estimate_steps_of_encode_felt252_data_and_calc_blake_hash` simple cases (felts input).
// - `blake_opcode_count` simple cases (felts input).
// - `cost_of_encode_felt252_data_and_calc_blake_hash` simple cases (felts input) (including partial
//   remainder).
// - `cost_of_encode_felt252_data_and_calc_blake_hash` compare against actual execution resources
//   from running a Cairo entry point (computing blake).
// - base steps costs - compare against actual execution resources by running on an empty input.
