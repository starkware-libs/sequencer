use tokio::sync::watch::{self, Receiver, Sender};

use crate::component_definitions::ComponentChannelClient;

#[cfg(test)]
#[path = "local_component_channel_client_test.rs"]
mod local_component_channel_client_test;

/// A local client that reads the latest value from a [`tokio::sync::watch`]
/// channel.
#[derive(Clone)]
pub struct LocalComponentChannelClient<T>
where
    T: Send + Sync + Clone,
{
    value_rx: Receiver<T>,
}

impl<T> LocalComponentChannelClient<T>
where
    T: Send + Sync + Clone,
{
    pub fn new(value_rx: Receiver<T>) -> Self {
        Self { value_rx }
    }

    pub fn new_with_initial_value(initial_value: T) -> (Sender<T>, Self) {
        let (value_tx, value_rx) = watch::channel(initial_value);
        (value_tx, Self { value_rx })
    }
}

impl<T> ComponentChannelClient<T> for LocalComponentChannelClient<T>
where
    T: Send + Sync + Clone,
{
    fn get_value(&self) -> T {
        // `borrow()` returns a reference to the value owned by the channel, hence we clone it.
        self.value_rx.borrow().clone()
    }
}
