use apollo_network_types::test_utils::{get_keypair, DUMMY_KEYPAIR};
use futures::channel::mpsc;
use futures::future::{ready, try_join};
use futures::sink::{drain, Drain};
use futures::SinkExt;
use libp2p::core::upgrade::InboundConnectionUpgrade;

use super::*;
use crate::authentication::negotiator::{MockNegotiator, NegotiationSide, NegotiatorOutput};
use crate::test_utils::get_connected_streams;

#[test]
fn test_composed_noise_config_generates_protocol_info() {
    const PROTOCOL_NAME: &str = "test_protocol_name";

    let mut negotiator = MockNegotiator::new();
    negotiator.expect_protocol_name().return_const(PROTOCOL_NAME);

    let config = ComposedNoiseConfig::<MockNegotiator, Drain<PeerId>>::new(
        &DUMMY_KEYPAIR,
        Some(negotiator),
        drain(),
    )
    .unwrap();

    let mut protocol_name_iterator = config.protocol_info();
    assert_eq!(protocol_name_iterator.next().unwrap(), format!("/noise_with_{PROTOCOL_NAME}"));
    assert!(protocol_name_iterator.next().is_none());
}

async fn perform_upgrade_with_negotiator_with_keys_and_dup_sink<T, S, Q>(
    server_keypair: &identity::Keypair,
    client_keypair: &identity::Keypair,
    server_negotiator: Option<T>,
    client_negotiator: Option<T>,
    server_dup_sink: S,
    client_dup_sink: Q,
) -> (PeerId, PeerId)
where
    T: Negotiator + 'static,
    S: Sink<PeerId> + Unpin + Send + 'static,
    S::Error: std::fmt::Debug,
    Q: Sink<PeerId> + Unpin + Send + 'static,
    Q::Error: std::fmt::Debug,
{
    let (client_stream, server_stream, _) = get_connected_streams().await;

    let ((client_peer_id, _), (server_peer_id, _)) = futures::future::try_join(
        ComposedNoiseConfig::<T, S>::new(server_keypair, server_negotiator, server_dup_sink)
            .unwrap()
            .upgrade_inbound(server_stream, "unused".to_string()),
        ComposedNoiseConfig::<T, Q>::new(client_keypair, client_negotiator, client_dup_sink)
            .unwrap()
            .upgrade_outbound(client_stream, "unused".to_string()),
    )
    .await
    .expect("Negotiation failed.");

    (client_peer_id, server_peer_id)
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
    perform_upgrade_with_negotiator_with_keys_and_dup_sink(
        server_keypair,
        client_keypair,
        server_negotiator,
        client_negotiator,
        drain(),
        drain(),
    )
    .await
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

    let mut server_negotiator = MockNegotiator::new();
    server_negotiator.expect_negotiate_connection().return_once(
        move |my_peer_id, other_peer_id, _, _, _| {
            assert_eq!(my_peer_id, server_peer_id);
            assert_eq!(other_peer_id, client_peer_id);
            ready(Ok(NegotiatorOutput::None)).boxed()
        },
    );

    let mut client_negotiator = MockNegotiator::new();
    client_negotiator.expect_negotiate_connection().return_once(
        move |my_peer_id, other_peer_id, _, _, _| {
            assert_eq!(my_peer_id, client_peer_id);
            assert_eq!(other_peer_id, server_peer_id);
            ready(Ok(NegotiatorOutput::None)).boxed()
        },
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
    let mut server_negotiator = MockNegotiator::new();
    server_negotiator
        .expect_negotiate_connection()
        .return_once(|_, _, _, _, _| ready(Ok(NegotiatorOutput::None)).boxed());

    let mut client_negotiator = MockNegotiator::new();
    client_negotiator
        .expect_negotiate_connection()
        .return_once(|_, _, _, _, _| ready(Ok(NegotiatorOutput::None)).boxed());

    perform_upgrade_with_negotiator(Some(server_negotiator), Some(client_negotiator)).await;
}

#[tokio::test]
async fn test_composed_noise_config_returns_failure_when_initiator_fails() {
    let mut server_negotiator = MockNegotiator::new();
    server_negotiator
        .expect_negotiate_connection()
        .return_once(|_, _, _, _, _| ready(Ok(NegotiatorOutput::None)).boxed());

    let mut client_negotiator = MockNegotiator::new();
    client_negotiator
        .expect_negotiate_connection()
        .return_once(|_, _, _, _, _| ready(Err(IoError::other("dummy error"))).boxed());

    let (client_stream, server_stream, _) = get_connected_streams().await;

    let res = try_join(
        ComposedNoiseConfig::<MockNegotiator, Drain<PeerId>>::new(
            &get_keypair(0),
            Some(server_negotiator),
            drain(),
        )
        .unwrap()
        .upgrade_inbound(server_stream, "unused".to_string()),
        ComposedNoiseConfig::<MockNegotiator, Drain<PeerId>>::new(
            &get_keypair(1),
            Some(client_negotiator),
            drain(),
        )
        .unwrap()
        .upgrade_outbound(client_stream, "unused".to_string()),
    )
    .await;

    res.unwrap_err();
}

#[tokio::test]
async fn test_composed_noise_config_returns_failure_when_responder_fails() {
    let mut server_negotiator = MockNegotiator::new();
    server_negotiator
        .expect_negotiate_connection()
        .return_once(|_, _, _, _, _| ready(Err(IoError::other("dummy error"))).boxed());

    let mut client_negotiator = MockNegotiator::new();
    client_negotiator
        .expect_negotiate_connection()
        .return_once(|_, _, _, _, _| ready(Ok(NegotiatorOutput::None)).boxed());

    let (client_stream, server_stream, _) = get_connected_streams().await;

    let res = try_join(
        ComposedNoiseConfig::<MockNegotiator, Drain<PeerId>>::new(
            &get_keypair(0),
            Some(server_negotiator),
            drain(),
        )
        .unwrap()
        .upgrade_inbound(server_stream, "unused".to_string()),
        ComposedNoiseConfig::<MockNegotiator, Drain<PeerId>>::new(
            &get_keypair(1),
            Some(client_negotiator),
            drain(),
        )
        .unwrap()
        .upgrade_outbound(client_stream, "unused".to_string()),
    )
    .await;

    res.unwrap_err();
}

#[tokio::test]
async fn test_composed_noise_config_transfers_messages_between_peers() {
    // For this test we create an actual implementation to work around lifetime issues of the
    // channel when using mocks.
    #[derive(Clone)]
    struct EvenOddNegotiator;

    #[async_trait]
    impl Negotiator for EvenOddNegotiator {
        type Error = std::io::Error;

        fn protocol_name(&self) -> &'static str {
            "unused"
        }

        async fn negotiate_connection(
            &mut self,
            _my_peer_id: PeerId,
            _other_peer_id: PeerId,
            connection_sender: &mut negotiator::ConnectionSender,
            connection_receiver: &mut negotiator::ConnectionReceiver,
            side: NegotiationSide,
        ) -> Result<NegotiatorOutput, Self::Error> {
            match side {
                NegotiationSide::Inbound => {
                    // a. Send '1'
                    connection_sender.send(vec![1]).await?;
                    // b. Expect '2'
                    assert_eq!(connection_receiver.next().await.unwrap().unwrap(), vec![2]);
                    // c. Send '3'
                    connection_sender.send(vec![3]).await?;
                    // d. Expect '4'
                    assert_eq!(connection_receiver.next().await.unwrap().unwrap(), vec![4]);
                }
                NegotiationSide::Outbound => {
                    // a. Expect '1'
                    assert_eq!(connection_receiver.next().await.unwrap().unwrap(), vec![1]);
                    // b. Send '2'
                    connection_sender.send(vec![2]).await?;
                    // c. Expect '3'
                    assert_eq!(connection_receiver.next().await.unwrap().unwrap(), vec![3]);
                    // d. Send '4'
                    connection_sender.send(vec![4]).await?;
                }
            }

            Ok(NegotiatorOutput::None)
        }
    }

    perform_upgrade_with_negotiator(Some(EvenOddNegotiator), Some(EvenOddNegotiator)).await;
}

