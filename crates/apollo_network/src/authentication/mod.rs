use core::future::Future;
use std::convert::{Infallible, TryFrom};
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::channel::mpsc::{self, channel, SendError};
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use futures::{FutureExt, Sink, Stream};
use libp2p::core::upgrade::{InboundConnectionUpgrade, OutboundConnectionUpgrade};
use libp2p::core::UpgradeInfo;
use libp2p::noise::{self, Error, Output};
use libp2p::{identity, PeerId};

pub struct ConnectionEnd<S, R> {
    sender: S,
    receiver: R,
}

pub enum NegotiatorResult {
    Ok,
    /// Returned when the handshake concluded that the currently connecting peer is a duplicate of
    /// the currently connected peer. `PeerId` is the (older) currently connected peer.
    DuplicatePeer(PeerId),
}

#[async_trait]
pub trait Negotiator: Send + Clone {
    type Message: TryFrom<Vec<u8>> + Into<Vec<u8>> + Send;

    async fn negotiate<S, R>(
        &self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection: ConnectionEnd<S, R>,
    ) -> Result<NegotiatorResult, Error>
    where
        S: Sink<Self::Message> + Unpin + Send,
        R: Stream<Item = Self::Message> + Unpin + Send;

    /// For example: "strk_id" or "strk_id_v2".
    fn protocol_name(&self) -> &'static str;
}

#[derive(Clone)]
pub struct DummyHandshakeType;

#[async_trait]
impl Negotiator for DummyHandshakeType {
    type Message = Vec<u8>;

    fn protocol_name(&self) -> &'static str {
        "do_nothing_handshake"
    }

    async fn negotiate<S, R>(
        &self,
        _my_peer_id: PeerId,
        _other_peer_id: PeerId,
        _connection: ConnectionEnd<S, R>,
    ) -> Result<NegotiatorResult, Error>
    where
        S: Sink<Self::Message> + Unpin + Send,
        R: Stream<Item = Self::Message> + Unpin + Send,
    {
        Ok(NegotiatorResult::Ok)
    }
}

#[derive(Clone)]
pub struct ComposedNoiseConfig<T>
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
    // TODO: The new security upgrade expects only takes in KeyPair. This method should be wrapper
    // by a function that will call this method.
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
            let (pk, mut io) = noise_upgrade_future.await?;

            // Example of how to write:
            let buf = vec![0; 1024];
            io.write_all(&buf).await?;

            let (negotiator_send, received_from_negotiator) = mpsc::channel::<T::Message>(1024);
            let (send_to_negotiator, negotiator_receive) = mpsc::channel::<T::Message>(1024);
            // TODO: Create a mapping from the network to these channels.

            let connection_end =
                ConnectionEnd { sender: negotiator_send, receiver: negotiator_receive };

            if let NegotiatorResult::DuplicatePeer(dup_peer_id) = self
                .negotiator
                .expect("This future should not have been returned if negotiator is None")
                .negotiate(self.my_peer_id, pk, connection_end)
                .await?
            {
                // TODO: Close the connection with the other peer.
            }

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
        async move {
            let (pk, mut io) = noise_upgrade_future.await?;
            Ok((pk, io))
        }
        .boxed()
    }
}
