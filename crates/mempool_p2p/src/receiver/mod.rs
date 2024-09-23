use async_trait::async_trait;
use starknet_mempool_infra::component_runner::ComponentStarter;
use starknet_mempool_infra::errors::ComponentError;
use starknet_mempool_infra::starters::DefaultComponentStarter;

pub struct MempoolP2pReceiver;

impl DefaultComponentStarter for MempoolP2pReceiver {}
