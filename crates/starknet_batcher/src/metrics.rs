use metrics::{describe_gauge, gauge};
use starknet_api::block::BlockNumber;

pub const STORAGE_HEIGHT: Metric =
    Metric { name: "batcher_storage_height", description: "The height of the batcher's storage" };

pub struct Metric {
    pub name: &'static str,
    pub description: &'static str,
}

pub fn register_metrics(storage_height: BlockNumber) {
    let storage_height_metric = gauge!(STORAGE_HEIGHT.name);
    describe_gauge!(STORAGE_HEIGHT.name, STORAGE_HEIGHT.description);
    #[allow(clippy::as_conversions)]
    storage_height_metric.set(storage_height.0 as f64);
}
