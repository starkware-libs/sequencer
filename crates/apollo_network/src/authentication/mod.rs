#[cfg(test)]
mod tests;

use core::future::Future;
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
use negotiator::{Error as NegotiatorError, Negotiator, Output as NegotiatorOutput};
use noise::Error as NoiseError;

pub mod negotiator;
pub mod test_util;

/// A security upgrade which allows running an additional, custom, negotiation after Noise
/// negotiations have completed successfully.
#[derive(Clone)]
pub(crate) struct ComposedNoiseConfig<T>
where
    T: Negotiator,
{
    noise_config: noise::Config,
    my_peer_id: PeerId,
    negotiator: Option<T>,
}

impl<T> ComposedNoiseConfig<T>
where
    T: Negotiator,
{
    // TODO(guy.f): Remove this once we use the ComposedNoiseConfig in the network manager.
    #[allow(dead_code)]
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
    Socket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = (PeerId, Output<Socket>);
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(mut self, socket: Socket, _: Self::Info) -> Self::Future {
        let noise_upgrade_future = self.noise_config.upgrade_inbound(socket, "unused");
        if self.negotiator.is_none() {
            return noise_upgrade_future;
        }
        async move {
            let (pk, io) = noise_upgrade_future.await?;

            let mut negotiator_channel = Framed::new(io, NegotiatorChannelCodec);

            if let NegotiatorOutput::DuplicatePeer(_dup_peer_id) = self
                .negotiator
                .as_mut()
                .expect("This future should not have been returned if negotiator is None")
                .negotiate_incoming_connection(self.my_peer_id, pk, &mut negotiator_channel)
                .await?
            {
                // TODO(guy.f): Close the connection with the other peer.
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
    Socket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = (PeerId, Output<Socket>);
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(mut self, socket: Socket, _: Self::Info) -> Self::Future {
        let noise_upgrade_future = self.noise_config.upgrade_outbound(socket, "unused");
        if self.negotiator.is_none() {
            return noise_upgrade_future;
        }
        async move {
            let (pk, io) = noise_upgrade_future.await?;

            let mut negotiator_channel = Framed::new(io, NegotiatorChannelCodec);

            if let NegotiatorOutput::DuplicatePeer(_dup_peer_id) = self
                .negotiator
                .as_mut()
                .expect("This future should not have been returned if negotiator is None")
                .negotiate_outgoing_connection(self.my_peer_id, pk, &mut negotiator_channel)
                .await?
            {
                // TODO(guy.f): Close the connection with the other peer.
            }

            let io = negotiator_channel.into_inner();

            Ok((pk, io))
        }
        .boxed()
    }
}

impl From<NegotiatorError> for NoiseError {
    fn from(error: NegotiatorError) -> Self {
        match error {
            NegotiatorError::Io(io_error) => NoiseError::Io(io_error),
            NegotiatorError::AuthenticationFailed => NoiseError::AuthenticationFailed,
        }
    }
}

#[derive(Default)]
pub struct NegotiatorChannelCodec;

// This encoder works by writing the size of the message first (as a varint), followed by the
// message itself serialized to bytes.
impl Encoder for NegotiatorChannelCodec {
    type Item<'a> = Vec<u8>;
    type Error = std::io::Error;

    fn encode(&mut self, item: Self::Item<'_>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // Encode the size using unsigned_varint
        let mut size_buf = unsigned_varint::encode::usize_buffer();
        let size_bytes = unsigned_varint::encode::usize(item.len(), &mut size_buf);

        // Write the size followed by the data
        dst.extend_from_slice(size_bytes);
        dst.extend_from_slice(&item);

        Ok(())
    }
}

// Decodes a message encoded with the NegotiatorChannelCodec. See there for more details.
impl Decoder for NegotiatorChannelCodec {
    type Item = Vec<u8>;
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
                    format!("Failed to decode size: {e}"),
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

        Ok(Some(data))
    }
}

/// This is a dummy implementation of the Negotiator trait used only so you can use
/// `Option<Negotiator>::None` (where you don't have a real type). Instances of this type should
/// never be created.
// We make it an enum to enforce that it is not possible to create an instance of this type.
// TODO(guy.f): Remove the allow(dead_code) once we use the ComposedNoiseConfig in the network
// manager.
#[allow(dead_code)]
#[derive(Clone)]
pub(crate) enum DummyNegotiatorType {}

#[async_trait]
impl Negotiator for DummyNegotiatorType {
    fn protocol_name(&self) -> &'static str {
        unreachable!("Methods of DummyNegotiatorType should never be invoked");
    }

    async fn negotiate_incoming_connection<NegotiatorChannel>(
        &mut self,
        _my_peer_id: PeerId,
        _other_peer_id: PeerId,
        _connection: &mut NegotiatorChannel,
    ) -> Result<NegotiatorOutput, NegotiatorError>
    where
        NegotiatorChannel: Sink<Vec<u8>, Error = std::io::Error>
            + Stream<Item = Result<Vec<u8>, std::io::Error>>
            + Unpin
            + Send,
    {
        unreachable!("Methods of DummyNegotiatorType should never be invoked");
    }

    async fn negotiate_outgoing_connection<NegotiatorChannel>(
        &mut self,
        _my_peer_id: PeerId,
        _other_peer_id: PeerId,
        _connection: &mut NegotiatorChannel,
    ) -> Result<NegotiatorOutput, NegotiatorError>
    where
        NegotiatorChannel: Sink<Vec<u8>, Error = std::io::Error>
            + Stream<Item = Result<Vec<u8>, std::io::Error>>
            + Unpin
            + Send,
    {
        unreachable!("Methods of DummyNegotiatorType should never be invoked");
    }
}
