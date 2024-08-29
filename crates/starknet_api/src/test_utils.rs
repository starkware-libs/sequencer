use crate::transaction::{ResourceBounds, ValidResourceBounds};

pub mod declare;
pub mod deploy_account;
pub mod invoke;

// TODO: Default testing bounds should probably be AllResourceBounds variant.
pub fn default_testing_resource_bounds() -> ValidResourceBounds {
    ValidResourceBounds::L1Gas(ResourceBounds { max_amount: 0, max_price_per_unit: 1 })
}
