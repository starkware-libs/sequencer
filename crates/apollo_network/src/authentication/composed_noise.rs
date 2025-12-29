use core::future::Future;
use std::pin::Pin;

use asynchronous_codec::Framed;
use futures::io::{AsyncRead, AsyncWrite};
use futures::{FutureExt, StreamExt};
use libp2p::core::upgrade::{InboundConnectionUpgrade, OutboundConnectionUpgrade};
use libp2p::core::UpgradeInfo;
use libp2p::noise::{self, Error as NoiseError, Output};
use libp2p::{identity, PeerId};

use crate::authentication::codec::NegotiatorChannelCodec;
use crate::authentication::negotiator::{NegotiationSide, Negotiator, NegotiatorOutput};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NegotiatorError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Noise(#[from] NoiseError),
    #[error("Authentication failed: {0}")]
    // We box the error to avoid generics complexity in this error type.
    AuthenticationFailed(#[from] Box<dyn std::error::Error + Send + Sync>),
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

        if self.negotiator.is_none() {
            return Box::pin(
                async move { noise_upgrade_future.await.map_err(NegotiatorError::Noise) },
            );
        }

        async move {
            let (pk, io) = noise_upgrade_future.await?;

            let connections = Framed::new(io, NegotiatorChannelCodec);
            let (mut connection_sender, mut connection_receiver) = connections.split();

            let negotiator = self
                .negotiator
                .as_mut()
                .expect("This future should not have been returned if negotiator is None");
            if let NegotiatorOutput::DuplicatePeer(_dup_peer_id) = negotiator
                .negotiate_connection(
                    self.my_peer_id,
                    pk,
                    &mut connection_sender,
                    &mut connection_receiver,
                    side,
                )
                .await
                .map_err(|e| NegotiatorError::AuthenticationFailed(Box::new(e)))?
            {
                // TODO(noam.s): Close the connection with the other peer.
            }

            let connections = connection_sender
                .reunite(connection_receiver)
                .expect("Failed to reunite connection");
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
