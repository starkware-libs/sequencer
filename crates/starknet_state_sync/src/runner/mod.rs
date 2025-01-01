#[cfg(test)]
mod test;

use async_trait::async_trait;
use futures::channel::mpsc::Receiver;
use futures::future::BoxFuture;
use futures::never::Never;
use futures::{FutureExt, StreamExt};
use papyrus_network::network_manager::{self, NetworkError};
use papyrus_p2p_sync::client::{P2pSyncClient, P2pSyncClientChannels, P2pSyncClientError};
use papyrus_p2p_sync::server::{P2pSyncServer, P2pSyncServerChannels};
use papyrus_p2p_sync::{Protocol, BUFFER_SIZE};
use papyrus_storage::{open_storage, StorageReader};
use starknet_class_manager_types::SharedClassManagerClient;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sequencer_infra::component_server::WrapperServer;
use starknet_sequencer_infra::errors::ComponentError;
use starknet_state_sync_types::state_sync_types::SyncBlock;

use crate::config::StateSyncConfig;

pub struct StateSyncRunner {
    network_future: BoxFuture<'static, Result<(), NetworkError>>,
    // TODO: change client and server to requester and responder respectively
    p2p_sync_client_future: BoxFuture<'static, Result<Never, P2pSyncClientError>>,
    p2p_sync_server_future: BoxFuture<'static, Never>,
}

#[async_trait]
impl ComponentStarter for StateSyncRunner {
    async fn start(&mut self) -> Result<(), ComponentError> {
        tokio::select! {
            result = &mut self.network_future => {
                result.map_err(|_| ComponentError::InternalComponentError)
            }
            result = &mut self.p2p_sync_client_future => {
                result.map_err(|_| ComponentError::InternalComponentError).map(|_never| ())
            }
            _never = &mut self.p2p_sync_server_future => {
                Err(ComponentError::InternalComponentError)
            }
        }
    }
}

impl StateSyncRunner {
    pub fn new(
        config: StateSyncConfig,
        new_block_receiver: Receiver<SyncBlock>,
        class_manager_client: SharedClassManagerClient,
    ) -> (Self, StorageReader) {
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
        let p2p_sync_client_channels = P2pSyncClientChannels::new(
            header_client_sender,
            state_diff_client_sender,
            transaction_client_sender,
            class_client_sender,
        );
        let p2p_sync_client = P2pSyncClient::new(
            config.p2p_sync_client_config,
            storage_reader.clone(),
            storage_writer,
            p2p_sync_client_channels,
            new_block_receiver.boxed(),
            class_manager_client.clone(),
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
        let p2p_sync_server_channels = P2pSyncServerChannels::new(
            header_server_receiver,
            state_diff_server_receiver,
            transaction_server_receiver,
            class_server_receiver,
            event_server_receiver,
        );
        let p2p_sync_server = P2pSyncServer::new(
            storage_reader.clone(),
            p2p_sync_server_channels,
            class_manager_client,
        );

        let network_future = network_manager.run().boxed();
        let p2p_sync_client_future = p2p_sync_client.run().boxed();
        let p2p_sync_server_future = p2p_sync_server.run().boxed();

        // TODO(shahak): add rpc.
        (Self { network_future, p2p_sync_client_future, p2p_sync_server_future }, storage_reader)
    }
}

pub type StateSyncRunnerServer = WrapperServer<StateSyncRunner>;
// TODO(shahak): fill with a proper version, or allow not specifying the node version.
const VERSION_FULL: &str = "";
