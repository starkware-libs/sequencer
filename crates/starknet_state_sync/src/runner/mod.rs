#[cfg(test)]
mod test;

use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::FutureExt;
use papyrus_network::network_manager::{self, NetworkError};
use papyrus_p2p_sync::client::{P2PSyncClient, P2PSyncClientChannels, P2PSyncClientError};
use papyrus_p2p_sync::server::{P2PSyncServer, P2PSyncServerChannels};
use papyrus_p2p_sync::{Protocol, BUFFER_SIZE};
use papyrus_storage::{open_storage, StorageReader};
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sequencer_infra::errors::ComponentError;

use crate::config::StateSyncConfig;

pub struct StateSyncRunner {
    network_future: BoxFuture<'static, Result<(), NetworkError>>,
    // TODO: change client and server to requester and responder respectively
    p2p_sync_client_future: BoxFuture<'static, Result<(), P2PSyncClientError>>,
    p2p_sync_server_future: BoxFuture<'static, ()>,
}

#[async_trait]
impl ComponentStarter for StateSyncRunner {
    async fn start(&mut self) -> Result<(), ComponentError> {
        tokio::select! {
            result = &mut self.network_future => {
                return result.map_err(|_| ComponentError::InternalComponentError);
            }
            result = &mut self.p2p_sync_client_future => return result.map_err(|_| ComponentError::InternalComponentError),
            () = &mut self.p2p_sync_server_future => {
                return Err(ComponentError::InternalComponentError);
            }
        }
    }
}

impl StateSyncRunner {
    pub fn new(config: StateSyncConfig) -> (Self, StorageReader) {
        let (storage_reader, storage_writer) =
            open_storage(config.storage_config).expect("StateSyncRunner failed opening storage");

        let mut network_manager = network_manager::NetworkManager::new(
            config.network_config,
            Some(VERSION_FULL.to_string()),
        );

        let header_client_sender = network_manager
            .register_sqmr_protocol_client(Protocol::SignedBlockHeader.into(), BUFFER_SIZE);
        let state_diff_client_sender =
            network_manager.register_sqmr_protocol_client(Protocol::StateDiff.into(), BUFFER_SIZE);
        let transaction_client_sender = network_manager
            .register_sqmr_protocol_client(Protocol::Transaction.into(), BUFFER_SIZE);
        let class_client_sender =
            network_manager.register_sqmr_protocol_client(Protocol::Class.into(), BUFFER_SIZE);
        let p2p_sync_client_channels = P2PSyncClientChannels::new(
            header_client_sender,
            state_diff_client_sender,
            transaction_client_sender,
            class_client_sender,
        );
        let p2p_sync_client = P2PSyncClient::new(
            config.p2p_sync_client_config,
            storage_reader.clone(),
            storage_writer,
            p2p_sync_client_channels,
        );

        let header_server_receiver = network_manager
            .register_sqmr_protocol_server(Protocol::SignedBlockHeader.into(), BUFFER_SIZE);
        let state_diff_server_receiver =
            network_manager.register_sqmr_protocol_server(Protocol::StateDiff.into(), BUFFER_SIZE);
        let transaction_server_receiver = network_manager
            .register_sqmr_protocol_server(Protocol::Transaction.into(), BUFFER_SIZE);
        let class_server_receiver =
            network_manager.register_sqmr_protocol_server(Protocol::Class.into(), BUFFER_SIZE);
        let event_server_receiver =
            network_manager.register_sqmr_protocol_server(Protocol::Event.into(), BUFFER_SIZE);
        let p2p_sync_server_channels = P2PSyncServerChannels::new(
            header_server_receiver,
            state_diff_server_receiver,
            transaction_server_receiver,
            class_server_receiver,
            event_server_receiver,
        );
        let p2p_sync_server = P2PSyncServer::new(storage_reader.clone(), p2p_sync_server_channels);

        let network_future = network_manager.run().boxed();
        let p2p_sync_client_future = p2p_sync_client.run().boxed();
        let p2p_sync_server_future = p2p_sync_server.run().boxed();

        // TODO(shahak): add rpc.
        (Self { network_future, p2p_sync_client_future, p2p_sync_server_future }, storage_reader)
    }
}

// TODO(shahak): fill with a proper version, or allow not specifying the node version.
const VERSION_FULL: &str = "";
