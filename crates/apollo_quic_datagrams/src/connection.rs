// Copyright 2020 Parity Technologies (UK) Ltd.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

mod connecting;
mod stream;

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

pub use connecting::Connecting;
use futures::future::BoxFuture;
use futures::FutureExt;
use libp2p::core::muxing::{StreamMuxer, StreamMuxerEvent};
pub use stream::Stream;
#[cfg(feature = "tokio")]
use tokio::task::JoinHandle;

use crate::{ConnectionError, Error};

/// State for a single opened QUIC connection.
pub struct Connection {
    /// Underlying connection.
    connection: quinn::Connection,
    /// Future for accepting a new incoming bidirectional stream.
    incoming: Option<
        BoxFuture<'static, Result<(quinn::SendStream, quinn::RecvStream), quinn::ConnectionError>>,
    >,
    /// Future for opening a new outgoing bidirectional stream.
    outgoing: Option<
        BoxFuture<'static, Result<(quinn::SendStream, quinn::RecvStream), quinn::ConnectionError>>,
    >,
    /// Future to wait for the connection to be closed.
    closing: Option<BoxFuture<'static, quinn::ConnectionError>>,
    /// Background task that periodically logs statistics.
    #[cfg(feature = "tokio")]
    stats_task: Option<JoinHandle<()>>,
}

impl Connection {
    /// Default interval for automatic statistics logging (30 seconds).
    pub const DEFAULT_STATS_LOG_INTERVAL: Duration = Duration::from_secs(30);

    /// Build a [`Connection`] from raw components.
    ///
    /// This function assumes that the [`quinn::Connection`] is completely fresh and none of
    /// its methods has ever been called. Failure to comply might lead to logic errors and panics.
    fn new(connection: quinn::Connection) -> Self {
        Self::new_with_stats_interval(connection, Self::DEFAULT_STATS_LOG_INTERVAL)
    }

    /// Build a [`Connection`] with a custom statistics logging interval.
    ///
    /// The background task will periodically log QUIC statistics at DEBUG level.
    /// Set `stats_interval` to control how often statistics are logged.
    ///
    /// Note: Automatic statistics logging only works with the `tokio` feature enabled.
    fn new_with_stats_interval(connection: quinn::Connection, stats_interval: Duration) -> Self {
        let remote_addr = connection.remote_address();

        tracing::debug!(
            remote_addr = %remote_addr,
            "QUIC connection established"
        );

        // Spawn background task to periodically log statistics (tokio only)
        #[cfg(feature = "tokio")]
        let stats_task = {
            let conn = connection.clone();
            Some(tokio::spawn(async move {
                let mut interval = tokio::time::interval(stats_interval);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                loop {
                    interval.tick().await;

                    // Stop if connection is closed
                    if conn.close_reason().is_some() {
                        break;
                    }

                    let stats = conn.stats();
                    let loss_rate = if stats.path.sent_packets > 0 {
                        (stats.path.lost_packets as f64 / stats.path.sent_packets as f64) * 100.0
                    } else {
                        0.0
                    };

                    tracing::info!(
                        remote_addr = %conn.remote_address(),
                        lost_packets = stats.path.lost_packets,
                        lost_bytes = stats.path.lost_bytes,
                        sent_packets = stats.path.sent_packets,
                        loss_rate_percent = %format!("{:.2}", loss_rate),
                        congestion_events = stats.path.congestion_events,
                        rtt_millis = stats.path.rtt.as_millis(),
                        cwnd = stats.path.cwnd,
                        udp_tx_datagrams = stats.udp_tx.datagrams,
                        udp_tx_bytes = stats.udp_tx.bytes,
                        udp_rx_datagrams = stats.udp_rx.datagrams,
                        udp_rx_bytes = stats.udp_rx.bytes,
                        "QUIC connection statistics"
                    );
                }

                tracing::debug!(
                    remote_addr = %conn.remote_address(),
                    "QUIC connection statistics logging stopped"
                );
            }))
        };

        // Avoid unused variable warning when tokio feature is not enabled
        #[cfg(not(feature = "tokio"))]
        let _ = stats_interval;

        Self {
            connection,
            incoming: None,
            outgoing: None,
            closing: None,
            #[cfg(feature = "tokio")]
            stats_task,
        }
    }
}

impl StreamMuxer for Connection {
    type Substream = Stream;
    type Error = Error;

    fn poll_inbound(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        let this = self.get_mut();

        let incoming = this.incoming.get_or_insert_with(|| {
            let connection = this.connection.clone();
            async move { connection.accept_bi().await }.boxed()
        });

        let (send, recv) = futures::ready!(incoming.poll_unpin(cx)).map_err(ConnectionError)?;
        this.incoming.take();
        let stream = Stream::new(send, recv);
        Poll::Ready(Ok(stream))
    }

    fn poll_outbound(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        let this = self.get_mut();

        let outgoing = this.outgoing.get_or_insert_with(|| {
            let connection = this.connection.clone();
            async move { connection.open_bi().await }.boxed()
        });

        let (send, recv) = futures::ready!(outgoing.poll_unpin(cx)).map_err(ConnectionError)?;
        this.outgoing.take();
        let stream = Stream::new(send, recv);
        Poll::Ready(Ok(stream))
    }

    fn poll(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<StreamMuxerEvent, Self::Error>> {
        // TODO: If connection migration is enabled (currently disabled) address
        // change on the connection needs to be handled.
        Poll::Pending
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.get_mut();

        let closing = this.closing.get_or_insert_with(|| {
            this.connection.close(From::from(0u32), &[]);
            tracing::debug!(
                remote_addr = %this.connection.remote_address(),
                "QUIC connection closing"
            );
            let connection = this.connection.clone();
            async move { connection.closed().await }.boxed()
        });

        match futures::ready!(closing.poll_unpin(cx)) {
            // Expected error given that `connection.close` was called above.
            quinn::ConnectionError::LocallyClosed => {
                tracing::debug!(
                    remote_addr = %this.connection.remote_address(),
                    "QUIC connection closed successfully"
                );
            }
            error => {
                tracing::warn!(
                    remote_addr = %this.connection.remote_address(),
                    error = %error,
                    "QUIC connection closed with error"
                );
                return Poll::Ready(Err(Error::Connection(ConnectionError(error))));
            }
        };

        Poll::Ready(Ok(()))
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // Abort the stats logging task if it's still running
        #[cfg(feature = "tokio")]
        if let Some(task) = self.stats_task.take() {
            task.abort();
        }

        tracing::trace!(
            remote_addr = %self.connection.remote_address(),
            "QUIC connection dropped"
        );
    }
}