#[tokio::test]
async fn test_composed_noise_config_sends_duplicate_peer_to_sink_for_server() {
    // The channel on which the duplicates will be reported.
    let (sender, mut receiver) = mpsc::unbounded::<PeerId>();

    const CLIENT_PEER_ID_INDEX: u8 = 0;
    const SERVER_PEER_ID_INDEX: u8 = 1;
    const DIFFERENT_PEER_ID_INDEX: u8 = 2;
    let duplicate_peer_id = get_keypair(DIFFERENT_PEER_ID_INDEX).public().to_peer_id();

    // Create server negotiator that returns DuplicatePeer(foo) for incoming connections
    let mut server_negotiator = MockNegotiator::new();
    server_negotiator
        .expect_negotiate_connection()
        .withf(move |_, _, _, _, side| side == &NegotiationSide::Inbound)
        .return_once(move |_, _, _, _, _| {
            ready(Ok(NegotiatorOutput::DuplicatePeer(duplicate_peer_id))).boxed()
        });

    // Create client negotiator that just succeeds.
    let mut client_negotiator = MockNegotiator::new();
    client_negotiator
        .expect_negotiate_connection()
        .withf(move |_, _, _, _, side| side == &NegotiationSide::Outbound)
        .return_once(move |_, _, _, _, _| ready(Ok(NegotiatorOutput::None)).boxed());

    perform_upgrade_with_negotiator_with_keys_and_dup_sink(
        &get_keypair(SERVER_PEER_ID_INDEX),
        &get_keypair(CLIENT_PEER_ID_INDEX),
        Some(server_negotiator),
        Some(client_negotiator),
        sender,
        drain(),
    )
    .await;

    let received_peer_id_to_disconnect = receiver.try_next().unwrap().unwrap();
    assert_eq!(received_peer_id_to_disconnect, duplicate_peer_id);
}

