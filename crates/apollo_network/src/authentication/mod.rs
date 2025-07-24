#[cfg(test)]
mod test;

use core::future::Future;
use std::io::Error as IoError;
use std::pin::Pin;

use async_trait::async_trait;
use asynchronous_codec::{Decoder, Encoder, Framed};
use bytes::{Buf, BytesMut};
use futures::io::{AsyncRead, AsyncWrite};
use futures::{FutureExt, SinkExt, StreamExt};
use libp2p::core::upgrade::{InboundConnectionUpgrade, OutboundConnectionUpgrade};
use libp2p::core::UpgradeInfo;
use libp2p::noise::{self, Error, Output};
use libp2p::{identity, PeerId};
use negotiator::{ConnectionEndpoint, Negotiator, NegotiatorError, NegotiatorOutput};
use noise::Error as NoiseError;

pub mod negotiator;

/// A wrapper around Framed that implements ConnectionEndpoint
pub struct FramedConnectionEndpoint<T: AsyncRead + AsyncWrite> {
    framed: Framed<T, NegotiatorChannelCodec>,
}

impl<T: AsyncRead + AsyncWrite> FramedConnectionEndpoint<T> {
    pub fn new(io: T) -> Self {
        Self { framed: Framed::new(io, NegotiatorChannelCodec) }
    }

    pub fn into_inner(self) -> T {
        self.framed.into_inner()
    }
}

#[async_trait]
impl<T> ConnectionEndpoint for FramedConnectionEndpoint<T>
where
    T: AsyncRead + AsyncWrite + Unpin + Send,
{
    async fn send(&mut self, data: Vec<u8>) -> Result<(), IoError> {
        self.framed.send(data).await
    }

    async fn receive(&mut self) -> Result<Vec<u8>, IoError> {
        match self.framed.next().await {
            Some(Ok(data)) => Ok(data),
            Some(Err(e)) => Err(e),
            None => Err(IoError::new(std::io::ErrorKind::UnexpectedEof, "Stream ended")),
        }
    }
}

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
enum NegotiationSide {
    Inbound,
    Outbound,
}

type UpgradeFuture<Socket> =
    Pin<Box<dyn Future<Output = Result<(PeerId, Output<Socket>), Error>> + Send>>;

impl<T> ComposedNoiseConfig<T>
where
    T: Negotiator + 'static,
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

    fn upgrade_connection<Socket>(
        mut self,
        socket: Socket,
        side: NegotiationSide,
    ) -> UpgradeFuture<Socket>
    where
        Socket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let noise_upgrade_future = match side {
            NegotiationSide::Inbound => self.noise_config.upgrade_inbound(socket, "unused"),
            NegotiationSide::Outbound => self.noise_config.upgrade_outbound(socket, "unused"),
        };

        if self.negotiator.is_none() {
            return noise_upgrade_future;
        }

        async move {
            let (pk, io) = noise_upgrade_future.await?;

            let mut connection_endpoint = FramedConnectionEndpoint::new(io);

            let negotiator = self
                .negotiator
                .as_mut()
                .expect("This future should not have been returned if negotiator is None");
            if let NegotiatorOutput::DuplicatePeer(_dup_peer_id) = match side {
                NegotiationSide::Inbound => {
                    negotiator
                        .negotiate_incoming_connection(
                            self.my_peer_id,
                            pk,
                            &mut connection_endpoint,
                        )
                        .await?
                }
                NegotiationSide::Outbound => {
                    negotiator
                        .negotiate_outgoing_connection(
                            self.my_peer_id,
                            pk,
                            &mut connection_endpoint,
                        )
                        .await?
                }
            } {
                // TODO(guy.f): Close the connection with the other peer.
            }

            let io = connection_endpoint.into_inner();

            Ok((pk, io))
        }
        .boxed()
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
            // TODO(guy.f): Check this value with leo@.
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

    fn upgrade_inbound(self, socket: Socket, _: Self::Info) -> Self::Future {
        self.upgrade_connection(socket, NegotiationSide::Inbound)
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

    fn upgrade_outbound(self, socket: Socket, _: Self::Info) -> Self::Future {
        self.upgrade_connection(socket, NegotiationSide::Outbound)
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
pub(crate) struct NegotiatorChannelCodec;

// This encoder works by writing the size of the message first (as a varint), followed by the
// message itself serialized to bytes.
// TODO(guy.f): Remove shared code with sqmr/messages.rs.
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
// TODO(guy.f): Move this to where it's used (network manager)
#[allow(dead_code)]
#[derive(Clone)]
pub(crate) enum DummyNegotiatorType {}

#[async_trait]
impl Negotiator for DummyNegotiatorType {
    fn protocol_name(&self) -> &'static str {
        unreachable!("Methods of DummyNegotiatorType should never be invoked");
    }

    async fn negotiate_incoming_connection(
        &mut self,
        _my_peer_id: PeerId,
        _other_peer_id: PeerId,
        _connection: &mut dyn ConnectionEndpoint,
    ) -> Result<NegotiatorOutput, NegotiatorError> {
        unreachable!("Methods of DummyNegotiatorType should never be invoked");
    }

    async fn negotiate_outgoing_connection(
        &mut self,
        _my_peer_id: PeerId,
        _other_peer_id: PeerId,
        _connection: &mut dyn ConnectionEndpoint,
    ) -> Result<NegotiatorOutput, NegotiatorError> {
        unreachable!("Methods of DummyNegotiatorType should never be invoked");
    }
}
