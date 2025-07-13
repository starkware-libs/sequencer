use std::io::Error as IoError;

use futures::future::try_join;
use futures::{SinkExt, StreamExt};
use lazy_static::lazy_static;
use libp2p::core::upgrade::InboundConnectionUpgrade;

use super::*;
use crate::authentication::negotiator::{Error as NegotiatorError, Output as NegotiatorOutput};
use crate::test_utils::get_connected_streams;

// TODO(guy.f): Move this to the test utils once get_peer_id() is merged from main-v14 to main.
lazy_static! {
    static ref DUMMY_KEYPAIR: identity::Keypair = get_keypair(0);
}

fn get_keypair(i: u8) -> identity::Keypair {
    let key = [i; 32];
    identity::Keypair::ed25519_from_bytes(key).unwrap()
}

/// A Negotiator for testing which runs the given closures when the methods are called.
// Note that we cannot use mock! due to the methods' signature that includes a generic type
// (NegotiatorChannel) which ends up being an internal type. This prevents us from being able to set
// expectation on the methods (since we don't know the type).
#[derive(Clone)]
struct ClosureNegotiator {
    incoming_closure: std::sync::Arc<
        dyn Fn(PeerId, PeerId) -> Result<NegotiatorOutput, NegotiatorError> + Send + Sync,
    >,
    outgoing_closure: std::sync::Arc<
        dyn Fn(PeerId, PeerId) -> Result<NegotiatorOutput, NegotiatorError> + Send + Sync,
    >,
    protocol_name_closure: std::sync::Arc<dyn Fn() -> &'static str + Send + Sync>,
}

impl ClosureNegotiator {
    fn new<F1, F2, F3>(
        incoming_closure: F1,
        outgoing_closure: F2,
        protocol_name_closure: F3,
    ) -> Self
    where
        F1: Fn(PeerId, PeerId) -> Result<NegotiatorOutput, NegotiatorError> + Send + Sync + 'static,
        F2: Fn(PeerId, PeerId) -> Result<NegotiatorOutput, NegotiatorError> + Send + Sync + 'static,
        F3: Fn() -> &'static str + Send + Sync + 'static,
    {
        Self {
            incoming_closure: std::sync::Arc::new(incoming_closure),
            outgoing_closure: std::sync::Arc::new(outgoing_closure),
            protocol_name_closure: std::sync::Arc::new(protocol_name_closure),
        }
    }
}

#[async_trait]
impl Negotiator for ClosureNegotiator {
    fn protocol_name(&self) -> &'static str {
        (self.protocol_name_closure)()
    }

    async fn negotiate_incoming_connection<NegotiatorChannel>(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        _connection: &mut NegotiatorChannel,
    ) -> Result<NegotiatorOutput, NegotiatorError>
    where
        NegotiatorChannel:
            Sink<Vec<u8>, Error = IoError> + Stream<Item = Result<Vec<u8>, IoError>> + Unpin + Send,
    {
        (self.incoming_closure)(my_peer_id, other_peer_id)
    }

    async fn negotiate_outgoing_connection<NegotiatorChannel>(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        _connection: &mut NegotiatorChannel,
    ) -> Result<NegotiatorOutput, NegotiatorError>
    where
        NegotiatorChannel:
            Sink<Vec<u8>, Error = IoError> + Stream<Item = Result<Vec<u8>, IoError>> + Unpin + Send,
    {
        (self.outgoing_closure)(my_peer_id, other_peer_id)
    }
}

#[test]
fn test_composed_noise_config_generates_protocol_info() {
    const PROTOCOL_NAME: &str = "test_protocol_name";

    let negotiator = ClosureNegotiator::new(
        |_, _| Ok(NegotiatorOutput::None),
        |_, _| Ok(NegotiatorOutput::None),
        || PROTOCOL_NAME,
    );

    let config =
        ComposedNoiseConfig::<ClosureNegotiator>::new(&DUMMY_KEYPAIR, Some(negotiator)).unwrap();

    assert_eq!(config.protocol_info().next().unwrap(), format!("/noise_with_{PROTOCOL_NAME}"));
}

