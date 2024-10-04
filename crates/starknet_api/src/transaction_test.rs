use crate::block::{GasPrice, NonzeroGasPrice};
use crate::execution_resources::GasAmount;
use crate::transaction::Fee;

#[test]
fn test_fee_div_ceil() {
    assert_eq!(GasAmount(1), Fee(1).div_ceil(NonzeroGasPrice::new(GasPrice(1)).unwrap()));
    assert_eq!(GasAmount(0), Fee(0).div_ceil(NonzeroGasPrice::new(GasPrice(1)).unwrap()));
    assert_eq!(GasAmount(1), Fee(1).div_ceil(NonzeroGasPrice::new(GasPrice(2)).unwrap()));
    assert_eq!(GasAmount(9), Fee(27).div_ceil(NonzeroGasPrice::new(GasPrice(3)).unwrap()));
    assert_eq!(GasAmount(10), Fee(28).div_ceil(NonzeroGasPrice::new(GasPrice(3)).unwrap()));
}
