use starknet_sequencer_infra::component_server::WrapperServer;

use crate::l1_provider_starter::L1ProviderStarter as L1ProviderStarterComponent;

pub type L1ProviderStarter = WrapperServer<L1ProviderStarterComponent>;
