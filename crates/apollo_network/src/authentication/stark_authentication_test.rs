use std::sync::Arc;

use apollo_network_types::network_types::PeerId;
use apollo_signature_manager_types::MockSignatureManagerClient;
use futures::channel::mpsc;
use hex::FromHex;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::crypto::utils::PublicKey;
use starknet_api::{felt, nonce};
use starknet_core::crypto::Signature;
use starknet_types_core::felt::Felt;

use crate::authentication::negotiator::{AuthNegotiator, ConnectionEnd};
use crate::authentication::stark_authentication::{StarkAuthNegotiator, StarkAuthNegotiatorError};
use crate::Bytes;

const INITIATOR_IDENTITY_SIGNATURE: Signature = Signature {
    r: Felt::from_hex_unchecked(
        "0x7a8dd91774e806fa5e0880c77d61ccb40fc33fcb5db00fdc5585758cf95f79d",
    ),
    s: Felt::from_hex_unchecked(
        "0x5bdd4ddd34e2708c03c14d5872de1d541476f4af9f92f48a3d77d5ea3e16cc0",
    ),
};

const RESPONDER_IDENTITY_SIGNATURE: Signature = Signature {
    r: Felt::from_hex_unchecked(
        "0x46941eff1144482236ae780e1056d845d34c81973b652982b87747408fc77b8",
    ),
    s: Felt::from_hex_unchecked(
        "0x4fbff5d13cd0279460bee30fa891346a441562a629342047d0730b405b8ee93",
    ),
};

const INVALID_SIGNATURE: Signature =
    Signature { r: Felt::from_hex_unchecked("0x1234"), s: Felt::from_hex_unchecked("0x5678") };

#[rstest]
#[case::both_valid(INITIATOR_IDENTITY_SIGNATURE, true, RESPONDER_IDENTITY_SIGNATURE, true)]
#[case::initiator_valid_responder_invalid(
    INITIATOR_IDENTITY_SIGNATURE,
    false, // Responder's signature is invalid; initiator should reject.
    INVALID_SIGNATURE,
    true
)]
#[case::initiator_invalid_responder_valid(
    INVALID_SIGNATURE,
    true,
    RESPONDER_IDENTITY_SIGNATURE,
    false // Initiator's signature is invalid; responder should reject.
)]
#[case::both_invalid(INVALID_SIGNATURE, false, INVALID_SIGNATURE, false)]
#[tokio::test]
async fn stark_authentication_protocol(
    #[case] initiator_signature: Signature,
    #[case] initiator_approves: bool,
    #[case] responder_signature: Signature,
    #[case] responder_approves: bool,
) {
    // Setup.

    // Setup connections: a bidirectional channel.
    let channel_size = 8;
    let (initiator_sender, initiator_receiver) = mpsc::channel::<Bytes>(channel_size);
    let (responder_sender, responder_receiver) = mpsc::channel::<Bytes>(channel_size);
    let initiator_connection =
        ChannelEnd { sender: initiator_sender, receiver: responder_receiver };
    let responder_connection =
        ChannelEnd { sender: responder_sender, receiver: initiator_receiver };

    // Set expectations: both sides will sign successfully.
    // Since the protocol is sequential, we can use the same mock signer for both sides.
    let mut expected_signatures = [initiator_signature, responder_signature].into_iter();
    let mut signer = MockSignatureManagerClient::new();
    signer.expect_identify().times(2).returning_st(move |_, _| {
        let signature = expected_signatures.next().unwrap().into();
        Box::pin(async move { Ok(signature) })
    });
    let signer = Arc::new(signer);

    // Setup peers.
    // TODO(Elin): use a test util once it's merged.
    let initiator_peer_id =
        Vec::from_hex("00205cccc292b9dcc77610797e5f47b23d2b0fb7b77010d76481fc2c0652f6ca2fc2")
            .unwrap();
    let initiator_peer_id = PeerId::from_bytes(&initiator_peer_id).unwrap();
    let initiator = StarkAuthNegotiator {
        peer_id: initiator_peer_id,
        public_key: PublicKey(felt!(
            "0x125d56b1fbba593f1dd215b7c55e384acd838cad549c4a2b9c6d32d264f4e2a"
        )),
        nonce: nonce!(0x1234),
        signer: signer.clone(),
    };

    let responder_peer_id =
        Vec::from_hex("0020812065dfb918a463ddd8038269ca5cc0ebc862bd7849214b1dfbe9e932812af6")
            .unwrap();
    let responder_peer_id = PeerId::from_bytes(&responder_peer_id).unwrap();
    let responder = StarkAuthNegotiator {
        peer_id: responder_peer_id,
        public_key: PublicKey(felt!(
            "0x02c5dbad71c92a45cc4b40573ae661f8147869a91d57b8d9b8f48c8af7f83159"
        )),
        nonce: nonce!(0x5678),
        signer,
    };

    // Test.
    let initiator_future = initiator
        .negotiate_outgoing_connection(
            initiator_peer_id,
            responder_peer_id,
            &mut initiator_connection,
        )
        .await;
    let responder_future = responder
        .negotiate_incoming_connection(
            responder_peer_id,
            initiator_peer_id,
            &mut responder_connection,
        )
        .await;

    let (initiator_negotiation_result, responder_negotiation_result) =
        tokio::join!(initiator_future, responder_future);

    // Assert successful negotiation.
    assert_eq!(initiator_negotiation_result.unwrap(), initiator_approves);
    assert_eq!(responder_negotiation_result.unwrap(), responder_approves);
}
