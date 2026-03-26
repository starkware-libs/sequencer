use std::sync::LazyLock;

use expect_test::expect;

use super::EthL1L2MessageHash;
use crate::core::EntryPointSelector;
use crate::transaction::L1HandlerTransaction;
use crate::{calldata, contract_address, felt, nonce};

// A transaction from MAINNET with tx hash
// 0x439e12f67962c353182d72b4af12c3f11eaba4b36e552aebcdcd6db66971bdb.
static L1_HANDLER_TX: LazyLock<L1HandlerTransaction> = LazyLock::new(|| L1HandlerTransaction {
    version: L1HandlerTransaction::VERSION,
    nonce: nonce!(0x18e94d),
    contract_address: contract_address!(
        "0x73314940630fd6dcda0d772d4c972c4e0a9946bef9dabf4ef84eda8ef542b82"
    ),
    entry_point_selector: EntryPointSelector(felt!(
        "0x1b64b1b3b690b43b9b514fb81377518f4039cd3e4f4914d8a6bdf01d679fb19"
    )),
    calldata: calldata![
        felt!("0xae0ee0a63a2ce6baeeffe56e7714fb4efe48d419"),
        felt!("0x455448"),
        felt!("0xc27947400e26e534e677afc2e9b2ec1bab14fc89"),
        felt!("0x4af4754baf89f1b8b449215a8ea7ce558824a33a5393eaa3829658549f2bfa2"),
        felt!("0x9184e72a000"),
        felt!("0x0")
    ],
});

#[test]
fn l1_handler_eth_msg_hash() {
    let eth_msg_hash = format!("{}", L1_HANDLER_TX.calc_eth_msg_hash());
    expect!["0x99b2a7830e1c860734b308d90bb05b0e09ecda0a2b243ecddb12c50bdebaa3a9"]
        .assert_eq(&eth_msg_hash);
}

#[test]
fn eth_l1l2_message_hash_serde() {
    let eth_msg_hash = L1_HANDLER_TX.calc_eth_msg_hash();
    let serialized = serde_json::to_string(&eth_msg_hash).unwrap();
    let deserialized = serde_json::from_str::<EthL1L2MessageHash>(&serialized).unwrap();
    assert_eq!(deserialized, eth_msg_hash);
}