#[tokio::test]
async fn test_composed_noise_config_sends_duplicate_peer_to_sink_for_client() {
    // The channel on which the duplicates will be reported.
    let (sender, mut receiver) = mpsc::unbounded::<PeerId>();

    const CLIENT_PEER_ID_INDEX: u8 = 0;
    const SERVER_PEER_ID_INDEX: u8 = 1;
    const DIFFERENT_PEER_ID_INDEX: u8 = 2;
    let duplicate_peer_id = get_keypair(DIFFERENT_PEER_ID_INDEX).public().to_peer_id();

    // Create server negotiator that just succeeds.
    let mut server_negotiator = MockNegotiator::new();
    server_negotiator
        .expect_negotiate_connection()
        .withf(move |_, _, _, _, side| side == &NegotiationSide::Inbound)
        .return_once(move |_, _, _, _, _| ready(Ok(NegotiatorOutput::None)).boxed());

    // Create client negotiator that returns DuplicatePeer(foo) for outgoing connections
    let mut client_negotiator = MockNegotiator::new();
    client_negotiator
        .expect_negotiate_connection()
        .withf(move |_, _, _, _, side| side == &NegotiationSide::Outbound)
        .return_once(move |_, _, _, _, _| {
            ready(Ok(NegotiatorOutput::DuplicatePeer(duplicate_peer_id))).boxed()
        });

    perform_upgrade_with_negotiator_with_keys_and_dup_sink(
        &get_keypair(SERVER_PEER_ID_INDEX),
        &get_keypair(CLIENT_PEER_ID_INDEX),
        Some(server_negotiator),
        Some(client_negotiator),
        drain(),
        sender,
    )
    .await;

    let received_peer_id_to_disconnect = receiver.try_next().unwrap().unwrap();
    assert_eq!(received_peer_id_to_disconnect, duplicate_peer_id);
}
