use expect_test::{expect, Expect};

/// The STRK fee token address that is deployed when initializing the default initial state used
/// by the virtual-OS flow tests and the apollo proof-flow integration tests. The resulting
/// address depends on the nonce of the deploying account — if extra init transactions are added
/// before the STRK fee token is deployed, the address must be updated. Run any failing test with
/// `UPDATE_EXPECT=1` to refresh, then regenerate the proof fixtures by running
/// `cargo +nightly-2025-07-14 test -p starknet_os_flow_tests --features
/// starknet_transaction_prover/stwo_proving --release generate_proof_fixtures -- --ignored`.
pub const EXPECTED_STRK_FEE_TOKEN_ADDRESS: Expect =
<<<<<<< HEAD
    expect!["0x2420d9498ea75b47b95e3102b4b19b2bde5fa29d5cdc666a5b13d0993c778bc"];
||||||| b392cf22a9
    expect!["0x40d4791db41c1685ab040ebd1580fac0fb4a51bd00fad38db281f5ed9ec7196"];
=======
    expect!["0x216e06f4761eb833ec9fbc9d08ae554427a2e6f23539d669a26d7e9997222b3"];
>>>>>>> origin/main-v0.14.3
