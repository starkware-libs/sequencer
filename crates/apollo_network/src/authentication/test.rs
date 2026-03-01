use std::io::Error as IoError;

use apollo_network_types::test_utils::{get_keypair, DUMMY_KEYPAIR};
use apollo_protobuf::authentication::ChallengeAndIdentity;
use apollo_protobuf::protobuf::StarkAuthentication;
use async_trait::async_trait;
use futures::future::{ready, try_join};
use futures::FutureExt;
use libp2p::core::upgrade::{InboundConnectionUpgrade, OutboundConnectionUpgrade};
use libp2p::core::UpgradeInfo;
use libp2p::{identity, PeerId};
use starknet_api::crypto::utils::{Challenge, PublicKey};

use super::*;
use crate::authentication::composed_noise::{ComposedNoise, NegotiatorError};
use crate::authentication::negotiator::{
    MockNegotiator,
    NegotiationSide,
    Negotiator,
    NegotiatorOutput,
};
use crate::test_utils::get_connected_streams;

impl Clone for MockNegotiator {
    fn clone(&self) -> Self {
        unimplemented!()
    }
}

fn create_no_op_mock_negotiator() -> MockNegotiator {
    let mut negotiator = MockNegotiator::new();
    negotiator
        .expect_negotiate_connection()
        .return_once(|_, _, _, _, _| ready(Ok(NegotiatorOutput::Success)).boxed());
    negotiator.expect_protocol_name().return_const("no_op");
    negotiator
}

#[allow(clippy::as_conversions)]
fn stark_auth_with_challenge(n: u8) -> StarkAuthentication {
    let challenge_and_identity = ChallengeAndIdentity {
        operational_public_key: PublicKey::default(),
        challenge: Challenge::from(n as u128),
    };
    challenge_and_identity.into()
}

#[test]
fn test_composed_noise_config_generates_protocol_info() {
    const PROTOCOL_NAME: &str = "test_protocol_name";

    let mut negotiator = MockNegotiator::new();
    negotiator.expect_protocol_name().return_const(PROTOCOL_NAME);

    let config = ComposedNoise::<MockNegotiator>::new(&DUMMY_KEYPAIR, negotiator).unwrap();

    let mut protocol_name_iterator = config.protocol_info();
    assert_eq!(protocol_name_iterator.next().unwrap(), format!("/noise_with_{PROTOCOL_NAME}"));
    assert!(protocol_name_iterator.next().is_none());
}

#[tokio::test]
async fn test_composed_noise_config_upgrade_with_none_negotiator() {
    let server_id = get_keypair(0);
    let client_id = get_keypair(1);

    let (reported_client_id, reported_server_id) =
        perform_composed_upgrade_with_negotiator_with_keys(
            &server_id,
            &client_id,
            create_no_op_mock_negotiator(),
            create_no_op_mock_negotiator(),
        )
        .await
        .expect("Negotiation failed.");

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
            ready(Ok(NegotiatorOutput::Success)).boxed()
        },
    );

    let mut client_negotiator = MockNegotiator::new();
    client_negotiator.expect_negotiate_connection().return_once(
        move |my_peer_id, other_peer_id, _, _, _| {
            assert_eq!(my_peer_id, client_peer_id);
            assert_eq!(other_peer_id, server_peer_id);
            ready(Ok(NegotiatorOutput::Success)).boxed()
        },
    );

    perform_composed_upgrade_with_negotiator_with_keys(
        &server_id,
        &client_id,
        server_negotiator,
        client_negotiator,
    )
    .await
    .expect("Negotiation failed.");
}

#[tokio::test]
async fn test_composed_noise_config_returns_failure_when_initiator_fails() {
    let mut server_negotiator = MockNegotiator::new();
    server_negotiator
        .expect_negotiate_connection()
        .return_once(|_, _, _, _, _| ready(Ok(NegotiatorOutput::Success)).boxed());

    let mut client_negotiator = MockNegotiator::new();
    client_negotiator
        .expect_negotiate_connection()
        .return_once(|_, _, _, _, _| ready(Err(IoError::other("dummy error"))).boxed());

    perform_composed_upgrade_with_negotiator(server_negotiator, client_negotiator)
        .await
        .expect_err("Negotiation should have failed.");
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
        .return_once(|_, _, _, _, _| ready(Ok(NegotiatorOutput::Success)).boxed());

    perform_composed_upgrade_with_negotiator(server_negotiator, client_negotiator)
        .await
        .expect_err("Negotiation should have failed.");
}

