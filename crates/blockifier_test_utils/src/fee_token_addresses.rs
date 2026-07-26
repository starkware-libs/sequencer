use expect_test::{expect, Expect};

/// The STRK fee token address that is deployed when initializing the default initial state used
/// by the virtual-OS flow tests and the apollo proof-flow integration tests. The resulting
/// address depends on the nonce of the deploying account — if extra init transactions are added
/// before the STRK fee token is deployed, the address must be updated. Run any failing test with
/// `UPDATE_EXPECT=1` to refresh, then regenerate the proof fixtures by running
/// `cargo +nightly-2026-01-15 test -p starknet_os_flow_tests --features
/// starknet_transaction_prover/stwo_proving --release generate_proof_fixtures -- --ignored`.
pub const EXPECTED_STRK_FEE_TOKEN_ADDRESS: Expect =
<<<<<<< HEAD
    expect!["0x2be5c606d0786bbcd31c8ef9b982f731f751ac15d98c9908fe7eb8e42ad295c"];
||||||| 9f78ee7cef
    expect!["0x216e06f4761eb833ec9fbc9d08ae554427a2e6f23539d669a26d7e9997222b3"];
=======
    expect!["0x70b02b86e0dc454e5d07f9015f103f571f697d4f85dfeac10dcbd8a296a893e"];
>>>>>>> origin/main-v0.14.3
