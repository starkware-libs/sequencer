use core::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

use async_trait::async_trait;
use asynchronous_codec::{Decoder, Encoder, Framed};
use bytes::{Buf, BytesMut};
use futures::io::{AsyncRead, AsyncWrite};
use futures::{FutureExt, Sink, Stream};
use libp2p::core::upgrade::{InboundConnectionUpgrade, OutboundConnectionUpgrade};
use libp2p::core::UpgradeInfo;
use libp2p::noise::{self, Error, Output};
use libp2p::{identity, PeerId};
use negotiator::{Negotiator, NegotiatorResult};

pub mod negotiator;

/// This is a security upgrade which allows running an additional, custom, negotiation after Noise
/// negotations have completed successfully.
#[derive(Clone)]
pub(crate) struct ComposedNoiseConfig<T>
where
    T: Negotiator,
    T::Message: TryFrom<Vec<u8>> + Into<Vec<u8>>,
{
    noise_config: noise::Config,
    my_peer_id: PeerId,
    negotiator: Option<T>,
}

impl<T> ComposedNoiseConfig<T>
where
    T: Negotiator,
    T::Message: TryFrom<Vec<u8>> + Into<Vec<u8>>,
{
    pub fn new(identity: &identity::Keypair, negotiator: Option<T>) -> Result<Self, Error> {
        Ok(Self {
            noise_config: noise::Config::new(identity)?,
            my_peer_id: identity.public().to_peer_id(),
            negotiator,
        })
    }
}

impl<T> UpgradeInfo for ComposedNoiseConfig<T>
where
    T: Negotiator,
    T::Message: TryFrom<Vec<u8>> + Into<Vec<u8>>,
{
    type Info = String;
    type InfoIter = std::iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        std::iter::once(match &self.negotiator {
            Some(negotiator) => format!("/noise_with_{}", negotiator.protocol_name()),
            None => String::from(
                self.noise_config
                    .protocol_info()
                    .next()
                    .expect("Noise protocol info should return a single value"),
            ),
        })
    }
}

impl<T, Socket> InboundConnectionUpgrade<Socket> for ComposedNoiseConfig<T>
where
    T: Negotiator + 'static,
    T::Message: TryFrom<Vec<u8>> + Into<Vec<u8>>,
    Socket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = (PeerId, Output<Socket>);
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, socket: Socket, _: Self::Info) -> Self::Future {
        let noise_upgrade_future = self.noise_config.upgrade_inbound(socket, "unused");
        if self.negotiator.is_none() {
            return noise_upgrade_future;
        }
        async move {
            let (pk, io) = noise_upgrade_future.await?;

            let mut negotiator_channel =
                Framed::new(io, NegotiatorChannelCodec::<T::Message>::new());

            if let NegotiatorResult::DuplicatePeer(_dup_peer_id) = self
                .negotiator
                .expect("This future should not have been returned if negotiator is None")
                .negotiate_incoming_connection(self.my_peer_id, pk, &mut negotiator_channel)
                .await?
            {
                // TODO: Close the connection with the other peer.
            }

            let io = negotiator_channel.into_inner();

            Ok((pk, io))
        }
        .boxed()
    }
}

impl<T, Socket> OutboundConnectionUpgrade<Socket> for ComposedNoiseConfig<T>
where
    T: Negotiator + 'static,
    T::Message: TryFrom<Vec<u8>> + Into<Vec<u8>>,
    Socket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = (PeerId, Output<Socket>);
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, socket: Socket, _: Self::Info) -> Self::Future {
        let noise_upgrade_future = self.noise_config.upgrade_outbound(socket, "unused");
        if self.negotiator.is_none() {
            return noise_upgrade_future;
        }
        async move {
            let (pk, io) = noise_upgrade_future.await?;

            let mut negotiator_channel =
                Framed::new(io, NegotiatorChannelCodec::<T::Message>::new());

            if let NegotiatorResult::DuplicatePeer(_dup_peer_id) = self
                .negotiator
                .expect("This future should not have been returned if negotiator is None")
                .negotiate_outgoing_connection(self.my_peer_id, pk, &mut negotiator_channel)
                .await?
            {
                // TODO: Close the connection with the other peer.
            }

            let io = negotiator_channel.into_inner();

            Ok((pk, io))
        }
        .boxed()
    }
}

