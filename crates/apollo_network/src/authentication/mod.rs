use core::future::Future;
use std::collections::VecDeque;
use std::convert::{Infallible, TryFrom};
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use asynchronous_codec::{Decoder, Encoder, Framed};
use bytes::{Buf, BytesMut};
use futures::channel::mpsc::{self, channel, SendError};
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use futures::{FutureExt, Sink, Stream};
use libp2p::core::upgrade::{InboundConnectionUpgrade, OutboundConnectionUpgrade};
use libp2p::core::UpgradeInfo;
use libp2p::noise::{self, Error, Output};
use libp2p::{identity, PeerId};

pub enum NegotiatorResult {
    Ok,
    /// Returned when the handshake concluded that the currently connecting peer is a duplicate of
    /// the currently connected peer. `PeerId` is the (older) currently connected peer.
    DuplicatePeer(PeerId),
}

#[async_trait]
pub trait Negotiator: Send + Clone {
    type Message: TryFrom<Vec<u8>> + Into<Vec<u8>> + Send;

    async fn negotiate_incoming_connection<NegotiatorChannel>(
        &self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection: &mut NegotiatorChannel,
    ) -> Result<NegotiatorResult, Error>
    where
        NegotiatorChannel: Sink<Self::Message, Error = std::io::Error>
            + Stream<Item = Result<Self::Message, std::io::Error>>
            + Unpin
            + Send;

    async fn negotiate_outgoing_connection<NegotiatorChannel>(
        &self,
        my_peer_id: PeerId,
        other_peer_id: PeerId,
        connection: &mut NegotiatorChannel,
    ) -> Result<NegotiatorResult, Error>
    where
        NegotiatorChannel: Sink<Self::Message, Error = std::io::Error>
            + Stream<Item = Result<Self::Message, std::io::Error>>
            + Unpin
            + Send;

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
        Ok(NegotiatorResult::Ok)
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

            if let NegotiatorResult::DuplicatePeer(dup_peer_id) = self
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

// TODO: reuse code from InboundConnectionUpgrade and complete.
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

            if let NegotiatorResult::DuplicatePeer(dup_peer_id) = self
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

pub struct NegotiatorChannelCodec<T> {
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

// BLA
// BLA
// BLA
// BLA
// BLA
// BLA
// BLA
// BLA
// BLA
//
// Old implementation.
// struct NegotiatorChannel<T, M> {
//     output: Output<T>,
//     pending_write: Option<Vec<u8>>,
//     pending_read: VecDeque<u8>,
//     _phantom: PhantomData<M>,
// }

// impl<T, M> NegotiatorChannel<T, M> {
//     pub fn new(output: Output<T>) -> Self {
//         Self { output, pending_write: None, pending_read: VecDeque::new(), _phantom: PhantomData
// }     }

//     /// Helper method to handle pending write operations.
//     fn poll_pending_write(
//         mut self: Pin<&mut Self>,
//         cx: &mut Context<'_>,
//     ) -> Poll<Result<(), std::io::Error>>
//     where
//         T: AsyncWrite + Unpin,
//     {
//         if let Some(data) = self.pending_write.take() {
//             match Pin::new(&mut self.output).poll_write(cx, &data) {
//                 Poll::Ready(Ok(n)) => {
//                     if n < data.len() {
//                         // Partial write, store unwritten part back in pending_write.
//                         self.pending_write = Some(data[n..].to_vec());
//                         Poll::Pending
//                     } else {
//                         // Successfully wrote all data, now flush
//                         Pin::new(&mut self.output).poll_flush(cx)
//                     }
//                 }
//                 Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
//                 Poll::Pending => {
//                     // Couldn't write yet, put data back.
//                     self.pending_write = Some(data);
//                     Poll::Pending
//                 }
//             }
//         } else {
//             // No pending data, just flush
//             Pin::new(&mut self.output).poll_flush(cx)
//         }
//     }
// }

// // Implement Unpin manually since M is not required to be Unpin however the compiler doesn't know
// // that PhantomData is still unpin.
// impl<T, M> Unpin for NegotiatorChannel<T, M> where T: Unpin {}

// impl<T, M> Stream for NegotiatorChannel<T, M>
// where
//     T: AsyncRead + Unpin,
//     M: TryFrom<Vec<u8>>,
// {
//     type Item = M;

//     fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
//         // TODO: Read what we actually need.

//         // TODO: Using pending_read to read the bytes until we have enough for the message.

//         let mut buf = vec![0; 4];

//         match Pin::new(&mut self.output).poll_read(cx, &mut buf) {
//             Poll::Ready(Ok(0)) => Poll::Ready(None), // End of stream
//             Poll::Ready(Ok(n)) => {
//                 // TODO: Handle reading until we have enough bytes for the message.
//                 buf.truncate(n);
//                 // Convert Vec<u8> to M
//                 match M::try_from(buf) {
//                     Ok(message) => Poll::Ready(Some(message)),
//                     Err(_) => Poll::Ready(None), // Conversion error treated as end of stream
//                 }
//             }
//             Poll::Ready(Err(_)) => Poll::Ready(None), // Error treated as end of stream
//             Poll::Pending => Poll::Pending,
//         }
//     }
// }

// impl<T, M> Sink<M> for NegotiatorChannel<T, M>
// where
//     T: AsyncWrite + Unpin,
//     M: Into<Vec<u8>>,
// {
//     type Error = std::io::Error;

//     fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
//         self.poll_pending_write(cx)
//     }

//     fn start_send(mut self: Pin<&mut Self>, item: M) -> Result<(), Self::Error> {
//         if self.pending_write.is_some() {
//             // We shouldn't get more send calls if we still have pending writes (which we
// reported             // in poll_ready).
//             return Err(std::io::Error::new(
//                 std::io::ErrorKind::WouldBlock,
//                 "Previous write still pending",
//             ));
//         }
//         // TODO: Write the length of the data before the data.
//         let mut data = Vec::new();
//         // data.push(/* varint size */);
//         data.append(&mut item.into());
//         self.pending_write = Some(data);
//         Ok(())
//     }

//     fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
//         self.poll_pending_write(cx)
//     }

//     fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(),
// Self::Error>> {         // Flush any pending writes before closing
//         match self.as_mut().poll_flush(cx) {
//             Poll::Ready(Ok(())) => Pin::new(&mut self.output).poll_close(cx),
//             Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
//             Poll::Pending => Poll::Pending,
//         }
//     }
// }
