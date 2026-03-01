use std::io::Error as IoError;

use apollo_network_types::test_utils::{get_keypair, DUMMY_KEYPAIR};
use apollo_protobuf::authentication::ChallengeAndIdentity;
use apollo_protobuf::protobuf::StarkAuthentication;
use async_trait::async_trait;
use futures::future::try_join;
use libp2p::core::upgrade::{InboundConnectionUpgrade, OutboundConnectionUpgrade};
use libp2p::core::UpgradeInfo;
use libp2p::{identity, PeerId};
use starknet_api::crypto::utils::{Challenge, PublicKey};

use crate::authentication::composed_noise::{ComposedNoise, NegotiatorError};
use crate::authentication::negotiator::{NegotiationSide, Negotiator, NegotiatorOutput};
use crate::test_utils::get_connected_streams;

fn stark_auth_with_challenge(n: u8) -> StarkAuthentication {
    let challenge_and_identity = ChallengeAndIdentity {
        operational_public_key: PublicKey::default(),
        challenge: Challenge::from(u128::from(n)),
    };
    challenge_and_identity.into()
}

#[derive(Clone)]
struct NoOpNegotiator {
    protocol_name: &'static str,
}

impl NoOpNegotiator {
    fn new() -> Self {
        Self { protocol_name: "no_op" }
    }

    fn with_protocol_name(protocol_name: &'static str) -> Self {
        Self { protocol_name }
    }
}

#[async_trait]
impl Negotiator for NoOpNegotiator {
    type WireMessage = StarkAuthentication;
    type Error = std::io::Error;

    fn protocol_name(&self) -> &'static str {
        self.protocol_name
    }

    async fn negotiate_connection(
        &mut self,
        _my_peer_id: PeerId,
        _other_peer_id: PeerId,
        _connection_sender: &mut dyn super::negotiator::ConnectionSender<Self::WireMessage>,
        _connection_receiver: &mut dyn super::negotiator::ConnectionReceiver<Self::WireMessage>,
        _side: NegotiationSide,
    ) -> Result<NegotiatorOutput, Self::Error> {
        Ok(NegotiatorOutput::Success)
    }
}

#[derive(Clone)]
struct FailingNegotiator;

#[async_trait]
impl Negotiator for FailingNegotiator {
    type WireMessage = StarkAuthentication;
    type Error = std::io::Error;

    fn protocol_name(&self) -> &'static str {
        "failing"
    }

    async fn negotiate_connection(
        &mut self,
        _my_peer_id: PeerId,
        _other_peer_id: PeerId,
        _connection_sender: &mut dyn super::negotiator::ConnectionSender<Self::WireMessage>,
        _connection_receiver: &mut dyn super::negotiator::ConnectionReceiver<Self::WireMessage>,
        _side: NegotiationSide,
    ) -> Result<NegotiatorOutput, Self::Error> {
        Err(IoError::other("dummy error"))
    }
}

#[derive(Clone)]
struct PeerIdAssertingNegotiator {
    expected_my_peer_id: PeerId,
    expected_other_peer_id: PeerId,
}

#[async_trait]
impl Negotiator for PeerIdAssertingNegotiator {
    type WireMessage = StarkAuthentication;
    type Error = std::io::Error;

    fn protocol_name(&self) -> &'static str {
        "peer_id_asserting"
    }

    async fn negotiate_connection(
        &mut self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        _connection_sender: &mut dyn super::negotiator::ConnectionSender<Self::WireMessage>,
        _connection_receiver: &mut dyn super::negotiator::ConnectionReceiver<Self::WireMessage>,
        _side: NegotiationSide,
    ) -> Result<NegotiatorOutput, Self::Error> {
        assert_eq!(my_peer_id, self.expected_my_peer_id);
        assert_eq!(other_peer_id, self.expected_other_peer_id);
        Ok(NegotiatorOutput::Success)
    }
}

/// Negotiator that exchanges messages in a ping-pong pattern to verify that the communication
/// channel works correctly. Inbound sends odd challenges (1, 3) and outbound sends even (2, 4).
#[derive(Clone)]
struct EvenOddNegotiator;

#[async_trait]
impl Negotiator for EvenOddNegotiator {
    type WireMessage = StarkAuthentication;
    type Error = std::io::Error;