#[tokio::test]
async fn test_composed_noise_config_transfers_messages_between_peers() {
    // For this test we create an actual implementation to work around lifetime issues of the
    // channel when using mocks.
    #[derive(Clone)]
    struct EvenOddNegotiator;

    #[async_trait]
    impl Negotiator for EvenOddNegotiator {
        type WireMessage = StarkAuthentication;
        type Error = std::io::Error;

        fn protocol_name(&self) -> &'static str {
            "unused"
        }

        async fn negotiate_connection(
            &mut self,
            _my_peer_id: PeerId,
            _other_peer_id: PeerId,
            connection_sender: &mut dyn negotiator::ConnectionSender<Self::WireMessage>,
            connection_receiver: &mut dyn negotiator::ConnectionReceiver<Self::WireMessage>,
            side: NegotiationSide,
        ) -> Result<NegotiatorOutput, Self::Error> {
            match side {
                NegotiationSide::Inbound => {
                    // a. Send challenge 1
                    let (_, res) = tokio::try_join!(
                        connection_sender.send(stark_auth_with_challenge(1)),
                        connection_receiver.receive()
                    )
                    .expect("failure");
                    // b. Expect challenge 2
                    assert_eq!(res, stark_auth_with_challenge(2));
                    // c. Send challenge 3
                    let (_, res) = tokio::try_join!(
                        connection_sender.send(stark_auth_with_challenge(3)),
                        connection_receiver.receive()
                    )
                    .expect("failure");
                    // d. Expect challenge 4
                    assert_eq!(res, stark_auth_with_challenge(4));
                }
                NegotiationSide::Outbound => {
                    // a. Send challenge 2
                    let (_, res) = tokio::try_join!(
                        connection_sender.send(stark_auth_with_challenge(2)),
                        connection_receiver.receive()
                    )
                    .expect("failure");
                    // b. Expect challenge 1
                    assert_eq!(res, stark_auth_with_challenge(1));
                    // c. Send challenge 4
                    let (_, res) = tokio::try_join!(
                        connection_sender.send(stark_auth_with_challenge(4)),
                        connection_receiver.receive()
                    )
                    .expect("failure");
                    // d. Expect challenge 3
                    assert_eq!(res, stark_auth_with_challenge(3));
                }
            }

            Ok(NegotiatorOutput::Success)
        }
    }

    perform_composed_upgrade_with_negotiator(EvenOddNegotiator, EvenOddNegotiator)
        .await
        .expect("Negotiation failed.");
}

/// Helper function to create the security upgrade pair (incoming/outgoing) and run them.
async fn perform_composed_upgrade_with_negotiator_with_keys<T>(
    server_keypair: &identity::Keypair,
    client_keypair: &identity::Keypair,
    server_negotiator: T,
    client_negotiator: T,
) -> Result<(PeerId, PeerId), NegotiatorError>
where
    T: Negotiator + Clone + 'static,
    T::WireMessage: Send,
{
    let (client_stream, server_stream, _) = get_connected_streams().await;

    let ((client_peer_id, _), (server_peer_id, _)) = try_join(
        ComposedNoise::new(server_keypair, server_negotiator)
            .unwrap()
            .upgrade_inbound(server_stream, "unused".to_string()),
        ComposedNoise::new(client_keypair, client_negotiator)
            .unwrap()
            .upgrade_outbound(client_stream, "unused".to_string()),
    )
    .await?;

    Ok((client_peer_id, server_peer_id))
}

/// Similar to perform_upgrade_with_negotiator_with_keys. To be used when you don't care about
/// the keys.
async fn perform_composed_upgrade_with_negotiator<T>(
    server_negotiator: T,
    client_negotiator: T,
) -> Result<(PeerId, PeerId), NegotiatorError>
where
    T: Negotiator + Clone + 'static,
    T::WireMessage: Send,
{
    let server_keypair = get_keypair(0);
    let client_keypair = get_keypair(1);

    perform_composed_upgrade_with_negotiator_with_keys(
        &server_keypair,
        &client_keypair,
        server_negotiator,
        client_negotiator,
    )
    .await
}

// TODO(noam.s): Test the duplicate peer case once implemented.
// TODO(noam.s): Add Tests that fail authentication at different stages
