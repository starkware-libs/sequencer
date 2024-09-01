use crate::transaction::{DeprecatedResourceBoundsMapping, Resource, ResourceBounds};

pub mod declare;
pub mod deploy_account;
pub mod invoke;
pub mod struct_impls;

pub const CHAIN_ID_NAME: &str = "SN_GOERLI";

pub fn default_testing_resource_bounds() -> DeprecatedResourceBoundsMapping {
    DeprecatedResourceBoundsMapping::try_from(vec![
        (Resource::L1Gas, ResourceBounds { max_amount: 0, max_price_per_unit: 1 }),
        // TODO(Dori, 1/2/2024): When fee market is developed, change the default price of
        //   L2 gas.
        (Resource::L2Gas, ResourceBounds { max_amount: 0, max_price_per_unit: 0 }),
    ])
    .unwrap()
}