    fn protocol_name(&self) -> &'static str {
        "even_odd"
    }

    async fn negotiate_connection(
        &mut self,
        _my_peer_id: PeerId,
        _other_peer_id: PeerId,
        connection_sender: &mut dyn super::negotiator::ConnectionSender<Self::WireMessage>,
        connection_receiver: &mut dyn super::negotiator::ConnectionReceiver<Self::WireMessage>,
        side: NegotiationSide,
    ) -> Result<NegotiatorOutput, Self::Error> {
        match side {
            NegotiationSide::Inbound => {
                let (_, res) = tokio::try_join!(
                    connection_sender.send(stark_auth_with_challenge(1)),
                    connection_receiver.receive()
                )
                .expect("failure");
                assert_eq!(res, stark_auth_with_challenge(2));
                let (_, res) = tokio::try_join!(
                    connection_sender.send(stark_auth_with_challenge(3)),
                    connection_receiver.receive()
                )
                .expect("failure");
                assert_eq!(res, stark_auth_with_challenge(4));
            }
            NegotiationSide::Outbound => {
                let (_, res) = tokio::try_join!(
                    connection_sender.send(stark_auth_with_challenge(2)),
                    connection_receiver.receive()
                )
                .expect("failure");
                assert_eq!(res, stark_auth_with_challenge(1));
                let (_, res) = tokio::try_join!(
                    connection_sender.send(stark_auth_with_challenge(4)),
                    connection_receiver.receive()
                )
                .expect("failure");
                assert_eq!(res, stark_auth_with_challenge(3));
            }
        }

        Ok(NegotiatorOutput::Success)
    }
}

#[test]
fn test_composed_noise_config_generates_protocol_info() {
    const PROTOCOL_NAME: &str = "test_protocol_name";

    let negotiator = NoOpNegotiator::with_protocol_name(PROTOCOL_NAME);
    let config = ComposedNoise::new(&DUMMY_KEYPAIR, negotiator).unwrap();

    let mut protocol_name_iterator = config.protocol_info();
    assert_eq!(protocol_name_iterator.next().unwrap(), format!("/noise_with_{PROTOCOL_NAME}"));
    assert!(protocol_name_iterator.next().is_none());
}

#[tokio::test]
async fn test_composed_noise_config_upgrade_with_no_op_negotiator() {
    let server_id = get_keypair(0);
    let client_id = get_keypair(1);

    let (reported_client_id, reported_server_id) =
        perform_composed_upgrade_with_negotiator_with_keys(
            &server_id,
            &client_id,
            NoOpNegotiator::new(),
            NoOpNegotiator::new(),
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

    let server_negotiator = PeerIdAssertingNegotiator {
        expected_my_peer_id: server_peer_id,
        expected_other_peer_id: client_peer_id,
    };
    let client_negotiator = PeerIdAssertingNegotiator {
        expected_my_peer_id: client_peer_id,
        expected_other_peer_id: server_peer_id,
    };

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
    perform_composed_upgrade_with_negotiator(NoOpNegotiator::new(), FailingNegotiator)
        .await
        .expect_err("Negotiation should have failed.");
}

// TODO(noam.s): Add a test for negotiation timeout when one side stops negotiating.
#[tokio::test]
async fn test_composed_noise_config_returns_failure_when_responder_fails() {
    perform_composed_upgrade_with_negotiator(FailingNegotiator, NoOpNegotiator::new())
        .await
        .expect_err("Negotiation should have failed.");
}

#[tokio::test]
async fn test_composed_noise_config_transfers_messages_between_peers() {
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

/// Similar to perform_composed_upgrade_with_negotiator_with_keys. To be used when you don't care
/// about the keys.
async fn perform_composed_upgrade_with_negotiator<S, C>(
    server_negotiator: S,
    client_negotiator: C,
) -> Result<(PeerId, PeerId), NegotiatorError>
where
    S: Negotiator + Clone + 'static,
    S::WireMessage: Send,
    C: Negotiator + Clone + 'static,
    C::WireMessage: Send,
{
    let server_keypair = get_keypair(0);
    let client_keypair = get_keypair(1);

    let (client_stream, server_stream, _) = get_connected_streams().await;

    let ((client_peer_id, _), (server_peer_id, _)) = try_join(
        ComposedNoise::new(&server_keypair, server_negotiator)
            .unwrap()
            .upgrade_inbound(server_stream, "unused".to_string()),
        ComposedNoise::new(&client_keypair, client_negotiator)
            .unwrap()
            .upgrade_outbound(client_stream, "unused".to_string()),
    )
    .await?;

    Ok((client_peer_id, server_peer_id))
}

// TODO(noam.s): Test the duplicate peer case once implemented.
// TODO(noam.s): Add tests that fail authentication at different stages.
