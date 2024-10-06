use crate::block::{GasPrice, NonzeroGasPrice};
use crate::execution_resources::GasAmount;
use crate::transaction::Fee;

#[test]
fn test_fee_div_ceil() {
    assert_eq!(
        GasAmount(1),
        Fee(1).checked_div_ceil(NonzeroGasPrice::new(GasPrice(1)).unwrap()).unwrap()
    );
    assert_eq!(
        GasAmount(0),
        Fee(0).checked_div_ceil(NonzeroGasPrice::new(GasPrice(1)).unwrap()).unwrap()
    );
    assert_eq!(
        GasAmount(1),
        Fee(1).checked_div_ceil(NonzeroGasPrice::new(GasPrice(2)).unwrap()).unwrap()
    );
    assert_eq!(
        GasAmount(9),
        Fee(27).checked_div_ceil(NonzeroGasPrice::new(GasPrice(3)).unwrap()).unwrap()
    );
    assert_eq!(
        GasAmount(10),
        Fee(28).checked_div_ceil(NonzeroGasPrice::new(GasPrice(3)).unwrap()).unwrap()
    );
}
