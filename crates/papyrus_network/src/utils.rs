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

    /// Polls the streams in the map. If a stream is ready, returns the key and the value. If a
    /// stream is finished, remove it from the map and return the key with None. If there is no
    /// ready steam (either because there are no streams or because non of the streams are ready),
    /// return Poll::Pending.
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

mod test {
    use std::collections::{HashMap, VecDeque};

    use futures::{stream::StreamExt, FutureExt};

    use super::*;

    #[tokio::test]
    async fn test_stream_hash_map_basic_usage() {
        // init a new stream hash map with two streams.
        let mut stream_hash_map = StreamHashMap::new(HashMap::new());
        stream_hash_map.insert("key1", futures::stream::iter(vec![1, 2, 3]));
        stream_hash_map.insert("key2", futures::stream::iter(vec![4, 5, 6]));

        let mut expected_values = HashMap::from([
            ("key1", VecDeque::from([1, 2, 3])),
            ("key2", VecDeque::from([4, 5, 6])),
        ]);

        // Because the streams are ready, now_or_never will return None when both streams are empty.
        // Using await would have resulted in an eternal loop because poll_next always matches the pattern
        while let Some(Some((key, maybe_value))) = stream_hash_map.next().now_or_never() {
            match maybe_value {
                Some(value) => {
                    println!("Received value {} from {}.", value, key);
                    let expected_value = expected_values.get_mut(key).unwrap();
                    assert_eq!(value, expected_value.pop_front().unwrap());
                }
                None => {
                    println!("key {} is an empty stream. Dropping the key.", key);
                    assert!(expected_values.get(key).unwrap().is_empty());
                    expected_values.remove(key);
                }
            }
        }
        // After polling the entire map, assert that all of the expected values have been polled
        assert!(expected_values.is_empty());

        // insert a new stream to the map and assert the map continues functioning properly.
        stream_hash_map.insert("key3", futures::stream::iter(vec![1,4,7]));
        expected_values.insert("key3", VecDeque::from([1,4,7]));

        while let Some(Some((key, maybe_value))) = stream_hash_map.next().now_or_never() {
            match maybe_value {
                Some(value) => {
                    println!("Received value {} from {}.", value, key);
                    let expected_value = expected_values.get_mut(key).unwrap();
                    assert_eq!(value, expected_value.pop_front().unwrap());
                }
                None => {
                    println!("key {} is an empty stream. Dropping the key.", key);
                    assert!(expected_values.get(key).unwrap().is_empty());
                    expected_values.remove(key);
                }
            }
        }

        // After polling the entire map, assert that all of the expected values have been polled
        assert!(expected_values.is_empty());
    }
}
