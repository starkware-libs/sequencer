//! Networking utility functions and types.
//!
//! This module provides utility functions and data structures that support
//! the broader networking functionality. It includes multiaddress utilities,
//! stream management helpers, and other common networking operations.
//!
//! ## Key Components
//!
//! - [`StreamMap`]: A concurrent stream multiplexer for managing multiple async streams
//! - Address utilities: Functions for creating and validating network addresses
//! - Helper functions for common networking operations

use core::net::Ipv4Addr;
use std::collections::btree_map::{Keys, ValuesMut};
use std::collections::BTreeMap;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use futures::stream::{Stream, StreamExt};
use libp2p::core::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};

/// A concurrent stream multiplexer that polls multiple streams in a round-robin fashion.
///
/// `StreamMap` allows you to manage multiple async streams with associated keys,
/// polling them fairly and yielding items along with their originating keys.
/// This is particularly useful for managing multiple protocol sessions or
/// connections where you need to track which stream produced each item.
///
/// This is a custom implementation of `StreamMap` from `tokio_stream`, adapted
/// to work with the `futures` crate's `Stream` trait rather than `tokio_stream`'s
/// version.
///
/// # Type Parameters
///
/// * `K` - The key type, must be orderable for internal storage in a `BTreeMap`
/// * `V` - The stream type, must implement `Stream + Unpin`
///
/// # Examples
///
/// ```rust,no_run
/// use std::collections::BTreeMap;
///
/// use apollo_network::utils::StreamMap;
/// use futures::stream::{self, StreamExt};
///
/// # async fn example() {
/// let mut map = BTreeMap::new();
/// map.insert("stream1", stream::iter(vec![1, 2, 3]));
/// map.insert("stream2", stream::iter(vec![4, 5, 6]));
///
/// let mut stream_map = StreamMap::new(map);
///
/// // Items will be yielded with their originating keys
/// while let Some((key, value)) = stream_map.next().await {
///     if let Some(v) = value {
///         println!("Got {} from {}", v, key);
///     }
/// }
/// # }
/// ```
///
/// # Fair Polling
///
/// The implementation uses round-robin polling to ensure fairness across all
/// streams. No single stream can monopolize the polling, which is important
/// for maintaining responsiveness in networking applications.
pub struct StreamMap<K: Unpin + Clone + Ord, V: Stream + Unpin> {
    map: BTreeMap<K, V>,
    last_waker_waiting_for_new_stream: Option<Waker>,
    next_index_to_poll: Option<usize>,
}

impl<K: Unpin + Clone + Ord, V: Stream + Unpin> StreamMap<K, V> {
    pub fn new(map: BTreeMap<K, V>) -> Self {
        Self { map, last_waker_waiting_for_new_stream: None, next_index_to_poll: None }
    }

    pub fn values_mut(&mut self) -> ValuesMut<'_, K, V> {
        self.map.values_mut()
    }

    pub fn keys(&self) -> Keys<'_, K, V> {
        self.map.keys()
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.map.get_mut(key)
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let res = self.map.insert(key, value);
        if let Some(waker) = self.last_waker_waiting_for_new_stream.take() {
            waker.wake();
        }
        res
    }
}

impl<K: Unpin + Clone + Ord, V: Stream + Unpin> Stream for StreamMap<K, V> {
    type Item = (K, Option<<V as Stream>::Item>);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let unpinned_self = Pin::into_inner(self);
        let mut finished_stream_key: Option<K> = None;
        let next_index_to_poll = unpinned_self.next_index_to_poll.take().unwrap_or_default();
        let size_of_map = unpinned_self.map.len();
        let keys = unpinned_self
            .map
            .keys()
            .cloned()
            .enumerate()
            .cycle()
            .skip(next_index_to_poll)
            .take(size_of_map)
            .collect::<Vec<_>>()
            .into_iter();
        for (index, key) in keys {
            let stream = unpinned_self.map.get_mut(&key).unwrap();
            // poll the stream
            match stream.poll_next_unpin(cx) {
                Poll::Ready(Some(value)) => {
                    unpinned_self.next_index_to_poll =
                        Some(index.checked_add(1).unwrap_or_default());
                    return Poll::Ready(Some((key.clone(), Some(value))));
                }
                Poll::Ready(None) => {
                    unpinned_self.next_index_to_poll = Some(index);
                    finished_stream_key = Some(key.clone());
                    // breaking and removing the finished stream from the map outside of the loop
                    // because we can't have two mutable references to the map.
                    break;
                }
                Poll::Pending => {}
            }
        }

        if let Some(finished_stream_key) = finished_stream_key {
            unpinned_self.map.remove(&finished_stream_key);
            return Poll::Ready(Some((finished_stream_key, None)));
        }
        unpinned_self.last_waker_waiting_for_new_stream = Some(cx.waker().clone());
        Poll::Pending
    }
}

pub fn is_localhost(address: &Multiaddr) -> bool {
    let maybe_ip4_address = address.iter().find_map(|protocol| match protocol {
        Protocol::Ip4(ip4_address) => Some(ip4_address),
        _ => None,
    });
    let Some(ip4_address) = maybe_ip4_address else {
        return false;
    };
    ip4_address == Ipv4Addr::LOCALHOST
}

/// Creates a `Multiaddr` from an `Ipv4Addr`, a port, and a `PeerId`.
pub fn make_multiaddr(ip: Ipv4Addr, port: u16, peer_id: Option<PeerId>) -> Multiaddr {
    let mut address = Multiaddr::empty().with(Protocol::Ip4(ip));
    // TODO(AndrewL): address = address.with(Protocol::Udp(port)).with(Protocol::QuicV1);
    address = address.with(Protocol::Tcp(port));
    if let Some(peer_id) = peer_id {
        address = address.with(Protocol::P2p(peer_id))
    }
    address
}
