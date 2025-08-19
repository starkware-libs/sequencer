// ConsensusBlockInfo tests.

use starknet_api::block::GasPrice;
use starknet_api::StarknetApiError;

#[test]
fn wei_to_fri_converts_correctly() {
    // Conversion rate if 1 ETH = 800 STRK.
    let conversion_rate = 8 * u128::pow(10, 20);
    let price_in_wei = GasPrice(5);
    let price_in_fri = GasPrice(4000);
    assert_eq!(price_in_wei.wei_to_fri(conversion_rate).unwrap(), price_in_fri);
    assert_eq!(price_in_fri.fri_to_wei(conversion_rate).unwrap(), price_in_wei);
}

#[test]
fn wei_to_fri_errors_on_gas_too_high() {
    assert!(
        GasPrice(u128::pow(2, 127)).wei_to_fri(4)
            == Err(StarknetApiError::GasPriceConversionError("Gas price is too high".to_string()))
    );
}

#[test]
fn fri_to_wei_errors_on_gas_too_high() {
    // Note this fails even if rate is 1, since we first multiply by WEI_PER_ETH=10^9
    assert!(
        GasPrice(u128::pow(2, 127)).fri_to_wei(1)
            == Err(StarknetApiError::GasPriceConversionError("Gas price is too high".to_string()))
    );
}

#[test]
fn fri_to_wei_errors_on_conversion_rate_zero() {
    assert!(
        GasPrice(5).fri_to_wei(0)
            == Err(StarknetApiError::GasPriceConversionError(
                "FRI to ETH rate must be non-zero".to_string()
            ))
    );
}
