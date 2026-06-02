use expect_test::{expect, Expect};

/// The STRK fee token address that is deployed when initializing the default initial state used
/// by the virtual-OS flow tests and the apollo proof-flow integration tests. The resulting
/// address depends on the nonce of the deploying account — if extra init transactions are added
/// before the STRK fee token is deployed, the address must be updated. Run any failing test with
/// `UPDATE_EXPECT=1` to refresh, then regenerate the proof fixtures by running
/// `cargo +nightly-2025-07-14 test -p starknet_os_flow_tests --features
/// starknet_transaction_prover/stwo_proving --release generate_proof_fixtures -- --ignored`.
pub const EXPECTED_STRK_FEE_TOKEN_ADDRESS: Expect =
    expect!["0x2420d9498ea75b47b95e3102b4b19b2bde5fa29d5cdc666a5b13d0993c778bc"];
