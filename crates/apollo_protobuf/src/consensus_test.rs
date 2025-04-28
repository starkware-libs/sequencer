// ConsensusBlockInfo tests.

use starknet_api::block::GasPrice;

#[test]
fn wei_to_fri_converts_correctly() {
    // Conversion rate if 1 ETH = 800 STRK.
    let conversion_rate = 8 * u128::pow(10, 20);
    let price_in_wei = GasPrice(5);
    let price_in_fri = GasPrice(4000);
    assert_eq!(price_in_wei.wei_to_fri(conversion_rate), price_in_fri);
    assert_eq!(price_in_fri.fri_to_wei(conversion_rate), price_in_wei);
}

#[test]
#[should_panic]
fn wei_to_fri_panics_on_gas_too_high() {
    GasPrice(u128::pow(2, 127)).wei_to_fri(4);
}
