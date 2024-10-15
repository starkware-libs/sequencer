use core::net::Ipv4Addr;
use std::collections::hash_map::{Keys, ValuesMut};
use std::collections::HashMap;
use std::hash::Hash;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use futures::stream::{Stream, StreamExt};
use libp2p::core::multiaddr::Protocol;
use libp2p::Multiaddr;

// This is an implementation of `StreamMap` from tokio_stream. The reason we're implementing it
// ourselves is that the implementation in tokio_stream requires that the values implement the
// Stream trait from tokio_stream and not from futures.
pub struct StreamHashMap<K: Unpin + Clone + Eq + Hash, V: Stream + Unpin> {
    map: HashMap<K, V>,
    wakers_waiting_for_new_stream: Vec<Waker>,
}

impl<K: Unpin + Clone + Eq + Hash, V: Stream + Unpin> StreamHashMap<K, V> {
    #[allow(dead_code)]
    pub fn new(map: HashMap<K, V>) -> Self {
        Self { map, wakers_waiting_for_new_stream: Default::default() }
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

impl<K: Unpin + Clone + Eq + Hash, V: Stream + Unpin> Stream for StreamHashMap<K, V> {
    type Item = (K, Option<<V as Stream>::Item>);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let unpinned_self = Pin::into_inner(self);
        let mut finished_stream_key: Option<K> = None;
        for (key, stream) in &mut unpinned_self.map {
            match stream.poll_next_unpin(cx) {
                Poll::Ready(Some(value)) => {
                    return Poll::Ready(Some((key.clone(), Some(value))));
                }
                Poll::Ready(None) => {
                    finished_stream_key = Some(key.clone());
                    // breaking and removing the finished stream from the map outside of the loop.
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
