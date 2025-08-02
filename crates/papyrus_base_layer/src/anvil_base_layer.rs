use std::ops::RangeInclusive;

use alloy::node_bindings::NodeError as AnvilError;
use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use async_trait::async_trait;
use colored::*;
use starknet_api::block::BlockHashAndNumber;
use url::Url;

use crate::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
    EthereumBaseLayerError,
    Starknet,
};
use crate::{BaseLayerContract, L1BlockHeader, L1BlockNumber, L1BlockReference, L1Event};

/// Initialize an anvil instance under the default port and deploy the Starknet contract.
///
/// Usage: use this in cargo integration tests (tests under `tests/` dir), which ensure
/// sequential execution of tests and only one instance of Anvil running at once. Using Anvil in
/// unit tests is not supported and is discouraged, since unit tests should not need to run a whole
/// L1 (and they are parallelized, which creates port issues). For unit tests, prefer using
/// `ProviderBuilder::new().on_mocked_client` to mock L1.
#[derive(Clone, Debug)]
pub struct AnvilBaseLayer {
    pub anvil_provider: DynProvider,
    pub ethereum_base_layer: EthereumBaseLayerContract,
}

impl AnvilBaseLayer {
    const DEFAULT_ANVIL_PORT: u16 = 8545;
    const DEFAULT_ANVIL_L1_DEPLOYED_ADDRESS: &str = "0x5fbdb2315678afecb367f032d93f642f64180aa3";

    /// Note: if you have port conflicts, this is because you are running anvil in unit tests, see
    /// usage docstring of the struct. Alternatively, you might have a zombie anvil instance
    /// running, but that should be impossible if using this service.
    pub async fn new() -> Self {
        let anvil_client = ProviderBuilder::new()
            .on_anvil_with_wallet_and_config(|anvil| anvil.port(Self::DEFAULT_ANVIL_PORT))
            .unwrap_or_else(|error| match error {
                AnvilError::SpawnError(e)
                    if e.to_string().contains("No such file or directory") =>
                {
                    panic!(
                        "\n{}\n{}\n",
                        "Anvil binary not found!".bold().red(),
                        "Install instructions (for local development):\n
                 cargo install \
                         --git https://github.com/foundry-rs/foundry anvil --locked --tag=v0.3.0"
                            .yellow()
                    )
                }
                _ => panic!("Failed to spawn Anvil: {}", error.to_string().red()),
            });

        Starknet::deploy(anvil_client.clone()).await.unwrap();

        let url: Url = format!("http://127.0.0.1:{}", Self::DEFAULT_ANVIL_PORT).parse().unwrap();
        let config = EthereumBaseLayerConfig {
            node_url: url,
            starknet_contract_address: Self::DEFAULT_ANVIL_L1_DEPLOYED_ADDRESS.parse().unwrap(),
            ..Default::default()
        };
        let root_client = anvil_client.root().clone();
        let contract = Starknet::new(config.starknet_contract_address, root_client);

        Self {
            anvil_provider: anvil_client.erased(),
            ethereum_base_layer: EthereumBaseLayerContract { config, contract },
        }
    }
}

#[async_trait]
impl BaseLayerContract for AnvilBaseLayer {
    type Error = EthereumBaseLayerError;

    async fn get_proved_block_at(
        &self,
        l1_block: L1BlockNumber,
    ) -> Result<BlockHashAndNumber, Self::Error> {
        self.ethereum_base_layer.get_proved_block_at(l1_block).await
    }

    async fn latest_proved_block(
        &self,
        finality: u64,
    ) -> Result<Option<BlockHashAndNumber>, Self::Error> {
        self.ethereum_base_layer.latest_proved_block(finality).await
    }

    async fn latest_l1_block_number(
        &self,
        finality: u64,
    ) -> Result<Option<L1BlockNumber>, Self::Error> {
        self.ethereum_base_layer.latest_l1_block_number(finality).await
    }

    async fn latest_l1_block(
        &self,
        finality: u64,
    ) -> Result<Option<L1BlockReference>, Self::Error> {
        self.ethereum_base_layer.latest_l1_block(finality).await
    }

    async fn l1_block_at(
        &self,
        block_number: L1BlockNumber,
    ) -> Result<Option<L1BlockReference>, Self::Error> {
        self.ethereum_base_layer.l1_block_at(block_number).await
    }

    async fn events<'a>(
        &'a self,
        block_range: RangeInclusive<L1BlockNumber>,
        event_identifiers: &'a [&'a str],
    ) -> Result<Vec<L1Event>, Self::Error> {
        self.ethereum_base_layer.events(block_range, event_identifiers).await
    }

    async fn get_block_header(
        &self,
        block_number: L1BlockNumber,
    ) -> Result<Option<L1BlockHeader>, Self::Error> {
        self.ethereum_base_layer.get_block_header(block_number).await
    }

    async fn set_provider_url(&mut self, _url: Url) -> Result<(), Self::Error> {
        unimplemented!("Anvil base layer is tied to a an Anvil server, url is fixed.")
    }
}