/// Helper function to create the security upgrade pair (incoming/outgoing) and run them. Asserts
/// that both sides of the negotiation returned success.
async fn perform_upgrade_with_negotiator_with_keys<T>(
    server_keypair: &identity::Keypair,
    client_keypair: &identity::Keypair,
    server_negotiator: Option<T>,
    client_negotiator: Option<T>,
) -> (PeerId, PeerId)
where
    T: Negotiator + 'static,
{
    let (client_stream, server_stream, _) = get_connected_streams().await;

    let ((client_peer_id, _), (server_peer_id, _)) = futures::future::try_join(
        ComposedNoiseConfig::new(server_keypair, server_negotiator)
            .unwrap()
            .upgrade_inbound(server_stream, "unused".to_string()),
        ComposedNoiseConfig::new(client_keypair, client_negotiator)
            .unwrap()
            .upgrade_outbound(client_stream, "unused".to_string()),
    )
    .await
    .expect("Negotiation faild.");

    (client_peer_id, server_peer_id)
}

/// Similar to perform_upgrade_with_negotiator_with_keys. To be used when you don't care about
/// the keys.
async fn perform_upgrade_with_negotiator<T>(
    server_negotiator: Option<T>,
    client_negotiator: Option<T>,
) -> (PeerId, PeerId)
where
    T: Negotiator + 'static,
{
    let server_keypair = get_keypair(0);
    let client_keypair = get_keypair(1);

    perform_upgrade_with_negotiator_with_keys(
        &server_keypair,
        &client_keypair,
        server_negotiator,
        client_negotiator,
    )
    .await
}

#[tokio::test]
async fn test_composed_noise_config_upgrade_with_none_negotiator() {
    let server_id = get_keypair(0);
    let client_id = get_keypair(1);

    let (reported_client_id, reported_server_id) = perform_upgrade_with_negotiator_with_keys::<
        DummyNegotiatorType,
    >(&server_id, &client_id, None, None)
    .await;

    assert_eq!(reported_client_id, client_id.public().to_peer_id());
    assert_eq!(reported_server_id, server_id.public().to_peer_id());
}

#[tokio::test]
async fn test_composed_noise_config_upgrade_passes_correct_peer_ids_to_negotiator() {
    let server_id = get_keypair(0);
    let client_id = get_keypair(1);

    let server_peer_id = server_id.public().to_peer_id();
    let client_peer_id = client_id.public().to_peer_id();

    let server_negotiator = ClosureNegotiator::new(
        move |my_peer_id, other_peer_id| {
            assert_eq!(my_peer_id, server_peer_id);
            assert_eq!(other_peer_id, client_peer_id);
            Ok(NegotiatorOutput::None)
        },
        |_, _| unreachable!("Server should not need to upgrade outgoing connection"),
        || unreachable!(""),
    );

    let client_negotiator = ClosureNegotiator::new(
        |_, _| unreachable!("Client should not need to upgrade incoming connection"),
        move |my_peer_id, other_peer_id| {
            assert_eq!(my_peer_id, client_peer_id);
            assert_eq!(other_peer_id, server_peer_id);
            Ok(NegotiatorOutput::None)
        },
        || unreachable!(""),
    );

    perform_upgrade_with_negotiator_with_keys(
        &server_id,
        &client_id,
        Some(server_negotiator),
        Some(client_negotiator),
    )
    .await;
}

#[tokio::test]
async fn test_composed_noise_config_returns_success_when_both_negotiators_do() {
    let server_negotiator = ClosureNegotiator::new(
        |_, _| Ok(NegotiatorOutput::None),
        |_, _| Ok(NegotiatorOutput::None),
        || unreachable!(""),
    );

    let client_negotiator = server_negotiator.clone();

    perform_upgrade_with_negotiator(Some(server_negotiator), Some(client_negotiator)).await;
}

#[tokio::test]
async fn test_composed_noise_config_returns_failure_when_initiator_fails() {
    let server_negotiator = ClosureNegotiator::new(
        |_, _| Ok(NegotiatorOutput::None),
        |_, _| Ok(NegotiatorOutput::None),
        || unreachable!(""),
    );

    let client_negotiator = ClosureNegotiator::new(
        |_, _| Ok(NegotiatorOutput::None),
        |_, _| Err(NegotiatorError::AuthenticationFailed),
        || unreachable!(""),
    );

    let (client_stream, server_stream, _) = get_connected_streams().await;

    let res = try_join(
        ComposedNoiseConfig::new(&get_keypair(0), Some(server_negotiator))
            .unwrap()
            .upgrade_inbound(server_stream, "unused".to_string()),
        ComposedNoiseConfig::new(&get_keypair(1), Some(client_negotiator))
            .unwrap()
            .upgrade_outbound(client_stream, "unused".to_string()),
    )
    .await;

    assert!(res.is_err());
}