pub(crate) struct NegotiatorChannelCodec<T> {
    _phantom: PhantomData<T>,
}

impl<T> NegotiatorChannelCodec<T> {
    pub fn new() -> Self {
        Self { _phantom: PhantomData }
    }
}

impl<T> Default for NegotiatorChannelCodec<T> {
    fn default() -> Self {
        Self::new()
    }
}

// This encoder works by writing the size of the message first (as a varint), followed by the
// message itself serialized to bytes.
impl<T> Encoder for NegotiatorChannelCodec<T>
where
    T: Into<Vec<u8>>,
{
    type Item<'a> = T;
    type Error = std::io::Error;

    fn encode(&mut self, item: Self::Item<'_>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // Convert T to Vec<u8>
        let data: Vec<u8> = item.into();

        // Encode the size using unsigned_varint
        let mut size_buf = unsigned_varint::encode::usize_buffer();
        let size_bytes = unsigned_varint::encode::usize(data.len(), &mut size_buf);

        // Write the size followed by the data
        dst.extend_from_slice(size_bytes);
        dst.extend_from_slice(&data);

        Ok(())
    }
}

// Decodes a message encoded with the NegotiatorChannelCodec. See there for more details.
impl<T> Decoder for NegotiatorChannelCodec<T>
where
    T: TryFrom<Vec<u8>>,
{
    type Item = T;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Try to decode the size first
        let original_len = src.len();
        let (size, remaining_bytes) = match unsigned_varint::decode::usize(src) {
            Ok((size, remaining)) => (size, remaining),
            Err(unsigned_varint::decode::Error::Insufficient) => {
                // Not enough bytes to decode the size yet
                return Ok(None);
            }
            Err(e) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Failed to decode size: {}", e),
                ));
            }
        };

        let size_bytes_consumed = original_len - remaining_bytes.len();

        // Check if we have enough bytes for the complete message
        if src.len() < size_bytes_consumed + size {
            // Not enough bytes yet
            return Ok(None);
        }

        // Remove the size bytes from the buffer
        src.advance(size_bytes_consumed);

        // Extract the data bytes
        let data = src.split_to(size).to_vec();

        // Convert bytes back to T
        match T::try_from(data) {
            Ok(item) => Ok(Some(item)),
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Failed to convert bytes to target type",
            )),
        }
    }
}

/// This is a dummy implementation of the Negotiator trait used only so you can use
/// Option<Negotiator>::None (where you don't have a real type). Instances of this type should never
/// be created.
// We make it an enum to enforce that it is not possible to create an instance of this type.
#[derive(Clone)]
pub(crate) enum DummyNegotiatorType {}

#[async_trait]
impl Negotiator for DummyNegotiatorType {
    type Message = Vec<u8>;

    fn protocol_name(&self) -> &'static str {
        unreachable!("Methods of DummyNegotiatorType should never be invoked");
    }

    async fn negotiate_incoming_connection<NegotiatorChannel>(
        &self,
        _my_peer_id: PeerId,
        _other_peer_id: PeerId,
        _connection: &mut NegotiatorChannel,
    ) -> Result<NegotiatorResult, Error>
    where
        NegotiatorChannel: Sink<Self::Message, Error = std::io::Error>
            + Stream<Item = Result<Self::Message, std::io::Error>>
            + Unpin
            + Send,
    {
        unreachable!("Methods of DummyNegotiatorType should never be invoked");
    }

    async fn negotiate_outgoing_connection<NegotiatorChannel>(
        &self,
        _my_peer_id: PeerId,
        _other_peer_id: PeerId,
        _connection: &mut NegotiatorChannel,
    ) -> Result<NegotiatorResult, Error>
    where
        NegotiatorChannel: Sink<Self::Message, Error = std::io::Error>
            + Stream<Item = Result<Self::Message, std::io::Error>>
            + Unpin
            + Send,
    {
        unreachable!("Methods of DummyNegotiatorType should never be invoked");
    }
}

#[cfg(test)]
mod tests {
    use lazy_static::lazy_static;
    use libp2p::core::upgrade::InboundConnectionUpgrade;
    use mockall::mock;

    use super::*;
    use crate::test_utils::get_connected_streams;

