use core::future::Future;
use std::pin::Pin;

use apollo_protobuf::codec::ProtoCodec;
use asynchronous_codec::Framed;
use futures::io::{AsyncRead, AsyncWrite};
use futures::{FutureExt, StreamExt};
use libp2p::core::upgrade::{InboundConnectionUpgrade, OutboundConnectionUpgrade};
use libp2p::core::UpgradeInfo;
use libp2p::noise::{self, Error as NoiseError, Output};
use libp2p::{identity, PeerId};
use tracing::debug;

use crate::authentication::negotiator::{NegotiationSide, Negotiator, NegotiatorOutput};

// TODO(noam.s): Consider this value again, maybe it should be configurable.
const MAX_WIRE_MESSAGE_SIZE: usize = 10000;

pub type NegotiatorCodec = ProtoCodec<Vec<u8>>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NegotiatorError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Noise(#[from] NoiseError),
    #[error("Custom negotiator error: {0}")]
    // We box the error to avoid generics complexity in this error type.
    CustomNegotiator(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// A security upgrade which allows running an additional, custom, negotiation after Noise
/// negotiations have completed successfully.
#[derive(Clone)]
pub struct ComposedNoiseConfig<T>
where
    T: Negotiator,
{
    noise_config: noise::Config,
    my_peer_id: PeerId,
    negotiator: Option<T>,
}

type UpgradeFuture<Socket> =
    Pin<Box<dyn Future<Output = Result<(PeerId, Output<Socket>), NegotiatorError>> + Send>>;

impl<T> ComposedNoiseConfig<T>
where
    T: Negotiator + 'static,
{
    // TODO(noam.s): Remove this once we use the ComposedNoiseConfig in the network manager.
    #[allow(dead_code)]
    pub fn new(
        identity: &identity::Keypair,
        negotiator: Option<T>,
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

            let connections = Framed::new(io, NegotiatorCodec::new(MAX_WIRE_MESSAGE_SIZE));
            let (mut connection_sender, mut connection_receiver) = connections.split();

            let negotiator_output = negotiator
                .negotiate_connection(
                    self.my_peer_id,
                    pk,
                    &mut connection_sender,
                    &mut connection_receiver,
                    side,
                )
                .await
                .map_err(|e| NegotiatorError::CustomNegotiator(Box::new(e)))?;
            if let NegotiatorOutput::DuplicatePeer(dup_peer_id) = negotiator_output {
                debug!("Duplicate peer detected: {dup_peer_id}");
                // TODO(noam.s): Close the connection with the other peer.
            }

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

impl<T> UpgradeInfo for ComposedNoiseConfig<T>
where
    T: Negotiator,
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

impl<T, Socket> InboundConnectionUpgrade<Socket> for ComposedNoiseConfig<T>
where
    T: Negotiator + 'static,
    Socket: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Output = (PeerId, Output<Socket>);
    type Error = NegotiatorError;
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
    type Error = NegotiatorError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, socket: Socket, _: Self::Info) -> Self::Future {
        self.upgrade_connection(socket, NegotiationSide::Outbound)
    }
}