#[tokio::test]
async fn test_composed_noise_config_returns_failure_when_responder_fails() {
    let server_negotiator = ClosureNegotiator::new(
        |_, _| Err(NegotiatorError::AuthenticationFailed),
        |_, _| Ok(NegotiatorOutput::None),
        || unreachable!(""),
    );

    let client_negotiator = ClosureNegotiator::new(
        |_, _| Ok(NegotiatorOutput::None),
        |_, _| Ok(NegotiatorOutput::None),
        || unreachable!(""),
    );

    let (client_stream, server_stream, _) = get_connected_streams().await;

    let res = try_join(
        ComposedNoiseConfig::new(&get_keypair(0), Some(server_negotiator))
            .unwrap()
            .upgrade_inbound(server_stream, "unused".to_string()),
        ComposedNoiseConfig::new(&get_keypair(1), Some(client_negotiator))
            .unwrap()
            .upgrade_outbound(client_stream, "unused".to_string()),
    )
    .await;

    assert!(res.is_err());
}

#[tokio::test]
async fn test_composed_noise_config_transfers_messages_between_peers() {
    // For this test we need to use the connection param which is of a generic type which we do
    // not know (it's an internal implementation detail of the ComposedNoiseConfig). This means we
    // have to write an actual implementation of the Negotiator trait.
    #[derive(Clone)]
    struct EvenOddNegotiator;

    #[async_trait]
    impl Negotiator for EvenOddNegotiator {
        fn protocol_name(&self) -> &'static str {
            "unused"
        }

        /// a. Send '1'
        /// b. Expect '2'
        /// c. Send '3'
        /// d. Expect '4'
        async fn negotiate_incoming_connection<NegotiatorChannel>(
            &mut self,
            _my_peer_id: PeerId,
            _other_peer_id: PeerId,
            connection: &mut NegotiatorChannel,
        ) -> Result<NegotiatorOutput, NegotiatorError>
        where
            NegotiatorChannel: Sink<Vec<u8>, Error = IoError>
                + Stream<Item = Result<Vec<u8>, IoError>>
                + Unpin
                + Send,
        {
            // a. Send '1'
            connection.send(vec![1]).await?;

            // b. Expect '2'
            assert_eq!(connection.next().await.unwrap().expect("Did not receive 2"), vec![2]);

            // c. Send '3'
            connection.send(vec![3]).await?;

            // d. Expect '4'
            assert_eq!(connection.next().await.unwrap().expect("Did not receive 4"), vec![4]);

            Ok(NegotiatorOutput::None)
        }

        // a. Expect '1'
        // b. Send '2'
        // c. Expect '3'
        // d. Send '4'
        async fn negotiate_outgoing_connection<NegotiatorChannel>(
            &mut self,
            _my_peer_id: PeerId,
            _other_peer_id: PeerId,
            connection: &mut NegotiatorChannel,
        ) -> Result<NegotiatorOutput, NegotiatorError>
        where
            NegotiatorChannel: Sink<Vec<u8>, Error = IoError>
                + Stream<Item = Result<Vec<u8>, IoError>>
                + Unpin
                + Send,
        {
            // a. Expect '1'
            assert_eq!(connection.next().await.unwrap().expect("Did not receive 1"), vec![1]);

            // b. Send '2'
            connection.send(vec![2]).await?;

            // c. Expect '3'
            assert_eq!(connection.next().await.unwrap().expect("Did not receive 3"), vec![3]);

            // d. Send '4'
            connection.send(vec![4]).await?;

            Ok(NegotiatorOutput::None)
        }
    }

    perform_upgrade_with_negotiator(Some(EvenOddNegotiator), Some(EvenOddNegotiator)).await;
}

// TODO(guy.f): Test the duplicate peer case once implemented.
