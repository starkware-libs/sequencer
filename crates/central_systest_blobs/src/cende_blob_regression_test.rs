use std::sync::LazyLock;

use blockifier::context::ChainInfo;
use expect_test::expect_file;
use serde_json;
use starknet_api::core::{ChainId, OsChainInfo};

static CHAIN_ID: LazyLock<ChainId> =
    LazyLock::new(|| ChainId::Other("SN_PREINTEGRATION_SEPOLIA".to_string()));
static CHAIN_INFO: LazyLock<ChainInfo> =
    LazyLock::new(|| ChainInfo { chain_id: CHAIN_ID.clone(), ..ChainInfo::create_for_testing() });

const CHAIN_INFO_PATH: &str = "../resources/chain_info.json";

#[test]
fn test_make_data() {
    expect_file![CHAIN_INFO_PATH].assert_eq(
        &serde_json::to_string_pretty(&OsChainInfo::from(&*CHAIN_INFO).to_hex_hashmap()).unwrap(),
    );
}
