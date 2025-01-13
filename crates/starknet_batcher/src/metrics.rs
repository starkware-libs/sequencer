use metrics::{counter, describe_counter};
use starknet_api::block::BlockNumber;

pub const STORAGE_HEIGHT: Metric =
    Metric { name: "batcher_storage_height", description: "The height of the batcher's storage" };

pub struct Metric {
    pub name: &'static str,
    pub description: &'static str,
}

pub fn register_metrics(storage_height: BlockNumber) {
    // Ideally, we would have a `Gauge` here because of reverts, but we can't because
    // the value will need to implement `Into<f64>` and `BlockNumber` doesn't.
    // In case of reverts, consider calling `absolute`.
    let storage_height_metric = counter!(STORAGE_HEIGHT.name);
    describe_counter!(STORAGE_HEIGHT.name, STORAGE_HEIGHT.description);
    storage_height_metric.absolute(storage_height.0);
}
