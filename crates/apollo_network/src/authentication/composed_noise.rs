use core::future::Future;
use std::io::Error as IoError;
use std::pin::Pin;

use apollo_protobuf::codec::ProtoCodec;
use async_trait::async_trait;
use asynchronous_codec::Framed;
use futures::io::{AsyncRead, AsyncWrite};
use futures::stream::{SplitSink, SplitStream};
use futures::{FutureExt, SinkExt, StreamExt};
use libp2p::core::upgrade::{InboundConnectionUpgrade, OutboundConnectionUpgrade};
use libp2p::core::UpgradeInfo;
use libp2p::noise::{self, Error as NoiseError, Output};
use libp2p::{identity, PeerId};
use prost::Message;

use crate::authentication::negotiator::{
    ConnectionReceiver,
    ConnectionSender,
    NegotiationSide,
    Negotiator,
};

// TODO(noam.s): Consider this value again, maybe it should be configurable.
const MAX_WIRE_MESSAGE_SIZE: usize = 1024;

#[async_trait]
impl<T, M> ConnectionSender<M> for SplitSink<Framed<T, ProtoCodec<M>>, M>
where
    T: AsyncRead + AsyncWrite + Unpin + Send,
    M: Message + Unpin + Send,
{
    async fn send(&mut self, data: M) -> Result<(), IoError> {
        SinkExt::send(self, data).await
    }
}

#[async_trait]
impl<T, M> ConnectionReceiver<M> for SplitStream<Framed<T, ProtoCodec<M>>>
where
    T: AsyncRead + AsyncWrite + Unpin + Send,
    M: Message + Default + Unpin + Send,
{
    async fn receive(&mut self) -> Result<M, IoError> {
        match self.next().await {
            Some(Ok(data)) => Ok(data),
            Some(Err(e)) => Err(e),
            None => Err(IoError::new(std::io::ErrorKind::UnexpectedEof, "Stream ended")),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NegotiatorError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Noise(#[from] NoiseError),
    #[error("Custom negotiator error: {0}")]
    // We box the error to avoid generics complexity in this error type.
    // TODO(noam.s): Consider making this error generic.
    CustomNegotiator(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// A security upgrade which allows running an additional, custom, negotiation after Noise
/// negotiations have completed successfully.
#[derive(Clone)]
pub struct ComposedNoise<TNegotiator>
where
    TNegotiator: Negotiator,
{
    noise_config: noise::Config,
    my_peer_id: PeerId,
    negotiator: Option<TNegotiator>,
}

type UpgradeFuture<Socket> =
    Pin<Box<dyn Future<Output = Result<(PeerId, Output<Socket>), NegotiatorError>> + Send>>;

impl<TNegotiator> ComposedNoise<TNegotiator>
where
    TNegotiator: Negotiator + 'static,
    TNegotiator::WireMessage: Send,
{
    // TODO(noam.s): Remove this once we use the ComposedNoiseConfig in the network manager.
    #[allow(dead_code)]
    pub fn new(
        identity: &identity::Keypair,
        negotiator: Option<TNegotiator>,
    ) -> Result<Self, NegotiatorError> {
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

        async move {
            let Some(negotiator) = self.negotiator.as_mut() else {
                return noise_upgrade_future.await.map_err(NegotiatorError::Noise);
            };

            let (pk, io) = noise_upgrade_future.await?;

            let codec = ProtoCodec::<TNegotiator::WireMessage>::new(MAX_WIRE_MESSAGE_SIZE);
            let connections = Framed::new(io, codec);
            let (mut connection_sender, mut connection_receiver) = connections.split();

            // TODO(noam.s): Add timeout mechanism for negotiation to prevent hanging connections.
            // Consider following libp2p's approach with configurable timeout duration where the
            // transport is configured
            negotiator
                .negotiate_connection(
                    self.my_peer_id,
                    pk,
                    &mut connection_sender,
                    &mut connection_receiver,
                    side,
                )
                .await
                .map_err(|e| NegotiatorError::CustomNegotiator(Box::new(e)))?;

            let connections = connection_sender.reunite(connection_receiver).expect(
                "Reuniting the sender and receiver, which were split from the same socket, should \
                 never fail",
            );
            let io = connections.into_inner();

            Ok((pk, io))
        }
        .boxed()
    }
}

impl<TNegotiator> UpgradeInfo for ComposedNoise<TNegotiator>
where
    TNegotiator: Negotiator,
{
    type Info = String;
    type InfoIter = std::iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        std::iter::once(match &self.negotiator {
            // TODO(noam.s): Check this value with product team.
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

impl<TNegotiator, Socket> InboundConnectionUpgrade<Socket> for ComposedNoise<TNegotiator>
where
    TNegotiator: Negotiator + 'static,
    TNegotiator::WireMessage: Send,
    Socket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = (PeerId, Output<Socket>);
    type Error = NegotiatorError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, socket: Socket, _: Self::Info) -> Self::Future {
        self.upgrade_connection(socket, NegotiationSide::Inbound)
    }
}

impl<TNegotiator, Socket> OutboundConnectionUpgrade<Socket> for ComposedNoise<TNegotiator>
where
    TNegotiator: Negotiator + 'static,
    TNegotiator::WireMessage: Send,
    Socket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = (PeerId, Output<Socket>);
    type Error = NegotiatorError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, socket: Socket, _: Self::Info) -> Self::Future {
        self.upgrade_connection(socket, NegotiationSide::Outbound)
    }
}
