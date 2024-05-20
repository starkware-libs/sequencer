use std::sync::Arc;

use assert_matches::assert_matches;
use axum::body::{Bytes, HttpBody};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use blockifier::context::ChainInfo;
use mempool_infra::component_server::ComponentServer;
use starknet_api::external_transaction::ExternalTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_mempool::mempool::{Mempool, MempoolCommunicationWrapper};
use starknet_mempool_types::mempool_types::{
    BatcherToMempoolChannels, BatcherToMempoolMessage, GatewayToMempoolMessage, MempoolClient,
    MempoolClientImpl, MempoolNetworkComponent, MempoolRequestAndResponseSender,
    MempoolToBatcherMessage, MempoolToGatewayMessage,
};
use tokio::sync::mpsc::channel;
use tokio::task;

use crate::config::{StatefulTransactionValidatorConfig, StatelessTransactionValidatorConfig};
use crate::gateway::{add_tx, AppState};
use crate::starknet_api_test_utils::invoke_tx;
use crate::state_reader_test_utils::test_state_reader_factory;
use crate::stateful_transaction_validator::StatefulTransactionValidator;
use crate::stateless_transaction_validator::StatelessTransactionValidator;
use crate::utils::{external_tx_to_account_tx, get_tx_hash};

const MEMPOOL_INVOCATIONS_QUEUE_SIZE: usize = 32;

pub fn app_state(mempool_client: Arc<dyn MempoolClient>) -> AppState {
    AppState {
        stateless_tx_validator: StatelessTransactionValidator {
            config: StatelessTransactionValidatorConfig {
                validate_non_zero_l1_gas_fee: true,
                max_calldata_length: 10,
                max_signature_length: 2,
                ..Default::default()
            },
        },
        stateful_tx_validator: Arc::new(StatefulTransactionValidator {
            config: StatefulTransactionValidatorConfig::create_for_testing(),
        }),
        state_reader_factory: Arc::new(test_state_reader_factory()),
        mempool_client,
    }
}

// TODO(Ayelet): add test cases for declare and deploy account transactions.
#[tokio::test]
async fn test_add_tx() {
    // TODO: Add fixture.
    // TODO -- remove gateway_network, batcher_network, and channels.
    let (_, rx_gateway_to_mempool) = channel::<GatewayToMempoolMessage>(1);
    let (tx_mempool_to_gateway, _) = channel::<MempoolToGatewayMessage>(1);
    let gateway_network =
        MempoolNetworkComponent::new(tx_mempool_to_gateway, rx_gateway_to_mempool);

    let (_, rx_mempool_to_batcher) = channel::<BatcherToMempoolMessage>(1);
    let (tx_batcher_to_mempool, _) = channel::<MempoolToBatcherMessage>(1);
    let batcher_network =
        BatcherToMempoolChannels { rx: rx_mempool_to_batcher, tx: tx_batcher_to_mempool };

    // Create and start the mempool server.
    let mempool = Mempool::new([], gateway_network, batcher_network);
    // TODO(Tsabary): wrap creation of channels in dedicated functions, take channel capacity from
    // config.
    let (tx_mempool, rx_mempool) =
        channel::<MempoolRequestAndResponseSender>(MEMPOOL_INVOCATIONS_QUEUE_SIZE);
    // TODO(Tsabary, 1/6/2024): Wrap with a dedicated create_mempool_server function.
    let mut mempool_server =
        ComponentServer::new(MempoolCommunicationWrapper::new(mempool), rx_mempool);
    task::spawn(async move {
        mempool_server.start().await;
    });

    let mempool_client = Arc::new(MempoolClientImpl::new(tx_mempool));

    let app_state = app_state(mempool_client);

    let tx = invoke_tx();
    let tx_hash = calculate_hash(&tx);
    let response = add_tx(State(app_state), tx.into()).await.into_response();

    let status_code = response.status();
    assert_eq!(status_code, StatusCode::OK);

    let response_bytes = &to_bytes(response).await;
    assert_eq!(tx_hash, serde_json::from_slice(response_bytes).unwrap());
}

async fn to_bytes(res: Response) -> Bytes {
    res.into_body().collect().await.unwrap().to_bytes()
}

fn calculate_hash(external_tx: &ExternalTransaction) -> TransactionHash {
    assert_matches!(
        external_tx,
        ExternalTransaction::Invoke(_),
        "Only Invoke supported for now, extend as needed."
    );

    let account_tx =
        external_tx_to_account_tx(external_tx, None, &ChainInfo::create_for_testing().chain_id)
            .unwrap();
    get_tx_hash(&account_tx)
}
