use core::net::Ipv4Addr;
use std::collections::btree_map::{Keys, ValuesMut};
use std::collections::BTreeMap;
use std::net::{TcpListener, UdpSocket};
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use futures::stream::{Stream, StreamExt};
use libp2p::core::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};

// This is an implementation of `StreamMap` from tokio_stream. The reason we're implementing it
// ourselves is that the implementation in tokio_stream requires that the values implement the
// Stream trait from tokio_stream and not from futures.
pub struct StreamMap<K: Unpin + Clone + Ord, V: Stream + Unpin> {
    map: BTreeMap<K, V>,
    wakers_waiting_for_new_stream: Vec<Waker>,
    next_index_to_poll: Option<usize>,
}

impl<K: Unpin + Clone + Ord, V: Stream + Unpin> StreamMap<K, V> {
    #[allow(dead_code)]
    pub fn new(map: BTreeMap<K, V>) -> Self {
        Self { map, wakers_waiting_for_new_stream: Default::default(), next_index_to_poll: None }
    }

    #[allow(dead_code)]
    pub fn values_mut(&mut self) -> ValuesMut<'_, K, V> {
        self.map.values_mut()
    }

    #[allow(dead_code)]
    pub fn keys(&self) -> Keys<'_, K, V> {
        self.map.keys()
    }

    #[allow(dead_code)]
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.map.get_mut(key)
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let res = self.map.insert(key, value);
        for waker in self.wakers_waiting_for_new_stream.drain(..) {
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
        unpinned_self.wakers_waiting_for_new_stream.push(cx.waker().clone());
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

/// Decides if QUIC over UDP or TCP should be used...
/// When changing this, change config files and yaml files by dumping
pub(super) const USE_QUIC: bool = false;

pub enum NetworkAddress {
    Ip4(Ipv4Addr),
    Dns(String),
}

impl NetworkAddress {
    pub const LOCALHOST: Self = NetworkAddress::Ip4(Ipv4Addr::LOCALHOST);
    pub const UNSPECIFIED: Self = NetworkAddress::Ip4(Ipv4Addr::UNSPECIFIED);
}

/// Creates a `Multiaddr` from an `Ipv4Addr`, a port, and a `PeerId`.
pub fn make_multiaddr(ip: NetworkAddress, port: u16, peer_id: Option<PeerId>) -> Multiaddr {
    let mut address = Multiaddr::empty();

    address = match ip {
        NetworkAddress::Ip4(ip) => address.with(Protocol::Ip4(ip)),
        NetworkAddress::Dns(url) => address.with(Protocol::Dns(std::borrow::Cow::Owned(url))),
    };

    address = if USE_QUIC {
        address.with(Protocol::Udp(port)).with(Protocol::QuicV1)
    } else {
        address.with(Protocol::Tcp(port))
    };

    if let Some(peer_id) = peer_id {
        address = address.with(Protocol::P2p(peer_id));
    };
    address
}

// WARNING(Tsabary): This is not a reliable way to obtain a free port; most notably it fails when
// multiple concurrent instances try to obtain ports using this function. Do NOT use this in
// production code, nor in tests, as they run concurrently.
pub fn find_free_port() -> u16 {
    // The socket is automatically closed when the function exits.
    // The port may still be available when accessed, but this is not guaranteed.
    // TODO(Asmaa): find a reliable way to ensure the port stays free.
    if USE_QUIC {
        let listener = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind");
        listener.local_addr().expect("Failed to get local address").port()
    } else {
        let listener = TcpListener::bind("0.0.0.0:0").expect("Failed to bind");
        listener.local_addr().expect("Failed to get local address").port()
    }
}

pub fn port_description() -> String {
    format!(
        "The port that the node listens on for incoming {}.",
        if USE_QUIC { "udp connections for quic" } else { "tcp connections" }
    )
}