    mock! {
        pub TestNegotiator {}

        #[async_trait]
        impl Negotiator for TestNegotiator {
            type Message = Vec<u8>;

            async fn negotiate_incoming_connection<NegotiatorChannel>(
                &self,
                my_peer_id: PeerId,
                other_peer_id: PeerId,
                connection: &mut NegotiatorChannel,
            ) -> Result<NegotiatorResult, Error>
            where
                NegotiatorChannel: Sink<Vec<u8>, Error = std::io::Error>
                    + Stream<Item = Result<Vec<u8>, std::io::Error>>
                    + Unpin
                    + Send;

            async fn negotiate_outgoing_connection<NegotiatorChannel>(
                &self,
                my_peer_id: PeerId,
                other_peer_id: PeerId,
                connection: &mut NegotiatorChannel,
            ) -> Result<NegotiatorResult, Error>
            where
                NegotiatorChannel: Sink<Vec<u8>, Error = std::io::Error>
                    + Stream<Item = Result<Vec<u8>, std::io::Error>>
                    + Unpin
                    + Send;

            fn protocol_name(&self) -> &'static str;
        }

        impl Clone for TestNegotiator {
            fn clone(&self) -> Self;
        }
    }

    // TODO: Move this to the test utils once merged from main-v14
    lazy_static! {
        static ref DUMMY_KEYPAIR: identity::Keypair = get_keypair(0);
    }

    fn get_keypair(i: u8) -> identity::Keypair {
        let key = [i; 32];
        identity::Keypair::ed25519_from_bytes(key).unwrap()
    }

    const PROTOCOL_NAME: &'static str = "test_protocol_name";

    #[derive(Clone)]
    struct TestForProtocolNameNegotiator;

    #[async_trait]
    impl Negotiator for TestForProtocolNameNegotiator {
        type Message = Vec<u8>;

        fn protocol_name(&self) -> &'static str {
            PROTOCOL_NAME
        }

        async fn negotiate_incoming_connection<NegotiatorChannel>(
            &self,
            _my_peer_id: PeerId,
            _other_peer_id: PeerId,
            _connection: &mut NegotiatorChannel,
        ) -> Result<NegotiatorResult, Error>
        where
            NegotiatorChannel: Sink<Self::Message, Error = std::io::Error>
                + Stream<Item = Result<Self::Message, std::io::Error>>
                + Unpin
                + Send,
        {
            unreachable!("Methods of DummyNegotiatorType should never be invoked");
        }

        async fn negotiate_outgoing_connection<NegotiatorChannel>(
            &self,
            _my_peer_id: PeerId,
            _other_peer_id: PeerId,
            _connection: &mut NegotiatorChannel,
        ) -> Result<NegotiatorResult, Error>
        where
            NegotiatorChannel: Sink<Self::Message, Error = std::io::Error>
                + Stream<Item = Result<Self::Message, std::io::Error>>
                + Unpin
                + Send,
        {
            unreachable!("Methods of DummyNegotiatorType should never be invoked");
        }
    }

    #[test]
    fn test_generates_protocol_info() {
        let negotiator = TestForProtocolNameNegotiator;

        let config = ComposedNoiseConfig::<TestForProtocolNameNegotiator>::new(
            &DUMMY_KEYPAIR,
            Some(negotiator),
        )
        .unwrap();

        assert_eq!(config.protocol_info().next().unwrap(), format!("/noise_with_{PROTOCOL_NAME}"));
    }

    #[tokio::test]
    async fn test_composed_noise_config_upgrade_with_none_negotiator() {
        // Create a keypair for the test
        let server_id = identity::Keypair::generate_ed25519();
        let client_id = identity::Keypair::generate_ed25519();

        // Get connected streams for testing
        let (client_stream, server_stream, _) = get_connected_streams().await;

        let ((reported_client_id, _), (reported_server_id, _)) = futures::future::try_join(
            ComposedNoiseConfig::<DummyNegotiatorType>::new(&server_id, None)
                .unwrap()
                .upgrade_inbound(server_stream, "unused".to_string()),
            ComposedNoiseConfig::<DummyNegotiatorType>::new(&client_id, None)
                .unwrap()
                .upgrade_outbound(client_stream, "unused".to_string()),
        )
        .await
        .unwrap();

        assert_eq!(reported_client_id, client_id.public().to_peer_id());
        assert_eq!(reported_server_id, server_id.public().to_peer_id());
    }

    // TODO(guy.f): Test the duplicate peer case once implemented.
}
